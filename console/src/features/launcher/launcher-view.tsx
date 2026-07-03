import { useCallback, useEffect, useState } from 'react'
import { Check, Copy, Cpu, FolderOpen, HardDrive, RefreshCw, Square } from 'lucide-react'
import { cn } from '@/lib/cn'
import { tl } from './strings'

// Wire types mirror the kestrel-runtime / launcher server contract (language-neutral).
type ModelFile = {
  path: string
  name: string
  arch: string
  quant: string
  params: string
  size_bytes: number
}
type ModelsData = { dir: string; models: ModelFile[]; total_bytes: number }
type RunningEngine = { base_url: string; kind: string; n_ctx: number | null; model: string | null }
type BinaryCandidate = { path: string; on_path: boolean }
type ScanResult = { binaries: BinaryCandidate[]; running: RunningEngine[] }
type EngineState = 'stopped' | 'loading' | 'running' | 'failed'
type EngineStatus = { state: EngineState; base_url: string; model: string; error: string }

const DIR_KEY = 'kestrel.modelsDir'

/**
 * Model launcher (ADR-0010). Guided top-down flow: point at your models folder,
 * pick a model, hit Load — it runs as a local llama.cpp server (start/stop/status).
 * Discovery/launch stay loopback + whitelisted-bin (§5); the agent connects to
 * whatever base_url its config points at.
 */
export function LauncherView() {
  const [dir, setDir] = useState(() => {
    try {
      return localStorage.getItem(DIR_KEY) ?? ''
    } catch {
      return ''
    }
  })
  const [models, setModels] = useState<ModelsData | null>(null)
  const [scan, setScan] = useState<ScanResult | null>(null)
  const [status, setStatus] = useState<EngineStatus | null>(null)
  const [busy, setBusy] = useState(false)
  const [apiError, setApiError] = useState(false)
  const [selectedBin, setSelectedBin] = useState('')
  const [pending, setPending] = useState('') // model path currently being launched
  const [copied, setCopied] = useState('')

  const loadStatus = useCallback(async () => {
    const res = await fetch('/api/launcher/status')
    if (res.ok) setStatus((await res.json()) as EngineStatus)
  }, [])

  const rescan = useCallback(async (d: string) => {
    setBusy(true)
    setApiError(false)
    try {
      const q = d ? `?dir=${encodeURIComponent(d)}` : ''
      const [mRes, sRes] = await Promise.all([
        fetch(`/api/launcher/models${q}`),
        fetch('/api/launcher/scan'),
      ])
      if (!mRes.ok || !sRes.ok) throw new Error('launcher api')
      const mData = (await mRes.json()) as ModelsData
      setModels(mData)
      setScan((await sRes.json()) as ScanResult)
      if (!d && mData.dir) setDir(mData.dir) // adopt auto-detected folder
      await loadStatus()
    } catch {
      setApiError(true)
    } finally {
      setBusy(false)
    }
  }, [loadStatus])

  // mount: initial scan with the persisted (or empty -> auto-detected) folder
  useEffect(() => {
    void rescan(dir)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // default the engine binary to the first discovered llama-server
  useEffect(() => {
    if (!selectedBin && scan?.binaries.length) setSelectedBin(scan.binaries[0].path)
  }, [scan, selectedBin])

  // poll status while an engine is loading or running
  useEffect(() => {
    if (status?.state !== 'loading' && status?.state !== 'running') return
    const t = setInterval(() => void loadStatus(), 1500)
    return () => clearInterval(t)
  }, [status?.state, loadStatus])

  const applyDir = () => {
    try {
      localStorage.setItem(DIR_KEY, dir)
    } catch {
      /* storage unavailable */
    }
    void rescan(dir)
  }

  const loadModel = async (m: ModelFile) => {
    if (!selectedBin) return
    setPending(m.path)
    try {
      await fetch('/api/launcher/launch', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ source: 'self', bin: selectedBin, model_path: m.path, model: m.name }),
      })
      await loadStatus()
    } catch {
      /* surfaced via status polling / failed state */
    } finally {
      setPending('')
    }
  }

  const stopEngine = async () => {
    await fetch('/api/launcher/stop', { method: 'POST' }).catch(() => {})
    await loadStatus()
  }

  const copyConnect = async (e: RunningEngine) => {
    try {
      await navigator.clipboard.writeText(connectSnippet(e))
      setCopied(e.base_url)
    } catch {
      /* clipboard blocked */
    }
  }

  const bins = scan?.binaries ?? []
  const running = scan?.running ?? []
  const list = models?.models ?? []
  const engineActive = status && status.state !== 'stopped'
  const canLoad = !!selectedBin

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-4xl px-6 py-8">
        <div className="mb-5 flex items-start justify-between gap-4">
          <div>
            <h2 className="text-[17px] font-semibold tracking-[-0.01em]">{tl('launcher.title')}</h2>
            <p className="mt-1 text-[13px] leading-relaxed text-ink-3">{tl('launcher.howto')}</p>
          </div>
          <button
            type="button"
            onClick={() => void rescan(dir)}
            disabled={busy}
            className="focus-ring flex h-8 shrink-0 items-center gap-1.5 rounded-md border border-line px-2.5 text-[13px] text-ink-2 transition-colors hover:bg-surface-2 disabled:opacity-50"
          >
            <RefreshCw className={cn('h-[15px] w-[15px]', busy && 'animate-spin')} strokeWidth={1.8} />
            {busy ? tl('launcher.scanning') : tl('launcher.rescan')}
          </button>
        </div>

        {apiError && (
          <div className="mb-5 rounded-lg border border-crit/40 bg-crit/10 px-4 py-3 text-[13px] text-ink-2">
            {tl('launcher.error')}
          </div>
        )}

        {/* Local server — the live status hero. Directs the user down when idle. */}
        <div
          className={cn(
            'mb-6 rounded-lg border px-4 py-3.5',
            status?.state === 'running'
              ? 'border-ok/30 bg-ok/5'
              : status?.state === 'failed'
                ? 'border-crit/30 bg-crit/5'
                : 'border-line bg-surface',
          )}
        >
          <div className="flex items-center gap-2">
            <span className="text-[13px] font-semibold text-ink-2">{tl('launcher.engine.title')}</span>
            <StateBadge state={status?.state ?? 'stopped'} />
            {engineActive && (
              <button
                type="button"
                onClick={() => void stopEngine()}
                className="focus-ring ml-auto flex items-center gap-1.5 rounded-md border border-line px-2.5 py-1 text-[12px] text-ink-2 transition-colors hover:bg-surface-2"
              >
                <Square className="h-[12px] w-[12px]" strokeWidth={2} />
                {tl('launcher.stop')}
              </button>
            )}
          </div>
          {engineActive ? (
            <div className="mt-2 space-y-1.5 text-[12.5px]">
              {status?.model && <div className="font-mono text-[13px] text-ink">{status.model}</div>}
              {status?.base_url && (
                <div className="flex items-center gap-1.5 text-ink-mute">
                  {tl('launcher.engine.reachable')}
                  <span className="rounded bg-surface-2 px-1.5 py-0.5 font-mono text-accent-ink">
                    {status.base_url}
                  </span>
                </div>
              )}
              {status?.state === 'failed' && status.error && (
                <div className="font-mono text-[12px] text-crit">{status.error}</div>
              )}
            </div>
          ) : (
            <p className="mt-1.5 text-[12.5px] text-ink-mute">{tl('launcher.engine.idle')}</p>
          )}
        </div>

        {/* Step 1 — models folder + engine binary */}
        <SectionLabel text={tl('launcher.step.folder')} />
        <div className="mb-6 flex flex-col gap-2 sm:flex-row sm:items-end">
          <div className="flex-1">
            <div className="flex gap-2">
              <div className="focus-within:border-line-2 flex min-w-0 flex-1 items-center gap-2 rounded-md border border-line bg-surface px-2.5">
                <FolderOpen className="h-[15px] w-[15px] shrink-0 text-ink-mute" strokeWidth={1.8} />
                <input
                  value={dir}
                  onChange={(e) => setDir(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && applyDir()}
                  placeholder={tl('launcher.dir.placeholder')}
                  spellCheck={false}
                  className="min-w-0 flex-1 bg-transparent py-2 font-mono text-[12.5px] text-ink-2 placeholder:text-ink-mute focus:outline-none"
                />
              </div>
              <button
                type="button"
                onClick={applyDir}
                className="focus-ring shrink-0 rounded-md border border-line px-3 text-[13px] text-ink-2 transition-colors hover:bg-surface-2"
              >
                {tl('launcher.rescan')}
              </button>
            </div>
          </div>
          <div className="sm:w-56">
            {bins.length > 0 ? (
              <div className="flex items-center gap-2 rounded-md border border-line bg-surface px-2.5">
                <Cpu className="h-[15px] w-[15px] shrink-0 text-ink-mute" strokeWidth={1.8} />
                <select
                  value={selectedBin}
                  onChange={(e) => setSelectedBin(e.target.value)}
                  className="w-full truncate bg-transparent py-2 font-mono text-[12.5px] text-ink-2 focus:outline-none"
                >
                  {bins.map((b) => (
                    <option key={b.path} value={b.path} className="bg-bezel">
                      {basename(b.path)}
                      {b.on_path ? ` · ${tl('launcher.bin.onPath')}` : ''}
                    </option>
                  ))}
                </select>
              </div>
            ) : (
              <div className="rounded-md border border-dashed border-line px-2.5 py-2 text-[12px] text-ink-mute">
                {tl('launcher.bin.none')}
              </div>
            )}
          </div>
        </div>

        {/* Step 2 — pick a model to load */}
        <div className="mb-2 flex items-center justify-between">
          <SectionLabel text={tl('launcher.step.pick')} />
          {models && list.length > 0 && (
            <span className="text-[12px] text-ink-mute">
              {tl('launcher.models.meta', { n: list.length, size: fmtBytes(models.total_bytes) })}
            </span>
          )}
        </div>
        {list.length === 0 ? (
          <div className="flex flex-col items-center gap-2 rounded-lg border border-dashed border-line px-4 py-10 text-center">
            <FolderOpen className="h-6 w-6 text-ink-mute" strokeWidth={1.5} />
            <p className="text-[13px] text-ink-2">{tl('launcher.models.empty')}</p>
            <p className="max-w-sm text-[12px] text-ink-mute">{tl('launcher.models.emptyHint')}</p>
          </div>
        ) : (
          <div className="flex flex-col gap-1.5">
            {list.map((m) => (
              <div
                key={m.path}
                className="flex items-center gap-3 rounded-lg border border-line bg-surface px-3 py-2.5 transition-colors hover:border-line-2"
              >
                <span className="grid h-9 w-9 shrink-0 place-items-center rounded-md bg-accent/12 text-accent-ink">
                  <Cpu className="h-[18px] w-[18px]" strokeWidth={1.7} />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[13.5px] font-medium text-ink" title={m.path}>
                    {m.name}
                  </div>
                  <div className="mt-0.5 flex flex-wrap items-center gap-1.5">
                    {m.arch && <Pill tone="plain">{m.arch}</Pill>}
                    {m.params && <Pill tone="plain">{m.params}</Pill>}
                    {m.quant && <Pill tone="accent">{m.quant}</Pill>}
                  </div>
                </div>
                <span className="shrink-0 whitespace-nowrap font-mono text-[12px] text-ink-mute">
                  {fmtBytes(m.size_bytes)}
                </span>
                <button
                  type="button"
                  onClick={() => void loadModel(m)}
                  disabled={!canLoad || pending === m.path}
                  title={canLoad ? undefined : tl('launcher.needBin')}
                  className="focus-ring shrink-0 rounded-md bg-accent px-3.5 py-1.5 text-[12.5px] font-semibold text-desktop transition-colors hover:bg-accent-2 disabled:cursor-not-allowed disabled:opacity-40"
                >
                  {pending === m.path ? tl('launcher.loadingBtn') : tl('launcher.load')}
                </button>
              </div>
            ))}
          </div>
        )}

        {/* Running engines detected on common ports */}
        <div className="mt-8">
          <SectionLabel text={tl('launcher.running.title')} />
          <div className="mt-2 overflow-hidden rounded-lg border border-line">
            {running.length === 0 ? (
              <div className="px-4 py-4 text-[12.5px] text-ink-mute">{tl('launcher.running.empty')}</div>
            ) : (
              running.map((e) => (
                <div
                  key={e.base_url}
                  className="flex items-center gap-3 border-b border-line px-4 py-2.5 last:border-b-0"
                >
                  <HardDrive className="h-[15px] w-[15px] shrink-0 text-ink-mute" strokeWidth={1.8} />
                  <Pill tone="accent">{e.kind}</Pill>
                  <span className="truncate font-mono text-[12.5px] text-ink-2">{e.base_url}</span>
                  <span className="ml-auto shrink-0 truncate text-[12px] text-ink-mute">
                    {e.model ?? ''}
                    {e.n_ctx ? ` · ctx ${e.n_ctx}` : ''}
                  </span>
                  <button
                    type="button"
                    onClick={() => void copyConnect(e)}
                    className="focus-ring flex shrink-0 items-center gap-1.5 rounded-md border border-line px-2 py-1 text-[12px] text-ink-2 transition-colors hover:bg-surface-2"
                  >
                    {copied === e.base_url ? (
                      <Check className="h-[12px] w-[12px]" strokeWidth={2.2} />
                    ) : (
                      <Copy className="h-[12px] w-[12px]" strokeWidth={1.8} />
                    )}
                    {copied === e.base_url ? tl('launcher.copied') : tl('launcher.running.use')}
                  </button>
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

function SectionLabel({ text }: { text: string }) {
  return <div className="text-[11.5px] font-semibold uppercase tracking-wide text-ink-3">{text}</div>
}

function Pill({ children, tone }: { children: React.ReactNode; tone: 'plain' | 'accent' }) {
  return (
    <span
      className={cn(
        'rounded px-1.5 py-0.5 font-mono text-[11px]',
        tone === 'accent'
          ? 'border border-accent/25 text-accent-ink'
          : 'bg-surface-2 text-ink-3',
      )}
    >
      {children}
    </span>
  )
}

function StateBadge({ state }: { state: EngineState }) {
  const map: Record<EngineState, [string, string]> = {
    stopped: [tl('launcher.engine.stopped'), 'text-ink-mute'],
    loading: [tl('launcher.engine.loading'), 'text-warn'],
    running: [tl('launcher.engine.running'), 'text-ok'],
    failed: [tl('launcher.engine.failed'), 'text-crit'],
  }
  const [label, color] = map[state] ?? map.stopped
  return (
    <span className={cn('flex items-center gap-1.5 text-[12px] font-medium', color)}>
      <span className={cn('h-1.5 w-1.5 rounded-full bg-current', state === 'loading' && 'animate-pulse')} />
      {label}
    </span>
  )
}

/** Running engine -> connect/delegate loadout snippet (ollama is openai-compat via delegate). */
function connectSnippet(e: RunningEngine): string {
  const isOllama = e.kind === 'ollama'
  const lines = [
    '[model]',
    `source = "${isOllama ? 'delegate' : 'connect'}"`,
    `base_url = '${e.base_url}'`,
    `kind = "${isOllama ? 'openai' : e.kind}"`,
  ]
  if (e.model) lines.push(`model = "${e.model}"`)
  if (e.n_ctx) lines.push(`n_ctx = ${e.n_ctx}`)
  return lines.join('\n')
}

function basename(p: string): string {
  const parts = p.split(/[\\/]/)
  return parts[parts.length - 1] || p
}

function fmtBytes(n: number): string {
  if (n <= 0) return '-'
  const gb = n / 1024 ** 3
  if (gb >= 1) return `${gb.toFixed(2)} GB`
  const mb = n / 1024 ** 2
  if (mb >= 1) return `${mb.toFixed(0)} MB`
  return `${(n / 1024).toFixed(0)} KB`
}

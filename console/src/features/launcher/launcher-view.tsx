import { useCallback, useEffect, useRef, useState } from 'react'
import { Check, Copy, Cpu, FolderOpen, RefreshCw, Square, SlidersHorizontal } from 'lucide-react'
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
type EngineStatus = {
  state: EngineState
  base_url: string
  model: string
  error: string
  logs: string[]
}

// Per-model tunables (mirrors kestrel-store ModelProfile). Held as strings for form
// binding; converted to the profile wire shape (numbers/array, blanks omitted) on save.
type Draft = { n_ctx: string; gpu_layers: string; port: string; max_tokens: string; extra_args: string }
const DEFAULT_DRAFT: Draft = { n_ctx: '', gpu_layers: 'auto', port: '', max_tokens: '', extra_args: '' }

/** A draft -> the profile wire body (blank fields omitted so they fall back to defaults). */
function toProfile(d: Draft) {
  return {
    n_ctx: d.n_ctx.trim() ? Number(d.n_ctx) : undefined,
    gpu_layers: d.gpu_layers || undefined,
    port: d.port.trim() ? Number(d.port) : undefined,
    max_tokens: d.max_tokens.trim() ? Number(d.max_tokens) : undefined,
    extra_args: d.extra_args.trim() ? d.extra_args.trim().split(/\s+/) : undefined,
  }
}

const DIR_KEY = 'kestrel.modelsDir'

// Module-level cache: scanning a big models folder + probing ports is slow, so a
// scan result is remembered for the session. Revisiting the page hydrates from
// this instantly (no rescan); only the Scan button or a folder change refetches.
// Engine status is live and always refetched on mount.
let SCAN_CACHE: { dir: string; models: ModelsData; scan: ScanResult } | null = null

/**
 * Model launcher (ADR-0010). Point at your models folder, pick a model, hit Load —
 * it runs as a local llama.cpp server (start/stop/status). Discovery/launch stay
 * loopback + whitelisted-bin (§5); the agent connects to whatever base_url its
 * config points at.
 */
export function LauncherView() {
  const [dir, setDir] = useState(() => {
    try {
      return localStorage.getItem(DIR_KEY) ?? ''
    } catch {
      return ''
    }
  })
  const [models, setModels] = useState<ModelsData | null>(SCAN_CACHE?.models ?? null)
  const [scan, setScan] = useState<ScanResult | null>(SCAN_CACHE?.scan ?? null)
  const [status, setStatus] = useState<EngineStatus | null>(null)
  const [busy, setBusy] = useState(false)
  const [apiError, setApiError] = useState(false)
  const [selectedBin, setSelectedBin] = useState('')
  const [pending, setPending] = useState('') // model path currently being launched
  const [copied, setCopied] = useState('')
  // per-model settings: which row's panel is open, each model's editable draft, and a
  // transient "saved" flash. Drafts are lazily hydrated from the backend profile on open.
  const [expanded, setExpanded] = useState<string | null>(null)
  const [drafts, setDrafts] = useState<Record<string, Draft>>({})
  const [saved, setSaved] = useState('')

  const loadStatus = useCallback(async () => {
    try {
      const res = await fetch('/api/launcher/status')
      if (res.ok) setStatus((await res.json()) as EngineStatus)
    } catch {
      /* status is best-effort */
    }
  }, [])

  const scanNow = useCallback(
    async (d: string) => {
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
        const sData = (await sRes.json()) as ScanResult
        const resolvedDir = !d && mData.dir ? mData.dir : d
        setModels(mData)
        setScan(sData)
        if (!d && mData.dir) setDir(mData.dir) // adopt auto-detected folder
        SCAN_CACHE = { dir: resolvedDir, models: mData, scan: sData }
      } catch {
        setApiError(true)
      } finally {
        setBusy(false)
      }
    },
    [],
  )

  // mount: hydrate from cache if present (instant, no rescan); always refresh live status.
  useEffect(() => {
    if (SCAN_CACHE) {
      setDir(SCAN_CACHE.dir)
    } else {
      void scanNow(dir)
    }
    void loadStatus()
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

  const applyScan = () => {
    try {
      localStorage.setItem(DIR_KEY, dir)
    } catch {
      /* storage unavailable */
    }
    void scanNow(dir)
  }

  const draftOf = (path: string): Draft => drafts[path] ?? DEFAULT_DRAFT
  const setField = (path: string, k: keyof Draft, v: string) =>
    setDrafts((d) => ({ ...d, [path]: { ...(d[path] ?? DEFAULT_DRAFT), [k]: v } }))

  // toggle a model's settings panel; hydrate its draft from the saved profile the first time.
  const toggleSettings = useCallback(
    async (m: ModelFile) => {
      setExpanded((cur) => (cur === m.path ? null : m.path))
      if (drafts[m.path]) return
      let next = DEFAULT_DRAFT
      try {
        const res = await fetch(`/api/launcher/profile?model=${encodeURIComponent(m.name)}`)
        if (res.ok) {
          const p = (await res.json()) as Partial<{
            n_ctx: number; gpu_layers: string; port: number; max_tokens: number; extra_args: string[]
          }>
          next = {
            n_ctx: p.n_ctx != null ? String(p.n_ctx) : '',
            gpu_layers: p.gpu_layers ?? 'auto',
            port: p.port != null ? String(p.port) : '',
            max_tokens: p.max_tokens != null ? String(p.max_tokens) : '',
            extra_args: (p.extra_args ?? []).join(' '),
          }
        }
      } catch {
        /* profile is best-effort; fall back to defaults */
      }
      setDrafts((d) => (d[m.path] ? d : { ...d, [m.path]: next }))
    },
    [drafts],
  )

  const saveProfile = async (m: ModelFile) => {
    const d = drafts[m.path]
    if (!d) return
    try {
      await fetch('/api/launcher/profile', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ model: m.name, ...toProfile(d) }),
      })
      setSaved(m.path)
      window.setTimeout(() => setSaved((s) => (s === m.path ? '' : s)), 1500)
    } catch {
      /* best-effort persistence */
    }
  }

  const loadModel = async (m: ModelFile) => {
    if (!selectedBin) return
    setPending(m.path)
    const p = toProfile(draftOf(m.path))
    try {
      await fetch('/api/launcher/launch', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          source: 'self',
          bin: selectedBin,
          model_path: m.path,
          model: m.name,
          n_ctx: p.n_ctx ?? 32768,
          gpu_layers: p.gpu_layers ?? 'auto',
          port: p.port ?? 8080,
          extra_args: p.extra_args ?? [],
          max_tokens: p.max_tokens ?? 0, // 0 = no generation cap
        }),
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
  const canLoad = !!selectedBin

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-4xl px-6 py-8">
        <h2 className="text-[17px] font-semibold tracking-[-0.01em]">{tl('launcher.title')}</h2>
        <p className="mt-1 mb-5 text-[13px] leading-relaxed text-ink-3">{tl('launcher.howto')}</p>

        {apiError && (
          <div className="mb-5 rounded-lg border border-crit/40 bg-crit/10 px-4 py-3 text-[13px] text-ink-2">
            {tl('launcher.error')}
          </div>
        )}

        {/* Local server — one-line live status strip + engine logs when active. */}
        <ServerStrip status={status} onStop={() => void stopEngine()} />
        {status && status.state !== 'stopped' && status.logs.length > 0 && (
          <LogsPanel logs={status.logs} />
        )}

        {/* Models folder + one Scan action + engine binary (compact). */}
        <div className="mb-6 mt-5">
          <div className="flex gap-2">
            <div className="focus-within:border-line-2 flex min-w-0 flex-1 items-center gap-2 rounded-md border border-line bg-surface px-2.5">
              <FolderOpen className="h-[15px] w-[15px] shrink-0 text-ink-mute" strokeWidth={1.8} />
              <input
                value={dir}
                onChange={(e) => setDir(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && applyScan()}
                placeholder={tl('launcher.dir.placeholder')}
                spellCheck={false}
                className="min-w-0 flex-1 bg-transparent py-2 font-mono text-[12.5px] text-ink-2 placeholder:text-ink-mute focus:outline-none"
              />
            </div>
            <button
              type="button"
              onClick={applyScan}
              disabled={busy}
              className="focus-ring flex h-[38px] shrink-0 items-center gap-1.5 rounded-md border border-line px-3 text-[13px] text-ink-2 transition-colors hover:bg-surface-2 disabled:opacity-50"
            >
              <RefreshCw className={cn('h-[14px] w-[14px]', busy && 'animate-spin')} strokeWidth={1.8} />
              {busy ? tl('launcher.scanning') : tl('launcher.rescan')}
            </button>
          </div>
          <div className="mt-2 flex items-center gap-1.5 text-[11.5px] text-ink-mute">
            <Cpu className="h-[13px] w-[13px] shrink-0" strokeWidth={1.8} />
            <span>{tl('launcher.bin.label')}</span>
            {bins.length > 1 ? (
              <select
                value={selectedBin}
                onChange={(e) => setSelectedBin(e.target.value)}
                className="max-w-[18rem] truncate bg-transparent font-mono text-ink-3 focus:outline-none"
              >
                {bins.map((b) => (
                  <option key={b.path} value={b.path} className="bg-bezel">
                    {basename(b.path)}
                  </option>
                ))}
              </select>
            ) : bins.length === 1 ? (
              <span className="truncate font-mono text-ink-3">
                {basename(bins[0].path)}
                {bins[0].on_path ? ` · ${tl('launcher.bin.onPath')}` : ''}
              </span>
            ) : (
              <span className="text-warn">{tl('launcher.bin.none')}</span>
            )}
          </div>
        </div>

        {/* Local models */}
        <div className="mb-2 flex items-baseline justify-between">
          <h3 className="text-[13px] font-semibold text-ink-2">{tl('launcher.models.title')}</h3>
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
            {list.map((m) => {
              const open = expanded === m.path
              return (
                <div
                  key={m.path}
                  className="overflow-hidden rounded-lg border border-line bg-surface transition-colors hover:border-line-2"
                >
                  <div className="flex items-center gap-3 px-3 py-2.5">
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
                      onClick={() => void toggleSettings(m)}
                      aria-label={tl('launcher.settings')}
                      title={tl('launcher.settings')}
                      aria-expanded={open}
                      className={cn(
                        'focus-ring grid h-8 w-8 shrink-0 place-items-center rounded-md border transition-colors',
                        open
                          ? 'border-accent/45 bg-accent/10 text-accent-ink'
                          : 'border-line text-ink-3 hover:bg-surface-2 hover:text-ink',
                      )}
                    >
                      <SlidersHorizontal className="h-[15px] w-[15px]" strokeWidth={1.9} />
                    </button>
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
                  {open && (
                    <ModelSettings
                      draft={draftOf(m.path)}
                      saved={saved === m.path}
                      onField={(k, v) => setField(m.path, k, v)}
                      onSave={() => void saveProfile(m)}
                    />
                  )}
                </div>
              )
            })}
          </div>
        )}

        {/* Running engines detected on common ports */}
        {running.length > 0 && (
          <div className="mt-8">
            <h3 className="mb-2 text-[13px] font-semibold text-ink-2">{tl('launcher.running.title')}</h3>
            <div className="overflow-hidden rounded-lg border border-line">
              {running.map((e) => (
                <div
                  key={e.base_url}
                  className="flex items-center gap-3 border-b border-line px-4 py-2.5 last:border-b-0"
                >
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
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

/** One-line live status strip for the engine started from here. */
function ServerStrip({ status, onStop }: { status: EngineStatus | null; onStop: () => void }) {
  const state = status?.state ?? 'stopped'
  const active = state === 'loading' || state === 'running'
  const tone =
    state === 'running'
      ? 'border-ok/30 bg-ok/5'
      : state === 'failed'
        ? 'border-crit/30 bg-crit/5'
        : 'border-line bg-surface'
  const dot =
    state === 'running'
      ? 'text-ok'
      : state === 'loading'
        ? 'text-warn'
        : state === 'failed'
          ? 'text-crit'
          : 'text-ink-mute'

  return (
    <div className={cn('flex items-center gap-2.5 rounded-lg border px-3.5 py-2.5 text-[12.5px]', tone)}>
      <span className={cn('h-1.5 w-1.5 shrink-0 rounded-full bg-current', dot, state === 'loading' && 'animate-pulse')} />
      <span className="shrink-0 font-medium text-ink-2">{tl('launcher.engine.title')}</span>
      {active && status ? (
        <>
          {status.model && <span className="truncate font-mono text-ink">{status.model}</span>}
          {status.base_url && (
            <span className="shrink-0 rounded bg-surface-2 px-1.5 py-0.5 font-mono text-accent-ink">
              {status.base_url}
            </span>
          )}
          <button
            type="button"
            onClick={onStop}
            className="focus-ring ml-auto flex shrink-0 items-center gap-1.5 rounded-md border border-line px-2.5 py-1 text-[12px] text-ink-2 transition-colors hover:bg-surface-2"
          >
            <Square className="h-[11px] w-[11px]" strokeWidth={2} />
            {tl('launcher.stop')}
          </button>
        </>
      ) : (
        <span className="truncate text-ink-mute">
          {state === 'failed' && status?.error ? status.error : tl('launcher.engine.idle')}
        </span>
      )}
    </div>
  )
}

/** Engine stderr logs (llama.cpp loading progress / errors). Auto-scrolls to newest. */
function LogsPanel({ logs }: { logs: string[] }) {
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (ref.current) ref.current.scrollTop = ref.current.scrollHeight
  }, [logs])
  return (
    <div className="mt-2 overflow-hidden rounded-lg border border-line">
      <div className="border-b border-line px-3 py-1.5 text-[11px] font-medium text-ink-3">
        {tl('launcher.logs.title')}
      </div>
      <div
        ref={ref}
        className="max-h-44 overflow-y-auto px-3 py-2 font-mono text-[11.5px] leading-relaxed text-ink-3"
      >
        {logs.map((l, i) => (
          <div key={i} className="break-all whitespace-pre-wrap">
            {l}
          </div>
        ))}
      </div>
    </div>
  )
}

/** Shared input class for the per-model settings fields. */
const INP =
  'focus-ring w-full rounded border border-line bg-surface px-2 py-1 font-mono text-[12px] text-ink-2 placeholder:text-ink-mute focus:outline-none'

/** Per-model tunables panel: launch flags (ctx/gpu/port/extra) + generation cap (max_tokens).
 *  Edits persist to the model's backend profile on blur; max_tokens also drives the live
 *  agent cap when this model is launched. */
function ModelSettings({
  draft,
  saved,
  onField,
  onSave,
}: {
  draft: Draft
  saved: boolean
  onField: (k: keyof Draft, v: string) => void
  onSave: () => void
}) {
  const numOnly = (v: string) => v.replace(/[^0-9]/g, '')
  return (
    <div className="border-t border-line bg-bezel/40 px-3 py-3">
      <div className="mb-2.5 flex items-center gap-1.5 text-[11px] font-medium text-ink-3">
        <SlidersHorizontal className="h-3 w-3" strokeWidth={2} />
        {tl('launcher.settings')}
        {saved && <span className="ml-1 text-ok">{tl('launcher.saved')}</span>}
      </div>
      <div className="grid grid-cols-2 gap-x-4 gap-y-2.5 sm:grid-cols-4">
        <FieldLabel label={tl('launcher.opt.ctx')}>
          <input
            value={draft.n_ctx}
            onChange={(e) => onField('n_ctx', numOnly(e.target.value))}
            onBlur={onSave}
            placeholder="32768"
            inputMode="numeric"
            className={INP}
          />
        </FieldLabel>
        <FieldLabel label={tl('launcher.opt.gpu')}>
          <select
            value={draft.gpu_layers}
            onChange={(e) => onField('gpu_layers', e.target.value)}
            onBlur={onSave}
            className={cn(INP, 'cursor-pointer')}
          >
            <option value="auto" className="bg-bezel">
              auto
            </option>
            <option value="max" className="bg-bezel">
              max
            </option>
          </select>
        </FieldLabel>
        <FieldLabel label={tl('launcher.opt.port')}>
          <input
            value={draft.port}
            onChange={(e) => onField('port', numOnly(e.target.value))}
            onBlur={onSave}
            placeholder="8080"
            inputMode="numeric"
            className={INP}
          />
        </FieldLabel>
        <FieldLabel label={tl('launcher.opt.maxtok')} hint={tl('launcher.opt.hint.unlimited')}>
          <input
            value={draft.max_tokens}
            onChange={(e) => onField('max_tokens', numOnly(e.target.value))}
            onBlur={onSave}
            placeholder="8192"
            inputMode="numeric"
            className={INP}
          />
        </FieldLabel>
        <FieldLabel label={tl('launcher.opt.extra')} className="col-span-2 sm:col-span-4">
          <input
            value={draft.extra_args}
            onChange={(e) => onField('extra_args', e.target.value)}
            onBlur={onSave}
            placeholder={tl('launcher.opt.extra.ph')}
            spellCheck={false}
            className={INP}
          />
        </FieldLabel>
      </div>
    </div>
  )
}

function FieldLabel({
  label,
  hint,
  className,
  children,
}: {
  label: string
  hint?: string
  className?: string
  children: React.ReactNode
}) {
  return (
    <label className={cn('flex flex-col gap-1', className)}>
      <span className="flex items-center gap-1 text-[11px] text-ink-mute">
        {label}
        {hint && <span className="opacity-70">· {hint}</span>}
      </span>
      {children}
    </label>
  )
}

function Pill({ children, tone }: { children: React.ReactNode; tone: 'plain' | 'accent' }) {
  return (
    <span
      className={cn(
        'shrink-0 rounded px-1.5 py-0.5 font-mono text-[11px]',
        tone === 'accent' ? 'border border-accent/25 text-accent-ink' : 'bg-surface-2 text-ink-3',
      )}
    >
      {children}
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

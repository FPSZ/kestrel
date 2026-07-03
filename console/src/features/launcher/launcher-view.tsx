import { useCallback, useEffect, useState } from 'react'
import { Check, Copy, RefreshCw } from 'lucide-react'
import { cn } from '@/lib/cn'
import { tl } from './strings'

// Wire types mirror kestrel-runtime::discover (language-neutral: paths / URLs /
// enum codes / numbers — no sentences). Kept local; not in the in-flight types.ts.
type BinaryCandidate = { path: string; on_path: boolean }
type RunningEngine = {
  base_url: string
  kind: string
  n_ctx: number | null
  model: string | null
}
type ScanResult = { binaries: BinaryCandidate[]; running: RunningEngine[] }

/**
 * Model launcher (ADR-0010). Discovers llama-server binaries + already-running
 * local engines and generates the loadout `[model]` block to enable one.
 * Discovery only suggests: nothing is spawned from here — the user saves the
 * snippet to a loadout (config-as-authorization, ADR-0010 §5).
 */
export function LauncherView() {
  const [scan, setScan] = useState<ScanResult | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState(false)
  const [snippet, setSnippet] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  const rescan = useCallback(async () => {
    setLoading(true)
    setError(false)
    try {
      const res = await fetch('/api/launcher/scan')
      if (!res.ok) throw new Error('scan unavailable')
      setScan((await res.json()) as ScanResult)
    } catch {
      setError(true)
      setScan(null)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void rescan()
  }, [rescan])

  const pick = (s: string) => {
    setSnippet(s)
    setCopied(false)
  }

  const copy = async () => {
    if (!snippet) return
    try {
      await navigator.clipboard.writeText(snippet)
      setCopied(true)
    } catch {
      /* clipboard blocked - the snippet is still selectable in the pre block */
    }
  }

  const running = scan?.running ?? []
  const binaries = scan?.binaries ?? []

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-2xl px-6 py-8">
        <div className="mb-6 flex items-start justify-between gap-4">
          <div>
            <h2 className="mb-1 text-[16px] font-semibold tracking-[-0.01em]">{tl('launcher.title')}</h2>
            <p className="max-w-prose text-[13px] leading-relaxed text-ink-3">{tl('launcher.subtitle')}</p>
          </div>
          <button
            type="button"
            onClick={() => void rescan()}
            disabled={loading}
            className="focus-ring flex h-8 shrink-0 items-center gap-1.5 rounded-md border border-line px-2.5 text-[13px] text-ink-2 transition-colors hover:bg-surface-2 disabled:opacity-50"
          >
            <RefreshCw className={cn('h-[15px] w-[15px]', loading && 'animate-spin')} strokeWidth={1.8} />
            {loading ? tl('launcher.scanning') : tl('launcher.rescan')}
          </button>
        </div>

        {error && (
          <div className="mb-6 rounded-lg border border-crit/40 bg-crit/10 px-4 py-3 text-[13px] text-ink-2">
            {tl('launcher.error')}
          </div>
        )}

        <Section title={tl('launcher.running.title')}>
          {running.length === 0 ? (
            <Empty text={tl('launcher.running.empty')} />
          ) : (
            running.map((e) => (
              <Row key={e.base_url} onUse={() => pick(runningSnippet(e))}>
                <span className="rounded bg-surface-2 px-1.5 py-0.5 font-mono text-[11px] text-accent-ink">
                  {e.kind}
                </span>
                <span className="truncate font-mono text-[13px] text-ink-2">{e.base_url}</span>
                <span className="ml-auto shrink-0 truncate text-[12px] text-ink-mute">
                  {e.model ?? ''}
                  {e.n_ctx ? ` ${tl('launcher.ctx', { n: e.n_ctx })}` : ''}
                </span>
              </Row>
            ))
          )}
        </Section>

        <Section title={tl('launcher.bin.title')}>
          {binaries.length === 0 ? (
            <Empty text={tl('launcher.bin.empty')} />
          ) : (
            binaries.map((c) => (
              <Row key={c.path} onUse={() => pick(binarySnippet(c))}>
                <span className="truncate font-mono text-[13px] text-ink-2">{c.path}</span>
                {c.on_path && (
                  <span className="shrink-0 rounded bg-surface-2 px-1.5 py-0.5 text-[11px] text-ink-3">
                    {tl('launcher.badge.onPath')}
                  </span>
                )}
              </Row>
            ))
          )}
        </Section>

        <div className="mt-8">
          <div className="mb-2 flex items-center justify-between">
            <h3 className="text-[13px] font-semibold text-ink-2">{tl('launcher.snippet.title')}</h3>
            {snippet && (
              <button
                type="button"
                onClick={() => void copy()}
                className="focus-ring flex h-7 items-center gap-1.5 rounded-md bg-accent px-2.5 text-[12px] font-medium text-desktop transition-colors hover:bg-accent-2"
              >
                {copied ? <Check className="h-[13px] w-[13px]" strokeWidth={2.2} /> : <Copy className="h-[13px] w-[13px]" strokeWidth={1.8} />}
                {copied ? tl('launcher.copied') : tl('launcher.copy')}
              </button>
            )}
          </div>
          <p className="mb-2 text-[12px] text-ink-mute">{tl('launcher.snippet.hint')}</p>
          {snippet ? (
            <pre className="overflow-x-auto rounded-lg border border-line bg-surface px-4 py-3 font-mono text-[12.5px] leading-relaxed text-ink-2">
              {snippet}
            </pre>
          ) : (
            <div className="rounded-lg border border-dashed border-line px-4 py-6 text-center text-[12.5px] text-ink-mute">
              {tl('launcher.snippet.empty')}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="mb-6">
      <h3 className="mb-2 text-[13px] font-semibold text-ink-2">{title}</h3>
      <div className="overflow-hidden rounded-lg border border-line">{children}</div>
    </div>
  )
}

function Row({ children, onUse }: { children: React.ReactNode; onUse: () => void }) {
  return (
    <div className="flex items-center gap-3 border-b border-line px-4 py-2.5 last:border-b-0">
      {children}
      <button
        type="button"
        onClick={onUse}
        className="focus-ring ml-auto shrink-0 rounded-md border border-line px-2.5 py-1 text-[12px] text-ink-2 transition-colors hover:bg-surface-2"
      >
        {tl('launcher.use')}
      </button>
    </div>
  )
}

function Empty({ text }: { text: string }) {
  return <div className="px-4 py-4 text-[12.5px] text-ink-mute">{text}</div>
}

/** Running engine -> connect/delegate loadout. Ollama is openai-compat via delegate. */
function runningSnippet(e: RunningEngine): string {
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

/** Binary -> self-launch loadout. Paths as TOML literal strings (single quotes)
 *  so Windows backslashes need no escaping. model_path left as a placeholder. */
function binarySnippet(c: BinaryCandidate): string {
  return [
    '[model]',
    'source = "self"',
    `bin = '${c.path}'`,
    "model_path = '<PATH_TO_YOUR_GGUF>'",
    'port = 8080',
    'n_ctx = 32768',
    'gpu_layers = "auto"',
    'kind = "llamacpp"',
  ].join('\n')
}

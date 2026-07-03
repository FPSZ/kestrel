import { useState } from 'react'
import { Terminal, Check, X, AlertTriangle, Loader, ChevronRight } from 'lucide-react'
import { client } from '@/lib/client'
import type { Block, ToolBlock } from '@/lib/store'
import { t } from '@/i18n'
import { Markdown } from './markdown'

/**
 * Presentational conversation renderer, shared by the live chat and the
 * read-only session replay. `interactive` gates the inline approval buttons;
 * `turnActive` lets the last block show the streaming caret / live "Thinking...".
 */
export function Conversation({
  blocks,
  interactive = true,
  turnActive = false,
}: {
  blocks: Block[]
  interactive?: boolean
  turnActive?: boolean
}) {
  return (
    <>
      {blocks.map((b, i) => (
        <BlockView
          key={b.seq}
          block={b}
          interactive={interactive}
          streaming={turnActive && i === blocks.length - 1}
        />
      ))}
    </>
  )
}

function BlockView({
  block,
  interactive,
  streaming,
}: {
  block: Block
  interactive: boolean
  streaming: boolean
}) {
  switch (block.kind) {
    case 'user': {
      const imgs = block.images ?? []
      return (
        <div className="max-w-[85%] self-end rounded-2xl rounded-br-md bg-surface-2 px-3.5 py-2 text-[16px] text-ink">
          {imgs.length > 0 && (
            <div className={`flex flex-wrap gap-1.5 ${block.text ? 'mb-1.5' : ''}`}>
              {imgs.map((src, i) => (
                <img
                  key={i}
                  src={src}
                  alt=""
                  className="max-h-56 max-w-full rounded-lg border border-line object-contain"
                />
              ))}
            </div>
          )}
          {block.text && <div className="whitespace-pre-wrap break-words">{block.text}</div>}
        </div>
      )
    }
    case 'assistant':
      return <AssistantBlock block={block} streaming={streaming} />
    case 'tool':
      return <ToolCard block={block} interactive={interactive} />
    case 'error':
      return (
        <div className="flex items-start gap-2 rounded-lg border border-crit/30 bg-crit/10 px-3 py-2 text-[13px] text-crit">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" strokeWidth={1.9} />
          <div className="min-w-0">
            {/* localized category (from stable ErrorCode) + dev-facing raw detail */}
            <div className="font-medium">{t(`error.${block.code ?? 'internal'}`)}</div>
            {block.message && (
              <div className="mt-0.5 whitespace-pre-wrap break-words font-mono text-[12px] text-crit/80">
                {block.message}
              </div>
            )}
          </div>
        </div>
      )
  }
}

function AssistantBlock({
  block,
  streaming,
}: {
  block: Extract<Block, { kind: 'assistant' }>
  streaming: boolean
}) {
  return (
    <div className="max-w-full">
      {/* actor row: copper spark + name + timestamp (design-study style) */}
      <div className="mb-2 flex items-center gap-2">
        <span className="h-2 w-2 rounded-full bg-gradient-to-br from-accent-2 to-accent-deep shadow-[0_0_10px_var(--color-accent)]" />
        <span className="text-[13.5px] font-medium tracking-wide text-ink-3">Kestrel</span>
        {block.ts != null && (
          <span className="ml-auto font-mono text-[11px] text-ink-mute">{fmtTime(block.ts)}</span>
        )}
      </div>

      {block.reasoning && <Thinking text={block.reasoning} live={streaming && !block.text} />}

      {block.text ? (
        <div className={`[&>*:last-child]:mb-0 ${streaming ? 'stream-caret' : ''}`}>
          <Markdown>{block.text}</Markdown>
        </div>
      ) : null}
    </div>
  )
}

/** Collapsed-by-default reasoning disclosure. Live: shimmering "Thinking...". */
function Thinking({ text, live }: { text: string; live: boolean }) {
  const [open, setOpen] = useState(false)
  return (
    <div className="mb-2.5">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="focus-ring flex items-center gap-1 rounded text-[13.5px] text-ink-mute transition-colors hover:text-ink-3"
      >
        <ChevronRight
          className={`h-4 w-4 transition-transform ${open ? 'rotate-90' : ''}`}
          strokeWidth={2}
        />
        <span className={live ? 'shimmer' : ''}>
          {live ? t('think.streaming') : t('think.label')}
        </span>
      </button>
      {open && (
        <div className="mt-1.5 ml-1.5 whitespace-pre-wrap break-words border-l border-line-2 pl-3 text-[14px] leading-relaxed text-ink-mute">
          {text}
        </div>
      )}
    </div>
  )
}

function ToolCard({ block, interactive }: { block: ToolBlock; interactive: boolean }) {
  const pending = block.status === 'pending_approval'
  return (
    <div
      className={`overflow-hidden font-mono text-[12.5px] ${
        pending ? 'glass-card' : 'rounded-lg border border-line bg-surface/40'
      }`}
    >
      <div className="flex items-center gap-2 border-b border-line px-3 py-1.5">
        <Terminal className="h-[14px] w-[14px] shrink-0 text-ink-3" strokeWidth={1.9} />
        <span className="font-semibold text-ink-2">{block.tool}</span>
        <span className="ml-auto shrink-0">
          <StatusBadge status={block.status} />
        </span>
      </div>

      <ArgsBlock args={block.args} />

      {block.review && pending && (
        <div className="flex gap-3 px-3 pt-2.5 font-sans text-[12.5px] leading-relaxed text-ink-3">
          <span className="w-0.5 shrink-0 rounded bg-gradient-to-b from-accent to-transparent" />
          <span>{block.review}</span>
        </div>
      )}

      {pending && interactive && <Approval block={block} />}

      {block.result != null && (
        <pre className="max-h-64 overflow-auto whitespace-pre-wrap px-3 py-2 text-[12px] leading-relaxed text-ink-3">
          {block.result}
        </pre>
      )}
    </div>
  )
}

function Approval({ block }: { block: ToolBlock }) {
  // optimistic: reflect the click instantly, don't wait for the SSE round-trip
  // to flip the card. the real event unmounts this component when it lands.
  const [sent, setSent] = useState<null | 'approve' | 'deny'>(null)
  const approve = () => {
    setSent('approve')
    void client.sendOp({ type: 'approve', call_id: block.callId })
  }
  const deny = () => {
    setSent('deny')
    void client.sendOp({ type: 'deny', call_id: block.callId, reason: 'user declined' })
  }

  if (sent) {
    return (
      <div className="flex items-center gap-2 px-3 py-2.5 font-sans text-[12.5px] text-ink-3">
        <Loader className="h-3.5 w-3.5 shrink-0 animate-spin" strokeWidth={2} />
        {sent === 'approve' ? t('tool.approving') : t('tool.declining')}
      </div>
    )
  }

  return (
    <div className="flex items-center gap-2 px-3 py-2.5 font-sans text-[12.5px]">
      <AlertTriangle className="h-4 w-4 shrink-0 text-warn" strokeWidth={1.9} />
      <span className="font-medium text-warn">
        {t('tool.approveAction', { risk: block.risk ? t(`risk.${block.risk}`) : '' })}
      </span>
      <div className="ml-auto flex items-center gap-1.5">
        <button
          type="button"
          onClick={approve}
          className="focus-ring flex items-center gap-1 rounded-md bg-accent px-2.5 py-1 font-medium text-desktop transition-colors hover:bg-accent-2"
        >
          <Check className="h-3.5 w-3.5" strokeWidth={2.2} /> {t('tool.approve')}
        </button>
        <button
          type="button"
          onClick={deny}
          className="focus-ring flex items-center gap-1 rounded-md border border-line-2 px-2.5 py-1 font-medium text-ink-2 transition-colors hover:bg-surface"
        >
          <X className="h-3.5 w-3.5" strokeWidth={2.2} /> {t('tool.deny')}
        </button>
      </div>
    </div>
  )
}

function StatusBadge({ status }: { status: ToolBlock['status'] }) {
  switch (status) {
    case 'running':
      return (
        <span className="flex items-center gap-1 text-ink-mute">
          <Loader className="h-3 w-3 animate-spin" strokeWidth={2} /> {t('status.running')}
        </span>
      )
    case 'ok':
      return (
        <span className="flex items-center gap-1 text-ok">
          <Check className="h-3 w-3" strokeWidth={2.4} /> {t('status.ok')}
        </span>
      )
    case 'error':
      return (
        <span className="flex items-center gap-1 text-crit">
          <X className="h-3 w-3" strokeWidth={2.4} /> {t('status.error')}
        </span>
      )
    case 'pending_approval':
      return <span className="text-accent-ink">{t('status.needsApproval')}</span>
  }
}

function fmtTime(ts?: number): string {
  if (ts == null) return ''
  try {
    return new Date(ts).toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    })
  } catch {
    return ''
  }
}

/** Full tool args, collapsed to 3 lines by default, click to expand (pretty). */
function ArgsBlock({ args }: { args: unknown }) {
  const [open, setOpen] = useState(false)
  const compact = stringifyArgs(args, false)
  if (!compact) return null
  return (
    <div
      onClick={() => setOpen((v) => !v)}
      title={open ? t('tool.collapse') : t('tool.expand')}
      className="cursor-pointer border-b border-line px-3 py-1.5 transition-colors hover:bg-surface/40"
    >
      <pre
        className={`whitespace-pre-wrap break-words font-mono text-[12.5px] leading-relaxed text-ink-3 ${
          open ? '' : 'line-clamp-3'
        }`}
      >
        {open ? stringifyArgs(args, true) : compact}
      </pre>
    </div>
  )
}

function stringifyArgs(args: unknown, pretty: boolean): string {
  if (args == null) return ''
  try {
    if (typeof args === 'string') return args
    return JSON.stringify(args, null, pretty ? 2 : 0)
  } catch {
    return ''
  }
}

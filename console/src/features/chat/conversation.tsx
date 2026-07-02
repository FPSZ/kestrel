import { Terminal, Check, X, AlertTriangle, Loader } from 'lucide-react'
import { client } from '@/lib/client'
import type { Block, ToolBlock } from '@/lib/store'

/**
 * Presentational conversation renderer, shared by the live chat and the
 * read-only session replay. `interactive` gates the inline approval buttons
 * (a historical replay must never send Ops into the current session).
 */
export function Conversation({ blocks, interactive = true }: { blocks: Block[]; interactive?: boolean }) {
  return (
    <>
      {blocks.map((b) => (
        <BlockView key={b.seq} block={b} interactive={interactive} />
      ))}
    </>
  )
}

function BlockView({ block, interactive }: { block: Block; interactive: boolean }) {
  switch (block.kind) {
    case 'user':
      return (
        <div className="max-w-[85%] self-end whitespace-pre-wrap rounded-2xl rounded-br-md bg-surface-2 px-3.5 py-2 text-[14px] text-ink">
          {block.text}
        </div>
      )
    case 'assistant':
      return (
        <div className="max-w-full whitespace-pre-wrap text-[14px] leading-relaxed text-ink-2">
          {block.text}
        </div>
      )
    case 'tool':
      return <ToolCard block={block} interactive={interactive} />
    case 'error':
      return (
        <div className="flex items-start gap-2 rounded-lg border border-crit/30 bg-crit/10 px-3 py-2 text-[13px] text-crit">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" strokeWidth={1.9} />
          <span className="whitespace-pre-wrap">{block.message}</span>
        </div>
      )
  }
}

function ToolCard({ block, interactive }: { block: ToolBlock; interactive: boolean }) {
  const args = compactArgs(block.args)
  return (
    <div className="overflow-hidden rounded-lg border border-line bg-surface/40 font-mono text-[12.5px]">
      <div className="flex items-center gap-2 border-b border-line px-3 py-1.5">
        <Terminal className="h-[14px] w-[14px] text-ink-3" strokeWidth={1.9} />
        <span className="font-semibold text-ink-2">{block.tool}</span>
        {args && <span className="truncate text-ink-mute">{args}</span>}
        <span className="ml-auto shrink-0">
          <StatusBadge status={block.status} />
        </span>
      </div>

      {block.status === 'pending_approval' && interactive && <Approval block={block} />}

      {block.result != null && (
        <pre className="max-h-64 overflow-auto whitespace-pre-wrap px-3 py-2 text-[12px] leading-relaxed text-ink-3">
          {block.result}
        </pre>
      )}
    </div>
  )
}

function Approval({ block }: { block: ToolBlock }) {
  const approve = () => void client.sendOp({ type: 'approve', call_id: block.callId })
  const deny = () => void client.sendOp({ type: 'deny', call_id: block.callId, reason: 'user declined' })
  return (
    <div className="flex items-center gap-2 bg-warn/10 px-3 py-2 font-sans text-[12.5px]">
      <AlertTriangle className="h-4 w-4 shrink-0 text-warn" strokeWidth={1.9} />
      <span className="text-ink-2">
        Approve <span className="font-semibold text-warn">{block.risk}</span> action?
      </span>
      <div className="ml-auto flex items-center gap-1.5">
        <button
          type="button"
          onClick={approve}
          className="focus-ring flex items-center gap-1 rounded-md bg-accent px-2.5 py-1 font-medium text-white transition-colors hover:bg-accent-2"
        >
          <Check className="h-3.5 w-3.5" strokeWidth={2.2} /> Approve
        </button>
        <button
          type="button"
          onClick={deny}
          className="focus-ring flex items-center gap-1 rounded-md border border-line-2 px-2.5 py-1 font-medium text-ink-2 transition-colors hover:bg-surface"
        >
          <X className="h-3.5 w-3.5" strokeWidth={2.2} /> Deny
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
          <Loader className="h-3 w-3 animate-spin" strokeWidth={2} /> running
        </span>
      )
    case 'ok':
      return (
        <span className="flex items-center gap-1 text-ok">
          <Check className="h-3 w-3" strokeWidth={2.4} /> ok
        </span>
      )
    case 'error':
      return (
        <span className="flex items-center gap-1 text-crit">
          <X className="h-3 w-3" strokeWidth={2.4} /> error
        </span>
      )
    case 'pending_approval':
      return <span className="text-warn">needs approval</span>
  }
}

function compactArgs(args: unknown): string {
  if (args == null) return ''
  try {
    const s = typeof args === 'string' ? args : JSON.stringify(args)
    return s.length > 140 ? `${s.slice(0, 140)}...` : s
  } catch {
    return ''
  }
}

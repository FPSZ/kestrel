import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { ArrowUp, Terminal, Check, X, AlertTriangle, Loader } from 'lucide-react'
import { client } from '@/lib/client'
import type { Block, ToolBlock } from '@/lib/store'

/**
 * Live conversation view. Presentational: receives folded blocks + turn state,
 * owns only the input box and the outbound Ops (user input, approvals).
 */
export function ChatView({ blocks, turnActive }: { blocks: Block[]; turnActive: boolean }) {
  const [text, setText] = useState('')
  const endRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: 'end' })
  }, [blocks, turnActive])

  const send = () => {
    const t = text.trim()
    if (!t || turnActive) return
    void client.sendOp({ type: 'user_input', text: t })
    setText('')
  }

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      send()
    }
  }

  const empty = blocks.length === 0

  return (
    <div className="flex h-full min-h-0 flex-col">
      {empty ? (
        <EmptyState />
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto flex max-w-3xl flex-col gap-4 px-4 py-6">
            {blocks.map((b) => (
              <BlockView key={b.seq} block={b} />
            ))}
            {turnActive && <Working />}
            <div ref={endRef} />
          </div>
        </div>
      )}

      <div className="shrink-0 p-3">
        <div className="mx-auto flex max-w-3xl items-end gap-2 rounded-xl border border-line bg-surface p-2 pl-3.5 transition-colors focus-within:border-line-2">
          <textarea
            rows={1}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Message Kestrel..."
            className="max-h-40 min-h-[24px] flex-1 resize-none bg-transparent py-1 text-[14px] text-ink placeholder:text-ink-mute focus:outline-none"
          />
          <button
            type="button"
            onClick={send}
            disabled={!text.trim() || turnActive}
            aria-label="Send"
            className="focus-ring grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-accent text-white transition-colors hover:bg-accent-2 disabled:cursor-not-allowed disabled:opacity-40"
          >
            <ArrowUp className="h-[18px] w-[18px]" strokeWidth={2.2} />
          </button>
        </div>
      </div>
    </div>
  )
}

function BlockView({ block }: { block: Block }) {
  switch (block.kind) {
    case 'user':
      return (
        <div className="self-end max-w-[85%] rounded-2xl rounded-br-md bg-surface-2 px-3.5 py-2 text-[14px] text-ink whitespace-pre-wrap">
          {block.text}
        </div>
      )
    case 'assistant':
      return (
        <div className="max-w-full text-[14px] leading-relaxed text-ink-2 whitespace-pre-wrap">
          {block.text}
        </div>
      )
    case 'tool':
      return <ToolCard block={block} />
    case 'error':
      return (
        <div className="flex items-start gap-2 rounded-lg border border-crit/30 bg-crit/10 px-3 py-2 text-[13px] text-crit">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" strokeWidth={1.9} />
          <span className="whitespace-pre-wrap">{block.message}</span>
        </div>
      )
  }
}

function ToolCard({ block }: { block: ToolBlock }) {
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

      {block.status === 'pending_approval' && <Approval block={block} />}

      {block.result != null && (
        <pre className="max-h-64 overflow-auto px-3 py-2 text-[12px] leading-relaxed text-ink-3 whitespace-pre-wrap">
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
      return <span className="flex items-center gap-1 text-ok"><Check className="h-3 w-3" strokeWidth={2.4} /> ok</span>
    case 'error':
      return <span className="flex items-center gap-1 text-crit"><X className="h-3 w-3" strokeWidth={2.4} /> error</span>
    case 'pending_approval':
      return <span className="text-warn">needs approval</span>
  }
}

function Working() {
  return (
    <div className="flex items-center gap-2 text-[13px] text-ink-mute">
      <Loader className="h-3.5 w-3.5 animate-spin" strokeWidth={2} />
      working
    </div>
  )
}

function EmptyState() {
  return (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
      <div className="grid h-12 w-12 place-items-center rounded-xl border border-line bg-surface font-mono text-[18px] font-bold text-accent-ink">
        K
      </div>
      <h2 className="text-[17px] font-semibold tracking-[-0.01em]">Kestrel</h2>
      <p className="max-w-sm text-[13.5px] leading-relaxed text-ink-3">
        Your local-model agent. Ask it to read, search, edit files or run shell
        commands in the working directory.
      </p>
    </div>
  )
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

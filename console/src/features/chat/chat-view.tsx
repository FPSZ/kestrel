import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { ArrowUp, Loader } from 'lucide-react'
import { client } from '@/lib/client'
import type { Block } from '@/lib/store'
import { Conversation } from './conversation'

/**
 * Live conversation view. Receives folded blocks + turn state, owns only the
 * input box and outbound Ops (user input, approvals via the Conversation).
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

  return (
    <div className="flex h-full min-h-0 flex-col">
      {blocks.length === 0 ? (
        <EmptyState />
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto flex max-w-3xl flex-col gap-4 px-4 py-6">
            <Conversation blocks={blocks} />
            {turnActive && (
              <div className="flex items-center gap-2 text-[13px] text-ink-mute">
                <Loader className="h-3.5 w-3.5 animate-spin" strokeWidth={2} />
                working
              </div>
            )}
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

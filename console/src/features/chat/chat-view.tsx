import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { ArrowUp, Loader, Brain, Square } from 'lucide-react'
import { client } from '@/lib/client'
import type { Block } from '@/lib/store'
import { Conversation } from './conversation'

/**
 * Live conversation view. Receives folded blocks + turn state, owns only the
 * input box and outbound Ops (user input, approvals via the Conversation).
 */
export function ChatView({ blocks, turnActive }: { blocks: Block[]; turnActive: boolean }) {
  const [text, setText] = useState('')
  const [think, setThink] = useState(true)
  const scrollRef = useRef<HTMLDivElement>(null)
  const atBottomRef = useRef(true)
  const taRef = useRef<HTMLTextAreaElement>(null)

  // Auto-scroll ONLY when the user is already near the bottom, so a streaming
  // reply doesn't yank the page down while they've scrolled up to read.
  useEffect(() => {
    if (!atBottomRef.current) return
    const el = scrollRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [blocks, turnActive])

  const onScroll = () => {
    const el = scrollRef.current
    if (!el) return
    atBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 64
  }

  // auto-grow the input up to a cap, then it scrolls internally
  useEffect(() => {
    const ta = taRef.current
    if (!ta) return
    ta.style.height = 'auto'
    ta.style.height = `${Math.min(ta.scrollHeight, 224)}px`
  }, [text])

  const send = () => {
    const t = text.trim()
    if (!t || turnActive) return
    void client.sendOp({ type: 'user_input', text: t, think })
    setText('')
    atBottomRef.current = true // our own send always jumps to the bottom
  }

  // Interrupt the running turn: cancels streaming AND kills any in-flight tool
  // subprocess (the op is handled mid-turn by core's select loops).
  const stop = () => {
    void client.sendOp({ type: 'cancel' })
  }

  // While a turn is running and the box is empty, the send button has nothing
  // to send - turn it into a stop button so the user can always bail out.
  const showStop = turnActive && !text.trim()

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
        <div ref={scrollRef} onScroll={onScroll} className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto flex min-w-0 max-w-3xl flex-col gap-4 px-4 py-6">
            <Conversation blocks={blocks} turnActive={turnActive} />
            {turnActive && (
              <div className="flex items-center gap-2 text-[13px] text-ink-mute">
                <Loader className="h-3.5 w-3.5 animate-spin" strokeWidth={2} />
                working
              </div>
            )}
          </div>
        </div>
      )}

      <div className="shrink-0 p-3">
        <div className="mx-auto max-w-3xl rounded-2xl border border-line bg-surface px-3.5 pt-3 pb-2.5 transition-colors focus-within:border-line-2">
          <textarea
            ref={taRef}
            rows={2}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Message Kestrel..."
            className="block max-h-56 min-h-[56px] w-full resize-none bg-transparent text-[16px] leading-relaxed text-ink placeholder:text-ink-mute focus:outline-none"
          />
          {/* control row: thinking toggle on the left, send on the right */}
          <div className="mt-1.5 flex items-center gap-2">
            <button
              type="button"
              onClick={() => setThink((v) => !v)}
              aria-pressed={think}
              title={think ? 'Thinking on - model reasons first (slower)' : 'Thinking off - answer directly'}
              className={`focus-ring flex items-center gap-1.5 rounded-lg border px-2.5 py-1 text-[13px] font-medium transition-colors ${
                think
                  ? 'border-accent/45 bg-accent/10 text-accent-ink'
                  : 'border-line text-ink-mute hover:text-ink-3'
              }`}
            >
              <Brain className="h-4 w-4" strokeWidth={2} />
              Thinking
            </button>
            {showStop ? (
              <button
                type="button"
                onClick={stop}
                aria-label="Stop"
                title="Stop - interrupt the current turn (kills any running command)"
                className="focus-ring ml-auto grid h-9 w-9 shrink-0 place-items-center rounded-lg border border-crit/45 bg-crit/15 text-crit transition-colors hover:bg-crit/25"
              >
                <Square className="h-[14px] w-[14px]" strokeWidth={2.4} fill="currentColor" />
              </button>
            ) : (
              <button
                type="button"
                onClick={send}
                disabled={!text.trim() || turnActive}
                aria-label="Send"
                className="focus-ring ml-auto grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-accent text-desktop transition-colors hover:bg-accent-2 disabled:cursor-not-allowed disabled:opacity-40"
              >
                <ArrowUp className="h-[18px] w-[18px]" strokeWidth={2.2} />
              </button>
            )}
          </div>
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

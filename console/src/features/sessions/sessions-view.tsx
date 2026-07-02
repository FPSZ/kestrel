import { useEffect, useState } from 'react'
import { History, ChevronRight } from 'lucide-react'
import { client } from '@/lib/client'
import { fold, initialState, type Block } from '@/lib/store'
import { cn } from '@/lib/cn'
import { Conversation } from '../chat/conversation'

/**
 * Read-only session replay. Lists persisted sessions and folds a selected
 * session's event log back into the same conversation view (non-interactive:
 * a historical replay must never send Ops into the current session).
 */
export function SessionsView() {
  const [sessions, setSessions] = useState<string[] | null>(null)
  const [selected, setSelected] = useState<string | null>(null)
  const [blocks, setBlocks] = useState<Block[]>([])
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    client.listSessions().then(setSessions).catch(() => setSessions([]))
  }, [])

  const open = (id: string) => {
    setSelected(id)
    setLoading(true)
    setBlocks([])
    client
      .sessionEvents(id)
      .then((events) => setBlocks(events.reduce(fold, initialState).blocks))
      .catch(() => setBlocks([]))
      .finally(() => setLoading(false))
  }

  return (
    <div className="flex h-full min-h-0">
      <aside className="flex w-64 shrink-0 flex-col border-r border-line">
        <div className="flex items-center gap-2 px-3 py-2.5 text-[11px] font-semibold uppercase tracking-wide text-ink-mute">
          <History className="h-3.5 w-3.5" strokeWidth={2} /> Sessions
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
          {sessions === null ? (
            <p className="px-2 py-2 text-[13px] text-ink-mute">Loading...</p>
          ) : sessions.length === 0 ? (
            <p className="px-2 py-2 text-[13px] text-ink-mute">No sessions yet.</p>
          ) : (
            sessions.map((id) => (
              <button
                key={id}
                type="button"
                onClick={() => open(id)}
                className={cn(
                  'flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-[13px] transition-colors',
                  selected === id ? 'bg-surface-2 text-ink' : 'text-ink-3 hover:bg-surface hover:text-ink-2',
                )}
              >
                <ChevronRight className="h-3.5 w-3.5 shrink-0 text-ink-mute" strokeWidth={2} />
                <span className="truncate font-mono">{id}</span>
              </button>
            ))
          )}
        </div>
      </aside>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {selected === null ? (
          <Center text="Select a session to replay." />
        ) : loading ? (
          <Center text="Loading session..." />
        ) : blocks.length === 0 ? (
          <Center text="This session has no events." />
        ) : (
          <div className="mx-auto flex max-w-3xl flex-col gap-4 px-4 py-6">
            <Conversation blocks={blocks} interactive={false} />
          </div>
        )}
      </div>
    </div>
  )
}

function Center({ text }: { text: string }) {
  return <div className="grid h-full place-items-center px-6 text-center text-[13px] text-ink-mute">{text}</div>
}

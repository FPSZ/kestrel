import { useEffect, useState } from 'react'
import { History } from 'lucide-react'
import { client } from '@/lib/client'
import { fold, initialState, type Block } from '@/lib/store'
import { t } from '@/i18n'
import { Conversation } from '../chat/conversation'

/**
 * Read-only replay of a single persisted session, shown in the main pane when a
 * past conversation is picked from the sidebar's Chat dropdown. Non-interactive:
 * a historical replay must never send Ops into the current session.
 *
 * (The session *list* now lives in the sidebar; this component only renders one.)
 */
export function SessionReplay({ id }: { id: string }) {
  const [blocks, setBlocks] = useState<Block[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    let live = true
    setLoading(true)
    setBlocks([])
    client
      .sessionEvents(id)
      .then((events) => {
        if (live) setBlocks(events.reduce(fold, initialState).blocks)
      })
      .catch(() => {
        if (live) setBlocks([])
      })
      .finally(() => {
        if (live) setLoading(false)
      })
    return () => {
      live = false
    }
  }, [id])

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-line px-4 py-2 text-[12px] text-ink-mute">
        <History className="h-3.5 w-3.5 shrink-0" strokeWidth={2} />
        <span className="truncate font-mono text-ink-3">{id}</span>
        <span className="ml-auto shrink-0 rounded bg-surface-2 px-1.5 py-0.5">{t('session.readonly')}</span>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {loading ? (
          <Center text={t('session.loading')} />
        ) : blocks.length === 0 ? (
          <Center text={t('session.empty')} />
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

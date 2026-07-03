import { useEffect, useState } from 'react'
import { MessageSquare, Cpu, Settings, ChevronRight, type LucideIcon } from 'lucide-react'
import { client } from '@/lib/client'
import { cn } from '@/lib/cn'
import { t } from '@/i18n'

/**
 * Left nav. Transparent - part of the shared frosted shell. Active item is
 * marked by a subtle surface fill plus a short accent bar, not a heavy pill.
 *
 * Chat + Sessions are merged: the Chat row carries an expand chevron that drops
 * down the conversation list (current session first, marked "live"; the rest are
 * read-only replays). Picking one drives the main pane via `onOpenSession`.
 */
export function Sidebar({
  collapsed,
  active,
  openedSession,
  currentSession,
  onNavigate,
  onOpenSession,
}: {
  collapsed: boolean
  active: string
  openedSession: string | null
  currentSession?: string
  onNavigate: (id: string) => void
  onOpenSession: (id: string | null) => void
}) {
  const [expanded, setExpanded] = useState(true)
  const [sessions, setSessions] = useState<string[]>([])

  const refresh = () => {
    client
      .listSessions()
      .then(setSessions)
      .catch(() => setSessions([]))
  }
  // fetch once on mount (inlined so the effect has no external deps)
  useEffect(() => {
    client
      .listSessions()
      .then(setSessions)
      .catch(() => setSessions([]))
  }, [])

  const toggleExpand = () => {
    setExpanded((v) => {
      if (!v) refresh() // opening: pick up any newly persisted conversations
      return !v
    })
  }

  const chatActive = active === 'chat'
  // current session id also appears in the persisted list - dedup + pin it first.
  const others = sessions.filter((s) => s !== currentSession)
  const ordered = currentSession ? [currentSession, ...others] : others

  return (
    <nav
      className={cn(
        'flex shrink-0 flex-col gap-1 p-2 transition-[width] duration-200 ease-out',
        collapsed ? 'w-14' : 'w-56',
      )}
    >
      {/* Chat row: label opens the live conversation, chevron toggles the list */}
      <div className="flex items-stretch gap-0.5">
        <NavButton
          icon={MessageSquare}
          label={t('nav.chat')}
          active={chatActive && openedSession === null}
          showBar={chatActive}
          collapsed={collapsed}
          grow
          onClick={() => {
            onNavigate('chat')
            onOpenSession(null)
          }}
        />
        {!collapsed && (
          <button
            type="button"
            onClick={toggleExpand}
            aria-label={t('nav.conversations')}
            aria-expanded={expanded}
            className="focus-ring grid w-7 shrink-0 place-items-center rounded-md text-ink-mute transition-colors hover:bg-surface hover:text-ink-2"
          >
            <ChevronRight
              className={cn('h-4 w-4 transition-transform', expanded && 'rotate-90')}
              strokeWidth={2}
            />
          </button>
        )}
      </div>

      {/* conversation dropdown (indented under Chat) */}
      {!collapsed && expanded && (
        <div className="mb-1 ml-3.5 flex flex-col gap-0.5 border-l border-line pl-2">
          {ordered.length === 0 ? (
            <p className="px-2 py-1 text-[12px] text-ink-mute">{t('nav.noSessions')}</p>
          ) : (
            ordered.map((id) => {
              const isCurrent = id === currentSession
              const selected = chatActive && (isCurrent ? openedSession === null : openedSession === id)
              return (
                <button
                  key={id}
                  type="button"
                  onClick={() => onOpenSession(isCurrent ? null : id)}
                  className={cn(
                    'focus-ring flex items-center gap-1.5 rounded-md px-2 py-1 text-left text-[12px] transition-colors',
                    selected ? 'bg-surface-2 text-ink' : 'text-ink-3 hover:bg-surface hover:text-ink-2',
                  )}
                >
                  <span className="truncate font-mono">{id}</span>
                  {isCurrent && (
                    <span className="ml-auto shrink-0 rounded bg-accent/15 px-1 text-[10px] font-medium text-accent-ink">
                      {t('nav.current')}
                    </span>
                  )}
                </button>
              )
            })
          )}
        </div>
      )}

      <NavButton
        icon={Cpu}
        label={t('nav.models')}
        active={active === 'models'}
        showBar={active === 'models'}
        collapsed={collapsed}
        onClick={() => onNavigate('models')}
      />
      <NavButton
        icon={Settings}
        label={t('nav.settings')}
        active={active === 'settings'}
        showBar={active === 'settings'}
        collapsed={collapsed}
        onClick={() => onNavigate('settings')}
      />

      <div className="mt-auto flex items-center gap-2.5 rounded-md p-2 text-ink-3">
        <span className="grid h-7 w-7 shrink-0 place-items-center rounded-full bg-surface-2 font-mono text-[12px]">
          U
        </span>
        {!collapsed && <span className="truncate text-[13px]">{t('nav.localMachine')}</span>}
      </div>
    </nav>
  )
}

function NavButton({
  icon: Icon,
  label,
  active,
  showBar,
  collapsed,
  grow = false,
  onClick,
}: {
  icon: LucideIcon
  label: string
  active: boolean
  showBar: boolean
  collapsed: boolean
  grow?: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={collapsed ? label : undefined}
      className={cn(
        'focus-ring relative flex h-9 items-center gap-2.5 rounded-md px-2.5 text-[13.5px] transition-colors',
        grow && 'flex-1',
        active ? 'bg-surface-2 text-ink' : 'text-ink-3 hover:bg-surface hover:text-ink-2',
      )}
    >
      {showBar && (
        <span className="absolute left-0 top-1/2 h-4 w-[2.5px] -translate-y-1/2 rounded-full bg-accent" />
      )}
      <Icon className="h-[17px] w-[17px] shrink-0" strokeWidth={1.8} />
      {!collapsed && <span className="truncate font-medium">{label}</span>}
    </button>
  )
}

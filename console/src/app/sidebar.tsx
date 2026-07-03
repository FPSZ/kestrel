import { useEffect, useState, type ReactNode } from 'react'
import {
  MessageSquare,
  Cpu,
  Settings,
  ChevronRight,
  Plus,
  Archive,
  Trash2,
  type LucideIcon,
} from 'lucide-react'
import { client } from '@/lib/client'
import { cn } from '@/lib/cn'
import { t } from '@/i18n'

const ARCHIVE_KEY = 'kestrel.archivedSessions'
function loadArchived(): string[] {
  try {
    const v = localStorage.getItem(ARCHIVE_KEY)
    return v ? (JSON.parse(v) as string[]) : []
  } catch {
    return []
  }
}
function saveArchived(ids: string[]) {
  try {
    localStorage.setItem(ARCHIVE_KEY, JSON.stringify(ids))
  } catch {
    /* non-fatal */
  }
}

/**
 * Left nav. Transparent - part of the shared frosted shell.
 *
 * Chat + Sessions are merged: the Chat row is one pill holding the label, a "+"
 * (new conversation) and an expand chevron; expanding drops down the conversation
 * list (current session pinned + marked "live"; the rest are read-only replays,
 * each with archive [hide] and delete [remove file, two-click] actions).
 */
export function Sidebar({
  collapsed,
  active,
  openedSession,
  currentSession,
  onNavigate,
  onOpenSession,
  onNewConversation,
}: {
  collapsed: boolean
  active: string
  openedSession: string | null
  currentSession?: string
  onNavigate: (id: string) => void
  onOpenSession: (id: string | null) => void
  onNewConversation: () => Promise<void> | void
}) {
  const [expanded, setExpanded] = useState(true)
  const [sessions, setSessions] = useState<string[]>([])
  const [archived, setArchived] = useState<string[]>(loadArchived)
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null)

  const refresh = () => {
    client
      .listSessions()
      .then(setSessions)
      .catch(() => setSessions([]))
  }
  useEffect(() => {
    client
      .listSessions()
      .then(setSessions)
      .catch(() => setSessions([]))
  }, [])

  const toggleExpand = () => {
    setExpanded((v) => {
      if (!v) refresh()
      return !v
    })
  }

  const newConversation = async () => {
    setConfirmDelete(null)
    await onNewConversation()
    refresh()
  }

  const archive = (id: string) => {
    const next = Array.from(new Set([...archived, id]))
    setArchived(next)
    saveArchived(next)
    if (openedSession === id) onOpenSession(null)
  }

  const clickDelete = (id: string) => {
    if (confirmDelete === id) {
      // second click - actually delete the file
      void client
        .deleteSession(id)
        .catch(() => {
          /* 409 (active) / 404 - ignore, refresh reflects truth */
        })
        .finally(() => {
          setConfirmDelete(null)
          if (openedSession === id) onOpenSession(null)
          refresh()
        })
    } else {
      setConfirmDelete(id) // first click - arm confirmation
    }
  }

  const chatActive = active === 'chat'
  const others = sessions.filter((s) => s !== currentSession && !archived.includes(s))
  const ordered = currentSession ? [currentSession, ...others] : others

  return (
    <nav
      className={cn(
        'flex shrink-0 flex-col gap-1 p-2 transition-[width] duration-200 ease-out',
        collapsed ? 'w-14' : 'w-56',
      )}
    >
      {/* Chat pill: label + new + expand chevron, all in one container */}
      <div
        className={cn(
          'relative flex h-9 items-center rounded-md transition-colors',
          chatActive ? 'bg-surface-2' : 'hover:bg-surface',
        )}
      >
        {chatActive && (
          <span className="absolute left-0 top-1/2 h-4 w-[2.5px] -translate-y-1/2 rounded-full bg-accent" />
        )}
        <button
          type="button"
          onClick={() => {
            onNavigate('chat')
            onOpenSession(null)
          }}
          title={collapsed ? t('nav.chat') : undefined}
          className={cn(
            'focus-ring flex h-full flex-1 items-center gap-2.5 rounded-md px-2.5 text-[13.5px] font-medium transition-colors',
            chatActive && openedSession === null ? 'text-ink' : 'text-ink-3 hover:text-ink-2',
          )}
        >
          <MessageSquare className="h-[17px] w-[17px] shrink-0" strokeWidth={1.8} />
          {!collapsed && <span className="truncate">{t('nav.chat')}</span>}
        </button>
        {!collapsed && (
          <div className="flex shrink-0 items-center gap-0.5 pr-1">
            <MiniButton title={t('nav.new')} onClick={newConversation}>
              <Plus className="h-4 w-4" strokeWidth={2} />
            </MiniButton>
            <MiniButton title={t('nav.conversations')} onClick={toggleExpand} expanded={expanded}>
              <ChevronRight
                className={cn('h-4 w-4 transition-transform', expanded && 'rotate-90')}
                strokeWidth={2}
              />
            </MiniButton>
          </div>
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
              const confirming = confirmDelete === id
              return (
                <div
                  key={id}
                  className={cn(
                    // fixed height so the hover actions never grow / jump the row
                    'group flex h-8 items-center gap-1 rounded-md px-2 text-[12px] transition-colors',
                    selected ? 'bg-surface-2 text-ink' : 'text-ink-3 hover:bg-surface hover:text-ink-2',
                  )}
                >
                  <button
                    type="button"
                    onClick={() => onOpenSession(isCurrent ? null : id)}
                    className="focus-ring flex min-w-0 flex-1 items-center gap-1.5 text-left"
                  >
                    <span className="truncate font-mono">{id}</span>
                    {isCurrent && (
                      <span className="shrink-0 rounded bg-accent/15 px-1 text-[10px] font-medium text-accent-ink">
                        {t('nav.current')}
                      </span>
                    )}
                  </button>

                  {!isCurrent &&
                    (confirming ? (
                      <button
                        type="button"
                        onClick={() => clickDelete(id)}
                        className="focus-ring flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-[11px] font-medium text-crit transition-colors hover:bg-crit/15"
                      >
                        <Trash2 className="h-3 w-3" strokeWidth={2} />
                        {t('nav.confirmDelete')}
                      </button>
                    ) : (
                      <div className="hidden shrink-0 items-center gap-0.5 group-hover:flex">
                        <RowAction title={t('nav.archive')} onClick={() => archive(id)}>
                          <Archive className="h-3.5 w-3.5" strokeWidth={1.9} />
                        </RowAction>
                        <RowAction title={t('nav.delete')} onClick={() => clickDelete(id)} danger>
                          <Trash2 className="h-3.5 w-3.5" strokeWidth={1.9} />
                        </RowAction>
                      </div>
                    ))}
                </div>
              )
            })
          )}
        </div>
      )}

      <NavButton
        icon={Cpu}
        label={t('nav.models')}
        active={active === 'models'}
        collapsed={collapsed}
        onClick={() => onNavigate('models')}
      />
      <NavButton
        icon={Settings}
        label={t('nav.settings')}
        active={active === 'settings'}
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

/** Small icon button living inside the Chat pill (new / expand). */
function MiniButton({
  title,
  onClick,
  expanded,
  children,
}: {
  title: string
  onClick: () => void
  expanded?: boolean
  children: ReactNode
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      aria-label={title}
      aria-expanded={expanded}
      className="focus-ring grid h-6 w-6 place-items-center rounded text-ink-mute transition-colors hover:bg-surface-2 hover:text-ink-2"
    >
      {children}
    </button>
  )
}

/** Per-conversation hover action (archive / delete). */
function RowAction({
  title,
  onClick,
  danger = false,
  children,
}: {
  title: string
  onClick: () => void
  danger?: boolean
  children: ReactNode
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      aria-label={title}
      className={cn(
        'focus-ring grid h-5 w-5 place-items-center rounded transition-colors',
        danger ? 'text-ink-mute hover:bg-crit/15 hover:text-crit' : 'text-ink-mute hover:bg-surface-2 hover:text-ink-2',
      )}
    >
      {children}
    </button>
  )
}

function NavButton({
  icon: Icon,
  label,
  active,
  collapsed,
  onClick,
}: {
  icon: LucideIcon
  label: string
  active: boolean
  collapsed: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={collapsed ? label : undefined}
      className={cn(
        'focus-ring relative flex h-9 items-center gap-2.5 rounded-md px-2.5 text-[13.5px] transition-colors',
        active ? 'bg-surface-2 text-ink' : 'text-ink-3 hover:bg-surface hover:text-ink-2',
      )}
    >
      {active && (
        <span className="absolute left-0 top-1/2 h-4 w-[2.5px] -translate-y-1/2 rounded-full bg-accent" />
      )}
      <Icon className="h-[17px] w-[17px] shrink-0" strokeWidth={1.8} />
      {!collapsed && <span className="truncate font-medium">{label}</span>}
    </button>
  )
}

import { MessageSquare, History, Settings, type LucideIcon } from 'lucide-react'
import { cn } from '@/lib/cn'

type NavItem = { id: string; label: string; icon: LucideIcon }

const NAV: NavItem[] = [
  { id: 'chat', label: 'Chat', icon: MessageSquare },
  { id: 'sessions', label: 'Sessions', icon: History },
  { id: 'settings', label: 'Settings', icon: Settings },
]

/**
 * Left nav. Transparent - part of the shared frosted shell. Active item is
 * marked by a subtle surface fill plus a short accent bar, not a heavy pill.
 */
export function Sidebar({
  collapsed,
  active,
  onNavigate,
}: {
  collapsed: boolean
  active: string
  onNavigate: (id: string) => void
}) {
  return (
    <nav
      className={cn(
        'flex shrink-0 flex-col gap-1 p-2 transition-[width] duration-200 ease-out',
        collapsed ? 'w-14' : 'w-56',
      )}
    >
      {NAV.map((item) => {
        const isActive = active === item.id
        const Icon = item.icon
        return (
          <button
            key={item.id}
            type="button"
            onClick={() => onNavigate(item.id)}
            title={collapsed ? item.label : undefined}
            className={cn(
              'focus-ring relative flex h-9 items-center gap-2.5 rounded-md px-2.5 text-[13.5px] transition-colors',
              isActive
                ? 'bg-surface-2 text-ink'
                : 'text-ink-3 hover:bg-surface hover:text-ink-2',
            )}
          >
            {isActive && (
              <span className="absolute left-0 top-1/2 h-4 w-[2.5px] -translate-y-1/2 rounded-full bg-accent" />
            )}
            <Icon className="h-[17px] w-[17px] shrink-0" strokeWidth={1.8} />
            {!collapsed && <span className="truncate font-medium">{item.label}</span>}
          </button>
        )
      })}

      <div className="mt-auto flex items-center gap-2.5 rounded-md p-2 text-ink-3">
        <span className="grid h-7 w-7 shrink-0 place-items-center rounded-full bg-surface-2 font-mono text-[12px]">
          U
        </span>
        {!collapsed && <span className="truncate text-[13px]">local machine</span>}
      </div>
    </nav>
  )
}

import { PanelLeft, Circle } from 'lucide-react'
import type { StreamStatus } from '@/lib/client'
import { t } from '@/i18n'

/**
 * Full-width top bar. Transparent - it sits on the shared frosted shell,
 * fused with the sidebar into one continuous surface. The status pill
 * reflects the live event-stream connection plus the active model.
 */
export function Topbar({
  title,
  collapsed,
  onToggle,
  status,
  model,
}: {
  title: string
  collapsed: boolean
  onToggle: () => void
  status: StreamStatus
  model?: string
}) {
  const dot =
    status === 'open' ? 'fill-ok text-ok' : status === 'connecting' ? 'fill-warn text-warn' : 'fill-crit text-crit'
  const label =
    status === 'open'
      ? (model ?? t('status.connected'))
      : status === 'connecting'
        ? t('status.connecting')
        : t('status.offline')

  return (
    <header className="flex h-14 shrink-0 items-center gap-3 px-3">
      <button
        type="button"
        onClick={onToggle}
        aria-label={collapsed ? t('nav.expandSidebar') : t('nav.collapseSidebar')}
        className="focus-ring grid h-8 w-8 place-items-center rounded-md text-ink-3 transition-colors hover:bg-surface hover:text-ink"
      >
        <PanelLeft className="h-[18px] w-[18px]" strokeWidth={1.8} />
      </button>

      <div className="flex items-center gap-2">
        <span className="grid h-7 w-7 place-items-center rounded-md bg-accent font-mono text-[13px] font-bold text-desktop">
          K
        </span>
        <span className="text-[15px] font-semibold tracking-[-0.01em]">Kestrel</span>
      </div>

      <span className="mx-1 h-4 w-px bg-line-2" />
      <h1 className="text-[14px] font-medium text-ink-2">{title}</h1>

      <div className="ml-auto flex items-center gap-2">
        <span className="flex items-center gap-1.5 rounded-full border border-line bg-surface px-2.5 py-1 text-[12px] text-ink-3">
          <Circle className={`h-2 w-2 ${dot}`} />
          {label}
        </span>
      </div>
    </header>
  )
}

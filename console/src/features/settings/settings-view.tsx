import { useHealth } from '@/lib/use-health'
import { cn } from '@/lib/cn'

/**
 * Read-only settings. Surfaces the active backend/model/workdir from
 * /api/health. Editing is via kestrel.toml + server restart for now.
 */
export function SettingsView() {
  const health = useHealth()
  const rows: [string, string | undefined][] = [
    ['Model', health?.model],
    ['Backend', health?.base_url],
    ['Working directory', health?.workdir],
    ['Session', health?.session],
  ]

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-2xl px-6 py-8">
        <h2 className="mb-1 text-[16px] font-semibold tracking-[-0.01em]">Settings</h2>
        <p className="mb-6 text-[13px] text-ink-3">
          Read-only for now. Edit kestrel.toml and restart kestrel-server to change these.
        </p>
        <div className="overflow-hidden rounded-lg border border-line">
          {rows.map(([k, v], i) => (
            <div
              key={k}
              className={cn('flex items-center gap-4 px-4 py-3 text-[13px]', i > 0 && 'border-t border-line')}
            >
              <span className="w-40 shrink-0 text-ink-3">{k}</span>
              <span className="truncate font-mono text-ink-2">{v ?? '-'}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

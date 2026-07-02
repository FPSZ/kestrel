import { useState } from 'react'
import { Topbar } from './topbar'
import { Sidebar } from './sidebar'
import { ChatView } from '@/features/chat/chat-view'
import { useConversation } from '@/lib/store'
import { useHealth } from '@/lib/use-health'

const TITLES: Record<string, string> = {
  chat: 'Chat',
  sessions: 'Sessions',
  settings: 'Settings',
}

/**
 * App shell. The topbar (full width) and sidebar (left) are transparent
 * children of one frosted surface (.glass-shell) so they read as a single
 * continuous panel; the content pane is inset as a bezel card. This is the
 * only structural idea borrowed from prior work - the treatment is flat.
 *
 * The live event stream is subscribed once here (single EventSource) and
 * passed down: conversation blocks to the chat view, stream status to the bar.
 */
export function AppShell() {
  const [collapsed, setCollapsed] = useState(false)
  const [active, setActive] = useState('chat')
  const convo = useConversation()
  const health = useHealth()

  return (
    <div className="h-screen overflow-hidden">
      <div className="glass-shell flex h-full flex-col overflow-hidden">
        <Topbar
          title={TITLES[active] ?? 'Kestrel'}
          collapsed={collapsed}
          onToggle={() => setCollapsed((v) => !v)}
          status={convo.status}
          model={health?.model}
        />
        <div className="flex min-h-0 flex-1 overflow-hidden">
          <Sidebar collapsed={collapsed} active={active} onNavigate={setActive} />
          <main className="min-h-0 flex-1 overflow-hidden p-1.5 pl-0">
            <div className="content-bezel flex h-full min-h-0 flex-col overflow-hidden">
              <ChatView blocks={convo.blocks} turnActive={convo.turnActive} />
            </div>
          </main>
        </div>
      </div>
    </div>
  )
}

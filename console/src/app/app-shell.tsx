import { useState } from 'react'
import { Topbar } from './topbar'
import { Sidebar } from './sidebar'
import { ChatView } from '@/features/chat/chat-view'
import { SessionReplay } from '@/features/sessions/sessions-view'
import { SettingsView } from '@/features/settings/settings-view'
import { LauncherView } from '@/features/launcher/launcher-view'
import { useConversation } from '@/lib/store'
import { useHealth } from '@/lib/use-health'
import { client } from '@/lib/client'
import { t } from '@/i18n'

const TITLE_KEYS: Record<string, string> = {
  chat: 'nav.chat',
  models: 'nav.models',
  settings: 'nav.settings',
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
  // which persisted conversation to replay; null = the live current session.
  const [openedSession, setOpenedSession] = useState<string | null>(null)
  // bumped on "new conversation" to reset + reconnect the folded live stream.
  const [convoGen, setConvoGen] = useState(0)
  // the live session id, set instantly on new-conversation (health polls slowly).
  const [currentOverride, setCurrentOverride] = useState<string | null>(null)
  const convo = useConversation(convoGen)
  const health = useHealth()
  const currentSession = currentOverride ?? health?.session

  const navigate = (id: string) => {
    setActive(id)
    if (id === 'chat') setOpenedSession(null)
  }
  const openSession = (id: string | null) => {
    setActive('chat')
    // selecting the current live session means "go live".
    setOpenedSession(id && id === currentSession ? null : id)
  }
  const newConversation = async () => {
    try {
      const id = await client.newSession()
      setCurrentOverride(id)
      setActive('chat')
      setOpenedSession(null)
      setConvoGen((g) => g + 1) // reconnect + reset fold to the new empty session
    } catch {
      /* server unavailable - leave state untouched */
    }
  }

  const replaying = active === 'chat' && openedSession !== null
  const title = replaying ? openedSession! : t(TITLE_KEYS[active] ?? 'nav.chat')

  return (
    <div className="h-screen overflow-hidden">
      <div className="glass-shell flex h-full flex-col overflow-hidden">
        <Topbar
          title={title}
          collapsed={collapsed}
          onToggle={() => setCollapsed((v) => !v)}
          status={convo.status}
          model={health?.model}
        />
        <div className="flex min-h-0 flex-1 overflow-hidden">
          <Sidebar
            collapsed={collapsed}
            active={active}
            openedSession={openedSession}
            currentSession={currentSession}
            onNavigate={navigate}
            onOpenSession={openSession}
            onNewConversation={newConversation}
          />
          <main className="min-h-0 flex-1 overflow-hidden p-1.5 pl-0">
            <div className="content-bezel flex h-full min-h-0 flex-col overflow-hidden">
              {active === 'models' ? (
                <LauncherView />
              ) : active === 'settings' ? (
                <SettingsView />
              ) : replaying ? (
                <SessionReplay id={openedSession!} />
              ) : (
                <ChatView blocks={convo.blocks} turnActive={convo.turnActive} />
              )}
            </div>
          </main>
        </div>
      </div>
    </div>
  )
}

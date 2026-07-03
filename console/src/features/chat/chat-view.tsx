import {
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ClipboardEvent,
  type ReactNode,
} from 'react'
import {
  ArrowUp,
  Loader,
  Brain,
  Square,
  ChevronDown,
  Check,
  MessageSquare,
  Zap,
  ClipboardList,
  X,
  AlertTriangle,
} from 'lucide-react'
import { client } from '@/lib/client'
import type { Block } from '@/lib/store'
import type { AgentMode } from '@/lib/types'
import { t } from '@/i18n'
import { Conversation } from './conversation'

/**
 * Live conversation view. Owns the composer: the message box, the thinking +
 * run-mode selectors, slash commands, and outbound Ops (user input / cancel;
 * approvals ride the Conversation).
 */
export function ChatView({ blocks, turnActive }: { blocks: Block[]; turnActive: boolean }) {
  const [text, setText] = useState('')
  // pasted images for the next message (base64 data URLs)
  const [images, setImages] = useState<string[]>([])
  // think + mode persist across refreshes (localStorage) so a chosen posture sticks.
  const [think, setThink] = useState<boolean>(() => loadPref('kestrel.think', true))
  const [mode, setMode] = useState<AgentMode>(() => loadPref<AgentMode>('kestrel.mode', 'ask'))
  useEffect(() => savePref('kestrel.think', think), [think])
  useEffect(() => savePref('kestrel.mode', mode), [mode])
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

  // Interrupt the running turn: cancels streaming AND kills any in-flight tool
  // subprocess (the op is handled mid-turn by core's select loops).
  const stop = () => {
    void client.sendOp({ type: 'cancel' })
  }

  // Pull image(s) out of a paste and attach them (base64) instead of pasting
  // the OS filename text. Non-image pastes fall through to normal text paste.
  const onPaste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const items = e.clipboardData?.items
    if (!items) return
    const files: File[] = []
    for (const it of items) {
      if (it.kind === 'file' && it.type.startsWith('image/')) {
        const f = it.getAsFile()
        if (f) files.push(f)
      }
    }
    if (files.length === 0) return
    e.preventDefault()
    for (const f of files) {
      const reader = new FileReader()
      reader.onload = () => {
        if (typeof reader.result === 'string') setImages((prev) => [...prev, reader.result as string])
      }
      reader.readAsDataURL(f)
    }
  }

  // Enter submits. A leading "/" is a command (think/mode/stop/help) handled
  // locally and never sent to the model; everything else is a turn.
  const submit = () => {
    const t = text.trim()
    if (t.startsWith('/')) {
      const [cmd, arg = ''] = t.slice(1).split(/\s+/)
      if (cmd === 'think') {
        setThink(arg === 'off' ? false : arg === 'on' ? true : !think)
        setText('')
      } else if (cmd === 'mode') {
        if (arg === 'ask' || arg === 'auto' || arg === 'plan') setMode(arg)
        setText('')
      } else if (cmd === 'stop') {
        if (turnActive) stop()
        setText('')
      } else if (cmd === 'help' || cmd === '') {
        setText('/') // reopen the palette listing every command
      }
      // unknown /command: leave the text so the user can fix it
      return
    }
    if (!t && images.length === 0) return // nothing to send
    if (turnActive) return // a message can't start while a turn runs
    void client.sendOp({
      type: 'user_input',
      text: t,
      think,
      mode,
      images: images.length ? images : undefined,
    })
    setText('')
    setImages([])
    atBottomRef.current = true // our own send always jumps to the bottom
  }

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      submit()
    } else if (e.key === 'Escape' && text.startsWith('/')) {
      e.preventDefault()
      setText('')
    }
  }

  const pickCmd = (c: SlashCmd) => {
    if (c.hasArg) {
      setText(`/${c.name} `)
      taRef.current?.focus()
    } else if (c.name === 'stop') {
      if (turnActive) stop()
      setText('')
    } else if (c.name === 'help') {
      setText('/')
      taRef.current?.focus()
    }
  }

  // While a turn runs and the box is empty, the send button becomes a stop
  // button so the user can always bail out.
  const showStop = turnActive && !text.trim()

  // A turn paused on a pending approval looks identical to "still working" but
  // needs YOUR input to continue — surface it loudly so it doesn't read as a hang.
  const awaitingApproval = blocks.some(
    (b) => b.kind === 'tool' && b.status === 'pending_approval',
  )

  const slashQuery = text.startsWith('/') ? text.slice(1) : null
  const firstTok = (slashQuery ?? '').split(/\s+/)[0].toLowerCase()
  const matched = slashQuery !== null ? COMMANDS.filter((c) => c.name.startsWith(firstTok)) : []

  return (
    <div className="flex h-full min-h-0 flex-col">
      {blocks.length === 0 ? (
        <EmptyState />
      ) : (
        <div ref={scrollRef} onScroll={onScroll} className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto flex min-w-0 max-w-3xl flex-col gap-4 px-4 py-6">
            <Conversation blocks={blocks} turnActive={turnActive} />
            {turnActive &&
              (awaitingApproval ? (
                <div className="flex items-center gap-2 rounded-lg border border-warn/40 bg-warn/10 px-3 py-2 text-[13px] font-medium text-warn">
                  <AlertTriangle className="h-4 w-4 shrink-0" strokeWidth={2} />
                  {t('chat.awaitingApproval')}
                </div>
              ) : (
                <div className="flex items-center gap-2 text-[13px] text-ink-mute">
                  <Loader className="h-3.5 w-3.5 animate-spin" strokeWidth={2} />
                  {t('chat.working')}
                </div>
              ))}
          </div>
        </div>
      )}

      <div className="shrink-0 p-3">
        {/* slash command palette */}
        {slashQuery !== null && (
          <div className="mx-auto mb-1.5 max-w-3xl overflow-hidden rounded-xl border border-line-2 bg-bezel p-1 shadow-[0_20px_50px_-24px_rgba(0,0,0,0.8)]">
            {matched.length ? (
              matched.map((c) => (
                <button
                  key={c.name}
                  type="button"
                  onClick={() => pickCmd(c)}
                  className="flex w-full items-center gap-3 rounded-lg px-2.5 py-1.5 text-left transition-colors hover:bg-surface-2"
                >
                  <span className="font-mono text-[13px] text-accent-ink">{c.usage}</span>
                  <span className="ml-auto text-[12px] text-ink-3">{c.desc}</span>
                </button>
              ))
            ) : (
              <div className="px-2.5 py-1.5 text-[13px] text-ink-mute">{t('palette.noMatch')}</div>
            )}
          </div>
        )}

        <div className="mx-auto max-w-3xl rounded-2xl border border-line bg-surface px-3.5 pt-3 pb-2.5 transition-colors focus-within:border-line-2">
          {images.length > 0 && (
            <div className="mb-2 flex flex-wrap gap-2">
              {images.map((src, i) => (
                <div
                  key={i}
                  className="group relative h-16 w-16 overflow-hidden rounded-lg border border-line-2"
                >
                  <img src={src} alt="" className="h-full w-full object-cover" />
                  <button
                    type="button"
                    onClick={() => setImages((prev) => prev.filter((_, j) => j !== i))}
                    aria-label={t('composer.removeImage')}
                    title={t('composer.removeImage')}
                    className="absolute right-0.5 top-0.5 grid h-4 w-4 place-items-center rounded-full bg-desktop/85 text-ink-2 transition-colors hover:text-ink"
                  >
                    <X className="h-3 w-3" strokeWidth={2.4} />
                  </button>
                </div>
              ))}
            </div>
          )}
          <textarea
            ref={taRef}
            rows={2}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={onKeyDown}
            onPaste={onPaste}
            placeholder={t('composer.placeholder')}
            className="block max-h-56 min-h-[56px] w-full resize-none bg-transparent text-[16px] leading-relaxed text-ink placeholder:text-ink-mute focus:outline-none"
          />
          {/* control row: thinking on the left, run-mode + send on the right */}
          <div className="mt-1.5 flex items-center gap-2">
            <Selector
              title={t('think.title')}
              align="left"
              tinted={think}
              value={think ? 'on' : 'off'}
              options={THINK_OPTS}
              onChange={(id) => setThink(id === 'on')}
            />
            <div className="ml-auto flex items-center gap-2">
              <Selector
                title={t('mode.title')}
                tinted={mode !== 'ask'}
                value={mode}
                options={MODE_OPTS}
                onChange={(id) => setMode(id as AgentMode)}
              />
              {showStop ? (
                <button
                  type="button"
                  onClick={stop}
                  aria-label={t('composer.stop')}
                  title={t('composer.stopTitle')}
                  className="focus-ring grid h-9 w-9 shrink-0 place-items-center rounded-lg border border-crit/45 bg-crit/15 text-crit transition-colors hover:bg-crit/25"
                >
                  <Square className="h-[14px] w-[14px]" strokeWidth={2.4} fill="currentColor" />
                </button>
              ) : (
                <button
                  type="button"
                  onClick={submit}
                  disabled={(!text.trim() && images.length === 0) || (turnActive && !text.startsWith('/'))}
                  aria-label={t('composer.send')}
                  className="focus-ring grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-accent text-desktop transition-colors hover:bg-accent-2 disabled:cursor-not-allowed disabled:opacity-40"
                >
                  <ArrowUp className="h-[18px] w-[18px]" strokeWidth={2.2} />
                </button>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

/** Persist a small UI preference across refreshes (best-effort; ignores quota/JSON errors). */
function loadPref<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(key)
    return v == null ? fallback : (JSON.parse(v) as T)
  } catch {
    return fallback
  }
}
function savePref(key: string, val: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(val))
  } catch {
    /* storage unavailable / full - non-fatal */
  }
}

/** One slash command entry. `usage` is command syntax (literal, not localized). */
type SlashCmd = { name: string; usage: string; desc: string; hasArg: boolean }
const COMMANDS: SlashCmd[] = [
  { name: 'think', usage: '/think on|off', desc: t('cmd.think.desc'), hasArg: true },
  { name: 'mode', usage: '/mode ask|auto|plan', desc: t('cmd.mode.desc'), hasArg: true },
  { name: 'stop', usage: '/stop', desc: t('cmd.stop.desc'), hasArg: false },
  { name: 'help', usage: '/help', desc: t('cmd.help.desc'), hasArg: false },
]

type Opt = { id: string; label: string; hint?: string; icon: ReactNode }

const THINK_OPTS: Opt[] = [
  { id: 'on', label: t('think.on.label'), hint: t('think.on.hint'), icon: <Brain className="h-4 w-4" strokeWidth={2} /> },
  { id: 'off', label: t('think.off.label'), hint: t('think.off.hint'), icon: <Brain className="h-4 w-4" strokeWidth={2} /> },
]

const MODE_OPTS: Opt[] = [
  { id: 'ask', label: t('mode.ask.label'), hint: t('mode.ask.hint'), icon: <MessageSquare className="h-4 w-4" strokeWidth={2} /> },
  { id: 'auto', label: t('mode.auto.label'), hint: t('mode.auto.hint'), icon: <Zap className="h-4 w-4" strokeWidth={2} /> },
  { id: 'plan', label: t('mode.plan.label'), hint: t('mode.plan.hint'), icon: <ClipboardList className="h-4 w-4" strokeWidth={2} /> },
]

/** Compact pop-up selector used for both the thinking and run-mode controls.
 *  `align` controls which edge the menu anchors to so it never spills off-screen:
 *  left-edge controls open leftward-aligned, right-edge controls rightward. */
function Selector({
  options,
  value,
  onChange,
  title,
  tinted = false,
  align = 'right',
}: {
  options: Opt[]
  value: string
  onChange: (id: string) => void
  title?: string
  tinted?: boolean
  align?: 'left' | 'right'
}) {
  const [open, setOpen] = useState(false)
  const cur = options.find((o) => o.id === value) ?? options[0]
  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        title={title}
        aria-haspopup="menu"
        aria-expanded={open}
        className={`focus-ring flex items-center gap-1.5 rounded-lg border px-2.5 py-1 text-[13px] font-medium transition-colors ${
          tinted
            ? 'border-accent/45 bg-accent/10 text-accent-ink'
            : 'border-line text-ink-2 hover:border-line-2 hover:text-ink'
        }`}
      >
        {cur.icon}
        <span>{cur.label}</span>
        <ChevronDown className="h-3.5 w-3.5 opacity-70" strokeWidth={2} />
      </button>
      {open && (
        <>
          {/* click-away backdrop */}
          <button
            aria-hidden
            tabIndex={-1}
            onClick={() => setOpen(false)}
            className="fixed inset-0 z-10 cursor-default"
          />
          <div
            role="menu"
            className={`absolute bottom-full z-20 mb-1.5 min-w-[208px] overflow-hidden rounded-xl border border-line-2 bg-bezel p-1 shadow-[0_20px_50px_-20px_rgba(0,0,0,0.85)] ${
              align === 'left' ? 'left-0' : 'right-0'
            }`}
          >
            {options.map((o) => {
              const active = o.id === value
              return (
                <button
                  key={o.id}
                  type="button"
                  role="menuitemradio"
                  aria-checked={active}
                  onClick={() => {
                    onChange(o.id)
                    setOpen(false)
                  }}
                  className={`flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left text-[13px] transition-colors ${
                    active ? 'bg-accent/12 text-accent-ink' : 'text-ink-2 hover:bg-surface-2'
                  }`}
                >
                  <span className={active ? 'text-accent-ink' : 'text-ink-3'}>{o.icon}</span>
                  <span className="flex-1 font-medium">{o.label}</span>
                  {o.hint && <span className="text-[11px] text-ink-mute">{o.hint}</span>}
                  {active && <Check className="h-3.5 w-3.5 shrink-0" strokeWidth={2.4} />}
                </button>
              )
            })}
          </div>
        </>
      )}
    </div>
  )
}

function EmptyState() {
  return (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
      <div className="grid h-12 w-12 place-items-center rounded-xl border border-line bg-surface font-mono text-[18px] font-bold text-accent-ink">
        K
      </div>
      <h2 className="text-[17px] font-semibold tracking-[-0.01em]">{t('empty.title')}</h2>
      <p className="max-w-sm text-[13.5px] leading-relaxed text-ink-3">{t('empty.body')}</p>
    </div>
  )
}

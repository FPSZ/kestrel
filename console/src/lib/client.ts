// KestrelClient: the transport-agnostic seam between the UI and core.
// v1 is SSE (events) + fetch (ops/health). A future Tauri build swaps this
// implementation for IPC without touching any UI code (ADR-0007).

import type { Health, KestrelEvent, Op } from './types'

export type StreamStatus = 'connecting' | 'open' | 'closed'

type EventHandler = (event: KestrelEvent) => void
type StatusHandler = (status: StreamStatus) => void

class KestrelClient {
  private source: EventSource | null = null

  /** Open the live event stream. EventSource auto-reconnects; the server
   *  replays the full snapshot on each (re)connect, so consumers must dedup
   *  by seq. Returns an unsubscribe function. */
  subscribe(onEvent: EventHandler, onStatus?: StatusHandler): () => void {
    onStatus?.('connecting')
    const source = new EventSource('/api/events')
    this.source = source

    source.onopen = () => onStatus?.('open')
    source.onmessage = (e) => {
      try {
        onEvent(JSON.parse(e.data) as KestrelEvent)
      } catch {
        // ignore malformed frame (keep-alive comments never reach onmessage)
      }
    }
    source.onerror = () => onStatus?.('connecting') // browser will retry

    return () => {
      source.close()
      if (this.source === source) this.source = null
      onStatus?.('closed')
    }
  }

  /** Submit an Op to core (user input, approval decision, cancel). */
  async sendOp(op: Op): Promise<void> {
    const res = await fetch('/api/ops', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(op),
    })
    if (!res.ok) throw new Error(`sendOp failed: ${res.status}`)
  }

  async health(): Promise<Health> {
    const res = await fetch('/api/health')
    if (!res.ok) throw new Error(`health failed: ${res.status}`)
    return (await res.json()) as Health
  }

  async listSessions(): Promise<string[]> {
    const res = await fetch('/api/sessions')
    if (!res.ok) throw new Error(`listSessions failed: ${res.status}`)
    return (await res.json()) as string[]
  }

  /** Start a fresh conversation: the server rotates its active session (clears
   *  core history) and returns the new id. Callers should reconnect the event
   *  stream afterwards so the fold resets to the empty session. */
  async newSession(): Promise<string> {
    const res = await fetch('/api/sessions', { method: 'POST' })
    if (!res.ok) throw new Error(`newSession failed: ${res.status}`)
    const { id } = (await res.json()) as { id: string }
    return id
  }

  /** Delete a persisted session's log file from disk. Rejected (409) for the
   *  active session. */
  async deleteSession(id: string): Promise<void> {
    const res = await fetch(`/api/sessions/${encodeURIComponent(id)}`, { method: 'DELETE' })
    if (!res.ok) throw new Error(`deleteSession failed: ${res.status}`)
  }

  async sessionEvents(id: string): Promise<KestrelEvent[]> {
    const res = await fetch(`/api/sessions/${encodeURIComponent(id)}/events`)
    if (!res.ok) throw new Error(`sessionEvents failed: ${res.status}`)
    return (await res.json()) as KestrelEvent[]
  }
}

/** Single-session personal app: one client for the process. */
export const client = new KestrelClient()

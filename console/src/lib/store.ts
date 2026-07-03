// Event-fold store. The UI state is a fold over the event stream - the same
// "state = fold(events)" model the core uses (ADR-0002). Events carry a
// monotonic per-session seq; we dedup by it so the snapshot the server
// replays on every (re)connect is idempotent.

import { useEffect, useState } from 'react'
import { client, type StreamStatus } from './client'
import type { KestrelEvent, RiskLevel } from './types'

export type ToolStatus = 'running' | 'ok' | 'error' | 'pending_approval'

export interface ToolBlock {
  kind: 'tool'
  seq: number
  callId: string
  tool: string
  args: unknown
  status: ToolStatus
  result?: string
  risk?: RiskLevel
  review?: string | null
}

export type Block =
  | { kind: 'user'; seq: number; ts?: number; text: string; images?: string[] }
  | { kind: 'assistant'; seq: number; ts?: number; text: string; reasoning?: string }
  | ToolBlock
  | { kind: 'error'; seq: number; ts?: number; message: string }

export interface ConversationState {
  blocks: Block[]
  turnActive: boolean
  lastSeq: number
}

export const initialState: ConversationState = {
  blocks: [],
  turnActive: false,
  lastSeq: -1,
}

/** Fold one event into conversation state. Pure; safe to replay. */
export function fold(state: ConversationState, event: KestrelEvent): ConversationState {
  if (event.seq <= state.lastSeq) return state // dedup snapshot-on-reconnect

  const blocks = state.blocks.slice()
  let turnActive = state.turnActive
  const p = event.payload

  const findTool = (callId: string) =>
    blocks.findIndex((b) => b.kind === 'tool' && b.callId === callId)

  switch (p.type) {
    case 'user_input':
      blocks.push({ kind: 'user', seq: event.seq, ts: event.ts, text: p.text, images: p.images })
      turnActive = true
      break
    case 'agent_text': {
      const last = blocks[blocks.length - 1]
      if (last && last.kind === 'assistant') {
        blocks[blocks.length - 1] = { ...last, text: last.text + p.text }
      } else {
        blocks.push({ kind: 'assistant', seq: event.seq, ts: event.ts, text: p.text })
      }
      break
    }
    case 'agent_reasoning': {
      // reasoning streams before the answer; attach it to the current assistant
      // block (creating one with empty text if the answer hasn't started yet).
      const last = blocks[blocks.length - 1]
      if (last && last.kind === 'assistant') {
        blocks[blocks.length - 1] = { ...last, reasoning: (last.reasoning ?? '') + p.text }
      } else {
        blocks.push({ kind: 'assistant', seq: event.seq, ts: event.ts, text: '', reasoning: p.text })
      }
      break
    }
    case 'tool_call_requested':
      blocks.push({
        kind: 'tool',
        seq: event.seq,
        callId: p.call_id,
        tool: p.tool,
        args: p.args,
        status: 'running',
      })
      break
    case 'approval_required': {
      const i = findTool(p.call_id)
      if (i >= 0) {
        blocks[i] = { ...(blocks[i] as ToolBlock), status: 'pending_approval', risk: p.risk, review: p.review }
      }
      break
    }
    case 'approval_resolved': {
      // approve flips the card to running (deny is followed by a tool_result
      // error, which sets 'error'). recording the decision is what makes a
      // page-switch / reconnect replay NOT re-show the approval prompt.
      const i = findTool(p.call_id)
      if (i >= 0 && p.approved) {
        blocks[i] = { ...(blocks[i] as ToolBlock), status: 'running' }
      }
      break
    }
    case 'tool_result': {
      const i = findTool(p.call_id)
      if (i >= 0) {
        blocks[i] = { ...(blocks[i] as ToolBlock), status: p.ok ? 'ok' : 'error', result: p.content }
      }
      break
    }
    case 'turn_completed':
      turnActive = false
      break
    case 'error':
      blocks.push({ kind: 'error', seq: event.seq, ts: event.ts, message: p.message })
      turnActive = false
      break
  }

  return { blocks, turnActive, lastSeq: event.seq }
}

/** Subscribe to the live stream and maintain folded conversation state.
 *
 * `resetKey` forces a fresh subscription + a fold reset: on "new conversation"
 * the server rotates its session and restarts seq at 0, which the seq-dedup
 * would otherwise drop against the old session's high lastSeq. Bumping the key
 * closes the old EventSource, clears the fold, and re-subscribes to the new
 * (empty) session snapshot. */
export function useConversation(resetKey = 0): ConversationState & { status: StreamStatus } {
  const [state, setState] = useState(initialState)
  const [status, setStatus] = useState<StreamStatus>('connecting')

  useEffect(() => {
    setState(initialState) // fresh session / reconnect: drop the old fold
    const unsub = client.subscribe(
      // stamp receive time at the SSE edge (kept out of pure fold, passed as data)
      (event) => setState((s) => fold(s, { ...event, ts: event.ts ?? Date.now() })),
      (st) => setStatus(st),
    )
    return unsub
  }, [resetKey])

  return { ...state, status }
}

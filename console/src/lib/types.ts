// TypeScript mirror of kestrel-protocol wire types (serde tag = "type",
// snake_case). Keep in sync with crates/kestrel-protocol; a future ts-rs
// export (ADR-0001) will generate these automatically.

export type CrewRole = 'lead' | 'copilot' | 'librarian' | 'critic' | 'system'

export type RiskLevel = 'read_only' | 'mutating' | 'destructive' | 'external'

/** Stable, language-neutral error classification (mirrors kestrel-protocol ErrorCode).
 * The UI localizes the code; `message` is dev-facing English detail. */
export type ErrorCode = 'backend' | 'tool' | 'store' | 'cancelled' | 'internal'

export type EventPayload =
  | { type: 'user_input'; text: string; images?: string[] }
  | { type: 'agent_text'; text: string }
  | { type: 'agent_reasoning'; text: string }
  | { type: 'tool_call_requested'; call_id: string; tool: string; args: unknown }
  | { type: 'approval_required'; call_id: string; risk: RiskLevel; review: string | null }
  | { type: 'approval_resolved'; call_id: string; approved: boolean }
  | { type: 'tool_result'; call_id: string; ok: boolean; content: string }
  | { type: 'turn_completed'; reason: string }
  | { type: 'context_budget'; used_tokens: number; n_ctx: number }
  | { type: 'error'; message: string; code?: ErrorCode }

export interface KestrelEvent {
  seq: number
  actor: CrewRole
  payload: EventPayload
  /** client-attached receive time (ms). the wire event has no timestamp yet;
   * stamped at the SSE edge for display. persisted event timestamps ride G11. */
  ts?: number
}

/** Per-turn run mode (mirrors kestrel-protocol AgentMode). ask=询问, auto=全部执行, plan=计划. */
export type AgentMode = 'ask' | 'auto' | 'plan'

export type Op =
  | { type: 'user_input'; text: string; think?: boolean; mode?: AgentMode; images?: string[] }
  | { type: 'approve'; call_id: string }
  | { type: 'deny'; call_id: string; reason: string | null }
  | { type: 'cancel' }

export interface Health {
  ok: boolean
  session: string
  model: string
  base_url: string
  workdir: string
}

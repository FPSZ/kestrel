// TypeScript mirror of kestrel-protocol wire types (serde tag = "type",
// snake_case). Keep in sync with crates/kestrel-protocol; a future ts-rs
// export (ADR-0001) will generate these automatically.

export type CrewRole = 'lead' | 'copilot' | 'librarian' | 'critic' | 'system'

export type RiskLevel = 'read_only' | 'mutating' | 'destructive' | 'external'

export type EventPayload =
  | { type: 'user_input'; text: string }
  | { type: 'agent_text'; text: string }
  | { type: 'tool_call_requested'; call_id: string; tool: string; args: unknown }
  | { type: 'approval_required'; call_id: string; risk: RiskLevel; review: string | null }
  | { type: 'tool_result'; call_id: string; ok: boolean; content: string }
  | { type: 'turn_completed'; reason: string }
  | { type: 'context_budget'; used_tokens: number; n_ctx: number }
  | { type: 'error'; message: string }

export interface KestrelEvent {
  seq: number
  actor: CrewRole
  payload: EventPayload
}

export type Op =
  | { type: 'user_input'; text: string }
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

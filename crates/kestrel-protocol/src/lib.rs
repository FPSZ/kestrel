//! # kestrel-protocol
//!
//! Kestrel 全 workspace 的共享纯类型：会话事件、前端操作、工具规格、风险分级。
//!
//! ## 职责边界
//!
//! - 只有类型定义与 serde 派生，**零逻辑、零 IO、零异步**。
//! - 被所有 crate 依赖；本 crate 不依赖任何内部 crate。
//! - 允许的第三方依赖：仅 serde / `serde_json`。
//! - 本 crate 是 v2 `WebUI` 的类型契约来源（届时经 ts-rs 导出 TS 类型，见 ADR-001）。
//!
//! ## 设计约束
//!
//! - [`Event`] 是 append-only 事件日志的原子单位：会话状态 = `fold(events)`（ADR-002）。
//! - 所有序列化格式变更都是破坏性变更，须走 CHANGELOG。

pub mod completion;
pub mod event;
pub mod message;
pub mod op;
pub mod risk;
pub mod tool_spec;

pub use completion::{BackendCapabilities, CompletionChunk, CompletionRequest};
pub use event::{CrewRole, Event, EventPayload, SessionId};
pub use message::{Message, Role, ToolCall};
pub use op::Op;
pub use risk::{Decision, RiskLevel};
pub use tool_spec::ToolSpec;

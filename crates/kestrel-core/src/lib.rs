//! # kestrel-core
//!
//! Kestrel 的核心：单线程 agent loop、context ledger（token 记账）、
//! 权限引擎，以及全部端口 trait（[`ports`]）。
//!
//! ## 职责边界
//!
//! - **零 IO**：不发 HTTP、不碰文件系统、不起子进程。全部外部世界经由
//!   [`ports`] 中的 trait 注入（依赖方向铁律，docs/architecture.md §4.1）。
//! - 允许的依赖：`kestrel-protocol`、tokio 异步原语、错误处理。
//!   禁止依赖任何适配器 crate（backend/tools/store）与前端 crate。
//! - 对前端的唯一接口：提交 [`kestrel_protocol::Op`]，消费有序的
//!   [`kestrel_protocol::Event`] 流（单消费者 mpsc，非 pub/sub——ADR-002）。
//!
//! ## 模块导览
//!
//! | 模块 | 职责 | 设计出处 |
//! | --- | --- | --- |
//! | [`agent`] | 单线程 turn 状态机 | §5.1 |
//! | [`ledger`] | token 预算 + KV 前缀联动 | §5.2 |
//! | [`permission`] | deny 优先 + 风险分级 | §5.3 |
//! | [`crew`] | 机组作业路由（确定性代码，非 LLM 决策） | §6.6 |
//! | [`tools`] | 工具集合（ToolSet），查找表 | §8 |
//! | [`ports`] | LlmBackend / Tool / Store 端口 trait | §3.3 |

pub mod agent;
pub mod crew;
pub mod ledger;
pub mod permission;
pub mod ports;
pub mod tools;

pub use agent::{Agent, AgentConfig, TurnLimits};
pub use ledger::{ContextLedger, estimate_messages};
pub use permission::{ApprovalPolicy, PermissionEngine};
pub use tools::ToolSet;

/// core 层错误。
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// 后端错误（经端口上抛）。
    #[error("backend: {0}")]
    Backend(String),
    /// 工具执行错误中不可恢复的部分（可恢复错误以 `ToolResult` 喂回模型）。
    #[error("tool: {0}")]
    Tool(String),
    /// 存储错误。
    #[error("store: {0}")]
    Store(String),
    /// 会话被用户取消。
    #[error("cancelled")]
    Cancelled,
}

impl CoreError {
    /// 映射到稳定的、语言中立的错误分类（地基 #3）。进事件日志时与开发向的
    /// `to_string()` 细节一起发出，前端据 code 本地化、把细节作次要上下文。
    #[must_use]
    pub fn code(&self) -> kestrel_protocol::ErrorCode {
        use kestrel_protocol::ErrorCode;
        match self {
            CoreError::Backend(_) => ErrorCode::Backend,
            CoreError::Tool(_) => ErrorCode::Tool,
            CoreError::Store(_) => ErrorCode::Store,
            CoreError::Cancelled => ErrorCode::Cancelled,
        }
    }
}

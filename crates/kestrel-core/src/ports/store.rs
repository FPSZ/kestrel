//! 存储端口。

use kestrel_protocol::{Event, SessionId};

use crate::CoreError;

/// 存储端口。实现方：`kestrel-store`（JSONL 事件日志）；测试用内存实现。
///
/// 事件日志是唯一事实源：append-only，一个机制同时提供
/// 持久化、审计、崩溃恢复与回放测试（ADR-002）。
#[async_trait::async_trait]
pub trait Store: Send + Sync {
    /// 追加一条事件（唯一的写操作——没有 update，没有 delete）。
    async fn append(&self, session: &SessionId, event: &Event) -> Result<(), CoreError>;

    /// 重放会话的全部事件（resume 与回放测试的入口）。
    async fn replay(&self, session: &SessionId) -> Result<Vec<Event>, CoreError>;
}

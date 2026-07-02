//! 前端操作（Op）：前端到 core 的唯一输入通道。
//!
//! 借鉴 codex-rs 的 SQ/EQ 思想（去掉其 JSON-RPC 仪式）：
//! 前端提交 [`Op`]，core 回以 [`crate::Event`] 流，永远单向、有序。

use serde::{Deserialize, Serialize};

/// 前端提交给 core 的操作。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Op {
    /// 用户输入一条消息，开启一轮。
    UserInput {
        /// 输入文本。
        text: String,
    },
    /// 批准挂起的风险动作。
    Approve {
        /// 挂起动作的调用标识。
        call_id: String,
    },
    /// 拒绝挂起的风险动作（理由会喂回模型）。
    Deny {
        /// 挂起动作的调用标识。
        call_id: String,
        /// 拒绝理由（喂回模型引导换路）。
        reason: Option<String>,
    },
    /// 中断当前轮（取消信号贯穿到子进程）。
    Cancel,
}

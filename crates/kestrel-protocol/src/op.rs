//! 前端操作（Op）：前端到 core 的唯一输入通道。
//!
//! 借鉴 codex-rs 的 SQ/EQ 思想（去掉其 JSON-RPC 仪式）：
//! 前端提交 [`Op`]，core 回以 [`crate::Event`] 流，永远单向、有序。

use serde::{Deserialize, Serialize};

/// agent 运行模式：本轮的权限姿态（前端"询问/全部执行/计划"三态，像 Claude Code）。
///
/// 每轮随 [`Op::UserInput`] 提交，决定权限门如何裁决工具风险。**不改 system prompt
/// 前缀**（前缀逐字节稳定铁律）——纯在权限层生效，Plan 模式靠"挡回写动作 + 纠错提示"
/// 让模型只出计划不落地。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    /// 询问：只读放行，写动作逐个问用户（默认，等价 on-request）。
    #[default]
    Ask,
    /// 全部执行：只读 + 工作区可变自动放行；破坏性/外联仍必问（权限门铁律不削弱）。
    Auto,
    /// 计划：只读放行，写动作一律挡回，模型只产出计划、不落地。
    Plan,
}

/// 前端提交给 core 的操作。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Op {
    /// 用户输入一条消息，开启一轮。
    UserInput {
        /// 输入文本。
        text: String,
        /// 是否启用思考通道（Qwen3 等推理模型的 enable_thinking）。
        /// 前端可省略，默认开——由 UI 的思考开关控制。
        #[serde(default = "default_think")]
        think: bool,
        /// 本轮运行模式（询问/全部执行/计划）。前端可省略，默认 Ask。
        #[serde(default)]
        mode: AgentMode,
        /// 随消息粘贴的图片（`data:image/...;base64,...` URL）。需要视觉模型才被理解；
        /// 文本模型忽略。前端可省略，默认空。
        #[serde(default)]
        images: Vec<String>,
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
    /// 开启一个全新会话：清空历史、轮换会话 id、seq 归零（前端"新建对话"）。
    ///
    /// id 由 server 分配（core 不含时钟/随机，保持确定性）；新会话日志从 seq 0 起。
    NewSession {
        /// server 分配的新会话标识。
        id: String,
    },
}

/// `UserInput.think` 的默认值：默认开思考（与旧行为一致）。
fn default_think() -> bool {
    true
}

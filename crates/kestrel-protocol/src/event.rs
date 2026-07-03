//! 会话事件：append-only 事件日志的原子单位。
//!
//! 铁律（docs/architecture.md 原则 1/4）：事件只追加、永不改写；
//! 会话状态是事件序列的折叠投影，禁止旁路可变状态。

use serde::{Deserialize, Serialize};

/// 会话标识。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

/// 机组角色：事件的产出者标签，前端据此渲染车道（docs/architecture.md §6.6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrewRole {
    /// 主脑：主循环，规划与写码。
    Lead,
    /// 副手：后台压缩、摘要、预读。
    Copilot,
    /// 书记：记忆与检索。
    Librarian,
    /// 审校：高危动作复核。
    Critic,
    /// 系统：非模型产生的事件（权限门、账本等）。
    System,
}

/// 一条会话事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// 会话内单调递增序号（事件日志的排序依据，不依赖时钟）。
    pub seq: u64,
    /// 产出者。
    pub actor: CrewRole,
    /// 事件内容。
    pub payload: EventPayload,
}

/// 事件内容。
///
/// M1 只需要主循环闭环所需的最小集合；机组相关事件在 M2 扩充。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventPayload {
    /// 用户输入。
    UserInput {
        /// 输入文本。
        text: String,
    },
    /// 模型产出的文本增量（流式）。
    AgentText {
        /// 文本增量。
        text: String,
    },
    /// 模型的思考/推理增量（流式，与正文 `AgentText` 分开的通道）。
    ///
    /// 前端默认折叠展示（`thinking...`，可展开）。语言中立：只搬运模型产出的
    /// 结构化增量，不含时钟/随机。
    AgentReasoning {
        /// 思考文本增量。
        text: String,
    },
    /// 模型请求调用工具。
    ToolCallRequested {
        /// 本轮内的调用标识。
        call_id: String,
        /// 工具名。
        tool: String,
        /// 调用参数。
        args: serde_json::Value,
    },
    /// 风险动作等待用户批准（挂起主循环）。
    ApprovalRequired {
        /// 对应的调用标识。
        call_id: String,
        /// 工具自报的风险等级。
        risk: crate::risk::RiskLevel,
        /// 审校模型的第二意见（未加载审校时为 None）。
        review: Option<String>,
    },
    /// 用户对挂起风险动作的裁决落账（批准/拒绝）。
    ///
    /// 权限门是铁律：审批决定必须成为事件日志的一部分，否则折叠状态只剩
    /// `ApprovalRequired`——重连/切页/回放会把"已批准、正在执行"错误还原成
    /// "重新弹出审批"。批准后据此把工具卡从 `pending_approval` 翻到 `running`，
    /// 长命令执行期间也有真实反馈而非永久"审批中"。语言中立：只存布尔。
    ApprovalResolved {
        /// 对应的调用标识。
        call_id: String,
        /// 是否批准（true=批准继续执行；false=拒绝，随后跟一条失败 `ToolResult`）。
        approved: bool,
    },
    /// 工具执行结果（已按摄入截断策略处理，见 docs/architecture.md §5.2）。
    ToolResult {
        /// 对应的调用标识。
        call_id: String,
        /// 工具是否执行成功。
        ok: bool,
        /// 结果文本（已截断）。
        content: String,
    },
    /// 一轮结束。
    TurnCompleted {
        /// 结束原因（自然结束 / 迭代上限 / 预算耗尽 / 用户中断）。
        reason: String,
    },
    /// 上下文预算快照（轮次边界发出，供前端画预算/KV 状态）。
    ///
    /// 语言中立 + 确定性（地基铁律：只存结构化数值，不存句子；
    /// token 数由历史估算得来，不含时钟/随机）。`used_tokens` 为近似值
    /// （字节级估算，非真实 tokenizer），前端应据实标注"近似"。
    ContextBudget {
        /// 已用 token 估算（近似）。
        used_tokens: u32,
        /// 后端上报的真实上下文长度（探测所得，见 `BackendCapabilities::n_ctx`）。
        n_ctx: u32,
    },
    /// 不可恢复错误（可恢复错误走 `ToolResult` 的 `ok: false` 喂回模型自纠错）。
    Error {
        /// 错误描述。
        message: String,
    },
}

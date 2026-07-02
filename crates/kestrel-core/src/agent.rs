//! 单线程 turn 状态机（ARCHITECTURE.md §5.1）。
//!
//! 一个 turn = 组装 prompt -> 流式推理 -> 解析工具调用 -> 权限门 ->
//! 执行工具 -> 追加结果，直到模型不再请求工具或触达迭代/预算上限。
//!
//! 铁律：
//! - 消息历史 append-only，永不原地改写（KV 前缀命中的前提）。
//! - 迭代上限 + token 预算双闸。
//! - 取消信号贯穿到子进程。
//! - 工具失败把具体错误喂回历史让模型自纠错，不静默重试。

/// turn 状态机的状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnState {
    /// context ledger 组装 prompt（前缀稳定，动态信息置尾）。
    Assembling,
    /// 流式推理中，边收边解析工具调用。
    Streaming,
    /// 风险动作挂起，等待 `Op::Approve` / `Op::Deny`。
    AwaitingApproval,
    /// 执行工具（只读并行、写状态串行）。
    ExecutingTools,
    /// 结果截断后追加进事件日志。
    AppendingResults,
    /// 一轮结束。
    Done {
        /// 结束原因。
        reason: String,
    },
}

/// agent 主循环的硬性约束（AutoGPT 无限循环的解药）。
#[derive(Debug, Clone)]
pub struct TurnLimits {
    /// 单轮最大迭代次数。
    pub max_iterations: u32,
    /// 单轮 token 预算。
    pub max_tokens: u64,
}

impl Default for TurnLimits {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            max_tokens: 65_536,
        }
    }
}

// TODO(M1): Agent 结构体 —— 持有 ports（Arc<dyn LlmBackend> 等）、
// 事件发送端（mpsc::Sender<Event>）、Op 接收端，实现 run() 主循环。

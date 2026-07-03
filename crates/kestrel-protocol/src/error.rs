//! 结构化错误分类（地基 #3 / foundations #3 / AGENTS.md §5）。
//!
//! 铁律：事件日志与前端契约**只搬运稳定的错误 code**，不搬运英文句子——句子是表现层的事，
//! 由前端按 locale 渲染（ADR-0008）。`code` 是语言中立、可测、可本地化的分类；随附的
//! `message` 是**开发向**的原始细节（英文、来自 OS / 网络 / serde 等，不本地化），前端作
//! 次要上下文展示。这样换语言不用改历史，回放 fixture 也不随语言漂移。

use serde::{Deserialize, Serialize};

/// 稳定错误分类。序列化为 `snake_case` 字符串，进事件日志（ADR-0011：只增不改）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// 后端错误：连不上模型、非 2xx 响应、流式中断。
    Backend,
    /// 工具执行中的**不可恢复**错误（可恢复错误走 `ToolResult` 的 `ok:false` 喂回模型自纠错）。
    Tool,
    /// 存储 / 事件日志错误。
    Store,
    /// 用户取消当前回合。
    Cancelled,
    /// 其他 / 未分类；同时兜前向兼容（旧代码读到新版本的未知 code 落到这里，不报错）。
    #[default]
    #[serde(other)]
    Internal,
}

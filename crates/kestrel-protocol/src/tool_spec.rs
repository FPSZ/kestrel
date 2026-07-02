//! 工具规格：注入模型上下文的工具描述。
//!
//! Token 纪律（docs/architecture.md 原则 2）：全部工具 schema 总预算 <= 1400 token，
//! 每个字段都要为它的 token 开销辩护。规格必须完全静态（前缀稳定性，原则 1）。

use serde::{Deserialize, Serialize};

/// 一个工具的静态规格。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    /// 工具名（进入模型上下文，也是注册表校验的键）。
    pub name: String,
    /// 面向模型的描述（当新员工入职文档写，但要抠 token）。
    pub description: String,
    /// 参数的 JSON Schema（保持浅层、少字段；additionalProperties 恒为 false）。
    pub parameters: serde_json::Value,
}

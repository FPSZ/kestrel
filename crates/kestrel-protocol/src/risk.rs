//! 风险分级与权限决策（docs/architecture.md §5.3）。

use serde::{Deserialize, Serialize};

/// 工具调用的风险等级，由工具根据实际参数自报。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// 只读：读文件、搜索、查状态。
    ReadOnly,
    /// 可变：写文件、改配置（工作区内）。
    Mutating,
    /// 破坏性：删除、覆盖、写系统目录。
    Destructive,
    /// 外联：任何出网行为。
    External,
}

/// 权限引擎的决策结果（deny 优先求值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// 自动放行。
    Allow,
    /// 挂起等待用户批准。
    AskUser,
    /// 拒绝（被 deny 规则命中的工具在模型看到之前就被预过滤）。
    Deny,
}

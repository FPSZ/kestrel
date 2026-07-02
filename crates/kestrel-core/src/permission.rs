//! 权限引擎：deny 优先 + 风险分级（docs/architecture.md §5.3）。
//!
//! - deny 优先求值：命中全局 deny 的工具在模型看到之前就从工具列表预过滤。
//! - 风险等级由工具按实际参数自报（[`crate::ports::Tool::risk`]）。
//! - 确认策略分档对齐 codex 的 `AskForApproval` 精神。

use kestrel_protocol::{Decision, RiskLevel};

/// 确认策略档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApprovalPolicy {
    /// 只读自动放行，其余都问。
    #[default]
    OnRequest,
    /// 只读与工作区内可变自动放行（信任工作区）。
    Auto,
    /// 所有写动作都问。
    Strict,
}

/// 权限引擎。
#[derive(Debug, Default)]
pub struct PermissionEngine {
    policy: ApprovalPolicy,
    // TODO(M1): 全局 deny 规则表（从配置加载），用于工具列表预过滤。
}

impl PermissionEngine {
    /// 以给定策略构建。
    #[must_use]
    pub fn new(policy: ApprovalPolicy) -> Self {
        Self { policy }
    }

    /// 对一次工具调用做决策（deny 规则优先，其后按风险与策略分档）。
    #[must_use]
    pub fn decide(&self, risk: RiskLevel) -> Decision {
        match (self.policy, risk) {
            (_, RiskLevel::ReadOnly) | (ApprovalPolicy::Auto, RiskLevel::Mutating) => {
                Decision::Allow
            }
            // Destructive 与 External 在任何档位都必须过用户（第一版单机个人版
            // 也不放松——OpenClaw 的教训，原则 5）。
            _ => Decision::AskUser,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destructive_always_asks_even_in_auto() {
        let engine = PermissionEngine::new(ApprovalPolicy::Auto);
        assert_eq!(engine.decide(RiskLevel::Destructive), Decision::AskUser);
        assert_eq!(engine.decide(RiskLevel::External), Decision::AskUser);
    }

    #[test]
    fn readonly_always_allowed() {
        for policy in [
            ApprovalPolicy::OnRequest,
            ApprovalPolicy::Auto,
            ApprovalPolicy::Strict,
        ] {
            let engine = PermissionEngine::new(policy);
            assert_eq!(engine.decide(RiskLevel::ReadOnly), Decision::Allow);
        }
    }
}

//! 权限引擎：deny 优先 + 风险分级（docs/architecture.md §5.3）。
//!
//! - deny 优先求值：命中全局 deny 的工具在模型看到之前就从工具列表预过滤。
//! - 风险等级由工具按实际参数自报（[`crate::ports::Tool::risk`]）。
//! - 确认策略分档对齐 codex 的 `AskForApproval` 精神。

use kestrel_protocol::{AgentMode, Decision, RiskLevel};

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
///
/// deny 优先（原则 5）：`deny_tools` 里的工具名有两道防线——
/// 组装时由 [`crate::ToolSet::deny`] 预过滤出工具列表（模型看不到，省 schema
/// token），运行时 [`decide_tool`] 再兜一层（防幻觉出被禁工具名）。
///
/// [`decide_tool`]: PermissionEngine::decide_tool
#[derive(Debug, Default)]
pub struct PermissionEngine {
    policy: ApprovalPolicy,
    /// 全局禁用的工具名（deny 优先，来自配置）。
    deny_tools: Vec<String>,
}

impl PermissionEngine {
    /// 以给定策略构建（无 deny 规则）。
    #[must_use]
    pub fn new(policy: ApprovalPolicy) -> Self {
        Self {
            policy,
            deny_tools: Vec::new(),
        }
    }

    /// 以策略 + 全局 deny 工具名构建。
    #[must_use]
    pub fn with_deny(policy: ApprovalPolicy, deny_tools: Vec<String>) -> Self {
        Self { policy, deny_tools }
    }

    /// 该工具是否被全局 deny 规则禁用。
    #[must_use]
    pub fn is_denied(&self, tool: &str) -> bool {
        self.deny_tools.iter().any(|d| d == tool)
    }

    /// 对一次具名工具调用做决策：deny 优先，其后按风险与策略分档。
    #[must_use]
    pub fn decide_tool(&self, tool: &str, risk: RiskLevel) -> Decision {
        if self.is_denied(tool) {
            return Decision::Deny;
        }
        self.decide(risk)
    }

    /// 按**本轮运行模式**（询问/全部执行/计划）裁决具名工具调用：deny 优先，
    /// 其后按模式 × 风险分档。模式来自前端 [`kestrel_protocol::Op::UserInput`]，
    /// 覆盖启动策略——UI 的"询问/全部执行/计划"三态就走这里。
    ///
    /// 铁律：无论何种模式，Destructive/External 都不会被自动放行（Auto 也必问，
    /// Plan 直接挡回）——权限门不可削弱。
    #[must_use]
    pub fn decide_tool_in_mode(&self, tool: &str, risk: RiskLevel, mode: AgentMode) -> Decision {
        if self.is_denied(tool) {
            return Decision::Deny;
        }
        match mode {
            // 计划：只读放行，其余一律挡回（模型据纠错提示只出计划、不落地）。
            AgentMode::Plan => match risk {
                RiskLevel::ReadOnly => Decision::Allow,
                _ => Decision::Deny,
            },
            // 全部执行：只读 + 工作区可变自动放行；破坏性/外联仍必问（铁律）。
            AgentMode::Auto => match risk {
                RiskLevel::ReadOnly | RiskLevel::Mutating => Decision::Allow,
                _ => Decision::AskUser,
            },
            // 询问：只读放行，其余逐个问（等价 on-request）。
            AgentMode::Ask => match risk {
                RiskLevel::ReadOnly => Decision::Allow,
                _ => Decision::AskUser,
            },
        }
    }

    /// 按风险与策略分档决策（不查 deny 名单，见 [`decide_tool`]）。
    ///
    /// [`decide_tool`]: PermissionEngine::decide_tool
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

    #[test]
    fn denied_tool_is_denied_regardless_of_risk() {
        let engine = PermissionEngine::with_deny(ApprovalPolicy::Auto, vec!["shell".to_owned()]);
        // 即便 Auto 会放行 Mutating，被 deny 的工具仍拒绝。
        assert_eq!(
            engine.decide_tool("shell", RiskLevel::Mutating),
            Decision::Deny
        );
        // 未被 deny 的只读工具照常放行。
        assert_eq!(
            engine.decide_tool("read", RiskLevel::ReadOnly),
            Decision::Allow
        );
    }

    #[test]
    fn ask_mode_asks_for_writes_allows_reads() {
        let engine = PermissionEngine::default();
        assert_eq!(
            engine.decide_tool_in_mode("read", RiskLevel::ReadOnly, AgentMode::Ask),
            Decision::Allow
        );
        assert_eq!(
            engine.decide_tool_in_mode("shell", RiskLevel::Mutating, AgentMode::Ask),
            Decision::AskUser
        );
    }

    #[test]
    fn auto_mode_allows_mutating_but_still_asks_destructive_external() {
        let engine = PermissionEngine::default();
        assert_eq!(
            engine.decide_tool_in_mode("edit", RiskLevel::Mutating, AgentMode::Auto),
            Decision::Allow
        );
        // 铁律：Auto 也不放行破坏性/外联。
        assert_eq!(
            engine.decide_tool_in_mode("shell", RiskLevel::Destructive, AgentMode::Auto),
            Decision::AskUser
        );
        assert_eq!(
            engine.decide_tool_in_mode("shell", RiskLevel::External, AgentMode::Auto),
            Decision::AskUser
        );
    }

    #[test]
    fn plan_mode_allows_only_readonly_denies_the_rest() {
        let engine = PermissionEngine::default();
        assert_eq!(
            engine.decide_tool_in_mode("search", RiskLevel::ReadOnly, AgentMode::Plan),
            Decision::Allow
        );
        assert_eq!(
            engine.decide_tool_in_mode("edit", RiskLevel::Mutating, AgentMode::Plan),
            Decision::Deny
        );
        assert_eq!(
            engine.decide_tool_in_mode("shell", RiskLevel::External, AgentMode::Plan),
            Decision::Deny
        );
    }

    #[test]
    fn deny_list_wins_in_every_mode() {
        let engine =
            PermissionEngine::with_deny(ApprovalPolicy::default(), vec!["shell".to_owned()]);
        for mode in [AgentMode::Ask, AgentMode::Auto, AgentMode::Plan] {
            assert_eq!(
                engine.decide_tool_in_mode("shell", RiskLevel::Mutating, mode),
                Decision::Deny
            );
        }
    }
}

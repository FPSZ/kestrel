//! Context Ledger：token 记账 + KV 前缀联动（docs/architecture.md §5.2）。
//!
//! 叫 ledger（账本）不叫 manager：核心职责是按后端上报的真实 `n_ctx`
//! 记账，而非拍脑袋截断。
//!
//! 三条纪律：
//! - 预算来自 `BackendCapabilities::n_ctx`，禁止硬编码。
//! - 摄入即截断：工具输出在 append 那一刻 head-tail 截断（在 `kestrel-tools`
//!   的 `truncate_head_tail` 完成，账本只负责记账，不重复截断）。
//! - 压缩少而狠：只在逼近预算（约 85%）时做一次大压缩，
//!   且由副手模型异地完成（§6.1），主脑 KV 前缀零扰动（压缩派发属 M2）。

use kestrel_protocol::Message;

/// 压缩触发阈值（已用占 `n_ctx` 的百分比）。达到即应派一次异地压缩（M2）。
const COMPACT_THRESHOLD_PCT: u64 = 85;

/// 每 token 的近似字节数。真实 tokenizer 因模型而异；此处用固定近似值
/// 保持 core 确定性（地基铁律：禁止把非确定性带进 core 事件路径）。
/// 面向预算"温度计"够用，且事件里已标注 `used_tokens` 为近似。
const BYTES_PER_TOKEN: usize = 4;

/// token 预算账本。
#[derive(Debug)]
pub struct ContextLedger {
    /// 后端真实上下文长度（探测所得）。
    n_ctx: u32,
    /// 当前已用估算。
    used: u32,
}

impl ContextLedger {
    /// 以后端探测到的真实上下文长度建账。
    #[must_use]
    pub fn new(n_ctx: u32) -> Self {
        Self { n_ctx, used: 0 }
    }

    /// 后端真实上下文长度。
    #[must_use]
    pub fn n_ctx(&self) -> u32 {
        self.n_ctx
    }

    /// 当前已用 token 估算。
    #[must_use]
    pub fn used(&self) -> u32 {
        self.used
    }

    /// 是否应触发一次异地压缩（阈值约 85%，压缩由副手完成）。
    #[must_use]
    pub fn should_compact(&self) -> bool {
        u64::from(self.used) * 100 >= u64::from(self.n_ctx) * COMPACT_THRESHOLD_PCT
    }

    /// 按当前完整历史重算已用 token（轮次边界调用，确定性、幂等）。
    ///
    /// 重算而非增量累加：历史 append-only，全量估算是 `O(history)` 的一次
    /// 纯函数运算，避免在多处 push 点分散记账、也天然与历史保持一致。
    pub fn recount(&mut self, history: &[Message]) {
        self.used = estimate_messages(history);
    }
}

/// 估算一组消息的 token 数（近似：总字节数 / `BYTES_PER_TOKEN`）。
///
/// 纯函数、确定性：仅依赖消息内容，不读时钟/环境（地基铁律 core 确定性）。
#[must_use]
pub fn estimate_messages(messages: &[Message]) -> u32 {
    let bytes: usize = messages
        .iter()
        .map(|m| {
            m.content.len()
                + m.tool_calls
                    .iter()
                    .map(|c| c.name.len() + c.arguments.to_string().len())
                    .sum::<usize>()
        })
        .sum();
    u32::try_from(bytes / BYTES_PER_TOKEN).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use kestrel_protocol::Role;

    use super::*;

    #[test]
    fn recount_is_deterministic_and_grows_with_history() {
        let mut ledger = ContextLedger::new(8192);
        assert_eq!(ledger.used(), 0);
        let history = vec![
            Message::text(Role::System, "you are a helpful agent"),
            Message::text(Role::User, "hello there, please help"),
        ];
        ledger.recount(&history);
        let first = ledger.used();
        assert!(first > 0);
        // 幂等：同一历史重算结果一致。
        ledger.recount(&history);
        assert_eq!(ledger.used(), first);
    }

    #[test]
    fn should_compact_trips_past_threshold() {
        let mut ledger = ContextLedger::new(100);
        ledger.recount(&[Message::text(Role::User, "x".repeat(4 * 84))]);
        assert!(!ledger.should_compact(), "84% must not trip");
        ledger.recount(&[Message::text(Role::User, "x".repeat(4 * 90))]);
        assert!(ledger.should_compact(), "90% must trip");
    }
}

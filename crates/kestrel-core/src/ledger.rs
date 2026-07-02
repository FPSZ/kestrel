//! Context Ledger：token 记账 + KV 前缀联动（docs/architecture.md §5.2）。
//!
//! 叫 ledger（账本）不叫 manager：核心职责是按后端上报的真实 `n_ctx`
//! 记账，而非拍脑袋截断。
//!
//! 三条纪律：
//! - 预算来自 `BackendCapabilities::n_ctx`，禁止硬编码。
//! - 摄入即截断：工具输出在 append 那一刻 head-tail 截断。
//! - 压缩少而狠：只在逼近预算（约 85%）时做一次大压缩，
//!   且由副手模型异地完成（§6.1），主脑 KV 前缀零扰动。

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

    /// 是否应触发一次异地压缩（阈值约 85%，压缩由副手完成）。
    #[must_use]
    pub fn should_compact(&self) -> bool {
        u64::from(self.used) * 100 >= u64::from(self.n_ctx) * 85
    }

    /// 记入新增 token。
    pub fn charge(&mut self, tokens: u32) {
        self.used = self.used.saturating_add(tokens);
    }
}

// TODO(M1): head-tail 截断实现（保头保尾，中间折叠为 "... [省略 N 行] ..."）。
// TODO(M2): 压缩作业派发给 crew::JobKind::Compact，产物仅在轮次边界并入。

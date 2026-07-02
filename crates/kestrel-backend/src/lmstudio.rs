//! LM Studio 后端。
//!
//! 实现要点（ARCHITECTURE.md §5.4）：
//! - `GET /api/v0/models` 查加载状态与上下文上限。
//! - JIT 加载冷启动（首请求可能数十秒）与 TTL 逐出的重试逻辑。
//! - 利用其上报的 TTFT / tok-s 统计做自适应调度。

// TODO(M1 后期): pub struct LmStudioBackend —— M1 以 llamacpp 为主，
// 本模块在 llamacpp 闭环后实现。

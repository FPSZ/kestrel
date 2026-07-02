//! `OpenAI` 兼容端点的保守兜底后端。
//!
//! 不假设任何扩展能力：无 slot、无 props 探测（`n_ctx` 取配置值）、
//! `save_cache` 为 no-op。让 Kestrel 能接任意 OpenAI 兼容服务，
//! 但本地专项优化只在 llamacpp/lmstudio 后端生效。

// TODO(M2): pub struct OpenAiCompatBackend

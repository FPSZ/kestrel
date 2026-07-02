//! # kestrel-backend
//!
//! [`kestrel_core::ports::LlmBackend`] 的实现集：本 workspace 唯一
//! 向 LLM 后端发 HTTP 的地方，也是本地专项优化的集中地
//! （docs/architecture.md §5.4）。
//!
//! ## 职责边界
//!
//! - 实现 core 的端口，不定义自己的抽象。
//! - 禁止依赖其他适配器 crate 与前端 crate。
//! - 差异化创新长在这里：slot 管理、`cache_prompt`、GBNF 注入、
//!   影子槽预热（§7）、能力探针（§5.4）——这正是 ADR-001 否决
//!   现成框架的理由，请勿引入遮蔽 HTTP 请求体的依赖。
//!
//! ## 模块导览
//!
//! | 模块 | 职责 |
//! | --- | --- |
//! | [`llamacpp`] | llama-server：--jinja 检查、slot save/restore、cache_prompt |
//! | [`lmstudio`] | LM Studio：JIT 冷启动重试、TTL、/api/v0 能力查询 |
//! | [`openai_compat`] | 任意 OpenAI 兼容端点的保守兜底 |
//! | [`probe`] | 首跑能力探针：微基准生成模型 profile |

pub mod llamacpp;
pub mod lmstudio;
pub mod openai_compat;
pub mod probe;

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

use std::sync::Arc;

use kestrel_core::ports::LlmBackend;
use kestrel_protocol::SecretString;

pub use llamacpp::LlamaCppBackend;
pub use lmstudio::LmStudioBackend;
pub use openai_compat::OpenAiCompatBackend;

/// 按配置的 `kind` 选择后端实现（组装根调用，避免 CLI / server 各写一份）。
///
/// - `"llamacpp"` -> [`LlamaCppBackend`]（`/props` 探测真实 `n_ctx`、slot 就绪）
/// - `"lmstudio"` -> [`LmStudioBackend`]（`/api/v0/models` 探测上下文长度）
/// - 其他（含 `"openai"` / `"auto"` / 空）-> [`OpenAiCompatBackend`]（保守兜底）
///
/// 兜底而非报错：本地用户换后端像换灯泡，不认识的 kind 退到最通用实现，
/// 不因一个拼写把整个 agent 拦下。
#[must_use]
pub fn build(
    kind: &str,
    base_url: impl Into<String>,
    api_key: SecretString,
    model: String,
    n_ctx: u32,
) -> Arc<dyn LlmBackend> {
    let base_url = base_url.into();
    match kind {
        "llamacpp" | "llama.cpp" | "llama_cpp" => {
            Arc::new(LlamaCppBackend::new(base_url, api_key, model, n_ctx))
        }
        "lmstudio" | "lm_studio" | "lm-studio" => {
            Arc::new(LmStudioBackend::new(base_url, api_key, model, n_ctx))
        }
        _ => Arc::new(OpenAiCompatBackend::new(base_url, api_key, model, n_ctx)),
    }
}

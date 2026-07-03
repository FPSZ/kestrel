//! llama.cpp (llama-server) 后端。
//!
//! 实现要点（调研依据见 docs/architecture.md §5.4）：
//! - 启动探测 `GET /props`：读真实 `n_ctx`（context ledger 记账依据，禁止硬编码）。
//! - 流式补全复用 [`OpenAiCompatBackend`]（llama-server 暴露 OpenAI 兼容
//!   `/v1/chat/completions`），请求恒带 `cache_prompt: true`；system/tools 逐字节稳定。
//! - `save_cache`（`/slots/{id}?action=save`）支撑会话切换与影子槽预热——涉及写状态、
//!   跨量化稳定性存疑（M2 高风险 spike），此处先 no-op，待可行性验证后再启用。

use kestrel_core::CoreError;
use kestrel_core::ports::{CompletionStream, LlmBackend};
use kestrel_protocol::{BackendCapabilities, CompletionRequest, SessionId};

use crate::openai_compat::OpenAiCompatBackend;

/// llama-server 后端：流式走 OpenAI 兼容层，探测走 llama.cpp 专属 `/props`。
#[derive(Debug, Clone)]
pub struct LlamaCppBackend {
    inner: OpenAiCompatBackend,
    base_url: String,
    api_key: String,
    model: String,
    n_ctx_fallback: u32,
    http: reqwest::Client,
}

impl LlamaCppBackend {
    /// 构造。`base_url` 不含 `/v1`（如 `http://127.0.0.1:8080`）。
    /// `n_ctx_fallback` 在 `/props` 探测失败时兜底。
    #[must_use]
    pub fn new(base_url: impl Into<String>, api_key: String, model: String, n_ctx: u32) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_owned();
        Self {
            inner: OpenAiCompatBackend::new(
                base_url.clone(),
                api_key.clone(),
                model.clone(),
                n_ctx,
            ),
            base_url,
            api_key,
            model,
            n_ctx_fallback: n_ctx,
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl LlmBackend for LlamaCppBackend {
    async fn stream(&self, req: CompletionRequest) -> Result<CompletionStream, CoreError> {
        self.inner.stream(req).await
    }

    async fn probe(&self) -> Result<BackendCapabilities, CoreError> {
        // GET /props 读真实上下文长度；任何失败都优雅回退到配置值（后端可能未启动）。
        let url = format!("{}/props", self.base_url);
        let mut builder = self.http.get(&url);
        if !self.api_key.is_empty() {
            builder = builder.bearer_auth(&self.api_key);
        }
        let n_ctx = match builder.send().await {
            Ok(resp) if resp.status().is_success() => resp
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|v| parse_n_ctx(&v))
                .unwrap_or(self.n_ctx_fallback),
            _ => self.n_ctx_fallback,
        };
        Ok(BackendCapabilities {
            n_ctx,
            // llama-server 需 --jinja 才向模型暴露工具。此处假定已开启；未开时模型
            // 看不见工具是最高频事故——由能力探针（M3）实测判定，此处不猜。
            native_tool_calls: true,
            slot_persistence: true,
            model_id: self.model.clone(),
        })
    }

    async fn save_cache(&self, _session: &SessionId) -> Result<(), CoreError> {
        // slot 序列化跨量化稳定性存疑（M2 影子槽 spike），未验证前不写状态：no-op。
        Ok(())
    }
}

/// 从 `/props` 响应里提取 `n_ctx`。llama.cpp 把它放在
/// `default_generation_settings.n_ctx`，老版本可能在顶层 `n_ctx`。
fn parse_n_ctx(v: &serde_json::Value) -> Option<u32> {
    v.get("default_generation_settings")
        .and_then(|g| g.get("n_ctx"))
        .or_else(|| v.get("n_ctx"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_n_ctx_from_generation_settings() {
        let v = serde_json::json!({
            "default_generation_settings": { "n_ctx": 32768 },
            "total_slots": 1
        });
        assert_eq!(parse_n_ctx(&v), Some(32768));
    }

    #[test]
    fn parse_n_ctx_from_top_level_fallback() {
        let v = serde_json::json!({ "n_ctx": 8192 });
        assert_eq!(parse_n_ctx(&v), Some(8192));
    }

    #[test]
    fn parse_n_ctx_absent_is_none() {
        let v = serde_json::json!({ "model": "x" });
        assert_eq!(parse_n_ctx(&v), None);
    }
}

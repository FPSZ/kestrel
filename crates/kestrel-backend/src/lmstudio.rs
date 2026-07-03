//! LM Studio 后端。
//!
//! 实现要点（docs/architecture.md §5.4）：
//! - `GET /api/v0/models` 查加载状态与上下文上限（`loaded_context_length`
//!   优先，回退 `max_context_length`）。
//! - 流式补全复用 [`OpenAiCompatBackend`]（LM Studio 暴露 OpenAI 兼容端点）。
//! - JIT 冷启动（首请求可能数十秒）与 TTL 逐出的自适应重试属后续优化，
//!   此处先给出探测 + 流式的可用闭环。

use kestrel_core::CoreError;
use kestrel_core::ports::{CompletionStream, LlmBackend};
use kestrel_protocol::{BackendCapabilities, CompletionRequest, SessionId};

use crate::openai_compat::OpenAiCompatBackend;

/// LM Studio 后端：流式走 OpenAI 兼容层，探测走 LM Studio 专属 `/api/v0/models`。
#[derive(Debug, Clone)]
pub struct LmStudioBackend {
    inner: OpenAiCompatBackend,
    base_url: String,
    api_key: String,
    model: String,
    n_ctx_fallback: u32,
    http: reqwest::Client,
}

impl LmStudioBackend {
    /// 构造。`base_url` 不含 `/v1`（如 `http://127.0.0.1:1234`）。
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
impl LlmBackend for LmStudioBackend {
    async fn stream(&self, req: CompletionRequest) -> Result<CompletionStream, CoreError> {
        self.inner.stream(req).await
    }

    async fn probe(&self) -> Result<BackendCapabilities, CoreError> {
        // GET /api/v0/models 读加载模型的上下文长度；失败优雅回退配置值。
        let url = format!("{}/api/v0/models", self.base_url);
        let mut builder = self.http.get(&url);
        if !self.api_key.is_empty() {
            builder = builder.bearer_auth(&self.api_key);
        }
        let n_ctx = match builder.send().await {
            Ok(resp) if resp.status().is_success() => resp
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|v| parse_context_len(&v, &self.model))
                .unwrap_or(self.n_ctx_fallback),
            _ => self.n_ctx_fallback,
        };
        Ok(BackendCapabilities {
            n_ctx,
            native_tool_calls: true,
            slot_persistence: false,
            model_id: self.model.clone(),
        })
    }

    async fn save_cache(&self, _session: &SessionId) -> Result<(), CoreError> {
        // LM Studio 无 slot 持久化概念：no-op。
        Ok(())
    }
}

/// 从 `/api/v0/models` 响应提取上下文长度。优先匹配配置的 model id，
/// 取不到则退到第一个已加载模型；`loaded_context_length` 优先于 `max_context_length`。
fn parse_context_len(v: &serde_json::Value, model: &str) -> Option<u32> {
    let arr = v.get("data").and_then(|d| d.as_array())?;
    let pick = arr
        .iter()
        .find(|m| m.get("id").and_then(serde_json::Value::as_str) == Some(model))
        .or_else(|| {
            arr.iter()
                .find(|m| m.get("state").and_then(serde_json::Value::as_str) == Some("loaded"))
        })
        .or_else(|| arr.first())?;
    pick.get("loaded_context_length")
        .or_else(|| pick.get("max_context_length"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_prefers_loaded_context_of_matching_model() {
        let v = serde_json::json!({
            "data": [
                { "id": "other", "loaded_context_length": 2048 },
                { "id": "qwen3", "state": "loaded",
                  "loaded_context_length": 16384, "max_context_length": 32768 }
            ]
        });
        assert_eq!(parse_context_len(&v, "qwen3"), Some(16384));
    }

    #[test]
    fn parse_falls_back_to_max_context() {
        let v = serde_json::json!({
            "data": [ { "id": "qwen3", "max_context_length": 8192 } ]
        });
        assert_eq!(parse_context_len(&v, "qwen3"), Some(8192));
    }

    #[test]
    fn parse_empty_is_none() {
        let v = serde_json::json!({ "data": [] });
        assert_eq!(parse_context_len(&v, "qwen3"), None);
    }
}

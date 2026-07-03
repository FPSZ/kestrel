//! OpenAI 兼容端点的流式后端。
//!
//! M1 的主力后端：llama-server 与 LM Studio 都暴露 OpenAI 兼容的
//! `/v1/chat/completions`，本实现同时覆盖两者。llama.cpp 专属优化
//! （/props 探测、slot save/restore）留给 [`crate::llamacpp`]。
//!
//! 前缀稳定性：请求恒带 `cache_prompt: true`（llama.cpp 扩展字段，
//! 其他后端忽略），配合 append-only 的消息历史复用 KV 缓存。

use futures::StreamExt;
use kestrel_core::CoreError;
use kestrel_core::ports::{CompletionStream, LlmBackend};
use kestrel_protocol::{
    BackendCapabilities, CompletionChunk, CompletionRequest, Message, Role, SessionId,
};

/// OpenAI 兼容流式后端。
#[derive(Debug, Clone)]
pub struct OpenAiCompatBackend {
    base_url: String,
    api_key: String,
    model: String,
    n_ctx: u32,
    http: reqwest::Client,
}

impl OpenAiCompatBackend {
    /// 构造。`base_url` 不含 `/v1`（如 `http://127.0.0.1:8080`）。
    #[must_use]
    pub fn new(base_url: impl Into<String>, api_key: String, model: String, n_ctx: u32) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            api_key,
            model,
            n_ctx,
            http: reqwest::Client::new(),
        }
    }

    fn build_body(&self, req: &CompletionRequest) -> serde_json::Value {
        let messages: Vec<serde_json::Value> = req.messages.iter().map(to_openai_message).collect();
        let tools: Vec<serde_json::Value> = req
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
            "cache_prompt": true,
            // Qwen3 等推理模型：由本轮开关决定是否开思考。开时 <think> 走独立的
            // reasoning_content 通道（否则带 tools 时模型常把推理内联进正文）；关时
            // 直接答、省延迟。不识别此字段的模型忽略之。
            "chat_template_kwargs": { "enable_thinking": req.think },
        });
        if !tools.is_empty() {
            body["tools"] = serde_json::Value::Array(tools);
        }
        body
    }
}

#[async_trait::async_trait]
impl LlmBackend for OpenAiCompatBackend {
    async fn stream(&self, req: CompletionRequest) -> Result<CompletionStream, CoreError> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let mut builder = self.http.post(&url).json(&self.build_body(&req));
        if !self.api_key.is_empty() {
            builder = builder.bearer_auth(&self.api_key);
        }
        let resp = builder
            .send()
            .await
            .map_err(|e| CoreError::Backend(format!("connect {url}: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CoreError::Backend(format!("http {status}: {text}")));
        }

        // 后台任务解析 SSE，chunk 经 channel 上抛；用 unfold 把 channel 包成 Stream。
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<CompletionChunk, CoreError>>(32);
        tokio::spawn(async move {
            let mut byte_stream = resp.bytes_stream();
            let mut buf = String::new();
            let mut acc = ToolCallAccumulator::default();
            while let Some(chunk) = byte_stream.next().await {
                let bytes = match chunk {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx
                            .send(Err(CoreError::Backend(format!("stream: {e}"))))
                            .await;
                        return;
                    }
                };
                buf.push_str(&String::from_utf8_lossy(&bytes));
                // 按 SSE 事件（以空行分隔）逐条处理，保留未完整的尾部。
                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim_end_matches('\r').to_owned();
                    buf.drain(..=pos);
                    let Some(data) = line.strip_prefix("data: ") else {
                        continue;
                    };
                    if data == "[DONE]" {
                        acc.flush(&tx).await;
                        let _ = tx.send(Ok(CompletionChunk::Done)).await;
                        return;
                    }
                    if let Some(true) = handle_sse_data(data, &mut acc, &tx).await {
                        return;
                    }
                }
            }
            // 流自然结束（无显式 [DONE]）。
            acc.flush(&tx).await;
            let _ = tx.send(Ok(CompletionChunk::Done)).await;
        });

        let stream = futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        });
        Ok(Box::pin(stream))
    }

    async fn probe(&self) -> Result<BackendCapabilities, CoreError> {
        // M1：返回配置值。llama.cpp 的 /props 实测探测留给 llamacpp 后端。
        Ok(BackendCapabilities {
            n_ctx: self.n_ctx,
            native_tool_calls: true,
            slot_persistence: false,
            model_id: self.model.clone(),
        })
    }

    async fn save_cache(&self, _session: &SessionId) -> Result<(), CoreError> {
        // 通用 OpenAI 端点无 slot 概念：no-op。
        Ok(())
    }
}

/// 把内部 [`Message`] 映射为 OpenAI 线格式。
fn to_openai_message(m: &Message) -> serde_json::Value {
    let role = match m.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    };
    // 有粘贴图片时，content 走 OpenAI 多模态数组（text part + 若干 image_url part）；
    // 否则保持纯字符串（文本模型与旧行为一致）。
    let content = if m.images.is_empty() {
        serde_json::Value::String(m.content.clone())
    } else {
        let mut parts = Vec::new();
        if !m.content.is_empty() {
            parts.push(serde_json::json!({ "type": "text", "text": m.content }));
        }
        for url in &m.images {
            parts.push(serde_json::json!({ "type": "image_url", "image_url": { "url": url } }));
        }
        serde_json::Value::Array(parts)
    };
    let mut v = serde_json::json!({ "role": role, "content": content });
    if !m.tool_calls.is_empty() {
        v["tool_calls"] = serde_json::Value::Array(
            m.tool_calls
                .iter()
                .map(|tc| {
                    serde_json::json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            // OpenAI 要求 arguments 为 JSON 字符串而非对象。
                            "arguments": tc.arguments.to_string(),
                        }
                    })
                })
                .collect(),
        );
    }
    if let Some(id) = &m.tool_call_id {
        v["tool_call_id"] = serde_json::Value::String(id.clone());
    }
    v
}

/// 流式工具调用分片累加器（按 index 聚合 id/name/arguments）。
#[derive(Default)]
struct ToolCallAccumulator {
    slots: Vec<PartialCall>,
}

#[derive(Default, Clone)]
struct PartialCall {
    id: String,
    name: String,
    arguments: String,
}

impl ToolCallAccumulator {
    fn slot(&mut self, index: usize) -> &mut PartialCall {
        if index >= self.slots.len() {
            self.slots.resize(index + 1, PartialCall::default());
        }
        &mut self.slots[index]
    }

    /// 把累积的工具调用拼装成完整 chunk 上抛（在流结束时调用）。
    async fn flush(&mut self, tx: &tokio::sync::mpsc::Sender<Result<CompletionChunk, CoreError>>) {
        for slot in std::mem::take(&mut self.slots) {
            if slot.name.is_empty() {
                continue;
            }
            let args = if slot.arguments.trim().is_empty() {
                serde_json::json!({})
            } else {
                serde_json::from_str(&slot.arguments)
                    .unwrap_or(serde_json::Value::String(slot.arguments.clone()))
            };
            let chunk = CompletionChunk::ToolCall {
                call_id: slot.id.clone(),
                tool: slot.name.clone(),
                args,
            };
            let _ = tx.send(Ok(chunk)).await;
        }
    }
}

/// 处理一条 SSE data 负载。返回 `Some(true)` 表示应终止流。
async fn handle_sse_data(
    data: &str,
    acc: &mut ToolCallAccumulator,
    tx: &tokio::sync::mpsc::Sender<Result<CompletionChunk, CoreError>>,
) -> Option<bool> {
    let json: serde_json::Value = serde_json::from_str(data).ok()?;
    let choice = json.get("choices")?.get(0)?;
    let delta = choice.get("delta")?;

    if let Some(text) = delta.get("content").and_then(|c| c.as_str())
        && !text.is_empty()
    {
        let send = tx
            .send(Ok(CompletionChunk::Text {
                delta: text.to_owned(),
            }))
            .await;
        if send.is_err() {
            return Some(true);
        }
    }

    // 思考通道：不同后端用 `reasoning_content`（llama.cpp / DeepSeek 风格）
    // 或 `reasoning`；两者都收，作为独立的 Reasoning 增量上抛（与正文分开）。
    if let Some(rtext) = delta
        .get("reasoning_content")
        .or_else(|| delta.get("reasoning"))
        .and_then(|c| c.as_str())
        && !rtext.is_empty()
    {
        let send = tx
            .send(Ok(CompletionChunk::Reasoning {
                delta: rtext.to_owned(),
            }))
            .await;
        if send.is_err() {
            return Some(true);
        }
    }

    if let Some(calls) = delta.get("tool_calls").and_then(|c| c.as_array()) {
        for call in calls {
            let index = call
                .get("index")
                .and_then(serde_json::Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(0);
            let slot = acc.slot(index);
            if let Some(id) = call.get("id").and_then(|v| v.as_str()) {
                id.clone_into(&mut slot.id);
            }
            if let Some(func) = call.get("function") {
                if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                    slot.name.push_str(name);
                }
                if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                    slot.arguments.push_str(args);
                }
            }
        }
    }

    Some(false)
}

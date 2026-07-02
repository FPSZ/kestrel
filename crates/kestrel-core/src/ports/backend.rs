//! LLM 后端端口。

use futures::stream::BoxStream;
use kestrel_protocol::{BackendCapabilities, CompletionChunk, CompletionRequest, SessionId};

use crate::CoreError;

/// 补全增量流（有序、可取消）。
pub type CompletionStream = BoxStream<'static, Result<CompletionChunk, CoreError>>;

/// LLM 后端端口。实现方：`kestrel-backend`（llamacpp / lmstudio / `openai_compat`）。
///
/// 实现纪律：
/// - 发送请求时保证 system prompt 与工具规格逐字节稳定（前缀稳定性，原则 1）。
/// - 流式工具调用分片在 backend 内拼装完整后再上抛（core 不理解分片）。
#[async_trait::async_trait]
pub trait LlmBackend: Send + Sync {
    /// 流式补全。
    async fn stream(&self, req: CompletionRequest) -> Result<CompletionStream, CoreError>;

    /// 探测后端能力（真实 `n_ctx`、原生工具调用、slot 持久化）。
    async fn probe(&self) -> Result<BackendCapabilities, CoreError>;

    /// 保存会话的 KV 状态（llama.cpp `/slots/{id}?action=save`）。
    /// 不支持的后端实现为 no-op 并返回 Ok。
    async fn save_cache(&self, session: &SessionId) -> Result<(), CoreError>;
}

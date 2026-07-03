//! LLM 补全的请求/响应类型：core 与 backend 之间的数据契约。

use serde::{Deserialize, Serialize};

use crate::message::Message;
use crate::tool_spec::ToolSpec;

/// 一次补全请求。
///
/// 前缀稳定性（原则 1）：`messages` 的头部（system + 早期历史）与 `tools`
/// 在会话生命周期内保持逐字节稳定，新内容只追加到尾部。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// 静态工具规格（顺序固定）。
    pub tools: Vec<ToolSpec>,
    /// 对话消息（含 system 头，append-only）。
    pub messages: Vec<Message>,
    /// 是否启用思考通道（enable_thinking）。由本轮用户输入的开关决定。
    pub think: bool,
    /// 单次生成的 token 上限（`None`=不设限）。掐断失控生成（如推理模型的思考死循环），
    /// 由后端映射到线缆的 `max_tokens`。
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

/// 流式补全的增量。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CompletionChunk {
    /// 文本增量。
    Text {
        /// 增量文本。
        delta: String,
    },
    /// 思考/推理增量（如 Qwen3 的 `reasoning_content`，与正文 Text 分开的通道）。
    Reasoning {
        /// 增量思考文本。
        delta: String,
    },
    /// 完整的工具调用（backend 负责把流式分片拼装成完整调用再上抛）。
    ToolCall {
        /// 本轮内的调用标识。
        call_id: String,
        /// 工具名。
        tool: String,
        /// 调用参数。
        args: serde_json::Value,
    },
    /// 流结束。
    Done,
}

/// 后端能力探测结果（llama.cpp `/props`、LM Studio `/api/v0/models`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendCapabilities {
    /// 真实上下文长度（context ledger 的记账依据，禁止硬编码）。
    pub n_ctx: u32,
    /// 是否支持原生工具调用（llama.cpp 需 --jinja）。
    pub native_tool_calls: bool,
    /// 是否支持 slot 保存/恢复（影子槽预热的前提）。
    pub slot_persistence: bool,
    /// 模型标识（能力探针 profile 的键）。
    pub model_id: String,
}

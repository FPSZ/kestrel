//! llama.cpp (llama-server) 后端。
//!
//! 实现要点（调研依据见 ARCHITECTURE.md §5.4）：
//! - 启动探测 `GET /props`：`chat_template`、`n_ctx`；未开 `--jinja` 时给出
//!   明确告警（模型看不见工具是最高频配置事故）。
//! - 请求恒带 `cache_prompt: true`；system/tools 逐字节稳定。
//! - `POST /slots/{id}?action=save|restore` 支撑会话切换与影子槽预热。
//! - 工具调用走原生 lazy grammar；无原生支持的模型按 profile 降级为
//!   Hermes-XML 提示词或 `json_schema` 约束。

// TODO(M1): pub struct LlamaCppBackend { base_url, http: reqwest::Client, ... }
//           impl LlmBackend for LlamaCppBackend

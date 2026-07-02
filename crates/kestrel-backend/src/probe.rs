//! 首跑能力探针（docs/architecture.md §5.4、§7）。
//!
//! 接入新模型时跑约 30 秒微基准：
//! 1. 原生工具调用格式可靠性（N 次固定任务的成功率）
//! 2. SEARCH/REPLACE 编辑成功率
//! 3. 指令遵循
//!
//! 产出 `profiles/<model>.toml`：工具调用协议档位
//! （native / hermes-xml / `json_schema` 约束兜底）与编辑格式
//! （search-replace / whole-file）。换模型零配置。

// TODO(M3): pub async fn probe_model(backend: &dyn LlmBackend) -> ModelProfile

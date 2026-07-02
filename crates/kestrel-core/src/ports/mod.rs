//! 端口 trait：core 与外部世界的全部接触面。
//!
//! 只在"确定会有第二个实现"的边界建端口（docs/architecture.md §3.3）：
//!
//! 1. [`LlmBackend`] —— llama.cpp / LM Studio / `OpenAI` 兼容兜底
//! 2. [`Tool`] —— 每个内置工具一个实现
//! 3. [`Store`] —— JSONL 事件日志 / 内存实现（测试用）
//!
//! 第四个边界（前端）不需要 trait：core 暴露 Op 入口与 Event 流，
//! 前端只是事件的渲染器。

mod backend;
mod store;
mod tool;

pub use backend::{CompletionStream, LlmBackend};
pub use store::Store;
pub use tool::{Tool, ToolCtx, ToolOutput};

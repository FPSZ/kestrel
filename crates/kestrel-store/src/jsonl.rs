//! JSONL 事件日志：Store 端口的默认实现。
//!
//! - 一个会话一个 `.jsonl` 文件，一行一个 Event，只追加。
//! - resume = 重放全部事件重建状态（状态 = fold(events)，ADR-002）。
//! - 回放测试直接消费同一格式（§7 Replay Harness）。

// TODO(M1): pub struct JsonlStore { root: PathBuf } impl Store for JsonlStore

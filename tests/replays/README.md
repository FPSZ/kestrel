# 回放测试 Fixtures

录制的会话事件日志（`.jsonl`），用于确定性回放测试
（docs/architecture.md §7 Replay Harness）：LLM 响应作为 fixture，
agent 的确定性外壳（权限、截断、压缩、编辑解析）无模型、毫秒级回归。

- 文件即 `kestrel-store` 的 JSONL 事件日志格式，无独立 schema。
- 录制方式与断言 DSL 在 M3 定稿。

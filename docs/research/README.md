# 调研报告归档

架构文档中所有结论的证据基础。报告保留调研当时的数据与链接，不回头改写；
后续新调研按 `YYYY-MM-主题.md` 追加。

| 报告 | 主题 | 支撑的设计 |
| --- | --- | --- |
| [2026-07-rust-agent-landscape.md](2026-07-rust-agent-landscape.md) | Rust 生态 agent 竞品架构（goose / codex-rs / rig / swiftide） | 架构风格选型、workspace 结构、端口设计 |
| [2026-07-local-backend-engineering.md](2026-07-local-backend-engineering.md) | llama.cpp / LM Studio 的 agent 相关能力与小模型工具调用可靠性 | 前缀稳定性原则、backend 层、能力探针、上下文账本 |
| [2026-07-heavy-agent-lessons.md](2026-07-heavy-agent-lessons.md) | 重型 agent（OpenHands / Claude Code / aider / AutoGPT / OpenClaw）的取舍教训 | 设计原则 1-7、权限系统、事件日志、编辑工具设计 |

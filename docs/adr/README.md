# 架构决策记录（ADR）

记录被认真考虑过的备选方案、否决理由与"重开条件"。
将来任何人（包括我们自己）质疑选型时，先读这里。

## 索引

| 编号 | 决策 | 状态 |
| --- | --- | --- |
| [0001](0001-language-rust.md) | 语言选型：Rust（对比 TypeScript+Bun / Python） | Accepted |
| [0002](0002-style-library-core-event-stream.md) | 架构风格：库核心 + 薄适配器 + 事件流 | Accepted |
| [0003](0003-rejected-alternatives.md) | 已否决方案速查（事件总线 / 纯 MCP / rig-core / Go / 插件市场） | Accepted |
| [0004](0004-inverted-cost-model.md) | 成本模型反转：为"token 免费、延迟贵"设计（投机代理 / 夜班） | Accepted |

## 约定

- 编号递增，永不删除；被推翻的决策标记 `Superseded by NNNN`，原文保留。
- 每份 ADR 必须写明：背景、备选方案对比、裁决理由、被否决方案的最强论点（诚实记录）、重开条件。
- 状态取值：`Proposed` / `Accepted` / `Superseded by NNNN`。

# ADR-0001 语言选型：Rust（对比 TypeScript+Bun / Python）

| | |
| --- | --- |
| 状态 | Accepted |
| 日期 | 2026-07-02 |

## 背景

初始偏好 Rust 出于直觉（"高性能语言"）。但 agent 外壳的性能瓶颈永远在模型推理——性能不构成理由，必须用真实理由重新裁决。三个候选各自的最强形态对比如下。

## 对比

| 维度（权重来自项目价值观） | Rust | TypeScript + Bun | Python + asyncio |
| --- | --- | --- | --- |
| 边界强制（高——"边界明确"是硬要求） | crate 私有性由编译器强制，最强 | eslint-boundaries / project references，靠工具链自觉，中 | 约定 + mypy，最弱 |
| 开源终态质量（高——"企业级、不是玩具"） | 单 exe 几 MB、无运行时、常驻内存最小 | `bun build --compile` 单 exe（约几十 MB），良 | 分发最差（pipx/uv），装机门槛高 |
| 迭代速度（中——第一版是个人验证） | 慢（借用检查、编译等待） | 快 | 最快 |
| 本地推理生态（中） | 自写 HTTP 客户端——但这恰是差异化所在（见下） | LM Studio SDK / MCP SDK 原生 TS | llama-cpp-python / HF / smolagents 最全 |
| v2 WebUI 全栈统一（低——v2 才需要） | 前后端两套语言，用 ts-rs 从 protocol crate 生成 TS 类型可补 | 一门语言共享 Event/Op 类型，最优 | 前后端割裂 |
| 社区先例与贡献者画像（中） | codex-rs / goose，本赛道最强先例；吸引在乎本地性能的贡献者 | opencode / nanocoder | aider / Open Interpreter |

## 裁决：Rust

定盘的三条理由：

1. **项目的身份是"打磨的开源终态"，不是"快速原型"。** 本项目的每一条价值观（企业级、精致、边界、不是玩具）都在给终态加权。迭代速度的劣势是暂时的、付一次的；边界与分发的优势是永久的、复利的。
2. **我们的差异化恰好长在最底层。** 全部创新（影子槽、前缀稳定、slot 管理、异地压缩）都要求对 HTTP 请求体和缓存状态的逐字节控制——TS/Python 的生态优势（现成 SDK）在这里反而是遮蔽层。生态帮不上忙的地方，生态就不是论据。
3. **机组 = 进程编排 + 消息通道，正是 tokio 的主场。** 模型全部跑在外部进程（llama-server），三种语言在这里都只是发 HTTP，Python 的 ML 生态优势落空。

## 被否决方案的最强论点（诚实记录）

- TS+Bun 的"CLI/后端/WebUI 一套类型"在 v2 会真实地痛——缓解措施是 `kestrel-protocol` 用 ts-rs 自动导出 TS 类型定义，把类型统一保住八成。
- Python 的"最快验证"在 M1 也真实——接受这个代价，用 LLM 辅助开发抵消一部分。

## 重开条件（满足任一即重议）

- 记忆/检索层需要进程内跑嵌入或重 ML 依赖（而非独立 llama-server 进程）→ Python 权重上升。
- WebUI 提前成为主要交互面（朋友们真的来用了且 TUI 沦为次要）→ TS 权重上升。
- M1 结束时 Rust 开发摩擦显著超出预期 → 全面重议。

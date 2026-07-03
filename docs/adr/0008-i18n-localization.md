# ADR-0008 本地化（i18n）：表现层本地化 + 语言中立的事件日志

| | |
| --- | --- |
| 状态 | Accepted |
| 日期 | 2026-07-03 |

## 背景

用户要求把 i18n 尽早锁进规范：趁代码还小，避免"做了很久再换"要重构一整轮硬编码字符串。
核心张力有三处，决定了 i18n 不能随手做：

1. **事件日志是持久状态且是测试基座**。`state = fold(events)`，且回放 harness 用事件日志当
   fixture。若事件里存的是渲染好的中文/英文句子，则：换语言要改写历史、回放 fixture 变得
   语言相关、core 不再语言无关——同时违背铁律 4（append-only）与"确定性回放"。
2. **模型侧 prompt 受铁律 1/2 约束**。system prompt、工具 schema 是逐字节稳定 + token 预算
   ≤2.5k 的手工优化资产。按 UI 语言切换它们会打碎 KV 前缀、触发 re-prefill、并可能超预算。
3. **core 零 IO**。读 locale、格式化日期这类带环境依赖的事，本就不该进 core。

## 决策

### 1. 本地化只发生在表现层；事件日志语言中立

core 与事件日志**只存稳定 code / key + 结构化参数，绝不存预渲染句子**。把 `Event`/`Error`
翻译成人话是**前端在渲染那一刻**的事。好处：一条会话可用任意语言渲染、换语言无需改历史、
回放 fixture 与语言无关（确定性）。

### 2. 模型侧 wire 内容豁免 i18n，保持规范英文 + 字节稳定

system prompt / 工具 schema / GBNF / 发给模型的角色标识**永远英文、逐字节稳定**，不随 UI
语言切换。**UI 语言与模型 prompt 语言解耦**。用户的聊天输入照常原样追加到消息尾部（用户
内容，不碰前缀）。本地模型英文能力通常也最强，这不是妥协而是正解。

### 3. 结构化、可本地化的错误

core 产出 `ErrorCode`（稳定枚举）+ 结构化 `params`，**不产出句子**。这既直接支撑 i18n，也让
错误机器可读、可断言、可在回放里稳定比对。用户看到的报错 = 前端按 code+params 查 catalog 渲染。

### 4. 每个前端一套"用户可见文本"catalog

- `console`（React）：轻量 `t(key, params)` + JSON catalog（`console/src/i18n/{en-US,zh-CN}.json`）。
  暂不引重型库；复数/性别等复杂需求出现再升级（见重开条件）。
- `kestrel-cli`：用户可见的 TUI/REPL 文本走 catalog。
- **开发日志（`tracing`）保持英文、不进 catalog**——它是给开发者看的，不是用户可见文本。
  这是对 AGENTS.md §5"日志英文"的细化：开发日志英文，用户可见文本本地化。

### 5. key 与格式化

- key = **稳定英文语义 ID**（与语言解耦，非某语言的原文）。en-US 作回退；缺失时
  回退 en-US，再回退 key 本身（可见，便于发现漏翻）。
- 日期/数字/相对时间走 **locale 感知格式化**（JS `Intl` / Rust 等价物），绝不手工拼接。
  时间戳 UTC 存、边缘格式化（见 [foundations.md](../planning/foundations.md)）。

### 6. 强制（不是自觉）

CI 拦"用户可见硬编码字符串"：`console` 用 ESLint `no-literal-string` 一类规则；CLI 侧用
约定 + 评审。把"不许硬编码"变成门禁而非口号。

### 7. 范围边界（明确什么不 i18n）

| 不本地化（英文 / 中文，见 AGENTS §5） | 本地化（走 catalog） |
| --- | --- |
| 代码标识符、注释、commit、开发日志（英文） | WebUI 界面文本 |
| 模型侧 wire：system prompt / 工具 schema / GBNF（英文，字节稳定） | CLI 用户可见 TUI/REPL 文本 |
| 设计文档（中文） | 由事件 code+params 在边缘渲染的用户可见错误 / 权限提示 |

## 被否决方案的最强论点（诚实记录）

- **事件里直接存本地化句子**（最省事，渲染直接打印）：否决——回放 fixture 变语言相关、
  换语言要改写历史、core 不再语言中立，直接违背 `state=fold(events)` 的确定性根基。
- **连模型 prompt 也随 UI 语言本地化**（对非英语用户"更自然"）：否决——打碎 KV 前缀触发
  re-prefill、涨 token 可能超 2.5k、且本地模型英文最强。真要做须另立 ADR，且按会话字节稳定，
  绝不由 UI 语言开关触发。
- **一上来就引重型 i18n 框架**（react-i18next / Fluent 全家桶）：暂不——小应用先用轻量
  `t()`+JSON 控制依赖与包体；复杂本地化需求出现再升级，不提前付费。

## 重开条件

- 需要复数/性别/上下文变体等复杂本地化 -> 引入 Fluent（Rust）/ i18next（TS）。
- 某模型在某语言下明显更强、想本地化模型 prompt -> 另立 ADR，按会话字节稳定，权衡 KV/token。
- 新增 RTL 语言（阿拉伯语/希伯来语）-> 补 CSS logical properties + `dir` 处理，catalog 已就绪。

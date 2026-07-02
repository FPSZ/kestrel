# Rust 生态 AI Agent 竞品架构调研

数据采集日期：2026-07-02（星标数、提交活跃度均为当日 GitHub API 实测值）。

## 1. Block goose — [github.com/block/goose](https://github.com/block/goose)

**状态**：50,559 stars，活跃，主语言 Rust。开源可扩展 AI Agent。

### 总体架构

**分层的 library-first 架构 + MCP 为中心的插件系统**。核心逻辑全部在库 crate `goose` 中，CLI、HTTP server（桌面端后端）、SDK 都是其薄壳消费者。Agent 内部是异步流式（`BoxStream<AgentEvent>`）+ channel 事件驱动（`tokio::select!` 复用工具消息流与 `ActionRequired` 审批流）。

### Crate 分解（crates/ 下 13 个）

- `goose` — 核心库：agents/（agent.rs、extension_manager、tool_execution、subagent_execution_tool、tool_confirmation_router）、providers/（50+ 文件）、permission/、session/、recipe/、scheduler、skills/、security/、context_mgmt/
- `goose-cli` / `goose-server`（桌面 App 的后端守护进程，Electron UI）
- `goose-mcp` — 内置扩展 = 进程内 MCP server（computercontroller、memory、tutorial 等）
- `goose-providers` / `goose-provider-types` — provider 实现与 trait/类型
- `goose-local-inference` — 本地推理：llamacpp/、mlx.rs、HF 模型下载、tool_emulation.rs / tool_parsing.rs（给无原生 function-calling 的本地模型做 toolshim）
- 其余：goose-acp-macros、goose-sdk(-types)、goose-download-manager、goose-test

### Agent 循环

`Agent::reply(user_message, session_config, cancel_token) -> BoxStream<AgentEvent>`（agent.rs 约 3600 行）→ reply_internal 内循环：provider 流式产出 → 权限检查 → dispatch_tool_call 并行分发到 ExtensionManager（即各 MCP client）→ 结果拼回消息继续下一轮。工具流用 `tokio::select!` 同时 yield `ToolStreamItem::Message` 与 `ToolStreamItem::ActionRequired`（用户审批请求内联在流里）。支持子 agent、recipe、定时任务。

### 工具/扩展机制：纯 MCP

`ExtensionConfig` enum：Stdio（子进程 MCP）、StreamableHttp、Builtin（进程内，即 goose-mcp）、Frontend、InlinePython、Sse（废弃）。没有独立的原生工具 trait——内置工具也是 MCP server，统一由 extension_manager 路由。

### LLM Provider 抽象

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn get_name(&self) -> &str;
    async fn stream(&self, model_config, system, messages, tools) -> Result<MessageStream, ProviderError>; // 必须实现，流式为一等公民
    async fn complete(...) -> ...; // 默认由 stream 聚合实现
    async fn fetch_supported_models(&self) -> ...;
}
```

外加 ProviderDescriptor / ProviderDef（from_env 工厂）+ 注册表 + declarative/（**配置声明式定义 OpenAI 兼容 provider，不用写代码**）。特殊的 ACP provider 把其他 CLI agent（claude/codex/copilot/gemini-cli）经 Agent Client Protocol 当 provider 用。本地模型：ollama 一等支持 + goose-local-inference 内嵌推理。

### 权限/安全

goose_mode（auto / approve / chat / smart_approve）、permission/ 模块 + tool_confirmation_router、tool_inspection.rs + extension_malware_check.rs。无 OS 级沙箱（与 codex 的最大差异）。

### 轻重

重：13 crate + Electron 桌面端 + axum server + OTel + PostHog 遥测；但核心 goose 库可单独复用。参考价值：MCP-everything 扩展模型、provider 声明式注册。

## 2. OpenAI Codex CLI — [github.com/openai/codex](https://github.com/openai/codex)

**状态**：95,026 stars，活跃，主语言 Rust（codex-rs 为主体，cargo+Bazel 双构建）。

### 总体架构

**事件驱动的 SQ/EQ（Submission Queue / Event Queue）协议核**：UI（TUI/exec/app-server）与 core 之间以 `Op` 提交 + `Event` 流回传解耦（protocol crate）。core 内部是 thread/turn 模型 + 工具注册表分发。「协议层 - 引擎层 - 多前端」分层。

### Crate 分解（约 100 个目录级 crate）

- 引擎：`core`（agent 循环、codex_thread.rs、thread_manager.rs、context_manager/、compact*.rs 上下文压缩、turn_diff_tracker）
- 协议/API：`protocol`（protocol.rs 定义 Op/Event/SandboxPolicy/AskForApproval）、app-server(-protocol)、codex-api、core-api
- 前端：`tui`（ratatui）、`cli`、`exec`（无头）、chatgpt、cloud-tasks
- 沙箱：linux-sandbox（Landlock+seccomp）、windows-sandbox-rs、sandboxing、bwrap、execpolicy（Starlark 规则语言写命令策略）、network-proxy、process-hardening
- 工具：`tools`、`apply-patch`（lark 语法文件定义 patch 格式）、file-search、unified_exec（持久 PTY shell）
- MCP：mcp-server（codex 本身暴露为 MCP server）、rmcp-client
- 本地模型：`ollama`、`lmstudio`、models-manager
- 其他：rollout（会话持久化）、state/thread-store、skills、hooks、code-mode、otel、login

### Agent 循环

turn-based：thread_manager 管多线程会话 → 每 turn 由 context_manager 组装 prompt → client.rs 走 Responses API/Chat Completions SSE 流 → 工具调用进入 core/src/tools/：registry.rs 的 `ToolRegistry { HashMap<ToolName, Arc<dyn CoreToolRuntime>> }` → router/orchestrator/parallel（支持并行工具执行）。上下文超限有多版本自动 compact。

### 工具机制：原生 Rust handler + MCP 双轨

core/src/tools/handlers/ 下每个工具一组 `*.rs + *_spec.rs`（spec = OpenAI function schema）：shell、unified_exec、apply_patch、plan、request_user_input、request_permissions、view_image、web_search、multi_agents、mcp.rs（MCP 工具桥接）、tool_search、dynamic.rs。

### 沙箱与审批（protocol.rs 实测定义）

```rust
pub enum SandboxPolicy {
    DangerFullAccess,
    ReadOnly { network_access: bool },
    ExternalSandbox { network_access: NetworkAccess },
    WorkspaceWrite { writable_roots: Vec<AbsolutePathBuf>, network_access: bool, ... },
}
pub enum AskForApproval {
    UnlessTrusted,   // 只有已知安全的只读命令自动放行
    OnRequest,       // 默认：模型自行决定何时请求升权
    Granular(GranularApprovalConfig),
    Never,
}
```

实现：macOS = Seatbelt；Linux = Landlock + seccomp；Windows = 受限 token。另有 execpolicy（Starlark）命令级策略与 network-proxy 网络管控。**四个项目中最完整的纵深防御模型。**

### 轻重

非常重：约 100 个 workspace crate、双构建体系、实时语音等全都在。参考价值在协议分层与沙箱设计，不适合作为依赖复用。

## 3. rig — [github.com/0xPlaygrounds/rig](https://github.com/0xPlaygrounds/rig)

**状态**：7,809 stars，活跃。模块化 LLM 应用框架库。

- `rig-core` + `rig-derive` + 伴生集成 crate（12+ 向量库、云 provider、rig-fastembed 本地 embedding）
- completion/（CompletionModel trait + streaming）、client/、providers/（约 25 个，含 ollama、llamafile 本地）
- agent/：Agent = CompletionModel + preamble + 静态/动态 context + 静态/动态 tools；builder 模式；`.multi_turn(n)`；hook.rs 工具调用拦截
- 工具 trait（实测定义）：

```rust
pub trait Tool: Sized {
    const NAME: &'static str;
    type Error; type Args: Deserialize; type Output: Serialize;
    async fn definition(&self, prompt: String) -> ToolDefinition;
    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error>;
}
```

外加对象安全包装 ToolDyn、ToolSet、ToolEmbedding（工具可入向量库、prompt 时 RAG 检索工具）、tool/rmcp.rs（MCP 集成）。

无内建权限模型。**轻**：核心必选依赖仅 reqwest/serde/tokio 量级，WASM 兼容。作为地基是四者中最合适的；缺会话持久化、沙箱、审批。

## 4. swiftide — [github.com/bosun-ai/swiftide](https://github.com/bosun-ai/swiftide)

**状态**：712 stars，活跃。流式索引/查询管线 + agent 库。

- crate 已拆：swiftide-core（共享 trait）、swiftide-agents、swiftide-indexing、swiftide-query、swiftide-integrations、swiftide-macros（#[tool] 派生宏）
- 管线是强类型状态机（编译期保证管线合法）；agent 是显式生命周期状态机 `state::State`（Pending/Running/Stopped(StopReason)，含 **FeedbackRequired 停机原因 = 人类审批点**）；丰富 hook 体系
- 独特抽象：`ToolExecutor` trait（`exec_cmd(&Command)`）——**工具执行环境可插拔**（本机/Docker/远端）；AgentContext/MessageHistory trait 分离会话存储；ToolFeedback::Approved/Refused（内建 HITL 审批原语）

中等偏重；社区小是主要风险。`ToolExecutor` 和状态机 + FeedbackRequired 设计值得借鉴。

## 5. 其他值得注意的项目

| 项目 | stars / 活跃度 | 定位与架构要点 |
| --- | --- | --- |
| [AutoAgents](https://github.com/liquidos-ai/AutoAgents)（liquidos-ai） | 700，活跃 | Actor 模型（Ractor）多 agent 框架；本地推理内嵌 autoagents-llamacpp / autoagents-mistral-rs |
| [mistral.rs](https://github.com/EricLBuehler/mistral.rs) | 7,407，日常活跃 | 纯 Rust（Candle）推理引擎；OpenAI 兼容 server、原生 tool calling、MCP client——适合当本地推理后端而非地基 |
| [kalosm / floneum](https://github.com/floneum/floneum) | 2,208，活跃 | Candle 系本地多模态高层接口，强项受控/结构化生成；无 agent 循环 |
| [tabby](https://github.com/TabbyML/tabby) | 33,664，活跃 | 自托管编码助手 server（Rust + llama.cpp）；tabby-agent 是 TypeScript |
| llm-chain / rustformers/llm | 停维护 / archived | 排除 |

## 6. 横向结论

- **扩展机制光谱**：goose = 纯 MCP；codex = 原生 Rust handler 为主、MCP 为辅；rig/swiftide = 原生 trait 为主，各自带 rmcp 桥。**"原生 trait + rmcp 桥接"是库类项目的共识做法。**
- **Provider 抽象共识**：trait 以 stream() 为主方法、complete() 提供默认实现；OpenAI 兼容端点作为兜底 provider。本地模型三条路：Ollama HTTP、llama.cpp 内嵌、mistral.rs 内嵌。
- **安全模型分档**：codex 独一档（OS 沙箱 + 策略语言）；goose 为应用层审批；swiftide 提供 HITL 原语与执行环境隔离点；rig 无内建。
- **地基选型**：要轻量地基看 rig-core；要执行隔离与状态机设计看 swiftide-agents；要权限/沙箱纵深看 codex-rs；要 MCP-first 扩展与多 provider 注册表看 goose。

## Sources

- [block/goose](https://github.com/block/goose) · [openai/codex](https://github.com/openai/codex) · [0xPlaygrounds/rig](https://github.com/0xPlaygrounds/rig) · [bosun-ai/swiftide](https://github.com/bosun-ai/swiftide)（结构与源码均经 GitHub API 实测）
- [swiftide AGENTS.md](https://github.com/bosun-ai/swiftide/blob/master/AGENTS.md) · [docs.rs/swiftide](https://docs.rs/swiftide/latest/swiftide/)
- [liquidos-ai/AutoAgents](https://github.com/liquidos-ai/AutoAgents) · [EricLBuehler/mistral.rs](https://github.com/EricLBuehler/mistral.rs) · [floneum/floneum](https://github.com/floneum/floneum) · [TabbyML/tabby](https://github.com/TabbyML/tabby)
- 生态综述：[Zylos Research: Rust-Native AI Agent Frameworks 2026](https://zylos.ai/research/2026-04-01-rust-native-ai-agent-frameworks-ecosystem-2026/)

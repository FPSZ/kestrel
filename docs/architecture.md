# Kestrel — 本地优先 AI Agent 架构设计文档

> 代号 **Kestrel**（红隼）：体型最小的猛禽之一，悬停精准、俯冲高效——小而精，不是玩具。
> （代号可换，不影响本文档任何设计。）

|          |                                                                         |
| -------- | ----------------------------------------------------------------------- |
| 版本     | 0.1（创始版）                                                           |
| 日期     | 2026-07-02                                                              |
| 状态     | 待评审                                                                  |
| 定位     | 专为本地部署模型（llama.cpp / LM Studio）设计的轻量高效 agent，开源项目 |
| 语言     | Rust                                                                    |
| 目标平台 | Windows 优先，架构上不排斥 Linux/macOS                                  |

---

## 0. 一句话定位

**市面上的 agent 是"为云端 API 设计、顺便兼容本地"；Kestrel 反过来：把本地推理的物理约束（prefill 慢、上下文小、KV 缓存宝贵）当一级设计约束，所有云端 agent 不在乎的地方，正是我们的主场。**

---

## 1. 设计原则（按优先级排序）

1. **前缀稳定性是一级架构约束。** 消费级显卡 prefill 仅数百 t/s（实测 7900XTX 生成 ~65 t/s 但 prefill 仅 ~600 t/s），20k token 历史每轮重算即 30s+。凡是打碎 KV 前缀缓存的设计（动态 system prompt、改写历史、工具列表顺序漂移）一律禁止。
2. **固定 token 开销 ≤ 2.5k。** Claude Code 的固定开销约 14.3k token（系统提示 + 27 个工具 schema），在云端 200k 窗口里占 7%，在本地 32k 窗口里占 45%——是致命的。我们：系统提示 ≤800 + 工具 schema ≤1400 + 记忆文件 ≤300。
3. **循环要薄，外壳要厚。** Claude Code 的代码里仅 ~1.6% 是 AI 决策逻辑，98.4% 是确定性基础设施（权限门、截断、压缩、恢复）。工程价值全在循环外——这正是 Rust 的用武之地。
4. **单线程主循环 + append-only 事件日志。** OpenHands 亲手废除了自己的 pub/sub 事件总线（"引发各种线程/异步问题，消息顺序几乎无保证"）；Claude Code 全部押注单线程循环 + 扁平历史。不重蹈覆辙。
5. **权限系统不是可选项。** OpenClaw 2026 年初的安全危机（CVE-2026-25253 一键 RCE、4 万实例裸奔公网、技能市场 820+ 恶意技能）是反面教材。即使第一版是单机个人版，权限门也进第一版。
6. **边界即 crate。** 模块边界由 Cargo workspace 的 crate 边界强制，编译器兜底，不靠代码规范自觉。依赖方向单向，core 不知道任何 IO 细节。
7. **可测试性内建。** 事件日志天然是回放测试基座；agent 的全部确定性行为可以无模型、毫秒级进 CI。

---

## 2. 竞品调研结论（2026-07 实测）

### 2.1 全景对比

| 项目 | 规模 | 架构路线 | 对我们的价值 | 不采纳的部分 |
| --- | --- | --- | --- | --- |
| **OpenAI codex-rs** (95k star) | ~100 crates | SQ/EQ 协议分层 + OS 级沙箱纵深 | `protocol` crate 思想（Op/Event 解耦前后端）；`SandboxPolicy`/`AskForApproval` 分档设计；工具 spec+handler 双文件模式 | 100 个 crate 的规模；Bazel 双构建；实时语音等全家桶 |
| **Block goose** (50k star) | 13 crates | library-first + 纯 MCP 扩展 | 库优先结构（CLI/server 都是薄壳）；provider trait 以 `stream()` 为主方法；`toolshim`（给无原生 FC 的本地模型做工具模拟） | 内置工具也走 MCP（多一层进程内协议开销）；Electron 桌面端；50+ provider |
| **rig** (7.8k star) | core+集成 | 轻量库地基 | `Tool` trait 设计（const NAME + 关联类型）；依赖控制的榜样 | 不直接依赖（见 §3.4） |
| **swiftide** (0.7k star) | 分层库 | 强类型状态机 | `ToolExecutor` trait（执行环境可插拔：本机/Docker）；`FeedbackRequired` 内建人类审批停机点 | indexing/query 管线（非我们的场景） |
| **OpenHands** | 巨型 | V0 事件总线 → V1 事件溯源 | V1 的修正即我们的起点：append-only EventLog + 单一状态源；沙箱 opt-in；`SecurityAnalyzer` 风险分级 | V0 的一切：pub/sub、140+ 字段配置、10GB 镜像 |
| **Claude Code** | 闭源 | 单线程主循环 | 循环设计、权限七模式（deny 优先）、上下文五级流水线、JSONL transcript、子代理只回摘要 | 27 个工具 14.3k 固定开销（云端奢侈品） |
| **aider** | Python | 编辑格式实证派 | SEARCH/REPLACE 为主 + 按模型配格式 + 整文件回退；repo-map 1k token 预算 | Python 生态 |

### 2.2 从失败中学到的（同样重要）

- **AutoGPT**：无约束自治 → 超过 4-5 步的目标基本达不成。→ 硬性迭代/预算上限。
- **OpenHands V0**：事件总线 + 万能配置 → 自己废除。→ 单循环 + 单文件 TOML 配置。
- **OpenClaw**：插件市场 + 默认联网监听 → 2026 年首个 agent 安全危机。→ 无插件市场；默认只监听 localhost；权限门第一版就有。
- **本地用户的真实抱怨**（HN/GitHub issues）：prefill 100 秒级、忘开 `--jinja` 模型看不见工具、"工具调用哪怕 5% 失败率就毁掉整个体验"、大 harness 的 system prompt 在本地窗口里是纯负担。→ 这四条各自对应我们的一个设计决策（§5.4、§6.3、§5.3、原则 2）。

---

## 3. 架构风格选型

### 3.1 结论

**库核心 + 薄适配器 + 事件流输出**（library core + thin adapters + event stream）。
端口（trait）只建在**确定会有第二个实现**的四个边界上；其余地方写朴素、直接、可读的代码。

### 3.2 被否决的方案及理由

| 方案                                           | 否决理由                                                                                                                                                                                                                                                                     |
| ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **教科书式六边形架构**                   | 六边形为"领域模型复杂、外部系统多变"的企业应用设计。我们恰好相反：外部边界少而稳定，复杂度在**运行时行为**（loop 状态机、token 预算、KV 联动）而非领域模型。全套 DTO 转换/依赖注入容器只会稀释代码密度。**取其魂（依赖单向、core 无 IO），弃其形（每层仪式）。** |
| **事件总线（pub/sub）**                  | OpenHands 亲手废除的路线，消息顺序无保证、调试地狱。我们用**单消费者的有序事件通道**（`tokio::sync::mpsc`）：core 产出事件，前端消费，永远单向、有序。                                                                                                               |
| **Actor 框架**（AutoAgents/Ractor 路线） | Actor 解决的是大规模并发实体的问题。我们只有一个 agent 循环 + 少量后台任务，`tokio::select!` + channel 足够，引入 actor 框架是拿大炮打蚊子，还引入一整套监督树的学习成本。                                                                                                 |
| **纯 MCP 扩展**（goose 路线）            | 内置工具也走 MCP 意味着每个工具调用多一层 JSON-RPC 序列化，且工具 schema 不受我们控制（MCP 工具 schema 常常比用户提示还费 token）。**内置工具用原生 trait（零开销、schema 逐 token 手工优化），MCP 作为 v2 的外接桥**——这也是 rig/swiftide/codex 的共识做法。        |
| **依赖 rig-core 做地基**                 | rig 的抽象质量不错，但我们的差异化恰好在它抽象掉的那一层：llama.cpp 的 slot 管理、`cache_prompt`、`/props` 探测、GBNF 注入都需要直接控制 HTTP 请求体。为省 2 千行代码引入一个把关键层遮住的依赖，不值。**自研 backend 层，trait 设计参考 rig/goose。**             |

### 3.3 四个端口（且仅此四个）

```rust
/// 1. LLM 后端 —— 第二实现明确存在：llama.cpp / LM Studio
trait LlmBackend {
    async fn stream(&self, req: CompletionRequest) -> Result<CompletionStream>;
    async fn probe(&self) -> Result<BackendCapabilities>;   // /props 或 /api/v0/models
    async fn save_cache(&self, session: &SessionId) -> Result<()>;  // 不支持则 no-op
}

/// 2. 工具 —— 每个内置工具是一个实现
trait Tool {
    const NAME: &'static str;
    fn spec(&self) -> &ToolSpec;                 // 静态 schema（前缀稳定性！）
    fn risk(&self, args: &Value) -> RiskLevel;   // 权限门用
    async fn call(&self, args: Value, ctx: &ToolCtx) -> Result<ToolOutput>;
}

/// 3. 前端 —— 第二实现已规划：CLI 现在 / WebUI 以后
///    形态：core 暴露 `run(op) -> impl Stream<Item = Event>`，前端只是事件的渲染器
///    （codex 的 SQ/EQ 思想，砍掉它的 JSON-RPC 仪式）

/// 4. 存储 —— 事件日志 / 会话 / 模型 profile
trait Store {
    async fn append(&self, session: &SessionId, event: &Event) -> Result<()>;
    async fn replay(&self, session: &SessionId) -> Result<Vec<Event>>;
}
```

### 3.4 语言选型：Rust（理由不是性能）

agent 外壳的性能瓶颈永远在模型推理，Python 外壳也不慢——性能不是理由。真正的理由：

1. **边界由编译器强制**：crate 边界的私有泄露编译不过，开源多人协作时比 code review 可靠。
2. **单二进制分发**：`cargo install` 或下载一个 exe，没有 Python 环境地狱；对将来"给朋友用"关键。
3. **赛道最强先例都是 Rust**（codex-rs、goose），tokio + reqwest + ratatui + axum 恰好覆盖全部需求。
4. 常驻进程内存小、无 GC——本地机器的内存要省给模型。

备选 Go（开发更快）被放弃：类型系统弱一档（边界表达力）、且本赛道无同级先例可抄。

---

## 4. 系统总体架构

### 4.1 分层图

```mermaid
graph TB
    subgraph frontends["前端层（薄壳，只渲染事件）"]
        CLI["kestrel-cli<br/>(ratatui TUI, v1)"]
        WEB["kestrel-server<br/>(axum + WebUI, v2 预留)"]
    end

    subgraph core["kestrel-core（纯逻辑，零 IO 依赖）"]
        LOOP["Agent Loop<br/>单线程 turn 状态机"]
        CTX["Context Ledger<br/>token 预算 + KV 联动"]
        PERM["Permission Engine<br/>deny 优先 + 风险分级"]
        PROTO["Event / Op 类型<br/>(kestrel-protocol)"]
    end

    subgraph adapters["适配器层"]
        BACKEND["kestrel-backend<br/>llama.cpp / LM Studio"]
        TOOLS["kestrel-tools<br/>shell / fs / search / browser"]
        STORE["kestrel-store<br/>JSONL 事件日志 / profile"]
    end

    subgraph external["外部世界"]
        LLAMA["llama-server<br/>(--jinja, slots)"]
        LMS["LM Studio<br/>(REST /api/v1)"]
        OS["PowerShell / 文件系统 / CDP 浏览器"]
    end

    CLI -->|Op| LOOP
    WEB -.->|Op| LOOP
    LOOP -->|"Event (mpsc, 有序)"| CLI
    LOOP --> CTX
    LOOP --> PERM
    LOOP -->|trait LlmBackend| BACKEND
    LOOP -->|trait Tool| TOOLS
    LOOP -->|trait Store| STORE
    BACKEND --> LLAMA
    BACKEND --> LMS
    TOOLS --> OS
```

**依赖方向铁律**：`前端 → core ← 适配器`，core 不依赖任何适配器 crate；适配器之间互不依赖；所有共享类型下沉到 `kestrel-protocol`。

### 4.2 Workspace 结构

```
kestrel/
├── Cargo.toml                    # workspace 根：统一 lints、profile、依赖版本
├── README.md / LICENSE-MIT / LICENSE-APACHE / CONTRIBUTING.md
├── deny.toml / rustfmt.toml      # 依赖白名单、格式化配置
├── .github/workflows/ci.yml      # fmt + clippy(-D warnings) + test + cargo-deny
├── docs/
│   ├── architecture.md           # 本文档
│   ├── adr/                      # 架构决策记录（0001-语言、0002-风格、...）
│   └── research/                 # 竞品与技术调研报告（结论的证据基础）
├── crates/
│   ├── kestrel-protocol/         # Event/Op/ToolSpec/RiskLevel 等纯类型 + serde
│   │                             #   依赖仅 serde；被所有 crate 依赖
│   ├── kestrel-core/             # agent loop、context ledger、permission engine、
│   │                             #   crew 路由 + ports/；只依赖 protocol + tokio(sync)
│   ├── kestrel-backend/          # LlmBackend 实现：llamacpp.rs / lmstudio.rs /
│   │                             #   openai_compat.rs（兜底）+ probe.rs（能力探针）
│   ├── kestrel-tools/            # 内置工具：shell.rs fs.rs search.rs（browser M4）
│   ├── kestrel-store/            # JSONL 事件日志、模型 profile、TOML 配置
│   └── kestrel-cli/              # v1 前端：极简 REPL 起步，M2 引入 ratatui TUI
│       └── (v2: kestrel-server)  # axum daemon + WebUI，架构预留、暂不建目录
├── profiles/                     # 内置模型 profile（qwen3-8b.toml 等，探针可覆盖）
└── tests/replays/                # 录制的回放测试 fixture（.jsonl）
```

7 个 crate 起步（对比：codex ~100、goose 13）。每个 crate 的 `lib.rs` 顶部用模块级文档注释声明其**职责边界与禁止依赖**，CI 用 `cargo-deny` 强制依赖白名单。

---

## 5. 核心设计

### 5.1 Agent Loop：单线程 turn 状态机

主循环是一个**单线程、单消费者、可取消**的状态机。一个 turn = 一次"组装 prompt → 流式推理 → 解析工具调用 → 权限门 → 执行工具 → 追加结果"的闭环，直到模型不再请求工具或触达迭代/预算上限。

```rust
enum TurnState {
    Assembling,          // context ledger 组装 prompt（前缀稳定！动态信息置尾）
    Streaming,           // backend.stream()，边收边解析 tool_call
    AwaitingApproval,    // 命中风险动作 → 事件通知前端，挂起等 Op::Approve/Deny
    ExecutingTools,      // 只读工具并行、写状态工具串行（codex/Claude Code 共识）
    AppendingResults,    // 结果 head-tail 截断后 append 进事件日志
    Done { reason },     // 无工具调用 / 迭代上限 / 预算耗尽 / 用户中断
}
```

**铁律**：

- 消息历史 **append-only**，永不原地改写——这是 KV 前缀缓存能命中的前提（原则 1）。
- 迭代上限（默认 10）+ token 预算双闸，治 AutoGPT 式无限循环。
- 取消信号（`CancellationToken`）贯穿到子进程：本地推理慢，可中断性比云端更关键。
- 工具执行失败时，把**具体错误**（最近似片段、行号）append 回历史让模型自纠错，而非静默重试（aider 的 reflection 经验）。

### 5.2 Context Ledger：token 预算 + KV 联动

不叫 "manager" 叫 **ledger（账本）**，因为它的核心职责是**按服务器上报的真实上下文长度记账**，而非拍脑袋截断。

- 启动时 `probe()` 拿到后端真实 `n_ctx`（llama.cpp `/props`、LM Studio `/api/v0/models`），预算按此算，不硬编码。
- **摄入即截断**：工具输出是头号 token 杀手（ReAct 循环里常占 70-80%），在 append 那一刻就 head-tail 截断（保头保尾，中间折叠成 `… [省略 N 行] …`），而不是等压缩时再处理。
- **压缩少而狠**：任何改写历史的压缩都会打碎 KV 前缀 → 触发全量 re-prefill。所以绝不每轮微调，只在逼近预算（~85%）时做**一次**大压缩——而且这次压缩由**副手模型异地完成**（见第 6 章），主脑 KV 不受影响。
- 压缩点与 llama.cpp `/slots/{id}?action=save` 联动：旧状态落盘，配合状态树分支（未来）可秒级回溯。

### 5.3 Permission Engine：deny 优先 + 风险分级

即使第一版是单机个人版，权限门也进第一版（OpenClaw 的教训）。

```rust
enum RiskLevel { ReadOnly, Mutating, Destructive, External }
enum Decision  { Allow, AskUser, Deny }
```

- **deny 优先求值**：命中全局 deny 规则的工具，在模型看到之前就从工具列表里预过滤掉（Claude Code 做法）——既安全又省 token schema。
- 每个工具自报 `risk(args)`：`rm -rf`、删文件、写系统目录 → `Destructive`；联网 → `External`。风险等级驱动确认策略。
- 确认策略分档（对齐 codex `AskForApproval`）：`auto`（只读自动放行）/ `on-request` / `strict`（所有写动作都问）。
- **审校模型是这一层的可视化**：高危动作触发时，14B 审校给出"稳/险 + 一句理由"，权限确认从冷冰冰的 y/n 变成有依据的第二意见（第 6 章）。

### 5.4 Backend 层：能力探针 + slot 管理

`kestrel-backend` 是唯一碰 HTTP 的地方，也是本地专项优化的集中地。

- **首跑能力探针**：接入新模型/后端时跑一组 ~30 秒微基准（原生工具调用格式可靠性、SEARCH/REPLACE 编辑成功率、指令遵循），自动判定该模型走哪条工具调用路线（原生 FC / Hermes-XML 提示词 / GBNF 约束兜底）与编辑格式，存成 `profiles/<model>.toml`。本地用户换模型像换灯泡一样频繁——这把"每换个模型手调半天"变成零配置。
- **前缀稳定发送**：system prompt + 工具定义完全静态、顺序固定；`cache_prompt` 默认开；长会话切换用 `/slots/{id}?action=save|restore`。
- **llama.cpp 专项**：强制 `--jinja`（否则模型看不见工具，最高频事故）；KV 缓存告警 Q8 以上（Q4 显著劣化工具调用）；GBNF/`json_schema` 作为约束兜底。
- **LM Studio 专项**：JIT 冷启动 + TTL 逐出的重试逻辑；利用其上报的 TTFT/tok-s 做自适应调度。

---

## 6. 【签名章】双模型剧场 · "机组"

> 这是 Kestrel 对用户的第一眼记忆点，也是所有本地优化的可视化外壳。

### 6.1 核心洞察：剧场是矛盾的解，不是装饰

整个架构的中心张力：**原则 1（前缀稳定，永不改写历史）** 与 **上下文管理（必须压缩，压缩即改写）** 直接冲突。

**解法**：让压缩、摘要、预读、检索这些"会污染/改写上下文"的活，全部交给**跑在独立进程、独立 KV 缓存的辅助模型**。主脑（35B）的 KV 前缀一个字节都不动，辅助模型只在**轮次边界**递进一份干净的产物。

于是每一个原本"看不见的舒服"都变成了**一个看得见的机组成员在干活**：压缩=副手、检索=书记、复核=审校、预热=影子槽。剧场不是给算力加皮肤，它是让 §5.2 的"异地压缩"、§7 的影子槽预热这些机制**长出一张脸**。这就是它同时"眼前一亮"和"经得起追问"的根本原因。

### 6.2 机组编制（角色即真实工作，随硬件增减）

| 成员 | 模型 | 职责 | 底层机制 |
| --- | --- | --- | --- |
| **主脑 Lead** | Qwen3-35B-A3B | 主循环：规划、写码、决定工具调用 | §5.1 agent loop |
| **副手 Copilot** | Qwen3-8B | 后台：压缩历史、摘要长工具输出、预读文件、起草 commit | §5.2 异地压缩 + §7 影子槽 |
| **书记 Librarian** | Qwen3-Embedding-4B | 静默：索引项目+记忆，按需递相关片段 | 本地语义记忆检索 |
| **审校 Critic** | Qwen3-14B（按需） | 高危动作前独立复核主脑计划，给"稳/险+理由" | §5.3 权限门的可视化 |

**为什么角色映射到真实活、而非表演**：机组成员之间**绝不做表演式对话**（那是多 agent 框架烧 token 的教训）。所有"协作"都是真实并行工作的可视化，不是为了演戏多调一次模型。

### 6.3 交互设计

```
┌ Kestrel · 会话: 重构 auth 模块 ································· 机组 4/4 [####]
│
│  你     重构登录逻辑，拆掉那个 800 行的 God 函数
│
│  主脑   我先看整体结构，读 auth.rs (812 行)
│           └ 副手   812 行 -> 190 token 摘要   [ok]
│           └ 书记   检索到 2 处相似重构
│         规划拆成 3 块：session.rs / token.rs / guard.rs
│
│  主脑   [!] 即将删除 verify_legacy()
│           └ 审校   险 — 还有 3 处引用未迁移，建议先加 @deprecated
│         采纳审校意见，改为标记弃用   [ok]
│
├─ 机组账本 ─────────────────────────────────
│  副手省 3,020 tok · 书记命中 2 · 审校拦截 1 险动作
│  主脑 62 tok/s · KV 缓存 [########·] 87% 命中
└─
```

- **主脑独占主车道，其余成员是缩进的暗色"低语"**——不与主对话抢注意力，可一键折叠。
- **交接时刻可见**：副手完成时，一条细线从副手车道流进主脑，显示摘要吞吐（`3,200 tok → 180 tok`）——价值转移看得见。
- **机组账本**：滚动统计"副手替主脑省下多少 token / prefill 秒数、书记命中几次、审校拦下几个险动作"。把无形优化变成可炫耀的数字——这是"说不出来的舒服"变"说得出来"的关键。
- **可命名/换肤**：允许用户给自己的模型起名——本地用户对自己的机器有归属感，个性化=情感黏性。

### 6.4 反噱头纪律（设计约束）

1. 成员间零表演式对话；所有协作是真实并行工作的渲染。
2. 编排是**确定性代码**（固定 job-type → 角色路由），不是让主脑花一次昂贵推理去"决定谁来干"。
3. 副手**永不阻塞**主脑：摘要没好，主脑照常推进，产物就绪时才在下一轮边界并入。
4. 低语而非喧哗：副手/书记活动默认暗色小字、可折叠。

### 6.5 优雅降级（尊重"本地开销大"）

机组规模随加载的模型数自动伸缩，用户按显存决定跑几个：

- **1 个模型** → 独奏模式，剧场坍缩成单车道，一切照常工作。
- **2 个** → 主脑 + 副手（核心剧场成立）。
- **3–4 个** → 全机组。

降级是零配置的：没加载审校模型，高危确认就回退成普通 y/n；没加载书记，就退回关键词检索。

### 6.6 架构落地（不是渲染把戏，是真实抽象）

1. **模型池**（`kestrel-backend`）：N 个 `LlmBackend` 实例，各指向一个 llama-server 进程/slot 或 LM Studio 模型；逐模型健康/负载探测。
2. **作业路由**（`kestrel-core`）：把 job-type（`Turn` / `Compact` / `Summarize` / `Prefetch` / `Retrieve` / `Review`）确定性地映射到机组角色——纯代码，非 LLM 决策。
3. **事件带 actor 标签**（`kestrel-protocol`）：每个 `Event` 携带 `actor: CrewRole`，前端据此渲染车道。对事件枚举只是小增量。
4. **并发与 KV 隔离**：副手/书记作业跑在独立 tokio 任务；产物**仅在轮次边界**并入主脑上下文，绝不中途插入（保前缀稳定）。这正是影子槽机制，现在有了一张脸。
5. **配置**：机组编制是一张 TOML 表（哪个模型担任哪个角色，或 `auto` 按已加载模型自动分配）。

---

## 7. 支撑创新（机组背后的引擎）

这些是让机组跑得动的底层机制——用户不直接看见，但它们是账本上那些数字的来源。

| 创新 | 作用 | 归属 |
| --- | --- | --- |
| **影子槽预热**（Shadow-Slot Prewarming） | 压缩后的新前缀在备用 slot 后台预填充，轮次边界原子切换——压缩从"卡 30 秒"变"无感知" | 副手的底层实现 |
| **能力探针**（Capability Probing） | 接入新模型自动微基准 → 生成协议档位与编辑格式 profile | §5.4，零配置换模型 |
| **确定性回放测试**（Replay Harness） | 事件日志录制 → LLM 响应变 fixture → 确定性外壳无模型毫秒级进 CI | 兑现"工程质量" |
| **异地压缩** | 压缩在副手进程完成，主脑 KV 前缀零扰动 | §5.2 + §6.1 的解 |

---

## 8. 工具层设计

- **≤10 个内置工具**起步（每个 schema 都吃前缀预算）：`shell`、`read`、`edit`、`search`（grep+glob 合一）、`browser`（CDP，非视觉）、`process`（系统管理）。
- **函数名注册表校验**：小模型爱幻觉函数名，执行前查注册表，未命中返回可操作错误。
- **编辑工具为弱模型设计**：默认 SEARCH/REPLACE 块（最贴训练分布）；解析宽容（容忍空白/fence 漂移）；匹配失败返回最近似片段；最弱模型留整文件重写回退；编辑格式 per-model 由探针配置。**强制编辑前先 Read**（防盲改）。
- 工具输出摄入即截断（§5.2）；返回高信号信息、语义化而非 UUID。

---

## 9. 路线图

| 阶段 | 交付 | 机组形态 |
| --- | --- | --- |
| **M1 骨架** | workspace + protocol + core loop + llama.cpp backend + shell/read/edit/search + CLI + 事件日志 | 独奏（仅主脑） |
| **M2 剧场核心** | 模型池 + 作业路由 + 副手（异地压缩/摘要）+ actor 事件 + 机组账本 | 主脑 + 副手 |
| **M3 全机组** | 书记（记忆检索）+ 审校（高危复核）+ 能力探针 + 回放测试进 CI | 全机组 |
| **M4 扩展** | browser/process 工具 + 状态树分支（slot save/restore）+ MCP 外接桥 | + 时间旅行 |
| **v2 预留** | `kestrel-server`（axum + WebUI）+ 多会话隔离 + 认证——给朋友用 | 不改 core 一行 |

---

## 10. 开源工程规范

- **双许可** MIT + Apache-2.0（Rust 生态惯例）；`README` / `CONTRIBUTING` / `CODE_OF_CONDUCT`。
- **CI 硬门槛**：`cargo fmt --check` + `cargo clippy -D warnings` + `cargo test` + 回放测试 + `cargo-deny`（依赖白名单，强制 §4.1 依赖方向）。
- **每个 crate `lib.rs` 顶部模块文档**声明职责边界与禁止依赖；边界由编译器兜底，非靠自觉。
- 语义化版本；`CHANGELOG` 遵循 Keep a Changelog；核心类型 crate（`kestrel-protocol`）稳定后谨慎破坏性变更。

---

## 附录 A：架构决策记录（ADR）

重大选型的备选方案、否决理由与重开条件已拆分为独立文件，见 [adr/](adr/)：

- [ADR-0001 语言选型：Rust](adr/0001-language-rust.md)
- [ADR-0002 架构风格：库核心 + 薄适配器 + 事件流](adr/0002-style-library-core-event-stream.md)
- [ADR-0003 已否决方案速查](adr/0003-rejected-alternatives.md)

本文档中的调研结论（§2、§5.4 等）的完整证据见 [research/](research/)。

---

*本文档为创始版（v0.1）。设计决策均附调研依据；代号 Kestrel 可换，不影响任何技术选型。*

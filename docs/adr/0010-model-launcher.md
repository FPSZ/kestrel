# ADR-0010 模型启动器：把模型作为 agent 的一部分来启动与监督

| | |
| --- | --- |
| 状态 | Accepted |
| 日期 | 2026-07-03 |

## 背景

Kestrel 现状：agent 连一个**已经在跑**的 OpenAI 兼容服务器（llama.cpp / LM Studio），自己不
启动模型。一次真实联调把这条设计的代价暴露无遗：手动敲 llama-server 参数、忘了 `--jinja`
模型就看不见工具、LM Studio 的 REST 端点要 API token 鉴权、端口与生命周期全靠手动。这与
[vision.md](../vision.md)"换模型像换灯泡一样零配置"直接打架；而 M2 的**模型池**要管 N 个后端，
缺的正是一个"启动 + 监督"的地基。

竞品把牌摊开：

| | 定位 | 强 | 弱（对 agent 而言） |
| --- | --- | --- | --- |
| **llama.cpp（llama-server）** | 推理**引擎** | 性能天花板、控制最全 | 纯 CLI、无模型管理、参数地狱 |
| **LM Studio** | 闭源 GUI **模型宿主** | 发现/下载/预设、GUI | 闭源、通用聊天 app 非 agent、REST token 摩擦、重 |
| **Ollama** | 极简 **runner** | `ollama run` DX 一流、自动管理 | 自有 registry/模板、通用、不懂 agent |

共同点：三者都在回答"怎么把模型跑起来给一个 API"，**没有一个是 agent**——不掌管 agent loop、
权限门、KV 感知的上下文、机组、工具。这是我们的差异化空间。

## 决策

给 Kestrel 增加一个**薄的模型启动器/监督器**（落在适配器层，core 不依赖它），它把模型
**作为 agent 的一部分**来启动。范围克制，原则如下。

### 1. 定位：不做"第二个 Ollama / LM Studio"

不做模型发现 / 下载 / registry，不重写引擎。只做"为 agent 而启动并监督 llama.cpp，并能委托
已有宿主"。发现与下载留给 Ollama / LM Studio / HuggingFace。

### 2. 三种后端来源，都是一等公民

- **a. 自启 llama.cpp（首选，控制最全）**：spawn `llama-server -m <gguf> --jinja -c <ctx>
  --host 127.0.0.1 --port <p> [gpu/kv flags]`，轮询 `/health` 就绪后 `LlmBackend` 再连。
- **b. 委托已有宿主**：检测到 `lms` / `ollama` 或已在跑的服务器，调它们或直接连
  （用户自己的 qon/qoff 高手流照用）。
- **c. 纯连接（现状）**：连一个已在跑的 `base_url`，零启动。

### 3. agent 感知的启动（核心差异化，通用宿主结构上做不到）

通用宿主为任意客户端起模型，不知道消费方是 agent；我们两头都占，于是能：

- **强制 `--jinja` + 选工具可靠的模板**——消灭"模型看不见工具"这个本地 agent 头号翻车点。
- **接能力探针（§5.4）**：首启跑 ~30 秒微基准，按该模型 @ 该量化的**真实 agent 能力**选
  工具调用协议 / 编辑格式，存 profile。别人只给通用 server，从不测"这模型 Q4 下工具调用行不行"。
- **KV / 前缀稳定接线**：`cache_prompt`、slot save/restore、影子槽预热（M2）——因为启动器与
  agent 共享 KV 策略。
- **模型池 / 机组（M2）**：按显存同起 主脑 + 副手 + 书记，作业路由到对的模型。
- **Loadout（[ADR-0006](0006-loadout-declarative-build.md)）落地**：一份清单声明"模型 @ 量化 +
  参数 + 工具 + 权限"，启动器一条命令实现。
- **参数调节 UI**：ctx / 温度 / top_p / GPU 层 / flash-attn / MTP / 投机 等做成控件
  （顺带兑现"方便调节每个参数"的诉求）。

### 4. 生命周期

start / stop / restart / 健康检查 / 闲时卸载（TTL）/ 崩溃重启；进程树可原子杀
（与创新候选 Process Bloodline 咬合）。

### 5. 安全（铁律，不可削弱）

- 引擎二进制与模型路径走**白名单 / 显式配置**，不接受任意路径 spawn（防越权执行）。
- 只绑 `127.0.0.1`；不自动联网拉模型（离线可验证）。
- spawn / kill 经权限门、可审计；宿主若需 token，按 [§5.1](../../AGENTS.md) 用 `SecretString`，
  不入事件日志 / 日志 / UI / 提交。
- 跨平台 GPU 参数矩阵先只覆盖已验证组合，未知硬件优雅降级；默认 GPU 卸载 `auto`，不强制
  `max`（防 OOM）。

### 6. 边界即 crate

落在适配器（可能新增 crate `kestrel-runtime`，或并入 backend 的 supervisor 模块，实现时定），
`kestrel-core` 不依赖它；通过端口暴露给前端。先定端口契约（`launch(spec) -> Handle` /
`health` / `stop`）再写实现。

## 被否决方案的最强论点（诚实记录）

- **完全不做，永远 BYO server（现状）**：最省事、职责最清晰（agent 只管 agent）。否决——与
  vision"零配置换模型"和 M2 模型池直接冲突，且实测证明手动起服务器是真实高频摩擦
  （`--jinja`、token、端口）。**保留"纯连接"模式**即可兼顾此论点，不必因噎废食。
- **做成完整模型宿主（对标 Ollama / LM Studio）**：功能全、可独立成产品。否决——重造发现 /
  下载 / registry / 跨平台 GPU 矩阵是数年工程，Ollama 领先太多，且偏离 agent 主线。我们只做
  "薄监督器 + 委托已有宿主"。
- **让 agent 自己用 shell 工具去起 llama-server**：零新代码，主脑写命令即可。否决——把关键
  基础设施交给不确定的 LLM 决策（违反"循环薄、外壳厚"），且无健康检查 / 生命周期 / 探针接线，
  脆且不可复现。启动器是确定性外壳该干的活。

## 重开条件

- 用户只用一种固定宿主（如永远 Ollama）→ 启动器退化为该宿主的薄委托，不必自启 llama.cpp。
- 需要远程 / 多机模型（vision 的 Local-First Relay）→ 启动器的 `base_url` 抽象已就绪，叠远程发现。
- 跨平台 GPU 支持成为瓶颈 → 评估直接依赖 Ollama 作引擎供给层，我们只做 agent 编排。

## 落地

里程碑见 [roadmap-board](../planning/roadmap-board.md)。先出可行性 spike（自启 llama.cpp +
健康轮询 + 纯连接回退），再接能力探针 / 模型池 / Loadout / 参数 UI。

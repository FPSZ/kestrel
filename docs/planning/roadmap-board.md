# Kestrel · 执行看板

> 这是全项目的**落地进度跟踪表**，对齐 [architecture.md §9 路线图](../architecture.md#9-路线图)。
> 分工：architecture.md 定 **做什么 / 为什么**（设计事实源），本表跟 **做到哪 / 下一步**。
> 本表不发明路线——凡未写进 architecture.md §9 的条目，一律进"待拍板"区，先立 ADR 再上升。

铁律不变（[AGENTS.md §2](../../AGENTS.md)）：前缀逐字节稳定、固定开销 <=2.5k、循环薄外壳厚、
单线程主循环 + append-only 事件日志、权限门不可削弱。任何一条被本表某任务违反，即该任务作废。

## 状态记号

| 记号    | 含义                                                        |
| ------- | ----------------------------------------------------------- |
| `[x]` | 已完成并提交                                                |
| `[~]` | 进行中                                                      |
| `[ ]` | 待办（属 architecture.md §9 已定路线）                     |
| `[!]` | 阻塞 / 暂缓（注明原因与解冻条件）                           |
| `[?]` | 待拍板（创新候选，需先立 ADR + 用户定方向，尚未进承诺路线） |

---

## 里程碑总览

| 里程碑                         | 交付                                                                       | 机组形态       | 状态                                   |
| ------------------------------ | -------------------------------------------------------------------------- | -------------- | -------------------------------------- |
| M1 骨架                        | core loop + OpenAI 兼容 backend + 4 工具 + 权限门 + JSONL 日志 + REPL      | 独奏           | `[x]` 已交付                         |
| WebUI 个人版                   | kestrel-server(SSE+Op) + console 扁平深色壳                                | 独奏可视化     | `[x]` 已交付                         |
| **地基（G）· 横切约定** | i18n + 设计令牌 + 事件日志前向兼容 + core 确定性 + 密钥 + 错误分类         | 与机组无关     | `[~]` **先于 M2**              |
| **创新种子小里程碑**     | 可靠性地基 + Glass Engine 起步 + 自我进化捕获起步                          | 独奏增强       | `[?]` **待拍板（见文末岔路）** |
| **模型启动器（L）**      | 自启 llama.cpp + 委托已有宿主 + 纯连接回退 + 参数 UI（ADR-0010）           | on-ramp        | `[ ]` 近期起步，全量骑 M2-M4         |
| **M2 剧场核心**          | 模型池 + 作业路由 + 副手异地压缩 + actor 事件 + 机组账本                   | 主脑+副手      | `[ ]` **下一个承诺里程碑**     |
| M3 全机组                      | 书记(记忆检索) + 审校(高危复核) + 能力探针 + 回放测试进 CI                 | 全机组         | `[ ]` 待办                           |
| M4 扩展                        | browser/process 工具 + 状态树分支 + MCP 桥 + 投机代理 + Loadout + 轮内取消 | +时间旅行/秒回 | `[ ]` 待办                           |
| M5 夜班                        | 闲时自主家务 + 夜班报告（默认只读，ADR-0004）                              | +夜班          | `[ ]` 待办                           |
| M6 睡眠周期                    | 本地 LoRA 自我进化（北极星，落地前另立 ADR）                               | 会做梦         | `[ ]` 待办（先立 ADR）               |
| 朋友版                         | kestrel-server 叠 认证 + 多会话隔离 + TLS                                  | 不改 core      | `[ ]` 待办                           |

下面按里程碑展开。已交付的折叠成结论，未做的按"protocol → core → 适配器 → server → console → 测试"的依赖顺序拆。

---

## [x] M1 骨架 — 已交付

workspace(7 crate) + `kestrel-protocol`(Event/Op/ToolSpec/RiskLevel) + `kestrel-core`
(agent loop 单线程 turn 状态机 / context ledger / permission engine / crew 骨架 / ports) +
`kestrel-backend`(OpenAI 兼容，覆盖 llama.cpp / LM Studio) + `kestrel-tools`(read/search/edit/shell) +
`kestrel-store`(JSONL 事件日志 + TOML 配置) + `kestrel-cli`(回合制 REPL) + 权限门 + 回放冒烟测试。
CI 硬门槛(fmt / clippy -D warnings / test / cargo-deny)全绿。

## [x] WebUI 个人版 — 已交付（ADR-0007）

`kestrel-server`(axum)= "repl.rs over HTTP"：`GET /api/events`(SSE，快照追平 + 按 seq 去重) +
`POST /api/ops` + `/api/health` + `/api/sessions[/{id}/events]` + release 托管 `console/dist`。
只绑 `127.0.0.1`、单人无认证。`console`(React+Vite+Tailwind v4)扁平深色壳：左侧栏+顶栏连体磨砂、
流式对话(markdown / 工具卡 / 内联审批 / 错误红卡)、会话回放、只读设置、顶栏连接状态灯、Vite:7823。
端到端已验证。T0-T2 逐条清单见本文件 git 历史（commit `16e7cc8`/`8c70d94`/`f6c7785`/`43f06f3`）。

遗留一条已在"地基修复"轮解冻：`[x]` **轮内取消** —— core 现在在流式与工具执行期间
并发监听 `Op::Cancel`，取消令牌贯穿到 shell 子进程；从 M4 提前落地（见下方"地基修复"段）。
前端 Stop 按钮改为真控件（后端已就绪），console 接线待办。

---

## [x] 地基修复 —— 让"文档写成铁律、代码还是 TODO"的地基名副其实

审查发现三处 architecture.md 当铁律宣传、代码里却是占位的地基空缺，先于新特性补齐；
顺带把两个近期可靠性/后端项做实。全程 core 一行不改设计、不新增 ADR（都是把已 Accepted
的设计落地，非新方向）。fmt/clippy(-D warnings)/test(35 通过)/deny 全绿。

- [X] F1  Context Ledger 接进主循环：`AgentConfig.n_ctx` 由后端 `probe()` 实测喂入；
  轮次边界按完整历史确定性重算 token（近似 bytes/4），发 `Event::ContextBudget`
  （结构化数值、语言中立、无时钟/随机——合地基铁律 #2/#5）。压缩派发仍属 M2。
- [X] F2  read-before-edit 强制：主循环层维护会话级已读文件集（纯字符串、零 IO、确定性），
  编辑未读文件直接挡回并喂可操作纠错提示（architecture.md §8 从注释变约束）。
- [X] F3  deny 优先预过滤：`config.deny_tools` 组装时经 `ToolSet::deny` 从工具列表删除
  （模型看不见、省 schema token），`PermissionEngine::decide_tool` 运行时兜底 deny。
- [X] F4  轮内取消：`stream_once` / 工具执行期间 `select!` 监听 `Op::Cancel`，触发轮级
  取消令牌（贯穿 shell 子进程），收尾为 `TurnCompleted{reason:"cancelled"}`。从 M4 提前。
- [X] F5  llama.cpp / LM Studio 专属后端：组合复用 OpenAI 兼容流式，`probe()` 分别打
  `/props`、`/api/v0/models` 读真实 `n_ctx`（失败优雅回退配置值）；`backend.kind` 选择，
  `kestrel_backend::build()` 工厂。slot save/restore 仍 no-op（影子槽属 M2 高风险 spike）。
- [X] F6  search 工具 regex + glob：`pattern`（默认字面，`regex:true` 转正则）+ 可选 `glob`
  文件名过滤（零依赖极简匹配器），向后兼容；补 grep+glob 合一（原 architecture.md §8 遗留）。
- [X] F7  回放 harness 种子：`tests/replays/auth_refactor.jsonl` fixture + `kestrel-store`
  回放测试，钉死"改 Event 形状即破坏历史/fixture"这条地基铁律。测试从 12 涨到 35。

**明确暂缓（各需先立 ADR / 属里程碑级，非本轮范围）**：

- `[?]` **新会话端点** —— 与 ADR-0007"单人单会话"冲突；会话生命周期需先立 ADR（属朋友版方向）。
- `[?]` **grammar 约束采样（GBNF/json_schema）** —— 即创新种子 S1，明文要求先立 ADR + 定方向。
- `[ ]` **M2 机组 / 副手异地压缩** —— 承诺里程碑（16 子任务，含高风险影子槽 spike），
  需真加载第二个模型，非一次清扫可负责任完成。JobKind 路由已就绪，池子未建。

---

## [~] 地基（G）—— 横切约定，先于 M2

"现在几行、以后重构一周"那一类。趁代码还小锁进规范，避免铺开后回填几百个调用点。
清单与取舍见 [foundations.md](foundations.md)；硬规则见 [AGENTS.md §5.1](../../AGENTS.md)；i18n 决策见 [ADR-0008](../adr/0008-i18n-localization.md)。

- [X] G1  ADR-0008：i18n / 语言中立事件日志 / 可本地化错误（已定：zh-CN+en-US、模型侧英文豁免）
- [X] G2  AGENTS.md §5.1 地基铁律 + §10 自查项（i18n / 设计令牌 / 确定性 / 密钥 / 前向兼容）
- [X] G3  foundations.md 地基清单（Tier 1-3 + 待拍板项）
- [ ] G4  protocol/core：错误分类 `ErrorCode` + 结构化 params，替换 core/tools 里硬编码英文错误串
- [ ] G5  console：i18n 脚手架（`t(key,params)` + `src/i18n/{en-US,zh-CN}.json`）+ 迁移现有硬编码文本
- [ ] G6  kestrel-cli：用户可见 TUI/REPL 文本走 catalog（tracing 开发日志保持英文）
- [ ] G7  console：设计令牌审计——组件里硬编码颜色/间距/字号挪进 `@theme` 令牌
- [ ] G8  CI：加 i18n / 硬编码门禁（console 用 ESLint `no-literal-string` 类规则）
- [ ] G9  ADR-0011 + 实现：事件日志 `schema_version` 信封 + 容忍解析（只增不改字段）
- [ ] G10 密钥：`SecretString` 型（不 Debug 打印）+ 审计日志/事件/UI/提交无泄漏
- [ ] G11 locale 格式化（日期/数字 Intl/Rust）+ 时间戳统一 UTC 存
- [X] G12 ADR-0009：存储位置定为 OS 标准目录（`.kestrel/` opt-in + 版本化迁移）
- [ ] G13 kestrel-store：改用 OS 标准目录（`directories` crate）+ `layout_version` 迁移钩子 + 从旧 `./sessions` 迁移（ADR-0009）
- [ ] G14 fmt/clippy/test/deny + console build 全绿；提交并推送地基段

Tier 2 其余（配置前向兼容 / a11y 基线 / 路径沙箱 / API 版本化 / 稳定 ID）已给默认，无异议即按默认随各里程碑落地。目前无待拍板项。

## [ ] 模型启动器（L）—— 近期 on-ramp（ADR-0010）

把模型**作为 agent 的一部分**来启动，消灭"手动起服务器 + 忘 --jinja + token 鉴权"的摩擦。
薄监督器、不做第二个 Ollama：只启动/监督 llama.cpp + 委托已有宿主 + 纯连接回退。差异化 =
通用宿主结构上做不到的 agent 感知启动（强制 --jinja、探针、KV/机组/Loadout 接线）。

- [X] L1  ADR-0010：模型启动器决策（定位/三种来源/安全边界/被否决方案）
- [ ] L2  端口契约：`launch(spec) -> Handle` / `health` / `stop`（先定契约再写实现）
- [ ] L3  可行性 spike：自启 llama.cpp（spawn `--jinja` + 健康轮询就绪）+ **纯连接回退**（现状零启动）
- [ ] L4  委托已有宿主：检测 `lms` / `ollama` / 已在跑的 server，调用或直连
- [ ] L5  模型注册表：扫本地 GGUF 目录（如 `D:\AI\local\models`）出可选列表；不做下载/registry
- [ ] L6  参数调节 UI：ctx / 温度 / top_p / GPU 层 / flash-attn / MTP / 投机 → 控件（兑现"方便调每个参数"）
- [ ] L7  安全：引擎/模型白名单 + spawn/kill 走权限门可审计 + 进程树原子杀（Process Bloodline 咬合）
- [ ] L8  接能力探针（§5.4，骑 M3）：首启微基准 → 按模型@量化选协议/编辑格式，存 profile
- [ ] L9  接模型池/机组（骑 M2）与 Loadout（ADR-0006）：一份清单声明 模型@量化+参数+工具+权限，一条命令落地

L2/L3 近期即可起步（与地基/创新并行）；L8/L9 骑 M2/M3。先出 spike 验证自启+回退，再逐步接探针/池/Loadout。

## [?] 执行沙箱 + 后台命令（X）—— 用户提出 2026-07-03，待立 ADR

用户直接提出的两项执行侧能力。**方向已由用户拍定（非 brainstorm 候选）**，但均不在
architecture.md §9，按看板纪律先立 ADR 选型再上升为承诺 `[ ]`。二者同属"模型如何**安全、
异步**地跑命令/脚本"，可组合（后台作业默认也进沙箱）。

- [?] X1  **执行沙箱**：模型生成的脚本/命令在隔离环境里跑，而非直接落在宿主机上。
  注意与现有 Tier 2 的**路径沙箱**（工具层 `resolve_within` 防路径逃逸）不同——这是真正的
  **执行隔离**。候选选型（待 ADR 定）：受限子进程（降权 token/Job Object）/ 容器 / 轻量 VM /
  WSL 沙箱。要求：与权限门咬合（危险动作默认入沙箱、宿主直跑需显式提权）、与 Process Bloodline
  （brainstorm 候选，原子杀进程树）咬合、沙箱内可读写的目录/网络白名单化。
- [?] X2  **后台命令执行**（像 Claude Code 的 run_in_background）：shell 加 `background` 选项 +
  **长命令启发式识别**（按命令特征/预估耗时）自动转后台，不再阻塞整轮。配套：job 注册表
  （list / 查看输出 / kill）、输出捞到 session 级缓冲、WebUI"后台任务"面板显示运行/完成/实时输出、
  server 退出统一清理子进程（合"取消即杀子进程"铁律）。**已于 2026-07-03 与用户过一遍设计草案**
  （见对话），待 ADR 收口。眼下阻塞式 shell 至少已有回合内打断兜底。

拍板落地路径：X1/X2 各立一个 ADR（X1 重在隔离选型 + 安全边界，X2 重在 job 生命周期 + 长命令判定）
→ 写进 architecture.md §9 → 本表 `[?]` 升 `[ ]`。

## [?] Slash 命令面 + 能力驱动思考控制（U）—— 用户提出 2026-07-03，待立 ADR

用户提出：像 Claude Code 一样，这类操作都走 `/` 命令；且思考不应只是开关，要**读取模型
支持的思考强度再让用户选**。二者相关——思考强度选择就是命令面的一等命令。方向已拍定，
均不在 architecture.md §9，先立 ADR 再上升。

- [?] U1  **Slash 命令面**：WebUI 输入框 + CLI 统一的 `/` 命令层（像 Claude Code）。
  候选命令：`/think <off|low|med|high>`、`/model <name>`（接启动器 L）、`/new`|`/clear`
  （新会话，接会话生命周期，属朋友版方向）、`/policy <auto|on-request|strict>`、`/help`、
  `/quit`（CLI 已有）。设计要点：客户端解析 → 一部分映射 core `Op`、一部分本地 UI 动作、
  一部分需新 server 端点；输入 `/` 弹自动补全面板。ADR 要定「哪些命令是 core Op、哪些是前端糖」。
- [?] U2  **能力驱动的思考控制**（替换当前 `enable_thinking` 布尔开关）：
  - 探针 `BackendCapabilities` 增 `reasoning` 能力枚举：None（非推理模型）/ Toggle（仅开关，
    如 Qwen3 `enable_thinking`，当前 qwen35b 属此）/ Effort（分级，如 `reasoning_effort`
    low/med/high）/ Budget（思考 token 预算）。接 M3.3 能力探针（§5.4）。
  - 协议 `Op::UserInput.think: bool` 升级为 `thinking: ThinkingSetting`（Off/On/Effort(level)/
    Budget(n)）；保留旧字段兜底，走事件日志前向兼容（ADR-0011）。
  - 后端按能力映射线缆参数（`enable_thinking` / `reasoning_effort` / thinking budget）。
  - UI 按探到的能力渲染对应控件：开关 / 分段(低-中-高) / 滑条；并可 `/think <level>` 调。
  - 诚实约束：当前 qwen35b 原生只有 on/off，不假装分级；分级只对真支持的模型出现。

拍板落地路径：U1（命令面）+ U2（思考能力）各立 ADR（U2 咬合 M3.3 能力探针与 ADR-0011 前向兼容）
→ 写进 architecture.md §9 → 本表 `[?]` 升 `[ ]`。

## [ ] M2 剧场核心 —— 下一个承诺里程碑（architecture.md §6）

目标：把"独奏"升级成"主脑 + 副手"，让 §5.2 的异地压缩、§7 的影子槽预热长出一张脸。
纪律（§6.4）：成员间零表演式对话；编排是确定性代码不是 LLM 决策；副手永不阻塞主脑；低语而非喧哗。
验收：2 模型下压缩由副手异地完成、主脑 KV 前缀零扰动、机组账本显示"副手省了多少 token/prefill 秒"；
1 模型下自动坍缩回独奏、一切照常（§6.5 优雅降级零配置）。

### protocol

- [ ] M2.1  `CrewRole` 枚举(Lead/Copilot/Librarian/Critic) + 每个 `Event` 带 `actor: CrewRole`（纯追加字段，不碰 prompt 前缀）
- [ ] M2.2  `JobType` 枚举(Turn/Compact/Summarize/Prefetch/Retrieve/Review)——作业路由的输入

### core（kestrel-core，零 IO）

- [ ] M2.3  作业路由：`JobType -> CrewRole` 确定性映射（扩 crew.rs，纯代码，非 LLM 决策 §6.6.2）
- [ ] M2.4  异地压缩触发：ledger 逼近预算 ~85% 时派一次 Compact 作业给副手；主脑历史仍 append-only，压缩产物仅在**轮次边界**并入（§5.2）
- [ ] M2.5  副手永不阻塞：Compact/Summarize/Prefetch 跑独立 tokio 任务，产物就绪才在下一轮边界并入，没好则主脑照常推进（§6.4.3）
- [ ] M2.6  机组账本记账：新增账本事件（副手省下的 token 数 / prefill 秒数、命中次数）——把无形优化变可炫耀数字（§6.3）

### backend（kestrel-backend）

- [ ] M2.7  模型池：N 个 `LlmBackend` 实例，配置驱动，各指一个 llama-server slot / LM Studio 模型；逐模型健康探测（§6.6.1）
- [ ] M2.8  影子槽预热：llama.cpp `/slots/{id}?action=save|restore`，压缩后新前缀在备用 slot 后台预填、轮次边界原子切换（§7）。**风险高，先做可行性 spike**（slot 序列化稳定性）

### store / 配置

- [ ] M2.9  机组编制 TOML 表：哪个模型担任哪个角色，或 `auto` 按已加载模型自动分配（§6.6.5）

### server + console

- [ ] M2.10 server：actor 标签事件经 SSE 透传（事件已带 actor，适配器只需不丢字段）
- [ ] M2.11 console：机组车道渲染——主脑独占主车道，副手/书记为缩进暗色"低语"、可一键折叠（§6.3）
- [ ] M2.12 console：交接时刻可见（副手完成时一条细线流进主脑，显示摘要吞吐 `3200 tok -> 180 tok`）
- [ ] M2.13 console：机组账本面板（滚动统计副手省下的 token / prefill 秒、命中数）
- [ ] M2.14 优雅降级：1 模型坍缩回独奏单车道、2 模型主脑+副手；降级零配置（§6.5）

### 测试 / 收口

- [ ] M2.15 回放 fixture：机组事件（actor 标签 + 账本）确定性回放，无模型进 CI
- [ ] M2.16 fmt + clippy(-D warnings) + test + cargo-deny + console build 全绿；提交并推送 M2

---

## [ ] M3 全机组（architecture.md §6.2 / §7）

在 M2 的主脑+副手上补齐书记与审校，换模型零配置，回放测试进 CI。

- [ ] M3.1  书记 Librarian：本地语义记忆索引（嵌入模型）+ 按需检索相关片段，仅轮次边界递进（不碰前缀）
- [ ] M3.2  审校 Critic：高危动作在权限门前独立复核主脑计划，给"稳/险 + 一句理由"——权限确认从 y/n 变有依据的第二意见（§5.3）
- [ ] M3.3  能力探针 Capability Probing：接入新模型跑 ~30 秒微基准（原生 FC 可靠性 / SEARCH-REPLACE 成功率 / 指令遵循）→ 判定工具调用路线与编辑格式 → 存 `profiles/<model>.toml`（§5.4）
- [ ] M3.4  console：书记/审校车道；审校裁决嵌进权限审批卡
- [ ] M3.5  优雅降级：无审校则高危回退普通 y/n；无书记则退关键词检索（§6.5）
- [ ] M3.6  回放测试进 CI（Replay Harness）：录制事件日志 → LLM 响应变 fixture → 确定性外壳毫秒级进 CI（§7）
- [ ] M3.7  fmt/clippy/test/deny 全绿；提交并推送 M3

---

## [ ] M4 扩展（architecture.md §9 / ADR-0004 / ADR-0006）

工具面扩展 + 时间旅行 + 秒回 + 装备编组，并补上轮内取消。

- [ ] M4.1  browser 工具（CDP，非视觉）+ process 工具（系统管理）——扩到 <=10 内置工具，逐个算 schema token 账（原则 2）
- [ ] M4.2  状态树分支：slot save/restore 支撑会话分叉/回溯，是 Glass Engine 中期 Fork/Scrubber 的底座
- [ ] M4.3  投机代理（ADR-0004）：空闲算力预计算 2-3 个未来分支，命中即秒回——成本反转的落地
- [ ] M4.4  Loadout 装备编组（ADR-0006）：声明式清单 + 成本感知编译器（实时算 token 账、超预算即拒）+ 向导；权限永不随导入继承
- [X] M4.5  轮内取消：core 支持 turn 中途取消（已在"地基修复"轮提前落地，见上文 F4；
  前端 Stop 按钮后端已就绪，console 接线待办）
- [ ] M4.6  MCP 外接桥（v2 外接，内置工具仍走原生 trait）
- [ ] M4.7  fmt/clippy/test/deny 全绿；提交并推送 M4

---

## [ ] M5 夜班（architecture.md §9 / ADR-0004）

- [ ] M5.1  闲时自主家务：记忆蒸馏、索引、探针复跑、草稿起草
- [ ] M5.2  夜班报告：默认只读、可解释、可关闭、可审计（安全红线）
- [ ] M5.3  外联/自主行为的审计与开关（AGENTS.md §7）

## [ ] M6 睡眠周期（北极星，先立 ADR）

- [ ] M6.1  **先落一个 ADR**：本地 LoRA 自我进化的偏好数据管道、深睡训练、探针考试上岗——落地前不写代码（vision.md + AGENTS.md §7：训练只能由用户亲手输入的数据触发）

## [ ] 朋友版（原 v2，不改 core 一行）

- [ ] F.1  kestrel-server 叠认证 + 多会话隔离 + TLS——从"单人本机"扩到"给朋友用"

---

## [?] 创新种子小里程碑 —— 待拍板

来源：[innovation-brainstorm.md](innovation-brainstorm.md)（候选池，非承诺）。头号裁决 **The Glass Engine**：
把本地推理的隐形物理（KV 缓存 / 前缀 / token 预算 / 机组 / 投机）变成可看可抓可分叉的 UI——
云端结构上做不到，是抄不走的视觉身份。

**这一整块 `[?]`：每条落地前先立 ADR，用户先定方向（文末岔路未决）。不占 M2 的承诺位，可与 M2/M3 并行。**
我的默认推荐是先种下面 3 颗（各代表"骨 / 脸 / 血"，互不冲突、都低成本）：

- [?] S1  Grammar-Constrained 工具调用（+伤疤 schema）—— 骨。GBNF 约束根除畸形 JSON，本地小模型可靠性地基（brainstorm 排名 2，几乎白送）
- [?] S2  Glass Engine 起步：Cache Lens 只读热力图 + Budget 边框 + Cache-Bust 预警 —— 脸。只需给 ledger 加 `Event::CacheState`/`Event::Budget`，前端纯 CSS/SVG（排名 1，标注"近似"不假装精确）
- [?] S3  Complaint Dock + Correction Replay —— 血。情绪峰值零摩擦捕获偏好对，锚定 event seq，现在就攒 M6 要的数据（排名 4，vision §3.1 的 UI 落地）

其余候选（近期可起步，同样先 ADR）：Verified-Then-Commit 编辑（排名 5）、Vertical Starter Missions
（排名 3，增长引擎）、Regression Exam Cards（排名 6，失败变可分享基准）、Process Bloodline（排名 7，
原子杀进程树喂权限引擎）、Proof-of-Offline 回执、Context Ledger Backpressure（四行代码，复用 ADR-0005）。
中/远期与降优先项见 brainstorm 原文。

---

## 现在能动的 / 卡在哪

- **可立即开工（属已定路线）**：M2 剧场核心。建议从 M2.1/M2.2（protocol 加 actor/JobType）起步——
  纯类型、零风险、给后面所有机组任务铺地基。M2.8 影子槽预热风险最高，先单独做可行性 spike。
- **需你先拍板才动**：创新种子小里程碑（S1-S3 及其余）。见下方岔路——这些还没进 architecture.md，
  只是候选池，你不定方向 + 不立 ADR 我不擅自开工。

### 待你定的岔路（来自 brainstorm，morning review 未决）

1. **头号创新**：Glass Engine 作 signature 视觉身份，同意？还是把 Vertical Starter Missions（增长引擎）提到头号？
2. **近期种几颗**：默认三选 S1+S2+S3（骨/脸/血），同意还是换？
3. **落地节奏**：创新种子插成 M2 之前的小里程碑，还是并进 M2/M3？
4. **代号**：Glass Engine / Soul Hash / Regression Cards 这些英文代号保留，还是换中文？

拍板后：选中项各立一个 ADR → 写进 architecture.md §9 → 在本表把对应 `[?]` 升成 `[ ]` 承诺任务。

---

## 进度日志

- `16e7cc8` M1→WebUI T0：kestrel-server(SSE+ops+health+sessions) + console 实时对话，端到端验证
- `8c70d94` T1：会话回放 + 只读 Settings + 抽出共享 Conversation 组件
- `f6c7785` innovation-brainstorm.md（4 视角综合）+ 看板进度
- `43f06f3` T2：助手 markdown 渲染 + 输入框自增高（Cancel 暂缓，并入 M4.5）
- 地基修复：Ledger 接线 + ContextBudget 事件 + probe 真实 n_ctx / read-before-edit 强制 /
  deny 预过滤 / 轮内取消（提前自 M4.5）/ llamacpp+lmstudio 专属后端 / search regex+glob /
  回放 harness 种子 fixture。测试 12→35，fmt+clippy+test+deny 全绿（详见上文"地基修复"段）
- `401a0fa` docs: 把在途文档收进 docs/planning/
- 本次：看板从"WebUI 单里程碑"升级为"项目级执行看板"，覆盖 M1→朋友版全路线图 + 待拍板创新种子区
- `2026-07-03` 记入用户提出的两项执行侧能力：X1 执行沙箱 + X2 后台命令（像 Claude Code），
  均 `[?]` 待立 ADR。同轮修复：审批决定落账（切页/重连不再重弹审批）+ WebUI 回合内打断按钮
- `2026-07-03` 再记 U1 Slash 命令面 + U2 能力驱动思考控制（读模型思考能力再让用户选，
  替换布尔开关），均 `[?]` 待立 ADR；U2 咬合 M3.3 能力探针

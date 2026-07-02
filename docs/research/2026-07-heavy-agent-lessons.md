# 重型 AI Agent 架构调研：保留 / 砍掉清单

调研时间：2026-07。目标：为本地优先的极简 Rust agent 提取"保留 / 砍掉"清单。

## 一、案例研究

### 1. OpenHands（原 OpenDevin）——"重"的解剖样本

**V0 核心抽象**：EventStream（pub/sub 事件总线）+ AgentController（生命周期监督）+ Runtime（Docker 沙箱抽象）。

**为什么重（官方自供状）**：
- EventStream 的 pub/sub 模型"非常令人困惑，引发各种线程/异步问题，消息顺序几乎无保证"——V1 直接废除；
- 单仓库混杂 agent 核心 + 两个 CLI + web server + React 前端 + 5 种 runtime provider；Docker 镜像近 10GB；
- 配置系统 140+ 字段、15 个类、2800 行配置代码；
- 强制 Docker 沙箱与 MCP（期望本地执行）冲突，导致工具双份实现。

**V1/SDK 的修正（即真正该保留的）**：
- AgentController + Runtime 坍缩为 Conversation + Workspace——"controller 没有挣到它的存在价值"；
- 事件溯源状态：append-only 不可变 EventLog + ConversationState 单一事实源 → 天然获得确定性回放、崩溃恢复、pause/resume；
- 沙箱改为 opt-in：默认 LocalWorkspace 进程内执行零开销，需要隔离时换 DockerWorkspace，agent 代码不变；
- 上下文压缩 LLMSummarizingCondenser：实测"API 成本最多降 2x，性能无退化"；
- 安全两件套：SecurityAnalyzer（工具调用风险分级）+ ConfirmationPolicy（高危动作等用户批准）。
- 成绩：SWE-Bench Verified 72.8%——砍掉重抽象不掉分。

链接：[The Path to OpenHands v1](https://www.openhands.dev/blog/the-path-to-openhands-v1) · [OpenHands SDK 论文 (arXiv:2511.03690)](https://arxiv.org/html/2511.03690v1)

### 2. Claude Code——"简单循环 + 确定性基础设施"的标杆

- **单线程主循环**：while 循环，模型有 tool call 就执行、追加结果、再调模型；单一扁平消息历史 = 完整审计轨迹。
- **关键比例**：仅约 1.6% 代码是 AI 决策逻辑，98.4% 是确定性基础设施（权限门、上下文管理、工具路由、恢复逻辑）。
- **权限系统**：7 种模式，deny 优先规则求值，被全局 deny 的工具在模型看到之前就预过滤；hook 拦截 + shell 沙箱纵深防御。
- **子代理**：隔离的新循环，只返回摘要不返回全史；子代理不能再生子代理。
- **上下文管理**：五级流水线（逐消息预算上限 → snip → microcompact → 读时折叠 → auto-compact 全量摘要）；触发阈值约 83-92%；CLAUDE.md 作为 Markdown 长期记忆。
- **并发模型**：无调度器；只读工具并行、写状态工具串行。
- **持久化**：会话 = append-only JSONL transcript，resume/fork 靠重放。
- **token 开销**：系统提示 + 27 个内建工具描述 + CLAUDE.md 约 14,328 tokens（200k 窗口的 7%）；单个工具结果默认截断 25,000 tokens。

链接：[Dive into Claude Code (arXiv:2604.14228)](https://arxiv.org/html/2604.14228v1) · [ZenML 分析](https://www.zenml.io/llmops-database/claude-code-agent-architecture-single-threaded-master-loop-for-autonomous-coding) · [系统提示归档](https://github.com/Piebald-AI/claude-code-system-prompts) · [token 追踪](https://dev.to/slima4/where-do-your-claude-code-tokens-actually-go-we-traced-every-single-one-423e)

### 3. Aider——编辑格式与 repo-map 的实证派

- **Repo-map**：tree-sitter 抽取符号 → 依赖图排名 → 默认 1000 token 预算，动态伸缩。
- **编辑格式谱系**：whole（整文件）/ diff（SEARCH/REPLACE 块）/ diff-fenced / udiff / editor-*。按模型自动选格式是核心工程经验。
- **实证数字**：GPT-4 Turbo 用 SEARCH/REPLACE 基线 20%，换 udiff 后 61%、懒惰降 3x。
- **四条 diff 格式设计原则**：Familiar（训练数据常见）、Simple（不要转义/行号）、High-Level（整块代码）、Flexible（解析宽容）。
- **对弱模型的官方建议**：量化本地模型跟不上复杂编辑提示时直接用 `--edit-format whole`。[Diff-XYZ 基准 (arXiv:2510.12487)](https://arxiv.org/abs/2510.12487)：search-replace 对大模型最好；带行号标记的 udiff 变体几乎全面垫底。

链接：[Edit formats](https://aider.chat/docs/more/edit-formats.html) · [Repo map](https://aider.chat/docs/repomap.html) · [Unified diffs](https://aider.chat/2023/12/21/unified-diffs.html)

### 4. 反面教材：AutoGPT / Open Interpreter / OpenClaw

- **AutoGPT**：开放式自治 → 无限循环、超过 4-5 步的目标基本达不成、token 成本失控、错误级联。教训：自治必须被迭代上限、预算、验证闸门约束。（[How to Fix AutoGPT](https://lorenzopieri.com/autogpt_fix/)）
- **Open Interpreter**：务实但环境依赖重、无权限分级直接执行代码。
- **OpenClaw（2026 年初的现象级教训）**：CVE-2026-25253 一键 RCE；21,639 → 40,214 个实例裸奔公网，35-63% 部署有漏洞；技能市场 10,700 个技能中 820+ 恶意。教训：**插件/技能市场 = 供应链攻击面；默认联网监听 = 灾难；权限系统不是可选项**。（[Reco](https://www.reco.ai/blog/openclaw-the-ai-agent-security-crisis-unfolding-right-now) · [Unit42](https://unit42.paloaltonetworks.com/openclaw-ai-supply-chain-risk/)）

### 5. 2025-2026 最佳实践共识

- Anthropic《[Building Effective Agents](https://www.anthropic.com/research/building-effective-agents)》：agent 本质 = "LLM 在循环中依据环境反馈使用工具"；最成功的实现不用复杂框架；ACI（Agent-Computer Interface）要像 UI 一样设计。
- Anthropic《[Writing Tools for Agents](https://www.anthropic.com/engineering/writing-tools-for-agents)》：工具描述当新员工入职文档写；分页/过滤/截断带合理默认值；返回高信号信息、把 UUID 解析成语义化文字。
- "Agent 就是个循环"运动：风向从多 agent 编排框架退回"单循环 + 好工具 + 好 harness"（[ghuntley.com/agent](https://ghuntley.com/agent/)）。

## 二、提取结论

### A. 每个严肃 agent 的最小组件集（KEEP）

| 组件 | 依据 |
| --- | --- |
| 单线程 while 循环 + 扁平消息历史 | Claude Code 全部押注于此；历史即审计轨迹 |
| 少量高质量工具（<= 10 个起步） | 工具质量优先于数量；每个 schema 都吃上下文 |
| 权限门（deny 优先 + 风险分级 + 确认策略） | Claude Code / OpenHands；OpenClaw 是没有它的下场 |
| 上下文管理（逐结果截断上限 → 阈值触发摘要压缩） | 25k/结果截断；约 83% 触发 compaction；Condenser 省 2x |
| append-only 事件日志（JSONL）= 持久化 + 回放 + resume | Claude Code transcript 与 OpenHands EventLog 殊途同归 |
| 硬性约束：迭代/预算上限、编辑失败重试反馈 | AutoGPT 的解药；aider 的 reflection |
| Markdown 项目记忆文件（惰性加载） | 最便宜的长期记忆 |
| 取消/中断（abort 传导到子进程） | 本地模型慢，可取消性更关键 |

### B. 企业重要 vs 膨胀（CUT）

**值得做（便宜且救命）**：审计日志（事件日志免费附带）、单文件配置、会话持久化/resume、取消、错误恢复、工具结果截断。

**砍掉（bloat）**：插件/技能市场、多 agent 编排框架（顶多留"子循环只回摘要、不可递归"一种）、绑死的 Web UI、pub/sub 事件总线、强制 Docker 沙箱（改 opt-in）、100+ provider 适配层、AgentController 式监督类。

### C. 关键数字速查

- Claude Code 固定开销约 14.3k tokens；工具结果截断 25k；compaction 触发约 83%；AI 决策逻辑仅占代码 1.6%
- aider repo-map 默认 1k tokens；udiff 使 GPT-4 Turbo 20% → 61%
- OpenHands V0 配置 2800 行、镜像约 10GB；V1 Condenser 省 2x；重构后 SWE-Bench Verified 72.8%
- AutoGPT：>4-5 步任务基本失败
- **对本地小模型的启示：14k 固定开销在 8k-32k 上下文里是致命的——系统提示 + 工具 schema 总预算应压到约 2-3k token 以内**

### D. 弱模型的文件编辑工具设计

1. 首选 SEARCH/REPLACE 块（精确字符串匹配替换）。
2. 绝对禁止：行号定位、需要模型算行数的 hunk header、带行标记的啰嗦格式。
3. 解析要宽容：容忍空白差异；匹配失败返回最近似片段喂回循环重试。
4. 给最弱模型留 whole 整文件回退。
5. 按模型能力分级切换格式（edit-format 做成 per-model 配置）。
6. 要求编辑前必须先 Read（防盲改）。

**一句话总纲**：循环本身 300 行就够；工程价值全在循环外的确定性外壳——权限门、截断、压缩、事件日志、宽容的编辑解析。

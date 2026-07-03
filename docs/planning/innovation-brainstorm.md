# 创新点头脑风暴 · 待评审

> 状态：草稿，供 morning review。这是方向候选池，不是承诺；任何一条要落地都先走 ADR。
> 生成方式：4 个独立视角各自阅读 vision/architecture/ADR 后产出，我（主脑）再做聚类、
> 批判、排序与裁决。原始点子约 38 个，本文档去重合并后保留精华，并给每条我的判断标签。
> 判断标签：**[劲爆]** 差异化+可行+惊艳 · **[种子]** 近期低成本可落 · **[远期]** 依赖 M4+ ·
> **[降优先]** 价值有限 · **[否决]** 不建议。

---

## 0. 一句话结论

你一直在找的那个"炸裂世界"的创新点，我的裁决是：**The Glass Engine（透明引擎）——
把本地推理的隐形物理（KV 缓存、前缀、token 预算、机组并行、投机分支）变成一个可看见、
可抓取、可分叉的界面对象。** 云端 agent 在架构上永远做不到（那些状态在别人的服务器上），
它同时满足"眼前一亮"、"只有 Kestrel 能做"、"是我们已有的 append-only 事件日志 + 本地 KV
的自然结果"三件事。这是我从 38 个点子里挑出的头号候选。

第二梯队三条与它咬合：**自由算力换可靠性**（让弱小模型可信）、**零摩擦自我进化捕获**
（喂养睡眠周期）、**垂直启动任务 + 回归考卷**（增长引擎 + 病毒式产物）。

---

## 1. 五大主题聚类

38 个点子自然收敛成 5 个主题。每个主题给"核心洞察 + 归入的点子 + 我的裁决"。

### A. 把隐形物理变成可操作的界面（The Glass Engine）

**核心洞察**：Kestrel 最重要的东西全是隐形的——KV 缓存边界、前缀稳定性、token 预算、
哪个机组成员在算、投机在预算什么。云端把这些藏在服务器后面且按 token 计费，用户永远
看不见也碰不到。我们本地拥有全部状态 + append-only 事件日志（状态 = fold(events)），
可以把它们变成**可视、可抓、可分叉、可回溯**的实体。这是别人抄不走的视觉身份。

归入的点子：

- **KV Cache Lens**（悬停会话标题 → 前缀热/冷区热力图，"我能看见我 agent 的工作记忆"）
- **KV-Cache Forking**（把进行中的会话分叉成多个 what-if 轨道并行跑，diff 后择一提交）
- **Plan Scrubber / 时间旅行**（底部时间轴拖动回溯事件日志，从任意点 branch 重来）
- **Prefix Cache-Bust Warning**（切 Loadout / 换 profile 前，若会打碎 KV 缓存，预警并
  估算 re-prefill 秒数，让用户择时批量变更）
- **Budget Thermometer**（内容边框随 token 预算升温而变琥珀，压缩完成时"呼气"回落）
- **Speculation Ticker**（右栏幽灵卡实时显示 agent 正在预计算的 2-3 个未来，命中即秒回、
  未命中即溶解——把"浪费的空闲算力"变成 UI 里最戏剧的瞬间）

**我的裁决 [劲爆]**：这是头号候选，但要分层落地，别一次吃成胖子。
- 近期种子（≤M3）：**Cache Lens 只读热力图** + **Cache-Bust Warning** + **Budget 边框**。
  三者都只需给 Context Ledger 加一个 `Event::CacheState { boundary, per_block_tokens }` /
  `Event::Budget { used_pct }`，前端纯 CSS/SVG。低成本、马上惊艳。
- 中期（M4，骑 slot save/restore + 投机代理）：**Fork** + **Scrubber** + **Speculation Ticker**。
- 风险：热力图/边界的准确性取决于 llama.cpp `/slots` 能报多少；先按 token 计数近似，
  标注"近似"，不要假装精确误导用户。Fork 依赖 slot 序列化跨量化稳定性（脆）。

### B. 用自由算力换可靠性（让弱小模型可信）

**核心洞察**：本地模型更小更弱，但 token 免费、延迟才贵（ADR-0004）。于是"花很多次
模型调用换一次可靠"在本地成立，在按 token 计费的云端不成立。这是"薄循环、厚外壳"
哲学的最强变现：外壳花自由算力把弱模型抬到可信。

归入的点子：

- **Grammar-Constrained Sampling + 伤疤 schema**（每次工具调用都上 GBNF 语法约束，
  从根上杜绝畸形 JSON；再按事件日志里该模型历史犯错的字段，收紧出一份"伤疤语法"，
  直接不允许它再吐 `"mode":"overwrite"` 这类幻觉 token）
- **Verified-Then-Commit 编辑**（edit 先写影子副本 → 确定性验证器解析 AST/JSON/语法 →
  8B 副手判"diff 是否匹配你说的改动" → 双过才落盘；失败把"第 47 行括号未闭合"这种
  具体错误喂回，而不是"编辑失败"）
- **Self-Consistency 投票**（高风险单答案步骤，8B 上并行跑 3-5 个草稿取多数，仅平票时
  才升 35B 仲裁——本地并行只花墙钟秒数，不花钱）
- **Confidence-Calibrated Escalation**（向 llama.cpp 要 `logprobs`，在工具参数生成时
  监测 token 熵；熵飙升=模型在猜，触发重采样/升 14B/反问用户，而不是幻觉一个答案）
- **Quantization-Ladder Arbitrage**（同请求同时跑 Q4 与 Q8，输出一致就用便宜的、发散
  就升贵的，长出"这类任务 Q4 够不够"的每用户标定）

**我的裁决 [种子/劲爆]**：这是把"玩具"变成"真好用"的骨干，价值最高但不最闪。
- 头号近期种子（≤M3）：**Grammar-Constrained 工具调用**。几乎零成本、消灭一整类失败，
  是本地小模型可靠性的地基。强烈建议第一个做。
- 次近期（≤M3）：**Verified-Then-Commit 编辑**（edit 是最高危工具，值得双保险）。
- 中期（M4-5，骑机组）：Self-Consistency 投票、logprobs 熵升级。
- 风险：logprobs 支持因 llama.cpp/LM Studio 版本而异（要能优雅降级）；语法约束对复杂
  嵌套 schema 偶尔编译失败（要回退无约束）；8B 判官本身会误判（语义检查设成建议、
  只在解析失败时硬拦）。

### C. 零摩擦捕获自我进化信号（喂养睡眠周期）

**核心洞察**：vision.md 的自我革命最缺的是"偏好对"——最难收集的信号。解法是在用户
情绪峰值（正被烂回答惹恼、正重读它时）用零摩擦交互就地捕获，事件日志天然提供证据锚点
（seq）。

归入的点子：

- **Complaint Dock**（每个助手块旁一个 flag，点开一行内联表单，就地把吐槽连同 seq 范围
  发给副手当书记，抽成结构化工单进夜班队列；块上留琥珀下划线记录，全程不离开对话）
- **Correction Replay**（把助手块拖进"编辑"，改成你想要的样子，自动格式化成
  `{rejected, chosen, context_seq}` 偏好对进训练队列；块上留金色下划线）
- **Inductive Tool-Schema Learning**（用户纠正的工具调用聚类，夜班用 8B 抽出"它违反的
  规则一句话"，写进该模型的 profile TOML 的工具描述里；规则见效则留、无效则升级到 LoRA）
- **Deterministic Macro Compilation**（夜班挖事件日志，同一工具序列被重复推导 3+ 次就
  参数化编译成命名宏，之后"跑宏 X？"毫秒执行、绕过模型）
- **Night Report as Morning Letter**（夜班产出不做成 dashboard，让副手用第一人称写一封
  ~150 字"你的 agent 写给你的信"：昨晚注意到什么、修了什么、还欠什么，作为次日首块，
  暖色调 + 月亮水印）
- **Soul Hash（灵魂指纹）**（每次 LoRA 训练后对 adapter 权重取确定性哈希，截成 8 词
  助记词显示："Kestrel · Orion-7 [maple ridge falcon dusk]"，权重变它就变，可分享、
  可 `restore --soul` 回滚）

**我的裁决 [种子]**：这是把 M6 愿景从"抽象后台"变成"日常可感"的关键，且近期就能种。
- 近期种子（≤M3）：**Complaint Dock** + **Correction Replay**。都只是新增 Op 类型 +
  副手书记任务 + 现有块的 hover 态。它们现在就开始攒 M6 需要的偏好数据（即使训练管道
  还没建，先"排队待训"）。这与 vision.md §3.1 的"吐槽驱动"完全一致，是它的 UI 落地。
- 中期（M5）：Morning Letter（依赖夜班）、Inductive schema learning、Macro compilation。
- 远期（M6+）：Soul Hash（依赖 LoRA 真落地）。
- 风险：用户会拿 Complaint Dock 发泄而非给可用反馈→副手书记要能把"纯情绪无证据"分类
  丢弃、不进训练（vision.md 的 provenance guard 已覆盖）；Correction 的风格化矛盾偏好
  要检测冲突；Macro 动态生成安全闭包需嵌入式 DSL（Rhai/Lua）而非裸 Rust。

### D. 离线/溯源做成可分享的护城河（垂直病毒式）

**核心洞察**：本地用户高度垂直（网安 CTF、政务离线、隐私）。离线 + 溯源不是口号而是
**架构可验证的事实**，能解锁云端 agent 法律上服务不了的受监管垂直；可分享的产物制造
网络效应。

归入的点子：

- **Vertical Starter Missions**（首跑问一句"你主要干嘛"，立刻载入对应 Loadout 并跑一个
  真任务而非教程："我们来对这个 CTF binary 做侦察"，60 秒内让新人产出，完成即得一个
  可晒的产物）
- **Regression Exam Cards**（每次吐槽/工具失败自动生成一张便携基准卡 `.krc`：冻结的
  事件日志切片 + 期望行为 + 确定性断言，用回放 harness 无需活模型即可复跑；社区一个
  git 仓库就是分布式基准套件，每张卡都是拉新面包屑）
- **Crew Bench Scores**（社区跑同一批卡，提交"哪个量化的 Qwen3-35B 真能用于工具调用"
  这种 HuggingFace 榜给不了的 agent-loop 实测数据；每次提交回链 Kestrel）
- **Proof-of-Offline / Privacy Perimeter**（会话结束签一份"本会话零出网"或"什么数据
  离开过机器"的本地签名回执，网安晒炫耀、政务/医疗当合规证据）
- **Loadout Forks & Ancestry**（每个 Loadout 带 parent 哈希 + changelog，`loadout diff`
  出人类可读 delta，社区像 git 一样 fork/追溯能力包谱系）
- **Dead-Drop Memory Bundles**（导出书记的向量索引成加密包，朋友导入即获域知识，不含
  工具/权限/原始数据——与 ADR-0006 的"权限不随导入继承"信任模型一致）
- **Local-First Relay**（两台机器直连加密隧道异步交接会话/Loadout/记忆快照，"我卡住的
  问题，你工作站的 70B 也许能啃"，每次交接是一次拉新转化）

**我的裁决 [种子/劲爆]**：这是"火"的引擎。
- 头号近期种子（≤M3）：**Vertical Starter Missions**。"装上就为 CTF 直接干活"比"它是个
  通用 agent"传播快得多；Rust 单二进制 + Loadout 让"60 秒离线可用"这句话可信。这是
  真正的增长引擎。
- 次近期（≤M3）：**Regression Exam Cards**。把失败变成可分享的基准，回放 harness 已有，
  一个 git 仓库就是社区基准。每张卡回链 Kestrel。
- 近期（≤M3）：Proof-of-Offline 回执（权限引擎已跟踪 External 风险，薄薄一层报告）。
- 中期（M4-5）：Crew Bench、Loadout ancestry、Memory Bundles、Relay。
- 风险：合规是慢销售周期，先靠网安"炫耀式分享"起量；Starter Mission 的步骤必须只用
  确定性结果步骤（读文件/grep/shell），模型输出只做旁白，否则一次失败的首体验砸口碑。

### E. OS 原生纵深（只有"在机器上"的 agent 能做）

**核心洞察**：Kestrel 操控的是真机——完整文件系统、进程、硬件、传感器。这里有一批
云端结构上做不到的差异化能力。

归入的点子：

- **Process Bloodline**（每个 shell 命令套进 Windows Job Object / Linux cgroup，实时
  资源计账 + 原子杀整棵进程树 + 意外子孙进程可见 → 喂回权限引擎当 Destructive 信号）
- **Crew Heartbeat**（顶栏四个小点代表机组，谁在算谁呼吸，critic 在权限模态前突然亮
  琥珀——让系统像活物）
- **Ghost Edit**（edit 落盘前把 diff 渲染成可拖动分隔线的叠加，用户"擦"出新版本再批准，
  可点行内联"建议改动"回传）
- **Memory-Mapped Tool-Output Cache**（按 tool+args+文件 mtime 哈希缓存只读工具输出，
  新会话跳过重复读文件/grep）
- **Thermal-Aware Scheduling**（读 GPU 温度，凉时爆发预计算、热时只做夜班）
- **Hardware Fingerprint / TPM 溯源**（用 TPM 派生设备密钥签事件日志与 LoRA adapter，
  给"本机本人产出"可验证溯源链）
- **Context Ledger Backpressure**（预算进危险区时，往尾部注入一行"[budget: 2100 tok
  剩余，请精简]"，模型自压缩，绝不碰前缀）

**我的裁决 [种子/降优先混合]**：
- 近期种子（≤M3，安全价值高）：**Process Bloodline**。让"操控你真机"这件事可信——
  原子杀进程树 + 意外 spawn 检测喂权限引擎，是真价值且只有本地能做。
- 近期种子（≤M3，几乎白送）：**Context Ledger Backpressure**（尾部注入，复用 ADR-0005
  的披露机制，四行代码）。
- 中期：Ghost Edit（高危 edit 的直接操作隐喻，惊艳）、Memory-mapped cache（隐形提速）。
- 降优先：**Thermal-Aware Scheduling**（每厂商传感器差异大、价值边际）、**Crew Heartbeat**
  （装饰性，nice-to-have，不是差异化核心）。
- 远期：TPM 溯源（依赖 LoRA 存在才有意义；VM 里跑的"本地"用户无 TPM）。

---

## 2. 劲爆 tier 排名（我的最终推荐序）

按"差异化 × 惊艳 × 可行"综合，给你一个可直接拍板的短名单：

| 排名 | 创新 | 主题 | 可行 | wow | 一句话为什么是它 |
|---|---|---|---|---|---|
| 1 | **The Glass Engine**（先 Cache Lens + Budget 边框 + Cache-Bust 预警） | A | 近期起步 | 5 | 唯一"云端结构上做不到 + 眼前一亮 + 是已有数据的自然结果"三合一，视觉身份抄不走 |
| 2 | **Grammar-Constrained 工具调用（+伤疤 schema）** | B | 近期 | 4 | 几乎白送，消灭一整类畸形调用失败，本地小模型可靠性的地基 |
| 3 | **Vertical Starter Missions** | D | 近期 | 5 | "装上就为 CTF 直接干活"是真正的增长引擎，比"通用 agent"传播快得多 |
| 4 | **Complaint Dock + Correction Replay** | C | 近期 | 4 | 零摩擦在情绪峰值捕获偏好对，现在就开始喂 M6 自我革命，vision 的 UI 落地 |
| 5 | **Verified-Then-Commit 编辑** | B | 近期 | 3 | 最高危工具的双保险，把"敢让它改我代码"变可信 |
| 6 | **Regression Exam Cards** | D | 近期 | 5 | 把失败变成可分享的基准，回放 harness 已有，每张卡回链拉新 |
| 7 | **Process Bloodline** | E | 近期 | 4 | 原子杀进程树 + 意外 spawn 喂权限引擎，让"操控真机"可信，只有本地能做 |
| 8 | **Speculation Ticker**（幽灵卡） | A | 中期(M4) | 5 | 把"浪费的空闲算力"变成 UI 最戏剧的瞬间，ADR-0004 成本反转的可视化高潮 |

注：1-7 都在近期（≤M3）可起步，与现有 M2/M3 路线不冲突、可并行。第 8 骑 M4 投机代理。

---

## 3. 协同关系（它们不是孤立的，是一台机器）

一个闭环，值得整体看：

```
用户吐槽/纠正 (Complaint Dock + Correction Replay, C)
      │  在情绪峰值零摩擦捕获，锚定 event seq
      ▼
每个失败 → 一张可分享基准卡 (Regression Cards, D)  ── 回链拉新 ──▶ 社区增长
      │
      ├─▶ 伤疤 schema / 工具描述补丁 (Inductive learning, B/C)  ── 立刻见效 ──▶ 少犯错
      │
      └─▶ 夜班蒸馏 → LoRA (M6) → Soul Hash 可见变化 (C)  ── 个人护城河
                                    │
                                    ▼
              早晨一封信告诉你昨晚它进步了 (Morning Letter, C)  ── 情感黏性

同时，全程 The Glass Engine (A) 让你看见 KV/预算/机组/投机在发生什么，
Grammar 约束 + Verified 编辑 (B) 让弱模型每一步都可信，
Vertical Starter Mission (D) 让新人 60 秒进入这个闭环。
```

即：**Glass Engine 是脸，Free-Compute 可靠性是骨，自我进化捕获是血，垂直+卡片是繁殖器。**
它们互相喂养，不是四个独立 feature。

---

## 4. 明确降优先 / 否决（诚实的取舍）

- **[降优先] Thermal-Aware Scheduling**：每厂商 GPU 传感器差异大、抽象成本高、价值边际。
- **[降优先] Crew Heartbeat / Budget Thermometer**：装饰性讨喜，但不是差异化核心，等
  Glass Engine 主体成型后作为点缀。
- **[远期，别现在碰] Soul Hash、Morning Letter**：情感回报强，但都依赖 M6 LoRA 真落地，
  现在做是空中楼阁。先把 Complaint/Correction 的数据管道种下，权重那头到 M6 再说。
- **[远期] TPM/硬件指纹溯源**：VM 里跑的"本地"用户没 TPM，回退指纹可伪造，安全保证打折；
  等有 adapter 可签时再评估。
- **[谨慎] KV-Cache Forking、Quantization Arbitrage、Macro Compilation**：都很酷，但分别
  卡在 slot 跨量化稳定性、双模型 VRAM 占用、安全动态代码执行——列为中期，落地前各自
  先做一次可行性验证 spike。

---

## 5. 给你早上定的岔路

1. **头号创新拍板**：Glass Engine 作为 Kestrel 的signature视觉身份，同意吗？还是你更想
   把某个第二梯队（比如 Vertical Starter Missions 的增长引擎）提到头号？
2. **近期做几个**：我建议近期只种 3 个种子——**Grammar 约束（可靠性地基）+ Cache Lens/
   Budget 边框（Glass Engine 起步）+ Complaint Dock（自我进化起步）**——各自小、互不冲突、
   分别代表"骨/脸/血"。同意这个三选，还是换？
3. **落地节奏**：这些要不要在 M2（机组核心）之前插一个"WebUI + 可靠性打磨"的小里程碑，
   还是并进 M2/M3？
4. **强调色/命名**：Glass Engine、Soul Hash、Regression Cards 这些代号你喜欢吗，还是要
   换成中文代号？

我的默认推荐：岔路 1 选 Glass Engine，岔路 2 选那三个种子，岔路 3 插一个小里程碑先把
WebUI 打磨到"每天想用"。你醒了改令牌一样——这些都还没写进 architecture.md，只是候选池。

---

## 附录：完整点子目录（按视角，含我的标签，不丢任何一条）

### 视角一 · 硬件原生/系统
- KV-Cache Forking — 会话分叉 what-if 并行 diff 择一 · **[远期]** slot 跨量化稳定性存疑
- Thermal-Aware Scheduling — 凉时爆发预计算 · **[降优先]**
- Quantization-Ladder Arbitrage — 双量化交叉验证 · **[远期]** VRAM 占用
- Replay Bus 确定性回归 — 事件日志零 LLM 回放测试 · **[种子]** 直接强化 CI，harness 已有
- Memory-Mapped Tool-Output Cache — 缓存只读工具输出 · **[中期]** 隐形提速
- Hardware Fingerprint 溯源 — TPM 签日志/adapter · **[远期]**
- Prefix Cache-Bust Warning — 换 Loadout 前预警打碎 KV · **[种子/劲爆]** 归入 Glass Engine
- Process Bloodline — Job Object 管进程树 · **[种子/劲爆]** 排名 7
- Context Ledger Backpressure — 尾部注入预算压力 · **[种子]** 四行代码，复用 ADR-0005

### 视角二 · 交互/UX
- Plan Scrubber 时间旅行 — 拖回溯 + branch · **[中期]** 归入 Glass Engine
- Speculation Ticker 幽灵卡 — 看 agent 押注你下一步 · **[劲爆]** 排名 8
- Complaint Dock — 内联就地吐槽锚定 seq · **[种子/劲爆]** 排名 4
- Budget Thermometer — 边框随预算升温 · **[种子]** 归入 Glass Engine
- Ghost Edit — 擦出 diff 新版再批准 · **[中期]** 高危 edit 的直接操作
- Night Report as Morning Letter — 第一人称晨信 · **[远期]** 依赖夜班
- Crew Heartbeat — 机组呼吸点 · **[降优先]** 装饰性
- Correction Replay — 编辑助手块成偏好对 · **[种子/劲爆]** 排名 4
- KV Cache Lens — 前缀热/冷热力图 · **[劲爆]** Glass Engine 近期头号，排名 1

### 视角三 · 小模型智能放大
- Grammar-Constrained + 伤疤 schema · **[劲爆]** 排名 2
- Self-Consistency 投票 · **[种子]** 中期骑机组
- Verified-Then-Commit 编辑 · **[种子/劲爆]** 排名 5
- Recursive 分解 + 能力宽度估计 · **[中期]** 依赖能力探针
- Deterministic Macro Compilation · **[远期]** 需安全 DSL
- Confidence-Calibrated Escalation (logprobs 熵) · **[中期]** 依赖 logprobs 支持
- Context-Length-Aware Prompt Skinning (knapsack) · **[种子]** 与 Loadout 咬合，保前缀稳定
- Inductive Tool-Schema Learning · **[种子]** 归入自我进化，写 profile TOML
- Parallel Hypothesis + Cheapest-Falsification · **[中期]** 诊断类任务结构化搜索

### 视角四 · 生态/护城河/病毒
- Proof-of-Offline Badge — 零出网签名回执 · **[种子]** 权限引擎已跟踪 External
- Regression Exam Cards — 失败变便携基准 · **[劲爆]** 排名 6
- Loadout Forks & Ancestry — git 式能力包谱系 · **[中期]** 一字段扩展 ADR-0006
- Crew Bench Scores — 社区本地模型 agent 榜 · **[中期]** 拉新回链
- Privacy Perimeter Declaration — 数据出境声明 · **[种子]** 解锁受监管垂直
- Local-First Relay — 朋友机对机会话交接 · **[中期]** NAT 穿透是坎
- Vertical Starter Missions — 60 秒垂直可用 · **[劲爆]** 排名 3，增长引擎
- Weight Fingerprint (Soul Hash) — LoRA 助记词身份 · **[远期]** 依赖 M6
- Dead-Drop Memory Bundles — 共享向量索引不含原始数据 · **[中期]** 跨嵌入模型可移植性

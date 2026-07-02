# Kestrel · AGENTS.md

> 本文件是本仓库的长期协作约定，适用于 AI 助手、自动化工具和人类协作者。
> 它只写稳定规则，不写短期计划、里程碑目标或某一版实现细节——那些在
> [docs/architecture.md](docs/architecture.md) 的路线图与各 [ADR](docs/adr/) 里。

修改仓库前先读本文件。若本文件与当前任务冲突，先说明冲突并与维护者确认，不要自行绕过。

---

## 1. 项目定位

**Kestrel** 是专为本地部署模型（llama.cpp / LM Studio）设计的轻量 agent。

一句话立场：市面上的 agent 是"为云端 API 设计、顺便兼容本地"；Kestrel 反过来，
把本地推理的物理约束——prefill 慢、上下文小、KV 缓存宝贵——当一级设计约束。

关键入口：

- [README.md](README.md)：项目门面、构建与运行。
- [docs/architecture.md](docs/architecture.md)：架构设计、设计原则、核心设计、路线图（唯一设计事实源）。
- [docs/adr/](docs/adr/)：架构决策记录（每个重大选型的备选方案、否决理由、重开条件）。
- [docs/research/](docs/research/)：竞品与技术调研（架构结论的证据基础）。
- [docs/vision.md](docs/vision.md)：北极星愿景（方向宣言，非当前承诺）。

---

## 2. 设计铁律（不可协商）

这些是 Kestrel 区别于其他 agent 的根本，违反即拒绝，不是风格问题。完整论证见
[docs/architecture.md](docs/architecture.md) 的"设计原则"。

1. **前缀稳定性。** system prompt、工具定义、消息历史前缀必须逐字节确定。任何让前缀
   变得不确定的改动（时间戳、随机排序、原地改写历史、动态注入到已缓存区）都会打碎
   KV 缓存、触发本地 30 秒级 re-prefill——直接拒绝。动态信息只允许追加到消息尾部。
2. **固定 token 开销 <= 2.5k。** 新增工具、扩 system prompt、改 schema 前先算 token 账。
   全部工具 schema 总预算 <= 1400 token。
3. **循环薄、外壳厚。** 决策逻辑尽量少，工程价值在确定性外壳（权限门、截断、压缩、
   事件日志、宽容解析）。
4. **单线程主循环 + append-only 事件日志。** 不引入 pub/sub 事件总线、不引入 actor 框架
   （见 [ADR-0002](docs/adr/0002-style-library-core-event-stream.md)）。会话状态 = 事件折叠，禁止旁路可变状态。
5. **权限门不是可选项。** Kestrel 操控用户的真实机器。破坏性/外联动作在任何策略档位都
   必须可被用户拦截。不得削弱权限门来图方便。

---

## 3. 工作原则

1. **先理解，后修改。** 先读相关文档与现有代码，不要跳过上下文直接改。
2. **边界即 crate。** 依赖方向铁律：`前端 -> core <- 适配器`。core 不依赖任何适配器
   crate，适配器之间互不依赖，共享类型下沉到 `kestrel-protocol`。由 `cargo-deny` 强制。
3. **结构先于内容。** 文件放到正确目录。根目录只保留长期入口文件。
4. **决策即资产。** 重大取舍落到 ADR，不要只存在对话或提交信息里。
5. **可复现是底线。** 涉及效果、性能、行为的结论，必须说明数据、步骤或脚本。
6. **小步修改。** 一次改动只解决一个清晰问题，不要把无关重构、格式化和内容改动混在一起。

---

## 4. 目录约定

仓库目录应保持清晰、自解释。新增目录前先确认其长期职责。

```text
kestrel/
├── README.md                 项目门面、构建与运行
├── AGENTS.md                 本文件：仓库协作约定
├── CONTRIBUTING.md           贡献者实操流程（构建、检查、提交）
├── Cargo.toml                workspace 根：统一 lints / profile / 依赖版本
├── deny.toml / rustfmt.toml  依赖白名单、格式化配置
├── kestrel.example.toml      配置示例（kestrel.toml 不入库）
├── docs/
│   ├── architecture.md       架构设计（唯一设计事实源）
│   ├── adr/                  架构决策记录（NNNN-标题.md，编号递增）
│   ├── research/             调研报告（YYYY-MM-主题.md）
│   └── vision.md             北极星愿景
├── crates/
│   ├── kestrel-protocol/     纯类型，零逻辑零 IO，被所有 crate 依赖
│   ├── kestrel-core/         agent loop / ledger / permission / crew / ports，零 IO
│   ├── kestrel-backend/      LlmBackend 实现（唯一碰 LLM HTTP 的 crate）
│   ├── kestrel-tools/        内置工具（实现 core 的 Tool 端口）
│   ├── kestrel-store/        事件日志 / 配置 / profile（Store 端口）
│   ├── kestrel-cli/          终端前端（组装根 + 事件渲染器）
│   └── kestrel-server/       WebUI 后端适配器（axum：SSE 推事件 + POST 收 Op）
├── console/                  WebUI 前端（React+Vite+Tailwind），纯前端，不进 Rust 依赖图
├── profiles/                 内置模型 profile（探针可覆盖为 *.local.toml）
└── tests/replays/            回放测试 fixture（.jsonl）
```

放置规则：

- 设计、原则、路线 -> `docs/architecture.md`
- 重大决策 -> `docs/adr/`（新增编号文件，永不删除，被推翻的标记 Superseded）
- 调研、外部参考 -> `docs/research/`
- 会话数据、编译产物、本地配置（`sessions/`、`target/`、`kestrel.toml`、`*.local.toml`）-> 不入库

不要把临时笔记、调试脚本、导出报告、截图散落在根目录。

---

## 5. 代码约定

技术栈以工程配置为准（Rust 2024，见 `Cargo.toml`）。本文件只规定通用要求：

- **语言分工**：代码、注释、标识符、commit、日志输出一律英文；设计文档中文（专有名词、
  协议名、代码标识符保留英文）。
- **禁用 emoji 与装饰性 Unicode 符号**，代码/注释/文档/日志/TUI 一律纯文本。角色与状态
  用文本标签（如 `[主脑]`、`[ok]`、`[!]`）。TUI 保留 box-drawing 结构字符即可。
- **每个 crate 的 `lib.rs` 顶部模块文档**声明职责边界与禁止依赖，改动语义时同步更新。
- **公开 API 必须有文档注释**（`missing_docs` 为 warn，CI 里 warning 即 error）。
- **端口只建在"确定有第二个实现"的边界**（LlmBackend / Tool / Store）。其余写朴素直接的代码，
  不做过度抽象。
- 外部输入必须校验；工具的文件操作不得越出工作目录。
- 不为临时验证引入长期依赖；确需引入时说明用途，并确认 `deny.toml` 许可证与禁用清单。
- 提交前本地跑：`cargo fmt --all` + `cargo clippy --workspace --all-targets -- -D warnings` +
  `cargo test --workspace`。CI 以同样命令外加 `cargo deny check` 作硬门槛。

---

## 6. 架构与决策约定

- 先明确边界，再讨论实现；先定义端口契约，再写实现。
- 重大架构或选型变化走 ADR：在 `docs/adr/` 新增编号递增的文件，写明背景、备选方案对比、
  裁决理由、被否决方案的最强论点、重开条件。旧决策被推翻时标记 `Superseded by NNNN`，
  原文保留。
- 架构文档要说明取舍、失败模式、降级策略与未解决问题。
- 涉及性能/延迟的声明要说明测量方式，不要把未验证效果写成结论。
- 愿景（vision.md）与当前设计（architecture.md）分开：愿景是方向，不是承诺；把愿景里的
  东西写进代码前，先落一个 ADR。

---

## 7. 安全红线

Kestrel 能执行命令、读写文件、访问网络——它操控的是用户的真实机器。因此：

绝不提交：

- 密钥、令牌、API Key、真实凭据、Cookie。
- 用户的会话数据、真实文件内容、未脱敏日志。
- 模型权重、数据集、大文件下载物。
- 未审查来源的第三方二进制或脚本。

行为红线：

- 不得削弱、绕过或默认关闭权限门。破坏性（删除/覆盖/写系统目录）与外联动作默认必须
  可被用户拦截。
- 工具的文件操作必须约束在工作目录内，拒绝路径逃逸。
- 不做插件/技能市场式的自动加载不可信代码（OpenClaw 教训，见调研）。
- 训练/自我进化相关能力（见 vision.md）只能用用户亲手输入的数据触发，文件/网页内容
  无权触发训练管道。
- 外联行为必须可解释、可关闭、可审计。

---

## 8. Git 规范

采用 Conventional Commits，英文，一次提交聚焦一件事：

```text
<type>(<scope>): <one-line summary>

<body: what changed, why, and impact. English.>
```

- `type`：`feat` / `fix` / `docs` / `chore` / `refactor` / `test` / `perf`。
- `scope`：crate 短名（protocol / core / backend / tools / store / cli）或 `docs` / `ci`。
- 正文说明动机与影响范围。
- 提交或推送只在用户明确要求时进行；若在默认分支上，先开分支。
- 不提交临时产物、密钥、大文件；不使用破坏性 Git 命令覆盖他人改动。
- commit message 结尾附 `Co-Authored-By` 署名（若由 AI 协助）。

---

## 9. AI 助手工作方式

1. 先判断任务属于文档、架构、ADR、实现还是验证。
2. 读取最小必要上下文，不要凭空假设；不确定就查代码或文档，别猜。
3. 涉及方向、架构、技术栈、目录结构、设计铁律时，先说明影响并等待确认。
4. 修改前说明将改什么；修改后说明改了什么、为什么、如何验证。
5. 发现文档或设计冲突时，指出冲突并给建议，不要静默选择。
6. 发现无关脏改动时不要顺手回滚，只处理当前任务相关内容。

回答要求：

- 默认中文，直接、清晰、少空话。
- 评价方案时优先指出风险、缺口和改进建议。
- 结论先行；给其他 AI 的提示词要能直接复制执行。

---

## 10. 交付前检查

完成非平凡改动前自查：

- 文件是否放在正确目录，根目录是否只有长期入口文件。
- 是否更新了相关文档、ADR、交叉引用。
- 是否混入临时文件、会话数据、日志或敏感信息。
- 是否违反任一设计铁律（第 2 节），尤其前缀稳定性与 token 预算。
- 依赖方向是否合规（`cargo deny check` 是否通过）。
- 是否本地跑过 fmt / clippy / test。
- 若涉及架构或选型，是否落了 ADR、说明了取舍与失败模式。

---

## 11. 事实优先级

发生冲突时，优先级如下：

1. 用户当前明确指令。
2. 本文件 `AGENTS.md`。
3. `docs/architecture.md`（设计事实源）。
4. `docs/adr/` 中状态为 Accepted 的决策记录。
5. `docs/research/` 中的调研结论。
6. 代码现状与注释。
7. `docs/vision.md`（方向，非承诺）。
8. 草稿、历史讨论和临时笔记。

冲突影响决策时，先提出问题，不要擅自改写事实。

---

## 修订记录

| 版本 | 日期 | 变更 |
|---|---|---|
| v1 | 2026-07-02 | 初稿：确立项目定位、设计铁律、目录/代码/文档约定、安全红线、Git 规范、AI 协作流程、事实优先级 |

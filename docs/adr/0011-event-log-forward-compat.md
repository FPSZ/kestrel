# ADR-0011 事件日志前向兼容：schema 信封 + 容忍解析 + 只增不改

| | |
| --- | --- |
| 状态 | Accepted |
| 日期 | 2026-07-03 |

## 背景

会话状态 = `fold(events)`（ADR-0002）：事件日志（`sessions/<id>.jsonl`，一行一个
[`kestrel_protocol::Event`]）是唯一的事实源，同时被 resume、会话回放、以及回放测试
harness（§7）消费。这意味着 **`Event` 的序列化形状是一个长期契约**——它一旦落盘，就要
被未来所有版本读回。

这是 [foundations.md](../planning/foundations.md) #6，属"现在几行、以后重构一周"那一类：
第一次天真地改 `Event` 形状（加个必填字段、重命名一个变体、给 `serde` 加
`deny_unknown_fields`），就会让**所有人的历史会话 + 全部 fixture** 反序列化失败。M1 起步时
`replay` 还是"任一行解析失败即整段 `Err` 返回"，一条坏行能让整个会话读不出——脆弱。

## 决策

三条一起构成前向兼容地基，全部 Tier 1（[AGENTS.md §5](../../AGENTS.md)）：

### 1. 只增不改不复用（字段与变体）

`Event` / `EventPayload` 及其嵌套结构：**只许追加**新字段（带 `#[serde(default)]`）与新变体。
**禁止**改已有字段的含义 / 类型、删字段、复用旧字段名作他用、改变体的 `snake_case` 线名、
改 `seq` 的语义。真要做破坏性变更，走第 3 条的版本递增 + 迁移，不得原地改。

### 2. 容忍解析（未知变体 / 未知字段 / 坏行都不 fail）

- **未知变体**：`EventPayload` 加 `#[serde(other)] Unknown` 单元变体。更新版本写入的、
  本版本还不认识的 `type`，反序列化落到 `Unknown` 而非报错；折叠 / 渲染时忽略。永不主动产生。
- **未知字段**：不启用 `deny_unknown_fields`——serde 默认忽略多出来的字段（新版加的字段对旧版透明）。
- **坏行**：`JsonlStore::replay` 单行解析失败**跳过并 `warn`**，不再整段 `Err`。兜的是截断写入 /
  真正损坏的行——一条坏数据不该毁掉整段历史。
- **前端对称**：console 的 `fold` 用 `switch(payload.type)` 无 `default` 抛错分支——未知 `type`
  自然被忽略，与 Rust 侧同构。

### 3. schema 版本信封 + 迁移钩子

- `kestrel_protocol::EVENT_LOG_SCHEMA_VERSION`（当前 `1`）是事件日志格式的规范版本号。
- `JsonlStore` 首次写入某数据目录时，落一个目录级 `.schema_version` 标记（`create_new` 原子写、
  只写一次），记录该批日志的写入版本——供未来破坏性迁移工具识别"这批日志是 schema vN 写的"。
- 破坏性变更（且仅此时）递增 `EVENT_LOG_SCHEMA_VERSION` 并加迁移钩子；日常的加字段 / 加变体
  **不**动版本号（那是靠第 1、2 条兼容，不需要迁移）。

与存储**布局**版本（ADR-0009 的 `layout_version`，管的是目录搬迁）正交：一个管"文件放哪"，
一个管"每行长啥样"。

## 备选方案对比

| 方案 | 取舍 | 裁决 |
| --- | --- | --- |
| 每行带 `v` 字段（逐记录版本） | 精确到记录，但每行都膨胀、且 99% 的行版本相同——噪声 | 否决：目录级标记足够，日志体积保持干净 |
| 无版本、纯靠 serde 容忍 | 最省事 | 部分采纳（容忍是核心），但缺"这批是哪版写的"的迁移锚点 |
| 独立 sidecar 元数据文件 + 首行 header 记录 | header 记录会破坏"每行都是一个 Event"的不变量 | 否决：污染 `fold(events)` 的纯粹性 |
| `deny_unknown_fields` + 严格 schema | 早暴露拼写错误 | 否决：与前向兼容直接冲突，旧代码读新日志必炸 |

## 被否决方案的最强论点（诚实记录）

"逐记录版本 + 严格 schema"在**多写者、强审计**的系统里是对的：每条记录自证版本、未知字段即
拒绝，能第一时间抓住格式漂移。我们否决它是因为 Kestrel 是**单进程、单写者、本地**的 append-only
日志，写者永远是当前版本的自己；真正的风险不是"写入脏数据"，而是"新代码读不了自己的旧日志"。
在这个约束下，容忍解析 + 目录级版本锚点用最小体积买到最需要的那条兼容性。若将来演进到多写者
（朋友版多会话 / 分布式），应重开本 ADR，届时逐记录版本与更严的校验可能重新占优。

## 重开条件

- 演进到多写者 / 跨进程并发写同一日志（朋友版、分布式）。
- 需要一次真正破坏性的 `Event` 形状变更（届时递增版本 + 写迁移钩子，并在此追加记录）。
- 事件体量大到目录级标记不足以支撑分批 / 增量迁移。

## 落地

- 硬规则：[AGENTS.md §5](../../AGENTS.md)（前向兼容铁律，Tier 1）。
- 实现：`kestrel-protocol`（`EVENT_LOG_SCHEMA_VERSION` + `EventPayload::Unknown`）、
  `kestrel-store`（`replay` 容忍解析 + `.schema_version` 标记）、console `fold`（未知 type 忽略）。
- 任务：[roadmap-board.md 地基（G）段](../planning/roadmap-board.md) G9。

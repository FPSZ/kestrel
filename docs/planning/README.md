# 在途文档（planning）

工作性、临时性的文档：任务看板、里程碑计划、头脑风暴、候选池。

与 `architecture.md` / `adr/` 的区别：**这里不是事实源**。一旦某个方向定案，结论上升到
`architecture.md`（设计事实源）或落一个 `adr/`（决策记录），本目录对应文档随之归档或删除。
换句话说，这里放的是"还在想 / 正在做"的东西，不是"已经定了"的东西。

| 文件 | 内容 | 状态 |
| --- | --- | --- |
| [roadmap-board.md](roadmap-board.md) | 项目级执行看板：对齐 architecture.md §9，跟踪 M1→朋友版全路线图落地进度 + 待拍板创新种子 | 进行中 |
| [foundations.md](foundations.md) | 地基约定清单：i18n 等"现在不定以后很麻烦"的横切约定（Tier 1-3 + 待拍板） | 进行中 |
| [innovation-brainstorm.md](innovation-brainstorm.md) | 创新点候选池（4 视角综合，待评审拍板） | 待评审 |

## 纪律

- 不放事实源：设计结论去 `architecture.md`，决策去 `adr/`。
- 定案后收敛：方向选定就把结论上升，别让候选池长期与事实源并存造成分裂。
- 命名自解释：执行看板 `*-board.md`，计划类 `*-plan.md`，头脑风暴 `*-brainstorm.md`。

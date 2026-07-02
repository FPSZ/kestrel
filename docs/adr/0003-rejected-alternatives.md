# ADR-0003 已否决方案速查

| | |
| --- | --- |
| 状态 | Accepted |
| 日期 | 2026-07-02 |

集中登记散落在架构文档各节的否决决定，便于速查。详细论证见对应章节。

| 方案 | 一句话否决理由 | 详见 |
| --- | --- | --- |
| pub/sub 事件总线 | OpenHands 亲手废除的路线（消息顺序无保证、调试地狱） | architecture.md §3.2 |
| 纯 MCP 内置工具（goose 路线） | 每次调用多一层 JSON-RPC，schema 不受控且费 token | architecture.md §3.2 |
| 依赖 rig-core 做地基 | 差异化恰在它抽象掉的那层（slot/KV/GBNF 需要直接控制 HTTP 请求体） | architecture.md §3.2 |
| Go | 类型系统弱一档（边界表达力），本赛道无同级先例 | architecture.md §3.4 / ADR-0001 |
| 插件/技能市场 | OpenClaw 820+ 恶意技能的供应链教训 | architecture.md §2.2 |
| 强制 Docker 沙箱 | 与本地执行冲突；改为 opt-in（OpenHands V1 的修正） | architecture.md §2.1 |

MCP 作为**外接桥**（非内置工具机制）规划于 M4，届时另立 ADR 评审。

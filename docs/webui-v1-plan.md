# WebUI 个人版 · 任务看板

按优先级推进。`[x]` 已完成并提交，`[~]` 进行中，`[ ]` 待办。
设计与架构见 [ADR-0007](adr/0007-webui-browser-axum.md)；本表只跟踪落地进度。

铁律不变：`kestrel-server` 是适配器（可依赖 core），`console` 通过 HTTP 契约通信、
不进 Rust 依赖图；core 一行不改。

---

## T0 — 让 WebUI 真正跑起来（端到端流式对话）

这是 must-have：在浏览器输入消息 -> agent 执行 -> 事件流式回渲。做完 T0 即提交。

- [x] T0.1  `kestrel-server` crate 骨架：Cargo.toml + workspace 成员 + 依赖（axum / tower-http / tokio-stream）
- [x] T0.2  组装 + AppState：加载 config、构建 agent、spawn run、事件泵（mpsc -> broadcast）
- [x] T0.3  `GET /api/events`（SSE）：store 快照追平 + broadcast 实时，按 seq 去重
- [x] T0.4  `POST /api/ops`：解析 Op 灌进 agent 的 op 通道
- [x] T0.5  `GET /api/health`：server 存活 + model / base_url / session / workdir
- [x] T0.6  release 下静态托管 `console/dist`（ServeDir）
- [x] T0.7  Rust 侧 fmt + clippy(-D warnings) + deny + test 全绿
- [x] T0.8  console：`KestrelClient`（SSE 订阅 + POST sendOp）+ 事件类型（对齐 protocol）
- [x] T0.9  console：事件折叠 store（events -> 回合 / 消息 / 工具调用），与 core 的 fold 同构
- [x] T0.10 console：流式对话渲染（AgentText 增量 / ToolCall / ToolResult / TurnCompleted / Error）
- [x] T0.11 console：发送框接 `POST /api/ops`（Enter 发送，回合内禁用）
- [x] T0.12 console：health -> 顶栏状态灯（connected / model）
- [x] T0.13 console typecheck + build 全绿
- [~] T0.14 提交并推送 T0

端到端已验证（无需真实 LLM）：POST user_input -> agent -> SSE 回传 seq0 user_input + seq1
error（后端 :8080 未起，如实报错）。整条链路 browser -> op 通道 -> agent -> event -> store+SSE 打通。
T0 顺带含最小内联审批（pending 工具块上 Approve/Deny）与自动滚动，本属 T1/T2，先落以求可用。

## T1 — 权限与会话（安全、可导航）

- [x] T1.1  console：审批 UI -> Approve / Deny（做成内联在工具卡上，比模态更贴合扁平风；模态留作 T2 可选）
- [x] T1.2  server：`GET /api/sessions`（列表）+ `GET /api/sessions/{id}/events`（回放）（T0 已建）
- [x] T1.3  console：会话列表 + 点击载入历史（只读回放视图，复用 Conversation，interactive=false）
- [x] T1.4  console：SSE 断线重连（EventSource 原生重连 + 客户端按 seq 去重，快照重放幂等）
- [~] T1.5  提交并推送 T1

顺带把对话渲染抽成共享的 `Conversation` 组件（live 与回放复用），并加了只读 Settings（读 /api/health）。

## T2 — 打磨

- [!] T2.1  console：Cancel（停止）按钮 -> `Op::Cancel` —— **暂缓**：core 只在审批点读
      `Op::Cancel`（agent.rs 明确把轮内取消留到 M2），现在放按钮会是"假控件"，不做。
      待 M2 core 支持轮内取消再接。
- [x] T2.2  console：助手文本 markdown 渲染（react-markdown + remark-gfm，扁平深色样式，
      内联 code 芯片 / 围栏 code 块，流式渐进解析）
- [x] T2.3  console：自动滚动（T0 已做）、空状态（已做）、输入框随内容自增高；错误已内联红卡
      渲染，toast 暂不需要
- [x] T2.4  console：键盘（Enter 发送 / Shift+Enter 换行，T0 已做）
- [~] T2.5  提交并推送 T2

---

## 夜间产出（T0 之后）

- [x] 深挖创新点，写成 [docs/innovation-brainstorm.md](innovation-brainstorm.md)，供早上评审。
      方式：4 个独立视角各自读 vision/architecture/ADR 后产出约 38 个点子，主脑聚类批判排序。
      头号裁决：**The Glass Engine**（把 KV/预算/机组/投机的隐形物理变成可看可抓可分叉的 UI）。

## 进度日志

- `16e7cc8` T0 全部：kestrel-server（SSE+ops+health+sessions）+ console 实时对话，端到端已验证
- `8c70d94` T1：会话回放 + 只读 Settings + 抽出共享 Conversation 组件
- （本条随提交更新）innovation-brainstorm.md + 看板进度

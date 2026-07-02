# ADR-0007 WebUI 个人版：浏览器 + axum，前端只是"第二个前端"

| | |
| --- | --- |
| 状态 | Accepted |
| 日期 | 2026-07-02 |

## 背景

CLI 够用但"看着不舒服"，用户要一个精致界面，先个人单机用（不急朋友版）。路线图原把
WebUI 放在 v2（`kestrel-server` + 多会话 + 认证，给朋友用）。现在提前，但砍到个人版：
单人、本机、无认证。

这不改变任何设计铁律——WebUI 和 CLI 一样，是"事件渲染器 + Op 发送器"，`core 一行不改`
（[原则 依赖方向](../architecture.md)）。前端状态 = 对事件流做 fold，与 core 的
"state = fold(events)"同构。

## 决策

### 传输：新增 `crates/kestrel-server`（axum 适配器）

它本质是"[repl.rs](../../crates/kestrel-cli/src/repl.rs) 搬到 HTTP 上"，与 `kestrel-cli` 平级兄弟：

| 端点 | 作用 |
| --- | --- |
| `GET /api/events`（SSE） | 把 core 原生 append-only 事件流逐条 serde 成 SSE 推给前端 |
| `POST /api/ops` | 接 `Op`（用户输入、审批决定）灌进 agent 的 op 通道 |
| `GET /api/sessions` + replay | 列历史会话、回放（`JsonlStore` 已支持） |
| 静态资源 | release 下由 server 托管 `console/` 构建产物；dev 下 Vite 代理 `/api` |

- 默认只绑 `127.0.0.1`，不暴露公网。v1 无认证（单人本机 localhost 无意义）。
- 权限门端到端保留：agent 发审批 Event -> 前端弹模态 -> 决定回传为 Op，与 CLI 的
  inline y/N 同一条回合契约（铁律 5 不破）。
- 认证 / 多会话隔离 / TLS = 朋友版的加法，将来在**同一个 server 上叠**，零重写。

### 前端：根目录 `console/`（全新，不抄结构）

- 栈：React 18 + TypeScript + Vite + Tailwind v4 + Radix（可访问的模态 / 滚动区）+ lucide 图标。
- transport-agnostic 接缝 `KestrelClient`：`subscribe(onEvent)` + `sendOp(op)`。v1 用
  SSE + fetch 实现；将来若要原生窗口，换成 Tauri IPC 实现即可，**UI 代码不动**。
- 依赖方向：`console` 是纯前端，只通过 HTTP 契约与 server 通信，**不进 Rust 依赖图**；
  `kestrel-server` 是适配器（可依赖 core），符合 `前端 -> core <- 适配器`。

### 设计语言：扁平化深色，只借一个结构

- 只从用户既有作品（Fulcrum console）借**一个结构**：顶栏 + 侧栏连成一块连续的磨砂面
  （二者都是透明子元素），内容区是磨砂里一块内缩 + 圆角 + 发丝边的"显示器边框"卡。
- **不抄其玻璃质感**：去掉顶角受光高光与 glossy 内斜面，做**扁平**的深色高端风
  （Linear / Raycast / 获奖作品那一路）——克制的单强调色、发丝边分层而非重投影、精确留白。
- 设计令牌集中在 `console/src/index.css` 的 `@theme`，改风格只动令牌，便于迭代。

### v1 交付边界（分阶段落地）

1. **外壳 + 设计语言**：连体磨砂深色壳，可 `npm run dev` 直接看。
2. **server 打通**：`kestrel-server` 的 SSE / ops，聊天回合流式渲染。
3. **功能补齐**：权限审批模态、会话列表回放。

不做：Loadout 编辑器、重设置页——往后放。

## 被否决方案的最强论点（诚实记录）

- **Tauri 原生桌面窗口**：独立 App 窗口 + 系统 vibrancy，更"App"、更像 Linear 桌面版。
  否决理由：多一套 Tauri / WebView2 工具链；IPC 桥是单机专用，朋友版仍需 axum，可能丢弃
  这层桥。浏览器 + axum **零丢弃**，就是 v2 服务器提前；CSS `backdrop-filter` 足够出磨砂。
  保留 `KestrelClient` 接缝后，将来加 Tauri 壳成本很低——不是放弃，是不提前付费。

## 重开条件

- 用户要原生窗口 / 系统级磨砂 -> 加一层 Tauri 壳，复用 `KestrelClient` 接缝，UI 不动。
- 要给朋友用 -> 在 `kestrel-server` 上叠认证 + 会话隔离 + TLS（即原 v2），不新起架构。
- 单机单会话不够（并发多会话）-> 需配合机组 / 会话隔离（M2/M3），届时另评估。

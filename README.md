# Kestrel

**专为本地部署模型（llama.cpp / LM Studio）设计的轻量 agent。**

市面上的 agent 是"为云端 API 设计、顺便兼容本地"；Kestrel 反过来：把本地推理的物理约束——prefill 慢、上下文小、KV 缓存宝贵——当一级设计约束。云端 agent 不在乎的地方，正是 Kestrel 的主场。

> 状态：早期开发中（M1 骨架阶段），API 不稳定。

## 为什么再造一个 agent

- **固定 token 开销 <= 2.5k**（对比 Claude Code 约 14.3k）——在 32k 本地窗口里这是生死线。
- **KV 前缀稳定性是铁律**：消息历史 append-only，压缩由独立进程的副手模型异地完成，主模型缓存零扰动。
- **双模型剧场**：主脑（大模型）写码规划，副手（小模型）后台压缩/摘要，书记（嵌入模型）检索记忆，审校（中模型）复核高危动作——全部是真实并行工作的可视化，随显存自动增减，单模型也能跑。
- **能力探针**：接入新模型自动跑 30 秒微基准，生成工具调用协议与编辑格式 profile，换模型零配置。
- **Loadout 装备编组**：本地用户高度垂直（网安打 CTF、政务管流程、家庭自动化各不相同）。用声明式清单把工具/权限/机组/人设编组成一份可分享的专用版本，编译器实时算 token 账、超预算即拒——把 2.5k 约束变成 UX，不是又一个连线玩具（[ADR-0006](docs/adr/0006-loadout-declarative-build.md)）。
- **成本模型反转**：第一个为"token 免费、延迟贵"设计的 agent——空闲算力用于投机预计算（体感秒回）与夜班自主家务；北极星是本地 LoRA 自我进化：云端 agent 记笔记，Kestrel 做梦（[docs/vision.md](docs/vision.md)）。

完整设计见 [docs/architecture.md](docs/architecture.md)；决策记录与调研报告见 [docs/](docs/README.md)；
协作约定见 [AGENTS.md](AGENTS.md)，贡献流程见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## Workspace 结构

| Crate | 职责 |
| --- | --- |
| `kestrel-protocol` | Event/Op/ToolSpec 等纯类型，零逻辑 |
| `kestrel-core` | agent loop、context ledger、权限引擎；定义全部端口 trait，零 IO |
| `kestrel-backend` | LlmBackend 实现：llama.cpp / LM Studio / OpenAI 兼容兜底 |
| `kestrel-tools` | 内置工具：shell / fs / search（browser 规划中） |
| `kestrel-store` | JSONL 事件日志、模型 profile、配置、Loadout 清单 |
| `kestrel-runtime` | 模型启动器/监督器：自启 llama.cpp / 委托宿主 / 纯连接（ADR-0010） |
| `kestrel-cli` | 终端前端（事件的渲染器） |
| `kestrel-server` | WebUI 后端适配器（axum：SSE 推事件 + POST 收 Op） |

`console/`（非 Rust crate）是 WebUI 前端（React + Vite + Tailwind），通过 HTTP 契约与
`kestrel-server` 通信，不进 Rust 依赖图。依赖方向铁律：`前端 -> core <- 适配器`，
core 不依赖任何适配器。

## 构建与运行

```sh
cargo build --release
cargo test

# 1) 起一个 OpenAI 兼容后端
#    llama-server -m model.gguf --jinja --port 8080
#    或 LM Studio -> Developer -> Start Server
# 2) 配置
cp kestrel.example.toml kestrel.toml   # 按需改 base_url / model / n_ctx
# 3) 运行
./target/release/kestrel
```

M1 是回合制终端 REPL：输入消息，agent 用 read/search/edit/shell 四个工具在工作目录内
干活，写动作走权限门确认。会话事件写入 OS 标准数据目录的 `sessions/*.jsonl`
（Windows `%LOCALAPPDATA%\Kestrel`、Linux `~/.local/share/kestrel`、macOS
`~/Library/Application Support/Kestrel`；[ADR-0009](docs/adr/0009-storage-layout.md)）——
想让数据留在项目里就在启动目录建个 `.kestrel/`（opt-in），或设 `KESTREL_DATA_DIR`。
设 `RUST_LOG=kestrel=debug` 看详细日志。

## 模型启动器（把模型当 agent 的一部分来起）

上面第 1 步要你手动起后端。想让 Kestrel 自己把模型起起来，就给它一份 Loadout 清单
（[ADR-0010](docs/adr/0010-model-launcher.md)）：在 `kestrel.toml` 里设
`loadout = "kestrel.loadout.toml"`，清单的 `[model]` 维度覆盖 `[backend]`。三种来源：

- `source = "self"`——自启 llama.cpp：spawn `llama-server`，强制 `--jinja` +
  `--host 127.0.0.1`，轮询 `/health` 就绪后再连；退出时自动 kill。`bin` / `model_path`
  必须是**绝对路径**（白名单=显式配置），只绑回环、不自动联网（[ADR-0010 §5](docs/adr/0010-model-launcher.md)）。
- `source = "delegate"`——委托已在跑的宿主（lms / ollama / 手起 llama-server）：可达才用，
  不代起、不代杀。
- `source = "connect"`——纯连接一个 `base_url`，零启动（等价 `[backend]` 现状）。

清单里 `persona` / `tools` / `permission` / `crew` 等维度是 [ADR-0006](docs/adr/0006-loadout-declarative-build.md)
的格式草案，当前**解析但不编译**（成本感知编译器随 M4 落地）。模板见
[kestrel.example.loadout.toml](kestrel.example.loadout.toml)：

```sh
cp kestrel.example.loadout.toml kestrel.loadout.toml   # 改 bin / model_path / n_ctx
# 在 kestrel.toml 里取消注释 loadout = "kestrel.loadout.toml"
./target/release/kestrel                               # 启动器自动起模型、就绪后进入 REPL
```

## WebUI（个人版）

扁平深色的浏览器控制台，与 CLI 平级——同一个 core 的"第二个前端"（[ADR-0007](docs/adr/0007-webui-browser-axum.md)）。
`kestrel-server` 把 core 的事件流经 SSE 推给浏览器、经 HTTP 收回 Op；只绑 `127.0.0.1`，
单人本机、无认证。

```sh
# 开发（两个进程，前端热更新）
cargo run -p kestrel-server            # axum on 127.0.0.1:4321
npm --prefix console install           # 首次
npm --prefix console run dev           # Vite on :7823，代理 /api -> :4321
# 打开 http://localhost:7823

# 发布（单二进制托管前端）
npm --prefix console run build         # 产出 console/dist
cargo run -p kestrel-server --release  # 直接托管 dist，打开 http://127.0.0.1:4321
```

功能：流式对话（助手文本 markdown 渲染 / 工具卡 / 内联权限审批 / 错误）、会话回放、
只读设置、顶栏实时连接状态。设计令牌集中在 [console/src/index.css](console/src/index.css)。

## License

MIT OR Apache-2.0，任选其一。

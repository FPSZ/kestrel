# Kestrel

**专为本地部署模型（llama.cpp / LM Studio）设计的轻量 agent。**

市面上的 agent 是"为云端 API 设计、顺便兼容本地"；Kestrel 反过来：把本地推理的物理约束——prefill 慢、上下文小、KV 缓存宝贵——当一级设计约束。云端 agent 不在乎的地方，正是 Kestrel 的主场。

> 状态：早期开发中（M1 骨架阶段），API 不稳定。

## 为什么再造一个 agent

- **固定 token 开销 <= 2.5k**（对比 Claude Code 约 14.3k）——在 32k 本地窗口里这是生死线。
- **KV 前缀稳定性是铁律**：消息历史 append-only，压缩由独立进程的副手模型异地完成，主模型缓存零扰动。
- **双模型剧场**：主脑（大模型）写码规划，副手（小模型）后台压缩/摘要，书记（嵌入模型）检索记忆，审校（中模型）复核高危动作——全部是真实并行工作的可视化，随显存自动增减，单模型也能跑。
- **能力探针**：接入新模型自动跑 30 秒微基准，生成工具调用协议与编辑格式 profile，换模型零配置。
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
| `kestrel-store` | JSONL 事件日志、模型 profile、配置 |
| `kestrel-cli` | 终端前端（事件的渲染器） |

依赖方向铁律：`前端 -> core <- 适配器`，core 不依赖任何适配器。

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
干活，写动作走权限门确认。会话事件写入 `sessions/*.jsonl`。设 `RUST_LOG=kestrel=debug`
看详细日志。

## License

MIT OR Apache-2.0，任选其一。

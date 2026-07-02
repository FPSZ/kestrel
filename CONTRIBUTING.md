# 贡献指南

先读 [AGENTS.md](AGENTS.md)——它是本仓库的协作契约（设计铁律、目录/代码约定、
安全红线、Git 规范）。本页只讲怎么把改动提交上来。

## 环境

- Rust 2024（`rust-version` 见 `Cargo.toml`），`rustup` 装 stable 即可。
- 起一个 OpenAI 兼容后端联调：llama-server（`--jinja`）或 LM Studio。
- 运行方式见 [README.md](README.md)。

## 提交前自检

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check          # 依赖白名单 + 许可证 + 依赖方向
```

CI 以同样的命令作为硬门槛，任何一项失败即拒绝合并。warning 即 error。

## 提交流程

1. 从最新 `main` 开分支，一个分支解决一个清晰问题。
2. 遵守 [AGENTS.md](AGENTS.md) 的设计铁律——尤其前缀稳定性、token 预算、依赖方向。
   重大设计/选型变更先在 [docs/adr/](docs/adr/) 落一个 ADR。
3. 新功能附带测试；涉及 agent 确定性行为的，优先用回放 fixture（`tests/replays/`）。
4. commit 用英文 Conventional Commits：`feat(core): ...` / `fix(backend): ...` / `docs: ...`，
   scope 用 crate 短名。
5. 开 PR，说明动机与影响范围。

对照 [AGENTS.md 第 10 节](AGENTS.md) 的交付前检查清单自查后再提。

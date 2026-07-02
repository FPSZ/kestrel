# 贡献指南

感谢关注 Kestrel。在提交代码前请读完这一页——规则不多，但每条都会被 CI 强制。

## 提交前自检

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI 会以同样的命令（外加 `cargo deny check`）作为硬门槛，任何一项失败即拒绝合并。

## 架构纪律

动手前先读 [ARCHITECTURE.md](ARCHITECTURE.md)，特别是：

1. **依赖方向铁律（§4.1）**：`前端 -> core <- 适配器`。core 不得依赖任何适配器 crate；适配器之间互不依赖；共享类型一律下沉到 `kestrel-protocol`。
2. **前缀稳定性（原则 1）**：任何让 system prompt、工具定义、历史前缀变得不确定的改动（时间戳、随机排序、原地改写历史）都会被拒绝——这不是风格问题，是产品的性能命脉。
3. **固定 token 预算（原则 2）**：新增工具或修改 schema 前先算 token 账；预算表在 ARCHITECTURE.md。
4. **重大设计变更走 ADR**：在 ARCHITECTURE.md 附录 A 追加决策记录，写明备选方案与否决理由。

## 代码规范

- 每个 crate 的 `lib.rs` 顶部模块文档声明职责边界与禁止依赖，改动语义时同步更新。
- 公开 API 必须有文档注释（`missing_docs` 是 warn，CI 里 warning 即 error）。
- 不使用 emoji 或装饰性 Unicode 符号（代码、注释、文档、日志输出一律纯文本）。
- 新功能附带测试；涉及 agent 行为的，优先用回放 fixture（`tests/replays/`）。

## 提交信息

采用 [Conventional Commits](https://www.conventionalcommits.org/)：`feat(core): ...` / `fix(backend): ...` / `docs: ...`。scope 用 crate 短名（protocol/core/backend/tools/store/cli）。

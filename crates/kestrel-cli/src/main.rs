//! Kestrel 终端前端。
//!
//! 职责边界：本 crate 是组装根（唯一同时依赖 core 与全部适配器的地方）
//! 与事件渲染器。所有决策逻辑都在 core，前端不含业务规则。
//!
//! M1 形态：极简 REPL（读一行 -> 提交 Op -> 渲染事件流）。
//! ratatui TUI 与机组车道渲染在 M2 引入。

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kestrel=info".into()),
        )
        .init();

    println!(
        "kestrel {} (M1 骨架，主循环尚未接线)",
        env!("CARGO_PKG_VERSION")
    );

    // TODO(M1): 加载 kestrel.toml -> 构建 LlamaCppBackend/JsonlStore/工具注册表
    //           -> 注入 core::agent -> REPL 循环（提交 Op、渲染 Event 流），
    //           届时恢复 fn main() -> anyhow::Result<()>。
}

//! Kestrel 终端前端。
//!
//! 职责边界：本 crate 是组装根（唯一同时依赖 core 与全部适配器的地方）
//! 与事件渲染器。所有决策逻辑都在 core，前端不含业务规则。
//!
//! M1 形态：回合制 REPL（读一行 -> 提交 Op -> 渲染事件流直到本轮结束）。
//! ratatui TUI 与机组车道渲染在 M2 引入。

mod repl;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use kestrel_core::{Agent, AgentConfig, PermissionEngine, TurnLimits};
use kestrel_protocol::{Event, Op, SessionId};
use kestrel_store::{Config, JsonlStore};
use tokio::sync::mpsc;

/// 精简 system prompt（前缀稳定、控制在低 token）。
const SYSTEM_PROMPT: &str = "\
You are Kestrel, a coding agent running locally on the user's machine.
Tools: read(path), search(pattern), edit(path,search,replace), shell(command).
Rules:
- Read a file before editing it.
- edit replaces an exact SEARCH block with a REPLACE block; match existing text exactly.
- Prefer search/read to understand before acting.
- Keep replies short. When the task is done, give a one-line summary and call no tool.
Paths are relative to the working directory.";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kestrel=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let config = Config::load(&PathBuf::from("kestrel.toml")).context("load kestrel.toml")?;
    let workdir = std::fs::canonicalize(&config.workdir).unwrap_or_else(|_| config.workdir.clone());
    // 展示用：去掉 Windows 扩展长度路径前缀 \\?\。
    let workdir_display = workdir
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();

    let backend = kestrel_backend::build(
        &config.backend.kind,
        config.backend.base_url.clone(),
        config.backend.api_key.clone(),
        config.backend.model.clone(),
        config.backend.n_ctx,
    );
    // 探测真实上下文长度喂给 context ledger（失败优雅回退配置值）。
    let n_ctx = match backend.probe().await {
        Ok(caps) => {
            tracing::info!(
                n_ctx = caps.n_ctx,
                native_tools = caps.native_tool_calls,
                "probed backend"
            );
            caps.n_ctx
        }
        Err(e) => {
            tracing::warn!(
                "probe failed ({e}); using configured n_ctx {}",
                config.backend.n_ctx
            );
            config.backend.n_ctx
        }
    };
    let store = Arc::new(JsonlStore::new(config.sessions_dir.clone()));
    let mut tools = kestrel_tools::builtin();
    let denied = tools.deny(&config.deny_tools);
    if denied > 0 {
        tracing::info!(denied, "deny-listed tools removed from tool set");
    }
    let permission = PermissionEngine::with_deny(
        parse_policy(&config.approval_policy),
        config.deny_tools.clone(),
    );

    let agent = Agent::new(
        backend,
        tools,
        store,
        permission,
        AgentConfig {
            system_prompt: SYSTEM_PROMPT.to_owned(),
            workdir: workdir.clone(),
            max_tool_output: 8_192,
            n_ctx,
            limits: TurnLimits::default(),
        },
    );

    let (op_tx, op_rx) = mpsc::channel::<Op>(32);
    let (event_tx, event_rx) = mpsc::channel::<Event>(256);
    let session = SessionId(format!("cli-{}", std::process::id()));

    let agent_handle = tokio::spawn(async move { agent.run(session, op_rx, event_tx).await });

    println!(
        "kestrel {} — backend {} · model {} · workdir {}",
        env!("CARGO_PKG_VERSION"),
        config.backend.base_url,
        config.backend.model,
        workdir_display
    );
    println!("输入消息开始对话，/quit 退出。\n");

    repl::run(op_tx, event_rx).await?;

    match agent_handle.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(anyhow::anyhow!("agent: {e}")),
        Err(e) => Err(anyhow::anyhow!("agent task: {e}")),
    }
}

fn parse_policy(s: &str) -> kestrel_core::ApprovalPolicy {
    use kestrel_core::ApprovalPolicy;
    match s {
        "auto" => ApprovalPolicy::Auto,
        "strict" => ApprovalPolicy::Strict,
        _ => ApprovalPolicy::OnRequest,
    }
}

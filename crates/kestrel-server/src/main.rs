//! Kestrel WebUI backend (axum adapter).
//!
//! 职责边界：与 `kestrel-cli` 平级的第二个前端适配器——组装 core + 全部适配器，
//! 把 core 的有序 `Event` 流经 SSE 推给浏览器，把浏览器的 `Op` 经 HTTP 灌回 core。
//! 决策逻辑全在 core，本 crate 只做传输编解码与事件扇出（ADR-0007）。
//! core 一行不改：这里复用 `kestrel-cli` 相同的 `Agent::run` 契约。

mod http;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use kestrel_core::{Agent, AgentConfig, PermissionEngine, TurnLimits};
use kestrel_protocol::{Event, Op, SessionId};
use kestrel_store::{Config, JsonlStore};
use tokio::sync::{broadcast, mpsc};

use http::AppState;

/// 精简 system prompt（与 CLI 一致，前缀稳定、低 token）。
/// 与 `kestrel-cli` 的同名常量保持一致；未来抽到共享的组装默认值（见任务看板 T2）。
const SYSTEM_PROMPT: &str = "\
You are Kestrel, a coding agent running locally on the user's machine.
Tools: read(path), search(pattern), edit(path,search,replace), shell(command).
Rules:
- Read a file before editing it.
- edit replaces an exact SEARCH block with a REPLACE block; match existing text exactly.
- Prefer search/read to understand before acting.
- Keep replies short. When the task is done, give a one-line summary and call no tool.
Paths are relative to the working directory.";

/// 默认绑定地址：只监听回环，不暴露公网（ADR-0007 安全约束）。
const BIND_ADDR: &str = "127.0.0.1:4321";

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

    let backend = Arc::new(kestrel_backend::OpenAiCompatBackend::new(
        config.backend.base_url.clone(),
        config.backend.api_key.clone(),
        config.backend.model.clone(),
        config.backend.n_ctx,
    ));
    let store = Arc::new(JsonlStore::new(config.sessions_dir.clone()));
    let tools = kestrel_tools::builtin();
    let permission = PermissionEngine::new(parse_policy(&config.approval_policy));

    let agent = Agent::new(
        backend,
        tools,
        store.clone(),
        permission,
        AgentConfig {
            system_prompt: SYSTEM_PROMPT.to_owned(),
            workdir: workdir.clone(),
            max_tool_output: 8_192,
            limits: TurnLimits::default(),
        },
    );

    let (op_tx, op_rx) = mpsc::channel::<Op>(32);
    let (event_tx, mut event_rx) = mpsc::channel::<Event>(256);
    // 事件泵：把 core 的单消费者 mpsc 扇出到 broadcast，供多个 SSE 订阅者与断线重连。
    let (events_bcast, _) = broadcast::channel::<Event>(1024);
    let session = SessionId(format!("web-{}", std::process::id()));

    let agent_session = session.clone();
    tokio::spawn(async move {
        if let Err(e) = agent.run(agent_session, op_rx, event_tx).await {
            tracing::error!("agent loop exited: {e}");
        }
    });

    let pump = events_bcast.clone();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            // 无订阅者不是错误（浏览器没连），事件已由 store 落盘。
            let _ = pump.send(event);
        }
    });

    let state = AppState {
        op_tx,
        events: events_bcast,
        store,
        session,
        model: config.backend.model.clone(),
        base_url: config.backend.base_url.clone(),
        workdir: workdir_display.clone(),
        sessions_dir: config.sessions_dir.clone(),
    };

    let app = http::router(state);
    let addr: SocketAddr = BIND_ADDR.parse().context("parse bind addr")?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;

    tracing::info!(%addr, model = %config.backend.model, "kestrel-server listening");
    eprintln!(
        "kestrel-server {} listening on http://{addr}  (model {}, workdir {})",
        env!("CARGO_PKG_VERSION"),
        config.backend.model,
        workdir_display
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("serve")?;

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    eprintln!("\nkestrel-server shutting down");
}

fn parse_policy(s: &str) -> kestrel_core::ApprovalPolicy {
    use kestrel_core::ApprovalPolicy;
    match s {
        "auto" => ApprovalPolicy::Auto,
        "strict" => ApprovalPolicy::Strict,
        _ => ApprovalPolicy::OnRequest,
    }
}

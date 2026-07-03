//! Kestrel WebUI backend (axum adapter).
//!
//! 职责边界：与 `kestrel-cli` 平级的第二个前端适配器——组装 core + 全部适配器，
//! 把 core 的有序 `Event` 流经 SSE 推给浏览器，把浏览器的 `Op` 经 HTTP 灌回 core。
//! 决策逻辑全在 core，本 crate 只做传输编解码与事件扇出（ADR-0007）。
//! core 一行不改：这里复用 `kestrel-cli` 相同的 `Agent::run` 契约。

mod http;
mod launcher;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock};

use anyhow::Context;
use kestrel_core::{Agent, AgentConfig, PermissionEngine, TurnLimits};
use kestrel_protocol::{Event, Op, SessionId};
use kestrel_runtime::EngineHandle;
use kestrel_store::{Config, JsonlStore, Layout, Loadout};
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

    // 解析 OS 标准数据/配置目录（ADR-0009）。启动目录用于 .kestrel/ opt-in 与旧
    // ./sessions 迁移探测，与 agent 的 workdir 是两回事。
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let layout = Layout::resolve(&cwd).context("resolve data/config layout")?;
    let config = Config::load(layout.config_file()).context("load config")?;
    let workdir = std::fs::canonicalize(&config.workdir).unwrap_or_else(|_| config.workdir.clone());
    // 展示用：去掉 Windows 扩展长度路径前缀 \\?\。
    let workdir_display = workdir
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();

    let sessions_dir = config
        .sessions_dir
        .clone()
        .unwrap_or_else(|| layout.sessions_dir());
    let store = Arc::new(JsonlStore::new(sessions_dir.clone()));
    // 解析引擎：有 loadout 就按清单自启 / 委托 / 连接（模型启动器，ADR-0010），
    // 否则现状纯连接。启动器在此阻塞到 /health 就绪。
    let engine = resolve_engine(&config, layout.config_file()).await?;
    let agent = assemble_agent(&engine, &config, workdir.clone(), store.clone()).await;

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
        session: Arc::new(RwLock::new(session)),
        session_seq: Arc::new(AtomicU64::new(1)),
        model: engine.model.clone(),
        base_url: engine.base_url.clone(),
        workdir: workdir_display.clone(),
        sessions_dir,
    };

    // 模型启动器路由自带状态，一行 merge 进来（挂 /api/launcher/*，不改 http.rs）。
    let app = http::router(state).merge(launcher::router());
    let addr: SocketAddr = BIND_ADDR.parse().context("parse bind addr")?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;

    tracing::info!(%addr, source = %engine.source, model = %engine.model, "kestrel-server listening");
    eprintln!(
        "kestrel-server {} listening on http://{addr}  ({}, model {}, workdir {})",
        env!("CARGO_PKG_VERSION"),
        engine.source,
        engine.model,
        workdir_display
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("serve")?;

    // 收尾：自启的引擎进程在此被杀（委托 / 连接为 no-op）。
    if let Some(handle) = engine.handle
        && let Err(e) = handle.stop().await
    {
        tracing::warn!("engine stop: {e}");
    }

    Ok(())
}

/// 已解析的引擎：一份后端连接参数 + （自启时）持有的进程句柄。
/// 与 `kestrel-cli` 的同名结构同构（组装根各自装配，未来抽共享默认值见 T2）。
struct Engine {
    /// 自启模式持有进程句柄以便收尾 kill；委托 / 连接为 `None`。
    handle: Option<EngineHandle>,
    /// 来源简述（`self:llama.cpp` / `delegate` / `connect`）。
    source: String,
    /// 后端连接层类型（`llamacpp` / `lmstudio` / `openai`）。
    kind: String,
    /// 后端连接的 `base_url`。
    base_url: String,
    /// API key（本地后端通常空）。
    api_key: String,
    /// 模型标识。
    model: String,
    /// 上下文长度兜底（probe 成功以实测覆盖）。
    n_ctx: u32,
}

/// 解析引擎：有 `config.loadout` 就按 Loadout 的 `[model]` 维度启动 / 委托 / 连接
/// （模型启动器，ADR-0010），否则退回 `[backend]` 纯连接。loadout 相对路径按配置
/// 文件所在目录解析。
async fn resolve_engine(config: &Config, config_file: &Path) -> anyhow::Result<Engine> {
    let Some(loadout_rel) = config.loadout.as_ref() else {
        return Ok(Engine {
            handle: None,
            source: "connect".to_owned(),
            kind: config.backend.kind.clone(),
            base_url: config.backend.base_url.clone(),
            api_key: config.backend.api_key.clone(),
            model: config.backend.model.clone(),
            n_ctx: config.backend.n_ctx,
        });
    };

    let loadout_path = resolve_relative(config_file, loadout_rel);
    let loadout = Loadout::load(&loadout_path)
        .with_context(|| format!("load loadout {}", loadout_path.display()))?;
    let m = &loadout.model;
    let spec = kestrel_runtime::LaunchSpec::from_parts(
        &m.source,
        m.bin.clone(),
        m.model_path.clone(),
        m.base_url.clone(),
        m.port,
        m.n_ctx,
        m.gpu_layers.clone(),
        m.extra_args.clone(),
    )?;
    let handle = kestrel_runtime::launch(spec)
        .await
        .context("launch model engine")?;
    Ok(Engine {
        source: handle.source().to_owned(),
        base_url: handle.base_url().to_owned(),
        kind: loadout.backend_kind().to_owned(),
        api_key: m.api_key.clone(),
        model: m.model.clone(),
        n_ctx: m.n_ctx,
        handle: Some(handle),
    })
}

/// 把可能是相对的 `p` 按 `base_file` 所在目录解析成绝对路径；`p` 本身绝对则原样返回。
fn resolve_relative(base_file: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_file
            .parent()
            .map_or_else(|| p.to_path_buf(), |dir| dir.join(p))
    }
}

/// 组装 agent：选后端 -> 探测真实 n_ctx -> deny 预过滤工具 -> 建权限门。
/// 与 `kestrel-cli` 的组装逻辑同构（core 一行不改，两个前端各自装配）。
/// 后端连接参数取自已解析的 [`Engine`]（loadout 启动或纯连接）；deny / 策略取自 `config`。
async fn assemble_agent(
    engine: &Engine,
    config: &Config,
    workdir: PathBuf,
    store: Arc<JsonlStore>,
) -> Agent {
    let backend = kestrel_backend::build(
        &engine.kind,
        engine.base_url.clone(),
        engine.api_key.clone(),
        engine.model.clone(),
        engine.n_ctx,
    );
    // 探测真实上下文长度喂给 context ledger（失败优雅回退引擎/配置值）。
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
                engine.n_ctx
            );
            engine.n_ctx
        }
    };
    let mut tools = kestrel_tools::builtin();
    let denied = tools.deny(&config.deny_tools);
    if denied > 0 {
        tracing::info!(denied, "deny-listed tools removed from tool set");
    }
    let permission = PermissionEngine::with_deny(
        parse_policy(&config.approval_policy),
        config.deny_tools.clone(),
    );
    Agent::new(
        backend,
        tools,
        store,
        permission,
        AgentConfig {
            system_prompt: SYSTEM_PROMPT.to_owned(),
            workdir,
            max_tool_output: 8_192,
            n_ctx,
            limits: TurnLimits::default(),
        },
    )
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

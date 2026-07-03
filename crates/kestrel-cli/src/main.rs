//! Kestrel 终端前端。
//!
//! 职责边界：本 crate 是组装根（唯一同时依赖 core 与全部适配器的地方）
//! 与事件渲染器。所有决策逻辑都在 core，前端不含业务规则。
//!
//! M1 形态：回合制 REPL（读一行 -> 提交 Op -> 渲染事件流直到本轮结束）。
//! ratatui TUI 与机组车道渲染在 M2 引入。

mod repl;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use kestrel_core::{Agent, AgentConfig, PermissionEngine, TurnLimits};
use kestrel_protocol::{Event, Op, SecretString, SessionId};
use kestrel_runtime::EngineHandle;
use kestrel_store::{Config, JsonlStore, Layout, Loadout};
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

    // 解析引擎：有 loadout 就按清单自启 / 委托 / 连接（模型启动器，ADR-0010），
    // 否则现状纯连接（参数取 [backend]）。启动器在此阻塞到 /health 就绪。
    let engine = resolve_engine(&config, layout.config_file()).await?;

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
    let sessions_dir = config
        .sessions_dir
        .clone()
        .unwrap_or_else(|| layout.sessions_dir());
    let store = Arc::new(JsonlStore::new(sessions_dir));
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
            system_prompt: format!(
                "{SYSTEM_PROMPT}{}",
                kestrel_tools::environment_block(&workdir)
            ),
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
        "kestrel {} — {} · backend {} · model {} · workdir {}",
        env!("CARGO_PKG_VERSION"),
        engine.source,
        engine.base_url,
        engine.model,
        workdir_display
    );
    println!("输入消息开始对话，/quit 退出。\n");

    repl::run(op_tx, event_rx).await?;

    // 收尾：自启的引擎进程在此被杀（委托 / 连接为 no-op，不代杀他人进程）。
    if let Some(handle) = engine.handle
        && let Err(e) = handle.stop().await
    {
        tracing::warn!("engine stop: {e}");
    }

    match agent_handle.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(anyhow::anyhow!("agent: {e}")),
        Err(e) => Err(anyhow::anyhow!("agent task: {e}")),
    }
}

/// 已解析的引擎：一份后端连接参数 + （自启时）持有的进程句柄。
struct Engine {
    /// 自启模式持有进程句柄以便收尾 kill；委托 / 连接为 `None`。
    handle: Option<EngineHandle>,
    /// 来源简述（`self:llama.cpp` / `delegate` / `connect`），用于横幅与日志。
    source: String,
    /// 后端连接层类型（`llamacpp` / `lmstudio` / `openai`）。
    kind: String,
    /// 后端连接的 `base_url`。
    base_url: String,
    /// API key（本地后端通常空）。脱敏类型（地基 #7）。
    api_key: SecretString,
    /// 模型标识。
    model: String,
    /// 上下文长度兜底（probe 成功以实测覆盖）。
    n_ctx: u32,
}

/// 解析引擎：有 `config.loadout` 就按 Loadout 的 `[model]` 维度启动 / 委托 / 连接
/// （模型启动器，ADR-0010），否则退回 `[backend]` 纯连接。
///
/// loadout 相对路径按**配置文件所在目录**解析（loadout 常与 kestrel.toml 放一起）。
async fn resolve_engine(config: &Config, config_file: &Path) -> anyhow::Result<Engine> {
    let Some(loadout_rel) = config.loadout.as_ref() else {
        // 无 loadout：现状纯连接，参数取 [backend]。
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

fn parse_policy(s: &str) -> kestrel_core::ApprovalPolicy {
    use kestrel_core::ApprovalPolicy;
    match s {
        "auto" => ApprovalPolicy::Auto,
        "strict" => ApprovalPolicy::Strict,
        _ => ApprovalPolicy::OnRequest,
    }
}

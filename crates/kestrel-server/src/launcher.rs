//! 模型启动器的服务端状态 + 生命周期（ADR-0010）。
//!
//! 定位对齐 LM Studio 的「Local Server」：把一个本地 llama.cpp 引擎**起起来/停掉/看状态**，
//! agent 按配置的 `base_url` 连它。启动器管引擎宿主，不热切 agent 的会话（换模型=换会话，
//! 前缀稳定铁律；那属会话边界，另做）。
//!
//! **只发现不越权、只回环、配置即授权**（§5）：真正 spawn 复用 [`kestrel_runtime::launch`]
//! （已校验白名单绝对路径 + 强制 `--jinja`/`127.0.0.1`）。

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use kestrel_runtime::{EngineHandle, LaunchSpec, launch};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// 引擎生命周期状态（语言中立枚举码，前端据此渲染）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineState {
    /// 未启动。
    #[default]
    Stopped,
    /// 正在启动 / 等 `/health` 就绪。
    Loading,
    /// 已就绪、可服务。
    Running,
    /// 启动失败（见 `error`）。
    Failed,
}

/// 启动器共享状态（跨 HTTP 处理器共享；持有子进程句柄=持有引擎生命周期）。
#[derive(Default)]
pub struct Launcher {
    state: EngineState,
    handle: Option<EngineHandle>,
    base_url: String,
    model: String,
    error: String,
}

/// 可 `Clone` 分发给各处理器的共享启动器。
pub type SharedLauncher = Arc<Mutex<Launcher>>;

/// 状态快照（`GET /api/launcher/status` 的 JSON 体，语言中立）。
#[derive(Debug, Clone, Serialize)]
pub struct StatusSnapshot {
    /// 引擎状态码。
    pub state: EngineState,
    /// 就绪后的 `base_url`（供 agent 连）。
    pub base_url: String,
    /// 当前（尝试）加载的模型标识。
    pub model: String,
    /// 失败原因（`state=failed` 时非空）。
    pub error: String,
    /// 引擎 stderr 最近日志行（自启引擎；供 UI 日志窗）。
    pub logs: Vec<String>,
}

impl Launcher {
    /// 当前状态快照（含引擎最近日志）。
    pub fn snapshot(&self) -> StatusSnapshot {
        StatusSnapshot {
            state: self.state,
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            error: self.error.clone(),
            logs: self.handle.as_ref().map(EngineHandle::recent_logs).unwrap_or_default(),
        }
    }
}

/// 启动一个引擎（后台任务里跑：立即回 `Loading`，就绪后转 `Running`，失败转 `Failed`）。
///
/// 已有在跑引擎时先停旧的（像 LM Studio 换模型）。真正 spawn 阻塞到 `/health` 就绪，
/// 期间前端轮询 `status` 看到 `Loading`。
pub async fn start(shared: SharedLauncher, spec: LaunchSpec, model: String) {
    // 先停旧引擎 + 置 Loading（不跨 await 持锁）。
    let old = {
        let mut g = shared.lock().await;
        let old = g.handle.take();
        g.state = EngineState::Loading;
        g.model = model;
        g.base_url.clear();
        g.error.clear();
        old
    };
    if let Some(h) = old {
        let _ = h.stop().await;
    }

    match launch(spec).await {
        Ok(handle) => {
            let mut g = shared.lock().await;
            handle.base_url().clone_into(&mut g.base_url);
            g.handle = Some(handle);
            g.state = EngineState::Running;
        }
        Err(e) => {
            let mut g = shared.lock().await;
            g.state = EngineState::Failed;
            g.error = e.to_string();
        }
    }
}

/// 停止当前引擎（自启的子进程被 kill；无则 no-op）。
pub async fn stop(shared: &SharedLauncher) {
    let handle = {
        let mut g = shared.lock().await;
        g.state = EngineState::Stopped;
        g.base_url.clear();
        g.error.clear();
        g.handle.take()
    };
    if let Some(h) = handle {
        let _ = h.stop().await;
    }
}

// ---------------------------------------------------------------------------
// HTTP 路由：自带 `SharedLauncher` 状态，由 main.rs 一行 merge 进 app，
// 不必碰 http.rs（避免与并行改动打架）。全部挂在 `/api/launcher/*`。
// ---------------------------------------------------------------------------

/// `GET /api/launcher/models` 的查询参数。
#[derive(Debug, Deserialize)]
struct ModelsQuery {
    /// 模型目录；空 / 缺省时用 [`kestrel_runtime::default_models_dir`] 兜底。
    dir: Option<String>,
}

/// `POST /api/launcher/launch` 的请求体（映射到 [`LaunchSpec::from_parts`]）。
#[derive(Debug, Deserialize)]
struct LaunchRequest {
    #[serde(default = "default_source")]
    source: String,
    bin: Option<String>,
    model_path: Option<String>,
    base_url: Option<String>,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_n_ctx")]
    n_ctx: u32,
    #[serde(default = "default_gpu_layers")]
    gpu_layers: String,
    #[serde(default)]
    extra_args: Vec<String>,
    #[serde(default)]
    model: String,
}

fn default_source() -> String {
    "self".to_owned()
}
fn default_port() -> u16 {
    8080
}
fn default_n_ctx() -> u32 {
    32_768
}
fn default_gpu_layers() -> String {
    "auto".to_owned()
}

/// 构建启动器路由（自带全新 [`SharedLauncher`] 状态）。挂 `/api/launcher/*`。
pub fn router() -> Router {
    let shared: SharedLauncher = Arc::new(Mutex::new(Launcher::default()));
    let inner = Router::new()
        .route("/launcher/models", get(models))
        .route("/launcher/status", get(status))
        .route("/launcher/launch", post(launch_engine))
        .route("/launcher/stop", post(stop_engine))
        .with_state(shared);
    Router::new().nest("/api", inner)
}

/// 列出模型目录下的本地 GGUF 模型 + 目录 + 总大小（只读发现）。
async fn models(Query(q): Query<ModelsQuery>) -> impl IntoResponse {
    let dir = q
        .dir
        .filter(|d| !d.trim().is_empty())
        .map(PathBuf::from)
        .or_else(kestrel_runtime::default_models_dir);
    let (dir_str, models) = match dir {
        Some(d) => {
            let list = kestrel_runtime::discover_models(&d);
            (
                d.to_string_lossy().trim_start_matches(r"\\?\").to_owned(),
                list,
            )
        }
        None => (String::new(), Vec::new()),
    };
    let total_bytes: u64 = models.iter().map(|m| m.size_bytes).sum();
    Json(serde_json::json!({
        "dir": dir_str,
        "models": models,
        "total_bytes": total_bytes,
    }))
}

/// 当前引擎状态快照（顺带崩溃检测：以为在跑但进程已退 -> 翻 Failed，保留日志）。
async fn status(State(shared): State<SharedLauncher>) -> impl IntoResponse {
    let mut g = shared.lock().await;
    if g.state == EngineState::Running {
        let dead = g.handle.as_mut().is_none_or(|h| !h.is_alive());
        if dead {
            g.state = EngineState::Failed;
            if g.error.is_empty() {
                "engine process exited unexpectedly".clone_into(&mut g.error);
            }
        }
    }
    Json(g.snapshot())
}

/// 启动一个引擎（后台任务，立即 202；前端轮询 status 看 loading -> running）。
async fn launch_engine(
    State(shared): State<SharedLauncher>,
    Json(req): Json<LaunchRequest>,
) -> impl IntoResponse {
    let spec = match LaunchSpec::from_parts(
        &req.source,
        req.bin.map(PathBuf::from),
        req.model_path.map(PathBuf::from),
        req.base_url,
        req.port,
        req.n_ctx,
        req.gpu_layers,
        req.extra_args,
    ) {
        Ok(spec) => spec,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    tokio::spawn(start(shared, spec, req.model));
    StatusCode::ACCEPTED.into_response()
}

/// 停止当前引擎。
async fn stop_engine(State(shared): State<SharedLauncher>) -> impl IntoResponse {
    stop(&shared).await;
    StatusCode::ACCEPTED
}

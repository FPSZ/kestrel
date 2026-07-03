//! HTTP 层：路由 + 处理器。把 core 的 `Event` / `Op` 契约编解码为 SSE / JSON。
//!
//! - `GET  /api/events`         SSE：store 快照追平 + broadcast 实时（按 seq 去重）
//! - `POST /api/ops`            解析 `Op` 灌进 agent 的 op 通道
//! - `GET  /api/health`         server 存活 + model / base_url / session / workdir
//! - `GET  /api/sessions`       列出会话 id
//! - `GET  /api/sessions/{id}/events`  回放某会话的全部事件
//! - `GET  /api/launcher/scan`  发现本机 llama-server 二进制候选 + 已在跑的本地引擎
//! - `/*`（fallback）           release 下托管 `console/dist`；dev 用 Vite 代理

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use futures::stream::{Stream, StreamExt};
use kestrel_core::ports::Store;
use kestrel_protocol::{Event, Op, SessionId};
use kestrel_store::JsonlStore;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::BroadcastStream;
use tower_http::services::ServeDir;

/// 共享状态（`Clone` 后分发给各处理器，内部均为 `Arc` / channel 句柄）。
#[derive(Clone)]
pub struct AppState {
    /// 提交 Op 到 agent 主循环（前端 -> core 的唯一入口）。
    pub op_tx: mpsc::Sender<Op>,
    /// 事件广播（agent 事件泵扇出到此，SSE 订阅者从此取）。
    pub events: broadcast::Sender<Event>,
    /// 事件日志存储（回放快照 / 历史会话）。
    pub store: Arc<JsonlStore>,
    /// 当前活动会话（内部可变：`POST /api/sessions` 新建对话时就地轮换）。
    pub session: Arc<RwLock<SessionId>>,
    /// 新会话 id 计数器（server 分配，避免 core 依赖时钟/随机）。
    pub session_seq: Arc<AtomicU64>,
    /// 后端模型名（状态展示）。
    pub model: String,
    /// 后端基址（状态展示）。
    pub base_url: String,
    /// 工作目录（展示用，已去 \\?\ 前缀）。
    pub workdir: String,
    /// 会话日志目录（列表用）。
    pub sessions_dir: PathBuf,
}

/// 构建路由。`/api/*` 是契约端点，其余回落到静态资源。
pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/events", get(events))
        .route("/ops", post(ops))
        .route("/sessions", get(list_sessions).post(new_session))
        .route("/sessions/{id}", delete(delete_session))
        .route("/sessions/{id}/events", get(session_events))
        .route("/launcher/scan", get(launcher_scan))
        .with_state(state);

    // release 下托管构建产物；dev 用 Vite 自带 server + 代理，dist 不存在也无妨。
    Router::new()
        .nest("/api", api)
        .fallback_service(ServeDir::new("console/dist"))
}

#[allow(clippy::unused_async)] // axum handler 必须是 async；本处理器无需 await
async fn health(State(s): State<AppState>) -> impl IntoResponse {
    let session = s.session.read().unwrap().0.clone();
    Json(serde_json::json!({
        "ok": true,
        "session": session,
        "model": s.model,
        "base_url": s.base_url,
        "workdir": s.workdir,
    }))
}

async fn ops(State(s): State<AppState>, Json(op): Json<Op>) -> impl IntoResponse {
    match s.op_tx.send(op).await {
        Ok(()) => StatusCode::ACCEPTED,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE, // agent 已退出
    }
}

/// SSE 事件流。先订阅 broadcast（避免快照与实时之间丢事件），再读快照，
/// 实时流按 seq 去重（快照已含的丢弃），lagged 帧跳过。
async fn events(
    State(s): State<AppState>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = s.events.subscribe();
    // 取当前会话快照（克隆出锁再 await，不跨 await 持锁）。
    let session = s.session.read().unwrap().clone();
    let snapshot = s.store.replay(&session).await.unwrap_or_default();
    let last_seq = snapshot.last().map(|e| e.seq);

    let snapshot_stream = futures::stream::iter(snapshot);
    let live_stream = BroadcastStream::new(rx).filter_map(move |r| {
        let out = match r {
            Ok(e) if last_seq.is_none_or(|ls| e.seq > ls) => Some(e),
            _ => None, // 快照已含（重复）或 Lagged
        };
        async move { out }
    });

    let merged = snapshot_stream.chain(live_stream).map(|ev| {
        let data = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".to_owned());
        Ok::<_, Infallible>(SseEvent::default().data(data))
    });

    Sse::new(merged).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

async fn list_sessions(State(s): State<AppState>) -> impl IntoResponse {
    // 按文件修改时间倒序：最近活跃的对话排在最上面（前端据此展示）。
    let mut entries: Vec<(std::time::SystemTime, String)> = Vec::new();
    if let Ok(mut rd) = tokio::fs::read_dir(&s.sessions_dir).await {
        while let Ok(Some(entry)) = rd.next_entry().await {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("jsonl")
                && let Some(stem) = p.file_stem().and_then(|st| st.to_str())
            {
                let mtime = entry
                    .metadata()
                    .await
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(std::time::UNIX_EPOCH);
                entries.push((mtime, stem.to_owned()));
            }
        }
    }
    entries.sort_by(|a, b| b.0.cmp(&a.0)); // 新 -> 旧
    let ids: Vec<String> = entries.into_iter().map(|(_, id)| id).collect();
    Json(ids)
}

async fn session_events(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match s.store.replay(&SessionId(id)).await {
        Ok(events) => Json(events).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// 新建对话（`POST /api/sessions`）：server 分配新会话 id、就地轮换当前会话，
/// 并通知 core 清空历史。前端收到 id 后应重连事件流（重置折叠状态）。
async fn new_session(State(s): State<AppState>) -> impl IntoResponse {
    let n = s.session_seq.fetch_add(1, Ordering::SeqCst);
    let id = format!("web-{}-{n}", std::process::id());
    {
        // 先切 AppState 的当前会话，再发 Op；不跨 await 持锁。
        *s.session.write().unwrap() = SessionId(id.clone());
    }
    if s.op_tx
        .send(Op::NewSession { id: id.clone() })
        .await
        .is_err()
    {
        return (StatusCode::SERVICE_UNAVAILABLE, "agent unavailable").into_response();
    }
    Json(serde_json::json!({ "id": id })).into_response()
}

/// 删除一个历史会话的日志文件（`DELETE /api/sessions/{id}`）。
/// 拒删当前活动会话；id 仅允许 `[A-Za-z0-9_-]` 防路径逃逸（真删本地文件，前端两次确认）。
async fn delete_session(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return (StatusCode::BAD_REQUEST, "invalid session id").into_response();
    }
    if id == s.session.read().unwrap().0 {
        return (StatusCode::CONFLICT, "cannot delete the active session").into_response();
    }
    let path = s.sessions_dir.join(format!("{id}.jsonl"));
    match tokio::fs::remove_file(&path).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// 模型启动器扫描：发现本机 `llama-server` 二进制候选 + 已在跑的本地引擎。
/// **只发现、不 spawn**（ADR-0010 §5）；结果是语言中立数据（路径/URL/枚举码/数字）。
/// 无需 `State`：每次现扫，不依赖会话状态。
async fn launcher_scan() -> impl IntoResponse {
    Json(kestrel_runtime::scan().await)
}

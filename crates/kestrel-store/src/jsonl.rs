//! JSONL 事件日志：Store 端口的默认实现。
//!
//! - 一个会话一个 `.jsonl` 文件，一行一个 Event，只追加。
//! - resume = 重放全部事件重建状态（状态 = fold(events)，ADR-002）。
//! - 回放测试直接消费同一格式（§7 Replay Harness）。

use std::path::PathBuf;

use async_trait::async_trait;
use kestrel_core::CoreError;
use kestrel_core::ports::Store;
use kestrel_protocol::{EVENT_LOG_SCHEMA_VERSION, Event, SessionId};
use tokio::io::AsyncWriteExt;

/// 目录级 schema 版本标记文件名（ADR-0011）。存在即视为该目录事件日志的写入版本已知。
const SCHEMA_MARKER: &str = ".schema_version";

/// 以文件系统为后端的 JSONL 事件日志。
#[derive(Debug, Clone)]
pub struct JsonlStore {
    root: PathBuf,
}

impl JsonlStore {
    /// 以给定根目录建库（目录会在首次写入时按需创建）。
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn path_for(&self, session: &SessionId) -> PathBuf {
        self.root.join(format!("{}.jsonl", session.0))
    }

    /// 首次写入时落 `.schema_version` 标记（ADR-0011）。`create_new` 保证只有一个
    /// 写者创建成功并写入版本号，其余撞 `AlreadyExists` 原子 no-op。非致命：标记写
    /// 失败也绝不挡事件写入，只告警。
    async fn ensure_schema_marker(&self) {
        let marker = self.root.join(SCHEMA_MARKER);
        match tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&marker)
            .await
        {
            Ok(mut f) => {
                let _ = f
                    .write_all(EVENT_LOG_SCHEMA_VERSION.to_string().as_bytes())
                    .await;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {} // 已存在：正常。
            Err(e) => tracing::warn!(error = %e, "write schema marker (non-fatal)"),
        }
    }
}

#[async_trait]
impl Store for JsonlStore {
    async fn append(&self, session: &SessionId, event: &Event) -> Result<(), CoreError> {
        let line = serde_json::to_string(event)
            .map_err(|e| CoreError::Store(format!("serialize event: {e}")))?;
        tokio::fs::create_dir_all(&self.root)
            .await
            .map_err(|e| CoreError::Store(format!("create sessions dir: {e}")))?;
        self.ensure_schema_marker().await;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path_for(session))
            .await
            .map_err(|e| CoreError::Store(format!("open log: {e}")))?;
        file.write_all(line.as_bytes())
            .await
            .and(file.write_all(b"\n").await)
            .map_err(|e| CoreError::Store(format!("write log: {e}")))?;
        Ok(())
    }

    async fn replay(&self, session: &SessionId) -> Result<Vec<Event>, CoreError> {
        let text = match tokio::fs::read_to_string(self.path_for(session)).await {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(CoreError::Store(format!("read log: {e}"))),
        };
        // 容忍解析（ADR-0011 / foundations #6）：单行解析失败**跳过并告警**，绝不
        // 因一行坏数据（截断写入 / 更新版本的畸形记录）让整段历史读不出。未知 `type`
        // 由 `EventPayload` 的 `#[serde(other)] Unknown` 变体兜住（不会走到这里的错误分支）；
        // 未知字段由 serde 默认忽略。这里的 skip 兜的是"根本不是合法 JSON 行"的损坏。
        let mut events = Vec::new();
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str(line) {
                Ok(event) => events.push(event),
                Err(e) => tracing::warn!(
                    session = %session.0,
                    line = i + 1,
                    error = %e,
                    "skip unparseable event log line (forward-compat / corruption)"
                ),
            }
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use kestrel_protocol::{CrewRole, EventPayload};

    use super::*;

    #[tokio::test]
    async fn append_then_replay_roundtrips() {
        let dir = std::env::temp_dir().join(format!("kestrel-jsonl-test-{}", std::process::id()));
        let store = JsonlStore::new(dir.clone());
        let session = SessionId("t1".to_owned());

        let e = Event {
            seq: 0,
            actor: CrewRole::Lead,
            payload: EventPayload::UserInput {
                text: "hello".to_owned(),
                images: Vec::new(),
            },
        };
        store.append(&session, &e).await.unwrap();

        let back = store.replay(&session).await.unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].seq, 0);

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn replay_missing_session_is_empty() {
        let store = JsonlStore::new(std::env::temp_dir().join("kestrel-nope"));
        let back = store
            .replay(&SessionId("does-not-exist".to_owned()))
            .await
            .unwrap();
        assert!(back.is_empty());
    }

    #[tokio::test]
    async fn append_writes_schema_marker() {
        let dir = std::env::temp_dir().join(format!("kestrel-mark-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        let store = JsonlStore::new(dir.clone());
        let e = Event {
            seq: 0,
            actor: CrewRole::Lead,
            payload: EventPayload::TurnCompleted {
                reason: "stop".to_owned(),
            },
        };
        store.append(&SessionId("m1".to_owned()), &e).await.unwrap();
        let marker = tokio::fs::read_to_string(dir.join(".schema_version"))
            .await
            .unwrap();
        assert_eq!(marker, "1", "marker records the schema version");
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn replay_tolerates_unknown_variant_and_skips_corruption() {
        // 前向兼容（ADR-0011）：未来版本的未知 type -> Unknown 变体；纯损坏行 -> 跳过。
        // 已知事件必须完好保留，不因夹在中间的坏行 / 未来事件而整段失败。
        let dir = std::env::temp_dir().join(format!("kestrel-fwd-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let log = dir.join("s.jsonl");
        let contents = concat!(
            r#"{"seq":0,"actor":"lead","payload":{"type":"user_input","text":"hi"}}"#,
            "\n",
            r#"{"seq":1,"actor":"copilot","payload":{"type":"future_event_v2","blob":42}}"#,
            "\n",
            "this is not json at all\n",
            r#"{"seq":2,"actor":"lead","payload":{"type":"turn_completed","reason":"stop"}}"#,
            "\n",
        );
        tokio::fs::write(&log, contents).await.unwrap();

        let store = JsonlStore::new(dir.clone());
        let back = store.replay(&SessionId("s".to_owned())).await.unwrap();
        // 3 条可解析（含 1 条 Unknown），第 3 行纯损坏被跳过。
        assert_eq!(back.len(), 3);
        assert!(matches!(back[0].payload, EventPayload::UserInput { .. }));
        assert!(matches!(back[1].payload, EventPayload::Unknown));
        assert!(matches!(
            back[2].payload,
            EventPayload::TurnCompleted { .. }
        ));
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}

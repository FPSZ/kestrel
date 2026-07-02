//! JSONL 事件日志：Store 端口的默认实现。
//!
//! - 一个会话一个 `.jsonl` 文件，一行一个 Event，只追加。
//! - resume = 重放全部事件重建状态（状态 = fold(events)，ADR-002）。
//! - 回放测试直接消费同一格式（§7 Replay Harness）。

use std::path::PathBuf;

use async_trait::async_trait;
use kestrel_core::CoreError;
use kestrel_core::ports::Store;
use kestrel_protocol::{Event, SessionId};
use tokio::io::AsyncWriteExt;

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
}

#[async_trait]
impl Store for JsonlStore {
    async fn append(&self, session: &SessionId, event: &Event) -> Result<(), CoreError> {
        let line = serde_json::to_string(event)
            .map_err(|e| CoreError::Store(format!("serialize event: {e}")))?;
        tokio::fs::create_dir_all(&self.root)
            .await
            .map_err(|e| CoreError::Store(format!("create sessions dir: {e}")))?;
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
        let mut events = Vec::new();
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let event = serde_json::from_str(line)
                .map_err(|e| CoreError::Store(format!("parse line {}: {e}", i + 1)))?;
            events.push(event);
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
}

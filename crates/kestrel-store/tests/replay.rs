//! 回放 harness 种子（docs/architecture.md §7）。
//!
//! 加载 `tests/replays/` 里录制的会话事件日志，无模型、毫秒级验证确定性外壳：
//! - JSONL 逐行反序列化回当前 `Event` schema（守住"改 Event 形状即破坏历史/fixture"
//!   这条地基铁律：一旦破坏性改字段，本测试立刻红）。
//! - append-only 不变量：seq 从 0 单调递增、无空洞（state = fold(events) 的前提）。
//! - fold 出的最终态自洽（每个 tool_result 都能配到先前的 tool_call）。
//!
//! 断言 DSL 的完整形态在 M3 定稿；此处先把"格式 + 不变量"钉死。

use std::collections::HashSet;
use std::path::PathBuf;

use kestrel_core::ports::Store;
use kestrel_protocol::{EventPayload, SessionId};
use kestrel_store::JsonlStore;

/// 仓库根的 `tests/replays/` 目录（相对本 crate 的 manifest）。
fn replays_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("replays")
}

#[tokio::test]
async fn auth_refactor_fixture_replays_and_holds_invariants() {
    let store = JsonlStore::new(replays_dir());
    let events = store
        .replay(&SessionId("auth_refactor".to_owned()))
        .await
        .expect("fixture must deserialize into current Event schema");

    assert!(!events.is_empty(), "fixture should not be empty");

    // seq 从 0 单调递增、无空洞。
    for (i, e) in events.iter().enumerate() {
        assert_eq!(e.seq, i as u64, "seq must be gapless and monotonic");
    }

    // fold 自洽：每个 tool_result 都能配到先前出现的 tool_call。
    let mut open_calls: HashSet<String> = HashSet::new();
    let mut budget_seen = false;
    for e in &events {
        match &e.payload {
            EventPayload::ToolCallRequested { call_id, .. } => {
                open_calls.insert(call_id.clone());
            }
            EventPayload::ToolResult { call_id, .. } => {
                assert!(
                    open_calls.contains(call_id),
                    "tool_result {call_id} has no preceding tool_call"
                );
            }
            EventPayload::ContextBudget { used_tokens, n_ctx } => {
                assert!(*n_ctx > 0 && used_tokens <= n_ctx, "budget must be sane");
                budget_seen = true;
            }
            _ => {}
        }
    }
    assert!(budget_seen, "fixture exercises the context_budget variant");
}

//! 主循环集成冒烟测试：用脚本化的 mock backend 驱动 Agent 走完
//! "文本回复"与"工具调用 -> 结果 -> 收尾"两条路径，无需真实模型。
//!
//! 这是回放测试基座（docs/architecture.md §7）的种子：确定性外壳
//! （权限门、事件流、工具分发）可无模型、毫秒级验证。

use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use kestrel_core::ports::{CompletionStream, LlmBackend, Store, Tool, ToolCtx, ToolOutput};
use kestrel_core::{Agent, AgentConfig, ApprovalPolicy, PermissionEngine, ToolSet, TurnLimits};
use kestrel_protocol::{
    AgentMode, BackendCapabilities, CompletionChunk, CompletionRequest, Event, EventPayload, Op,
    RiskLevel, SessionId, ToolSpec,
};
use tokio::sync::mpsc;

/// 每次 stream() 调用弹出脚本里的下一批 chunk。
#[derive(Clone)]
struct ScriptedBackend {
    script: Arc<Mutex<Vec<Vec<CompletionChunk>>>>,
}

#[async_trait]
impl LlmBackend for ScriptedBackend {
    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<CompletionStream, kestrel_core::CoreError> {
        let batch = {
            let mut s = self.script.lock().unwrap();
            if s.is_empty() {
                Vec::new()
            } else {
                s.remove(0)
            }
        };
        let stream = futures::stream::iter(batch.into_iter().map(Ok));
        Ok(Box::pin(stream))
    }

    async fn probe(&self) -> Result<BackendCapabilities, kestrel_core::CoreError> {
        Ok(BackendCapabilities {
            n_ctx: 8192,
            native_tool_calls: true,
            slot_persistence: false,
            model_id: "mock".to_owned(),
        })
    }

    async fn save_cache(&self, _session: &SessionId) -> Result<(), kestrel_core::CoreError> {
        Ok(())
    }
}

/// 内存事件存储（回放/断言用）。
#[derive(Default, Clone)]
struct MemStore {
    events: Arc<Mutex<Vec<Event>>>,
}

#[async_trait]
impl Store for MemStore {
    async fn append(
        &self,
        _session: &SessionId,
        event: &Event,
    ) -> Result<(), kestrel_core::CoreError> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
    async fn replay(&self, _session: &SessionId) -> Result<Vec<Event>, kestrel_core::CoreError> {
        Ok(self.events.lock().unwrap().clone())
    }
}

/// 无副作用的只读回声工具。
struct EchoTool {
    spec: ToolSpec,
}

#[async_trait]
impl Tool for EchoTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn risk(&self, _args: &serde_json::Value) -> RiskLevel {
        RiskLevel::ReadOnly
    }
    async fn call(
        &self,
        args: serde_json::Value,
        _ctx: &ToolCtx,
    ) -> Result<ToolOutput, kestrel_core::CoreError> {
        Ok(ToolOutput {
            ok: true,
            content: format!("echo: {args}"),
        })
    }
}

fn build_agent(script: Vec<Vec<CompletionChunk>>, store: MemStore) -> Agent {
    let backend = Arc::new(ScriptedBackend {
        script: Arc::new(Mutex::new(script)),
    });
    let mut tools = ToolSet::new();
    tools.register(Arc::new(EchoTool {
        spec: ToolSpec {
            name: "echo".to_owned(),
            description: "echo args".to_owned(),
            parameters: serde_json::json!({"type": "object"}),
        },
    }));
    Agent::new(
        backend,
        tools,
        Arc::new(store),
        PermissionEngine::new(ApprovalPolicy::Auto),
        AgentConfig {
            system_prompt: "test".to_owned(),
            workdir: std::env::temp_dir(),
            max_tool_output: 4096,
            n_ctx: 8192,
            limits: TurnLimits::default(),
        },
    )
}

#[tokio::test]
async fn plain_text_reply_completes_turn() {
    let store = MemStore::default();
    let agent = build_agent(
        vec![vec![
            CompletionChunk::Text {
                delta: "hello".to_owned(),
            },
            CompletionChunk::Done,
        ]],
        store.clone(),
    );

    let (op_tx, op_rx) = mpsc::channel(8);
    let (event_tx, mut event_rx) = mpsc::channel(64);
    let handle =
        tokio::spawn(async move { agent.run(SessionId("t".to_owned()), op_rx, event_tx).await });

    op_tx
        .send(Op::UserInput {
            text: "hi".to_owned(),
            think: true,
            mode: AgentMode::Auto,
        })
        .await
        .unwrap();
    // 等待 TurnCompleted。
    let mut saw_text = false;
    let mut saw_done = false;
    while let Some(ev) = event_rx.recv().await {
        match ev.payload {
            EventPayload::AgentText { .. } => saw_text = true,
            EventPayload::TurnCompleted { .. } => {
                saw_done = true;
                break;
            }
            _ => {}
        }
    }
    drop(op_tx);
    handle.await.unwrap().unwrap();

    assert!(saw_text, "should stream assistant text");
    assert!(saw_done, "turn should complete");

    // 事件日志的 seq 单调递增（append-only 不变量）。
    let events = store.events.lock().unwrap();
    for (i, e) in events.iter().enumerate() {
        assert_eq!(e.seq, i as u64);
    }
}

#[tokio::test]
async fn tool_call_then_result_then_completes() {
    let store = MemStore::default();
    // 第一次调用返回工具调用；第二次返回收尾文本。
    let agent = build_agent(
        vec![
            vec![
                CompletionChunk::ToolCall {
                    call_id: "c1".to_owned(),
                    tool: "echo".to_owned(),
                    args: serde_json::json!({"x": 1}),
                },
                CompletionChunk::Done,
            ],
            vec![
                CompletionChunk::Text {
                    delta: "done".to_owned(),
                },
                CompletionChunk::Done,
            ],
        ],
        store.clone(),
    );

    let (op_tx, op_rx) = mpsc::channel(8);
    let (event_tx, mut event_rx) = mpsc::channel(64);
    let handle =
        tokio::spawn(async move { agent.run(SessionId("t".to_owned()), op_rx, event_tx).await });

    op_tx
        .send(Op::UserInput {
            text: "go".to_owned(),
            think: true,
            mode: AgentMode::Auto,
        })
        .await
        .unwrap();

    let mut kinds = Vec::new();
    while let Some(ev) = event_rx.recv().await {
        let is_done = matches!(ev.payload, EventPayload::TurnCompleted { .. });
        kinds.push(ev.payload);
        if is_done {
            break;
        }
    }
    drop(op_tx);
    handle.await.unwrap().unwrap();

    let ps = payloads_owned(&kinds);
    assert!(ps.contains(&"UserInput"), "{ps:?}");
    assert!(ps.contains(&"ToolCallRequested"), "{ps:?}");
    assert!(ps.contains(&"ToolResult"), "{ps:?}");
    assert!(ps.contains(&"TurnCompleted"), "{ps:?}");

    // ReadOnly 工具在 Auto 策略下自动放行：不应出现审批事件。
    assert!(!ps.contains(&"ApprovalRequired"), "readonly must not ask");
}

fn payloads_owned(payloads: &[EventPayload]) -> Vec<&'static str> {
    payloads
        .iter()
        .map(|p| match p {
            EventPayload::UserInput { .. } => "UserInput",
            EventPayload::AgentText { .. } => "AgentText",
            EventPayload::AgentReasoning { .. } => "AgentReasoning",
            EventPayload::ToolCallRequested { .. } => "ToolCallRequested",
            EventPayload::ApprovalRequired { .. } => "ApprovalRequired",
            EventPayload::ApprovalResolved { .. } => "ApprovalResolved",
            EventPayload::ToolResult { .. } => "ToolResult",
            EventPayload::TurnCompleted { .. } => "TurnCompleted",
            EventPayload::ContextBudget { .. } => "ContextBudget",
            EventPayload::Error { .. } => "Error",
        })
        .collect()
}

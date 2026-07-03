//! 地基行为集成测试：read-before-edit 约束、轮内取消、预算事件。
//!
//! 与 `loop_smoke.rs` 同构：脚本化 backend + 内存 store + 假工具，无真实模型。
//! 覆盖本轮新增的确定性外壳行为（docs/architecture.md §5.1/§5.2/§8）。

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

/// 每次 stream() 弹出脚本里的下一批 chunk。
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
        Ok(Box::pin(futures::stream::iter(batch.into_iter().map(Ok))))
    }
    async fn probe(&self) -> Result<BackendCapabilities, kestrel_core::CoreError> {
        Ok(BackendCapabilities {
            n_ctx: 8192,
            native_tool_calls: true,
            slot_persistence: false,
            model_id: "mock".to_owned(),
        })
    }
    async fn save_cache(&self, _s: &SessionId) -> Result<(), kestrel_core::CoreError> {
        Ok(())
    }
}

#[derive(Default, Clone)]
struct MemStore {
    events: Arc<Mutex<Vec<Event>>>,
}

#[async_trait]
impl Store for MemStore {
    async fn append(&self, _s: &SessionId, e: &Event) -> Result<(), kestrel_core::CoreError> {
        self.events.lock().unwrap().push(e.clone());
        Ok(())
    }
    async fn replay(&self, _s: &SessionId) -> Result<Vec<Event>, kestrel_core::CoreError> {
        Ok(self.events.lock().unwrap().clone())
    }
}

/// 名为 name 的假工具，risk 固定，call 返回给定 ok + 固定内容。
struct FakeTool {
    spec: ToolSpec,
    risk: RiskLevel,
    ok: bool,
}

impl FakeTool {
    fn new(name: &str, risk: RiskLevel, ok: bool) -> Arc<Self> {
        Arc::new(Self {
            spec: ToolSpec {
                name: name.to_owned(),
                description: name.to_owned(),
                parameters: serde_json::json!({"type": "object"}),
            },
            risk,
            ok,
        })
    }
}

#[async_trait]
impl Tool for FakeTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn risk(&self, _a: &serde_json::Value) -> RiskLevel {
        self.risk
    }
    async fn call(
        &self,
        _a: serde_json::Value,
        _c: &ToolCtx,
    ) -> Result<ToolOutput, kestrel_core::CoreError> {
        Ok(ToolOutput {
            ok: self.ok,
            content: format!("{} done", self.spec.name),
        })
    }
}

/// 阻塞工具：一直等到取消令牌触发再返回（模拟长跑的 shell）。
struct BlockTool {
    spec: ToolSpec,
}

#[async_trait]
impl Tool for BlockTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn risk(&self, _a: &serde_json::Value) -> RiskLevel {
        RiskLevel::ReadOnly
    }
    async fn call(
        &self,
        _a: serde_json::Value,
        ctx: &ToolCtx,
    ) -> Result<ToolOutput, kestrel_core::CoreError> {
        ctx.cancel.cancelled().await;
        Err(kestrel_core::CoreError::Cancelled)
    }
}

fn agent_with(tools: ToolSet, script: Vec<Vec<CompletionChunk>>, store: MemStore) -> Agent {
    let backend = Arc::new(ScriptedBackend {
        script: Arc::new(Mutex::new(script)),
    });
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

fn tool_call(id: &str, name: &str, args: serde_json::Value) -> CompletionChunk {
    CompletionChunk::ToolCall {
        call_id: id.to_owned(),
        tool: name.to_owned(),
        args,
    }
}

/// 收集事件直到 TurnCompleted / 通道关闭。
async fn collect_until_complete(rx: &mut mpsc::Receiver<Event>) -> Vec<EventPayload> {
    let mut out = Vec::new();
    while let Some(ev) = rx.recv().await {
        let done = matches!(ev.payload, EventPayload::TurnCompleted { .. });
        out.push(ev.payload);
        if done {
            break;
        }
    }
    out
}

#[tokio::test]
async fn edit_without_prior_read_is_blocked() {
    let mut tools = ToolSet::new();
    tools.register(FakeTool::new("edit", RiskLevel::Mutating, true));
    let store = MemStore::default();
    let agent = agent_with(
        tools,
        vec![
            vec![
                tool_call("c1", "edit", serde_json::json!({"path": "a.rs"})),
                CompletionChunk::Done,
            ],
            vec![
                CompletionChunk::Text {
                    delta: "ok".to_owned(),
                },
                CompletionChunk::Done,
            ],
        ],
        store,
    );

    let (op_tx, op_rx) = mpsc::channel(8);
    let (event_tx, mut event_rx) = mpsc::channel(64);
    let h =
        tokio::spawn(async move { agent.run(SessionId("t".to_owned()), op_rx, event_tx).await });
    op_tx
        .send(Op::UserInput {
            text: "edit it".to_owned(),
            think: true,
            mode: AgentMode::Auto,
            images: Vec::new(),
        })
        .await
        .unwrap();
    let payloads = collect_until_complete(&mut event_rx).await;
    drop(op_tx);
    h.await.unwrap().unwrap();

    // edit 未先 read -> ToolResult ok:false，内容提示先 read。
    let blocked = payloads.iter().any(|p| {
        matches!(p, EventPayload::ToolResult { ok: false, content, .. }
            if content.contains("must read"))
    });
    assert!(blocked, "edit before read must be blocked: {payloads:?}");
}

#[tokio::test]
async fn edit_after_read_is_allowed() {
    let mut tools = ToolSet::new();
    tools.register(FakeTool::new("read", RiskLevel::ReadOnly, true));
    tools.register(FakeTool::new("edit", RiskLevel::Mutating, true));
    let store = MemStore::default();
    let agent = agent_with(
        tools,
        vec![
            // 同一轮内先 read 再 edit 同一路径。
            vec![
                tool_call("c1", "read", serde_json::json!({"path": "a.rs"})),
                tool_call("c2", "edit", serde_json::json!({"path": "./a.rs"})),
                CompletionChunk::Done,
            ],
            vec![
                CompletionChunk::Text {
                    delta: "done".to_owned(),
                },
                CompletionChunk::Done,
            ],
        ],
        store,
    );

    let (op_tx, op_rx) = mpsc::channel(8);
    let (event_tx, mut event_rx) = mpsc::channel(64);
    let h =
        tokio::spawn(async move { agent.run(SessionId("t".to_owned()), op_rx, event_tx).await });
    op_tx
        .send(Op::UserInput {
            text: "go".to_owned(),
            think: true,
            mode: AgentMode::Auto,
            images: Vec::new(),
        })
        .await
        .unwrap();
    let payloads = collect_until_complete(&mut event_rx).await;
    drop(op_tx);
    h.await.unwrap().unwrap();

    // edit 的 ToolResult 应为 ok:true（"./a.rs" 与 "a.rs" 归一化为同一文件）。
    let edited_ok = payloads.iter().any(|p| {
        matches!(p, EventPayload::ToolResult { ok: true, content, .. }
            if content.contains("edit done"))
    });
    assert!(edited_ok, "edit after read must succeed: {payloads:?}");
}

#[tokio::test]
async fn cancel_mid_tool_completes_with_cancelled() {
    let mut tools = ToolSet::new();
    tools.register(Arc::new(BlockTool {
        spec: ToolSpec {
            name: "block".to_owned(),
            description: "blocks".to_owned(),
            parameters: serde_json::json!({"type": "object"}),
        },
    }));
    let store = MemStore::default();
    let agent = agent_with(
        tools,
        vec![vec![
            tool_call("c1", "block", serde_json::json!({})),
            CompletionChunk::Done,
        ]],
        store,
    );

    let (op_tx, op_rx) = mpsc::channel(8);
    let (event_tx, mut event_rx) = mpsc::channel(64);
    let h =
        tokio::spawn(async move { agent.run(SessionId("t".to_owned()), op_rx, event_tx).await });

    op_tx
        .send(Op::UserInput {
            text: "run".to_owned(),
            think: true,
            mode: AgentMode::Auto,
            images: Vec::new(),
        })
        .await
        .unwrap();

    // 等 block 工具开始执行（收到 ToolCallRequested）后再发 Cancel。
    let mut cancelled_reason = false;
    let mut sent_cancel = false;
    while let Some(ev) = event_rx.recv().await {
        match &ev.payload {
            EventPayload::ToolCallRequested { .. } if !sent_cancel => {
                sent_cancel = true;
                op_tx.send(Op::Cancel).await.unwrap();
            }
            EventPayload::TurnCompleted { reason } => {
                cancelled_reason = reason == "cancelled";
                break;
            }
            _ => {}
        }
    }
    drop(op_tx);
    h.await.unwrap().unwrap();

    assert!(
        cancelled_reason,
        "mid-tool cancel should complete turn as 'cancelled'"
    );
}

#[tokio::test]
async fn turn_emits_context_budget() {
    let tools = ToolSet::new();
    let store = MemStore::default();
    let agent = agent_with(
        tools,
        vec![vec![
            CompletionChunk::Text {
                delta: "hi".to_owned(),
            },
            CompletionChunk::Done,
        ]],
        store.clone(),
    );

    let (op_tx, op_rx) = mpsc::channel(8);
    let (event_tx, mut event_rx) = mpsc::channel(64);
    let h =
        tokio::spawn(async move { agent.run(SessionId("t".to_owned()), op_rx, event_tx).await });
    op_tx
        .send(Op::UserInput {
            text: "hello".to_owned(),
            think: true,
            mode: AgentMode::Auto,
            images: Vec::new(),
        })
        .await
        .unwrap();

    // 读到 ContextBudget 或通道耗尽。
    let mut saw_budget = false;
    while let Some(ev) = event_rx.recv().await {
        if let EventPayload::ContextBudget { n_ctx, .. } = ev.payload {
            assert_eq!(n_ctx, 8192, "budget carries probed n_ctx");
            saw_budget = true;
            break;
        }
    }
    drop(op_tx);
    h.await.unwrap().unwrap();
    assert!(saw_budget, "each turn must emit a ContextBudget snapshot");
}

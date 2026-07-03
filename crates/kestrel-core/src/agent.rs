//! 单线程 turn 状态机（docs/architecture.md §5.1）。
//!
//! 一个 turn = 组装 prompt -> 流式推理 -> 解析工具调用 -> 权限门 ->
//! 执行工具 -> 追加结果，直到模型不再请求工具或触达迭代上限。
//!
//! 铁律：
//! - 消息历史 append-only，永不原地改写（KV 前缀命中的前提）。
//! - 迭代上限治 AutoGPT 式无限循环。
//! - 工具失败把具体错误喂回历史让模型自纠错，不静默重试。

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use futures::StreamExt;
use kestrel_protocol::{
    CompletionChunk, CompletionRequest, CrewRole, Decision, Event, EventPayload, Message, Op, Role,
    SessionId, ToolCall,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::CoreError;
use crate::ledger::ContextLedger;
use crate::permission::PermissionEngine;
use crate::ports::{LlmBackend, Store, ToolCtx};
use crate::tools::ToolSet;

/// read 工具名：read-before-edit 约束用（主循环层约束，见 docs/architecture.md §8）。
const READ_TOOL: &str = "read";
/// edit 工具名：编辑前必须先 read 同一路径。
const EDIT_TOOL: &str = "edit";

/// agent 主循环的硬性约束（AutoGPT 无限循环的解药）。
#[derive(Debug, Clone)]
pub struct TurnLimits {
    /// 单轮最大迭代次数（一次模型调用 + 工具执行为一轮迭代）。
    pub max_iterations: u32,
}

impl Default for TurnLimits {
    fn default() -> Self {
        Self { max_iterations: 10 }
    }
}

/// agent 配置（组装根注入）。
pub struct AgentConfig {
    /// 静态 system prompt（前缀稳定，不含时间戳等动态内容）。
    pub system_prompt: String,
    /// 工作目录。
    pub workdir: PathBuf,
    /// 单个工具输出的截断上限（字节）。
    pub max_tool_output: usize,
    /// 后端真实上下文长度（探测所得，喂给 context ledger 记账；禁止硬编码）。
    pub n_ctx: u32,
    /// 迭代约束。
    pub limits: TurnLimits,
}

/// 单线程 agent。持有端口实现，消费 Op、产出 Event。
pub struct Agent {
    backend: Arc<dyn LlmBackend>,
    tools: ToolSet,
    store: Arc<dyn Store>,
    permission: PermissionEngine,
    config: AgentConfig,
}

/// 一轮对话的事件出口与状态：把 session/store/event_tx/seq 打包，
/// 避免每个方法重复传四个参数。
struct Turn<'a> {
    session: &'a SessionId,
    store: &'a dyn Store,
    event_tx: &'a mpsc::Sender<Event>,
    seq: u64,
}

impl Turn<'_> {
    /// 追加事件到日志并推给前端（唯一的事件出口）。
    async fn emit(&mut self, actor: CrewRole, payload: EventPayload) -> Result<(), CoreError> {
        let event = Event {
            seq: self.seq,
            actor,
            payload,
        };
        self.seq += 1;
        self.store.append(self.session, &event).await?;
        // 前端已断开不是致命错误：日志已落盘，静默停止推送。
        let _ = self.event_tx.send(event).await;
        Ok(())
    }
}

impl Agent {
    /// 组装。
    #[must_use]
    pub fn new(
        backend: Arc<dyn LlmBackend>,
        tools: ToolSet,
        store: Arc<dyn Store>,
        permission: PermissionEngine,
        config: AgentConfig,
    ) -> Self {
        Self {
            backend,
            tools,
            store,
            permission,
            config,
        }
    }

    /// 主循环：从 `op_rx` 收 Op，向 `event_tx` 发 Event，直到 op 通道关闭。
    ///
    /// 单消费者、有序（ADR-002）：core 是 Op 的唯一消费者、Event 的唯一生产者。
    pub async fn run(
        &self,
        session: SessionId,
        mut op_rx: mpsc::Receiver<Op>,
        event_tx: mpsc::Sender<Event>,
    ) -> Result<(), CoreError> {
        let mut history = vec![Message::text(Role::System, &self.config.system_prompt)];
        // 已被模型读过的文件（read-before-edit 约束）。会话级：轮 1 读、轮 2 编辑也算。
        // 纯字符串集合，不碰文件系统——保持 core 零 IO 与确定性。
        let mut read_paths: HashSet<String> = HashSet::new();
        let mut ledger = ContextLedger::new(self.config.n_ctx);
        let mut turn = Turn {
            session: &session,
            store: self.store.as_ref(),
            event_tx: &event_tx,
            seq: 0,
        };

        while let Some(op) = op_rx.recv().await {
            match op {
                Op::UserInput { text } => {
                    turn.emit(
                        CrewRole::Lead,
                        EventPayload::UserInput { text: text.clone() },
                    )
                    .await?;
                    history.push(Message::text(Role::User, text));
                    // 每一轮一个新的取消令牌（Op::Cancel 触发它，贯穿流式与工具子进程）。
                    let cancel = CancellationToken::new();
                    // 一轮内的错误（后端连不上、工具基础设施故障）不杀会话：
                    // 报成 Error 事件，回到提示符，用户可修复后重试。
                    match self
                        .run_turn(
                            &mut turn,
                            &mut history,
                            &mut read_paths,
                            &mut op_rx,
                            &cancel,
                        )
                        .await
                    {
                        Ok(()) => {}
                        // 用户中断：正常收尾（不是错误），前端与 CLI 都按 TurnCompleted 处理。
                        Err(CoreError::Cancelled) => {
                            turn.emit(
                                CrewRole::Lead,
                                EventPayload::TurnCompleted {
                                    reason: "cancelled".to_owned(),
                                },
                            )
                            .await?;
                        }
                        Err(e) => {
                            turn.emit(
                                CrewRole::System,
                                EventPayload::Error {
                                    message: e.to_string(),
                                },
                            )
                            .await?;
                        }
                    }
                    // 轮次边界发一份预算快照（确定性：按完整历史重算）。
                    // `should_compact()` 逼近阈值的消费（派发异地压缩）属 M2；
                    // 现在先如实把预算播成事件，UI/用户可感知逼近上限。
                    ledger.recount(&history);
                    turn.emit(
                        CrewRole::System,
                        EventPayload::ContextBudget {
                            used_tokens: ledger.used(),
                            n_ctx: ledger.n_ctx(),
                        },
                    )
                    .await?;
                }
                // 轮外收到审批/取消：无挂起动作，忽略。
                Op::Approve { .. } | Op::Deny { .. } | Op::Cancel => {}
            }
        }
        Ok(())
    }

    async fn run_turn(
        &self,
        turn: &mut Turn<'_>,
        history: &mut Vec<Message>,
        read_paths: &mut HashSet<String>,
        op_rx: &mut mpsc::Receiver<Op>,
        cancel: &CancellationToken,
    ) -> Result<(), CoreError> {
        for _ in 0..self.config.limits.max_iterations {
            if cancel.is_cancelled() {
                return Err(CoreError::Cancelled);
            }
            let (text, calls) = self.stream_once(turn, history, op_rx, cancel).await?;
            history.push(Message::assistant_calls(text, calls.clone()));

            if calls.is_empty() {
                return turn
                    .emit(
                        CrewRole::Lead,
                        EventPayload::TurnCompleted {
                            reason: "stop".to_owned(),
                        },
                    )
                    .await;
            }

            for call in calls {
                self.execute_call(turn, history, read_paths, op_rx, cancel, &call)
                    .await?;
            }
        }

        turn.emit(
            CrewRole::Lead,
            EventPayload::TurnCompleted {
                reason: "max_iterations".to_owned(),
            },
        )
        .await
    }

    /// 一次模型调用，收集文本与完整的工具调用，边收边发事件。
    ///
    /// 流式期间并发监听 `op_rx`：收到 [`Op::Cancel`] 立即触发取消令牌并中断
    /// （本地推理慢，可中断性比云端更关键，§5.1 铁律）。轮内收到的其他 Op
    /// （前端已在回合内禁用输入）忽略——与轮外忽略语义一致。
    async fn stream_once(
        &self,
        turn: &mut Turn<'_>,
        history: &[Message],
        op_rx: &mut mpsc::Receiver<Op>,
        cancel: &CancellationToken,
    ) -> Result<(String, Vec<ToolCall>), CoreError> {
        let req = CompletionRequest {
            tools: self.tools.specs(),
            messages: history.to_vec(),
        };
        let mut stream = self.backend.stream(req).await?;
        let mut text = String::new();
        let mut calls = Vec::new();

        loop {
            let chunk = tokio::select! {
                biased;
                op = op_rx.recv() => {
                    match op {
                        // Cancel 或前端断开：触发令牌，中断本轮。
                        Some(Op::Cancel) | None => {
                            cancel.cancel();
                            return Err(CoreError::Cancelled);
                        }
                        // 回合内的其他 Op 忽略（前端回合内禁用输入）。
                        Some(_) => continue,
                    }
                }
                chunk = stream.next() => match chunk {
                    Some(c) => c?,
                    None => break,
                },
            };
            match chunk {
                CompletionChunk::Text { delta } => {
                    text.push_str(&delta);
                    turn.emit(CrewRole::Lead, EventPayload::AgentText { text: delta })
                        .await?;
                }
                CompletionChunk::ToolCall {
                    call_id,
                    tool,
                    args,
                } => {
                    turn.emit(
                        CrewRole::Lead,
                        EventPayload::ToolCallRequested {
                            call_id: call_id.clone(),
                            tool: tool.clone(),
                            args: args.clone(),
                        },
                    )
                    .await?;
                    calls.push(ToolCall {
                        id: call_id,
                        name: tool,
                        arguments: args,
                    });
                }
                CompletionChunk::Done => break,
            }
        }
        Ok((text, calls))
    }

    /// 单个工具调用：注册表校验 -> read-before-edit -> 权限门 -> 执行 -> 结果入历史。
    async fn execute_call(
        &self,
        turn: &mut Turn<'_>,
        history: &mut Vec<Message>,
        read_paths: &mut HashSet<String>,
        op_rx: &mut mpsc::Receiver<Op>,
        cancel: &CancellationToken,
        call: &ToolCall,
    ) -> Result<(), CoreError> {
        if cancel.is_cancelled() {
            return Err(CoreError::Cancelled);
        }

        let Some(tool) = self.tools.get(&call.name).cloned() else {
            let msg = format!(
                "unknown tool '{}'. Available: {}",
                call.name,
                self.tools
                    .specs()
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            return finish_call(turn, history, call, false, msg).await;
        };

        // read-before-edit（原则：防盲改，docs/architecture.md §8）：编辑一个未读过的
        // 文件直接挡回，喂给模型可操作的纠错提示，而不是让它盲改。
        if call.name == EDIT_TOOL
            && let Some(path) = call.arguments.get("path").and_then(|p| p.as_str())
            && !read_paths.contains(&normalize_path(path))
        {
            let msg = format!(
                "must read '{path}' before editing it. Call read(path=\"{path}\") first, \
                 then retry the edit."
            );
            return finish_call(turn, history, call, false, msg).await;
        }

        let risk = tool.risk(&call.arguments);
        match self.permission.decide_tool(&call.name, risk) {
            Decision::Deny => {
                return finish_call(turn, history, call, false, "denied by policy".to_owned())
                    .await;
            }
            Decision::AskUser => {
                turn.emit(
                    CrewRole::System,
                    EventPayload::ApprovalRequired {
                        call_id: call.id.clone(),
                        risk,
                        review: None,
                    },
                )
                .await?;
                match await_approval(op_rx, &call.id).await? {
                    Approval::Approved => {}
                    Approval::Denied(reason) => {
                        let msg = reason.unwrap_or_else(|| "denied by user".to_owned());
                        return finish_call(turn, history, call, false, msg).await;
                    }
                }
            }
            Decision::Allow => {}
        }

        // 取消令牌贯穿到子进程：用本轮令牌的子令牌，Op::Cancel 触发即杀 shell 子进程
        // （§5.1 铁律）。工具执行期间并发监听 op_rx 的 Cancel。
        let ctx = ToolCtx {
            workdir: self.config.workdir.clone(),
            max_output_bytes: self.config.max_tool_output,
            cancel: cancel.child_token(),
        };
        let out = {
            let call_fut = tool.call(call.arguments.clone(), &ctx);
            tokio::pin!(call_fut);
            loop {
                tokio::select! {
                    op = op_rx.recv() => {
                        match op {
                            // 触发令牌后不 drop future，而是继续 await 让工具观察到取消、
                            // 优雅收尾（如 shell 杀子进程）。
                            Some(Op::Cancel) | None => cancel.cancel(),
                            Some(_) => {}
                        }
                    }
                    res = &mut call_fut => break res?,
                }
            }
        };

        // read 成功后记账，供后续 edit 的 read-before-edit 校验。
        if call.name == READ_TOOL
            && out.ok
            && let Some(path) = call.arguments.get("path").and_then(|p| p.as_str())
        {
            read_paths.insert(normalize_path(path));
        }

        finish_call(turn, history, call, out.ok, out.content).await
    }
}

/// 归一化路径用于 read-before-edit 比对：纯字符串规整，不碰文件系统
/// （core 零 IO + 确定性）。统一分隔符、去掉前导 `./`，让 `./a.rs` 与
/// `a.rs`、`a\b.rs` 与 `a/b.rs` 视作同一文件。这是防盲改的启发式护栏，
/// 真正的路径逃逸边界在工具层的 `resolve_within`。
fn normalize_path(p: &str) -> String {
    p.trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_owned()
}

/// 把工具结果写进历史并发事件。
async fn finish_call(
    turn: &mut Turn<'_>,
    history: &mut Vec<Message>,
    call: &ToolCall,
    ok: bool,
    content: String,
) -> Result<(), CoreError> {
    history.push(Message::tool_result(&call.id, &content));
    turn.emit(
        CrewRole::System,
        EventPayload::ToolResult {
            call_id: call.id.clone(),
            ok,
            content,
        },
    )
    .await
}

/// 在审批点等待用户裁决，忽略不匹配当前 call 的 Op。
async fn await_approval(
    op_rx: &mut mpsc::Receiver<Op>,
    call_id: &str,
) -> Result<Approval, CoreError> {
    while let Some(op) = op_rx.recv().await {
        match op {
            Op::Approve { call_id: id } if id == call_id => return Ok(Approval::Approved),
            Op::Deny {
                call_id: id,
                reason,
            } if id == call_id => {
                return Ok(Approval::Denied(reason));
            }
            Op::Cancel => return Err(CoreError::Cancelled),
            _ => {}
        }
    }
    // 通道关闭 = 前端退出：视作取消。
    Err(CoreError::Cancelled)
}

enum Approval {
    Approved,
    Denied(Option<String>),
}

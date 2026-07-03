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
    AgentMode, CompletionChunk, CompletionRequest, CrewRole, Decision, Event, EventPayload,
    Message, Op, RiskLevel, Role, SessionId, ToolCall,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::CoreError;
use crate::ledger::ContextLedger;
use crate::permission::PermissionEngine;
use crate::ports::{LlmBackend, Store, Tool, ToolCtx, ToolOutput};
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

/// 一轮对话的事件出口与会话级状态：把 session/store/event_tx/seq 打包，
/// 避免每个方法重复传参。虽名为 Turn，实为整个会话生命周期存活（在 `run` 里
/// 创建一次、跨轮复用），故 `seq` 与 `read_paths` 都随会话累积。
struct Turn<'a> {
    /// 当前会话（拥有所有权：`Op::NewSession` 会就地轮换它，seq 归零）。
    session: SessionId,
    store: &'a dyn Store,
    event_tx: &'a mpsc::Sender<Event>,
    seq: u64,
    /// 已被模型读过的文件（read-before-edit 约束，会话级累积）。
    read_paths: HashSet<String>,
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
        self.store.append(&self.session, &event).await?;
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
        let mut ledger = ContextLedger::new(self.config.n_ctx);
        let mut turn = Turn {
            session,
            store: self.store.as_ref(),
            event_tx: &event_tx,
            seq: 0,
            // 纯字符串集合，不碰文件系统——保持 core 零 IO 与确定性。
            read_paths: HashSet::new(),
        };

        while let Some(op) = op_rx.recv().await {
            match op {
                Op::UserInput {
                    text,
                    think,
                    mode,
                    images,
                } => {
                    turn.emit(
                        CrewRole::Lead,
                        EventPayload::UserInput {
                            text: text.clone(),
                            images: images.clone(),
                        },
                    )
                    .await?;
                    history.push(Message::user(text, images));
                    // 每一轮一个新的取消令牌（Op::Cancel 触发它，贯穿流式与工具子进程）。
                    let cancel = CancellationToken::new();
                    // 一轮内的错误（后端连不上、工具基础设施故障）不杀会话：
                    // 报成 Error 事件，回到提示符，用户可修复后重试。
                    match self
                        .run_turn(&mut turn, &mut history, &mut op_rx, &cancel, think, mode)
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
                                    code: e.code(),
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
                // 新建对话：清空历史与已读集、轮换会话 id、seq 归零。新会话日志从 0 起，
                // 前端会重连事件流并重置折叠状态（旧 seq 高于新 0 会被去重误丢，故必须重置）。
                Op::NewSession { id } => {
                    history = vec![Message::text(Role::System, &self.config.system_prompt)];
                    ledger.recount(&history);
                    turn.session = SessionId(id);
                    turn.seq = 0;
                    turn.read_paths.clear();
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
        op_rx: &mut mpsc::Receiver<Op>,
        cancel: &CancellationToken,
        think: bool,
        mode: AgentMode,
    ) -> Result<(), CoreError> {
        for _ in 0..self.config.limits.max_iterations {
            if cancel.is_cancelled() {
                return Err(CoreError::Cancelled);
            }
            let (text, calls) = self
                .stream_once(turn, history, op_rx, cancel, think)
                .await?;
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
                self.execute_call(turn, history, op_rx, cancel, &call, mode)
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
        think: bool,
    ) -> Result<(String, Vec<ToolCall>), CoreError> {
        let req = CompletionRequest {
            tools: self.tools.specs(),
            messages: history.to_vec(),
            think,
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
                // 思考增量：只转发给前端折叠展示，不并入正文 `text`（不进历史）。
                CompletionChunk::Reasoning { delta } => {
                    turn.emit(CrewRole::Lead, EventPayload::AgentReasoning { text: delta })
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
        op_rx: &mut mpsc::Receiver<Op>,
        cancel: &CancellationToken,
        call: &ToolCall,
        mode: AgentMode,
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
            && !turn.read_paths.contains(&normalize_path(path))
        {
            let msg = format!(
                "must read '{path}' before editing it. Call read(path=\"{path}\") first, \
                 then retry the edit."
            );
            return finish_call(turn, history, call, false, msg).await;
        }

        let risk = tool.risk(&call.arguments);
        match self.permission.decide_tool_in_mode(&call.name, risk, mode) {
            Decision::Deny => {
                let msg = deny_message(mode, &call.name, risk);
                return finish_call(turn, history, call, false, msg).await;
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
                    Approval::Approved => {
                        // 批准落账：折叠状态据此把工具翻到 running（切页/回放不再重弹审批）。
                        turn.emit(
                            CrewRole::System,
                            EventPayload::ApprovalResolved {
                                call_id: call.id.clone(),
                                approved: true,
                            },
                        )
                        .await?;
                    }
                    Approval::Denied(reason) => {
                        // 拒绝同样落账（审计对称），随后跟一条失败 ToolResult 喂回模型换路。
                        turn.emit(
                            CrewRole::System,
                            EventPayload::ApprovalResolved {
                                call_id: call.id.clone(),
                                approved: false,
                            },
                        )
                        .await?;
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
        let out =
            run_tool_watching_cancel(&tool, call.arguments.clone(), &ctx, op_rx, cancel).await?;

        // read 成功后记账，供后续 edit 的 read-before-edit 校验。
        if call.name == READ_TOOL
            && out.ok
            && let Some(path) = call.arguments.get("path").and_then(|p| p.as_str())
        {
            turn.read_paths.insert(normalize_path(path));
        }

        finish_call(turn, history, call, out.ok, out.content).await
    }
}

/// 权限门挡回时喂回模型的文案：计划模式给可操作的"只出计划"提示（不改 system
/// prompt，前缀稳定），其余情况用通用拒绝串。
fn deny_message(mode: AgentMode, tool: &str, risk: RiskLevel) -> String {
    if matches!(mode, AgentMode::Plan) && risk != RiskLevel::ReadOnly {
        format!(
            "plan mode: '{tool}' is a {risk:?} action and was NOT executed. \
             Describe what you would do (the plan); the user will switch to \
             execute mode to actually run it."
        )
    } else {
        "denied by policy".to_owned()
    }
}

/// 执行工具，同时并发监听 [`Op::Cancel`]：触发即取消令牌（不 drop future，
/// 让工具观察到取消、优雅收尾，如 shell 杀子进程）。前端断开（`None`）同理。
async fn run_tool_watching_cancel(
    tool: &Arc<dyn Tool>,
    args: serde_json::Value,
    ctx: &ToolCtx,
    op_rx: &mut mpsc::Receiver<Op>,
    cancel: &CancellationToken,
) -> Result<ToolOutput, CoreError> {
    let call_fut = tool.call(args, ctx);
    tokio::pin!(call_fut);
    loop {
        tokio::select! {
            op = op_rx.recv() => match op {
                Some(Op::Cancel) | None => cancel.cancel(),
                Some(_) => {}
            },
            res = &mut call_fut => break res,
        }
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

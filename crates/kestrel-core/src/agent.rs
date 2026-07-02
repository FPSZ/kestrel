//! 单线程 turn 状态机（docs/architecture.md §5.1）。
//!
//! 一个 turn = 组装 prompt -> 流式推理 -> 解析工具调用 -> 权限门 ->
//! 执行工具 -> 追加结果，直到模型不再请求工具或触达迭代上限。
//!
//! 铁律：
//! - 消息历史 append-only，永不原地改写（KV 前缀命中的前提）。
//! - 迭代上限治 AutoGPT 式无限循环。
//! - 工具失败把具体错误喂回历史让模型自纠错，不静默重试。

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
use crate::permission::PermissionEngine;
use crate::ports::{LlmBackend, Store, ToolCtx};
use crate::tools::ToolSet;

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
                    // 一轮内的错误（后端连不上、工具基础设施故障）不杀会话：
                    // 报成 Error 事件，回到提示符，用户可修复后重试。取消同理。
                    if let Err(e) = self.run_turn(&mut turn, &mut history, &mut op_rx).await {
                        turn.emit(
                            CrewRole::System,
                            EventPayload::Error {
                                message: e.to_string(),
                            },
                        )
                        .await?;
                    }
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
    ) -> Result<(), CoreError> {
        for _ in 0..self.config.limits.max_iterations {
            let (text, calls) = self.stream_once(turn, history).await?;
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
                self.execute_call(turn, history, op_rx, &call).await?;
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
    async fn stream_once(
        &self,
        turn: &mut Turn<'_>,
        history: &[Message],
    ) -> Result<(String, Vec<ToolCall>), CoreError> {
        let req = CompletionRequest {
            tools: self.tools.specs(),
            messages: history.to_vec(),
        };
        let mut stream = self.backend.stream(req).await?;
        let mut text = String::new();
        let mut calls = Vec::new();

        while let Some(chunk) = stream.next().await {
            match chunk? {
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

    /// 单个工具调用：注册表校验 -> 权限门 -> 执行 -> 结果入历史。
    async fn execute_call(
        &self,
        turn: &mut Turn<'_>,
        history: &mut Vec<Message>,
        op_rx: &mut mpsc::Receiver<Op>,
        call: &ToolCall,
    ) -> Result<(), CoreError> {
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

        let risk = tool.risk(&call.arguments);
        match self.permission.decide(risk) {
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

        // M1：每次调用一个全新的取消令牌。把 Op::Cancel 接到长任务的中断留待 M2。
        let ctx = ToolCtx {
            workdir: self.config.workdir.clone(),
            max_output_bytes: self.config.max_tool_output,
            cancel: CancellationToken::new(),
        };
        let out = tool.call(call.arguments.clone(), &ctx).await?;
        finish_call(turn, history, call, out.ok, out.content).await
    }
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

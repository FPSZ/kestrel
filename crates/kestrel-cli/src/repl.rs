//! 回合制 REPL：驱动 core 的一轮，把事件流渲染到终端。
//!
//! M1 是同步回合制（读一行 -> 一轮 -> 渲染直到 TurnCompleted），最简且正确。
//! 审批在事件流内联处理：收到 ApprovalRequired 就地问 y/n。

use kestrel_protocol::{AgentMode, Event, EventPayload, Op, RiskLevel};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

/// 运行 REPL 直到用户退出或事件通道关闭。
///
/// slash 命令（与 WebUI 命令面同源）：`/think on|off`、`/mode ask|auto|plan`、
/// `/help`、`/quit`。非 slash 输入即一条用户消息，按当前 think/mode 提交一轮。
pub async fn run(
    op_tx: mpsc::Sender<Op>,
    mut event_rx: mpsc::Receiver<Event>,
) -> anyhow::Result<()> {
    let mut stdin = BufReader::new(tokio::io::stdin()).lines();
    // 会话内可切的开关（slash 命令改，随每轮 UserInput 提交）。
    let mut think = true;
    let mut mode = AgentMode::Ask;

    loop {
        prompt("> ").await?;
        let Some(line) = stdin.next_line().await? else {
            break; // EOF (Ctrl-D)
        };
        let line = line.trim().to_owned();
        if line.is_empty() {
            continue;
        }
        // slash 命令：不进模型，直接改本地状态或退出。
        if line.starts_with('/') {
            match handle_slash(&line, &mut think, &mut mode) {
                SlashOutcome::Quit => break,
                SlashOutcome::Handled => continue,
                SlashOutcome::NotACommand => {} // 落到下面当普通消息发
            }
        }

        if op_tx
            .send(Op::UserInput {
                text: line,
                think,
                mode,
            })
            .await
            .is_err()
        {
            break; // agent 已退出
        }

        // 渲染本轮事件，直到 TurnCompleted / Error。
        if !drain_turn(&op_tx, &mut event_rx, &mut stdin).await? {
            break;
        }
    }

    drop(op_tx); // 关闭 op 通道，让 agent 主循环收尾退出
    Ok(())
}

enum SlashOutcome {
    Quit,
    Handled,
    NotACommand,
}

/// 解析并执行一条 slash 命令。改 `think`/`mode` 就地生效，回执打印到终端。
fn handle_slash(line: &str, think: &mut bool, mode: &mut AgentMode) -> SlashOutcome {
    let mut parts = line.split_whitespace();
    let cmd = parts.next().unwrap_or("");
    let arg = parts.next().unwrap_or("");
    match cmd {
        "/quit" | "/exit" => SlashOutcome::Quit,
        "/help" => {
            println!(
                "  命令：/think on|off   /mode ask|auto|plan   /help   /quit\n\
                 \x20 当前：思考={}  模式={}",
                if *think { "on" } else { "off" },
                mode_label(*mode)
            );
            SlashOutcome::Handled
        }
        "/think" => {
            match arg {
                "on" | "" => *think = true,
                "off" => *think = false,
                other => {
                    println!("  /think 需 on|off（收到 '{other}'）");
                    return SlashOutcome::Handled;
                }
            }
            println!("  思考 = {}", if *think { "on" } else { "off" });
            SlashOutcome::Handled
        }
        "/mode" => {
            let next = match arg {
                "ask" => AgentMode::Ask,
                "auto" => AgentMode::Auto,
                "plan" => AgentMode::Plan,
                other => {
                    println!("  /mode 需 ask|auto|plan（收到 '{other}'）");
                    return SlashOutcome::Handled;
                }
            };
            *mode = next;
            println!("  模式 = {}", mode_label(*mode));
            SlashOutcome::Handled
        }
        // 未知的 /xxx：不当命令，原样作为消息发给模型。
        _ => SlashOutcome::NotACommand,
    }
}

fn mode_label(mode: AgentMode) -> &'static str {
    match mode {
        AgentMode::Ask => "询问(ask)",
        AgentMode::Auto => "全部执行(auto)",
        AgentMode::Plan => "计划(plan)",
    }
}

/// 渲染一轮的事件流。返回 false 表示应结束 REPL（通道关闭）。
async fn drain_turn(
    op_tx: &mpsc::Sender<Op>,
    event_rx: &mut mpsc::Receiver<Event>,
    stdin: &mut tokio::io::Lines<BufReader<tokio::io::Stdin>>,
) -> anyhow::Result<bool> {
    while let Some(event) = event_rx.recv().await {
        match event.payload {
            EventPayload::AgentText { text } => {
                print!("{text}");
                flush().await?;
            }
            EventPayload::ToolCallRequested { tool, args, .. } => {
                println!("\n  [调用] {tool} {}", compact(&args));
            }
            EventPayload::ToolResult { ok, content, .. } => {
                let tag = if ok { "结果" } else { "失败" };
                println!("  [{tag}] {}", first_lines(&content, 8));
            }
            EventPayload::ApprovalRequired { call_id, risk, .. } => {
                let op = ask_approval(stdin, call_id, risk).await?;
                if op_tx.send(op).await.is_err() {
                    return Ok(false);
                }
            }
            EventPayload::TurnCompleted { .. } => {
                println!();
                return Ok(true);
            }
            EventPayload::Error { message } => {
                eprintln!("\n[错误] {message}");
                return Ok(true);
            }
            // 预算快照在轮次边界发出：CLI 用一行低调提示，逼近上限才有存在感。
            EventPayload::ContextBudget { used_tokens, n_ctx } => {
                if n_ctx > 0 && u64::from(used_tokens) * 100 >= u64::from(n_ctx) * 75 {
                    let pct = u64::from(used_tokens) * 100 / u64::from(n_ctx);
                    println!("  [context] {used_tokens}/{n_ctx} tok (~{pct}%)");
                }
            }
            // CLI 无需渲染：用户输入已回显、思考增量避免刷屏、审批裁决已内联反馈。
            EventPayload::UserInput { .. }
            | EventPayload::AgentReasoning { .. }
            | EventPayload::ApprovalResolved { .. } => {}
        }
    }
    Ok(false)
}

async fn ask_approval(
    stdin: &mut tokio::io::Lines<BufReader<tokio::io::Stdin>>,
    call_id: String,
    risk: RiskLevel,
) -> anyhow::Result<Op> {
    prompt(&format!("\n  [批准? {risk:?}] y/N: ")).await?;
    let ans = stdin.next_line().await?.unwrap_or_default();
    if matches!(ans.trim(), "y" | "Y" | "yes") {
        Ok(Op::Approve { call_id })
    } else {
        Ok(Op::Deny {
            call_id,
            reason: Some("user declined".to_owned()),
        })
    }
}

async fn prompt(s: &str) -> anyhow::Result<()> {
    let mut out = tokio::io::stdout();
    out.write_all(s.as_bytes()).await?;
    out.flush().await?;
    Ok(())
}

async fn flush() -> anyhow::Result<()> {
    tokio::io::stdout().flush().await?;
    Ok(())
}

fn compact(v: &serde_json::Value) -> String {
    let s = v.to_string();
    first_lines(&s, 1).chars().take(120).collect()
}

fn first_lines(s: &str, n: usize) -> String {
    let mut out: Vec<&str> = s.lines().take(n).collect();
    if s.lines().count() > n {
        out.push("...");
    }
    out.join("\n")
}

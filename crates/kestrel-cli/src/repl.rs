//! 回合制 REPL：驱动 core 的一轮，把事件流渲染到终端。
//!
//! M1 是同步回合制（读一行 -> 一轮 -> 渲染直到 TurnCompleted），最简且正确。
//! 审批在事件流内联处理：收到 ApprovalRequired 就地问 y/n。

use kestrel_protocol::{Event, EventPayload, Op, RiskLevel};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

/// 运行 REPL 直到用户退出或事件通道关闭。
pub async fn run(
    op_tx: mpsc::Sender<Op>,
    mut event_rx: mpsc::Receiver<Event>,
) -> anyhow::Result<()> {
    let mut stdin = BufReader::new(tokio::io::stdin()).lines();

    loop {
        prompt("> ").await?;
        let Some(line) = stdin.next_line().await? else {
            break; // EOF (Ctrl-D)
        };
        let line = line.trim().to_owned();
        if line.is_empty() {
            continue;
        }
        if line == "/quit" || line == "/exit" {
            break;
        }

        if op_tx.send(Op::UserInput { text: line }).await.is_err() {
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
            EventPayload::UserInput { .. } => {}
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

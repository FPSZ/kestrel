//! shell 工具：执行命令（Windows 走 PowerShell，其他平台走 sh）。
//!
//! - 输出按 `ToolCtx::max_output_bytes` 截断（摄入即截断，§5.2）。
//! - 取消信号杀掉子进程（§5.1 铁律）。
//! - risk() 按命令内容自报，宁高勿低：破坏性模式 -> Destructive，
//!   出网命令 -> External，其余 -> Mutating。

use async_trait::async_trait;
use kestrel_core::CoreError;
use kestrel_core::ports::{Tool, ToolCtx, ToolOutput};
use kestrel_protocol::{RiskLevel, ToolSpec};
use tokio::io::AsyncReadExt;

use crate::util::{str_arg, truncate_head_tail};

const DESTRUCTIVE_PATTERNS: &[&str] = &[
    "rm -rf",
    "rm -r",
    "rmdir /s",
    "del /",
    "format ",
    "mkfs",
    "> /dev/",
    "remove-item",
    "diskpart",
];
const NETWORK_PATTERNS: &[&str] = &[
    "curl ",
    "wget ",
    "invoke-webrequest",
    "invoke-restmethod",
    "iwr ",
    "git push",
    "git pull",
    "git clone",
    "ssh ",
    "scp ",
    "nc ",
];

/// 执行一条 shell 命令。
pub struct ShellTool {
    spec: ToolSpec,
}

impl Default for ShellTool {
    fn default() -> Self {
        Self {
            spec: ToolSpec {
                name: "shell".to_owned(),
                description: "Run a shell command in the workdir and return combined \
                              stdout/stderr. Arg: command."
                    .to_owned(),
                parameters: serde_json::json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["command"],
                    "properties": {
                        "command": { "type": "string", "description": "the command line to run" }
                    }
                }),
            },
        }
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn risk(&self, args: &serde_json::Value) -> RiskLevel {
        let cmd = args
            .get("command")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_lowercase();
        if DESTRUCTIVE_PATTERNS.iter().any(|p| cmd.contains(p)) {
            RiskLevel::Destructive
        } else if NETWORK_PATTERNS.iter().any(|p| cmd.contains(p)) {
            RiskLevel::External
        } else {
            RiskLevel::Mutating
        }
    }

    async fn call(&self, args: serde_json::Value, ctx: &ToolCtx) -> Result<ToolOutput, CoreError> {
        let command = match str_arg(&args, "command") {
            Ok(c) => c.to_owned(),
            Err(e) => {
                return Ok(ToolOutput {
                    ok: false,
                    content: e,
                });
            }
        };

        let mut cmd = build_command(&command);
        cmd.current_dir(&ctx.workdir);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolOutput {
                    ok: false,
                    content: format!("spawn failed: {e}"),
                });
            }
        };

        // 取出管道，并发读到底（避免管道填满死锁），同时监听取消。
        // 缓冲区由 async 块拥有并返回，避免 pinned future 长期借用外层变量。
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let read_fut = async move {
            let mut out_buf = Vec::new();
            let mut err_buf = Vec::new();
            let read_out = async {
                if let Some(s) = stdout.as_mut() {
                    let _ = s.read_to_end(&mut out_buf).await;
                }
            };
            let read_err = async {
                if let Some(s) = stderr.as_mut() {
                    let _ = s.read_to_end(&mut err_buf).await;
                }
            };
            tokio::join!(read_out, read_err);
            (out_buf, err_buf)
        };
        tokio::pin!(read_fut);

        let (out_buf, err_buf, status) = tokio::select! {
            () = ctx.cancel.cancelled() => {
                let _ = child.start_kill();
                return Err(CoreError::Cancelled);
            }
            (out_buf, err_buf) = &mut read_fut => {
                let status = child.wait().await;
                (out_buf, err_buf, status)
            }
        };

        let status = match status {
            Ok(s) => s,
            Err(e) => {
                return Ok(ToolOutput {
                    ok: false,
                    content: format!("command error: {e}"),
                });
            }
        };

        let mut combined = String::from_utf8_lossy(&out_buf).into_owned();
        let stderr_text = String::from_utf8_lossy(&err_buf);
        if !stderr_text.trim().is_empty() {
            combined.push_str("\n[stderr]\n");
            combined.push_str(&stderr_text);
        }
        if !status.success() {
            let code = status.code().unwrap_or(-1);
            combined = format!("[exit {code}]\n{combined}");
        }
        Ok(ToolOutput {
            ok: status.success(),
            content: truncate_head_tail(combined.trim_end(), ctx.max_output_bytes),
        })
    }
}

/// shell 工具实际调用的解释器描述。与下面 `build_command` 的 cfg 分支同源：
/// 二者必须在同一文件、同一 cfg 条件下，保证 system prompt 里对 shell 的声明
/// 不与真正执行命令的进程漂移（否则模型会照着错误的 shell 生成命令）。
#[cfg(windows)]
pub const SHELL_DESC: &str = "Windows PowerShell (powershell.exe, v5.1 syntax)";
#[cfg(not(windows))]
pub const SHELL_DESC: &str = "/bin/sh (POSIX shell)";

#[cfg(windows)]
fn build_command(command: &str) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", command]);
    cmd
}

#[cfg(not(windows))]
fn build_command(command: &str) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("sh");
    cmd.args(["-c", command]);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    fn risk_of(cmd: &str) -> RiskLevel {
        ShellTool::default().risk(&serde_json::json!({ "command": cmd }))
    }

    #[test]
    fn destructive_commands_classified_high() {
        assert_eq!(risk_of("rm -rf build"), RiskLevel::Destructive);
        assert_eq!(risk_of("Remove-Item -Recurse x"), RiskLevel::Destructive);
        assert_eq!(risk_of("mkfs.ext4 /dev/sdb"), RiskLevel::Destructive);
    }

    #[test]
    fn network_commands_classified_external() {
        assert_eq!(risk_of("curl https://example.com"), RiskLevel::External);
        assert_eq!(risk_of("git push origin main"), RiskLevel::External);
    }

    #[test]
    fn plain_commands_classified_mutating() {
        assert_eq!(risk_of("cargo build"), RiskLevel::Mutating);
        assert_eq!(risk_of("ls -la"), RiskLevel::Mutating);
    }

    #[test]
    fn classification_is_case_insensitive() {
        assert_eq!(risk_of("RM -RF /tmp/x"), RiskLevel::Destructive);
        assert_eq!(risk_of("CURL http://x"), RiskLevel::External);
    }
}

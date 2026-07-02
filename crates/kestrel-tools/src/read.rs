//! read 工具：读取工作区内的文件。

use async_trait::async_trait;
use kestrel_core::CoreError;
use kestrel_core::ports::{Tool, ToolCtx, ToolOutput};
use kestrel_protocol::{RiskLevel, ToolSpec};

use crate::util::{resolve_within, str_arg, truncate_head_tail};

/// 读取文件内容（相对 workdir）。
pub struct ReadTool {
    spec: ToolSpec,
}

impl Default for ReadTool {
    fn default() -> Self {
        Self {
            spec: ToolSpec {
                name: "read".to_owned(),
                description: "Read a UTF-8 text file. Arg: path (relative to workdir). \
                              Required before editing a file."
                    .to_owned(),
                parameters: serde_json::json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["path"],
                    "properties": {
                        "path": { "type": "string", "description": "file path relative to workdir" }
                    }
                }),
            },
        }
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn risk(&self, _args: &serde_json::Value) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    async fn call(&self, args: serde_json::Value, ctx: &ToolCtx) -> Result<ToolOutput, CoreError> {
        let path = match str_arg(&args, "path").and_then(|p| resolve_within(&ctx.workdir, p)) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolOutput {
                    ok: false,
                    content: e,
                });
            }
        };
        match tokio::fs::read_to_string(&path).await {
            Ok(text) => Ok(ToolOutput {
                ok: true,
                content: truncate_head_tail(&text, ctx.max_output_bytes),
            }),
            Err(e) => Ok(ToolOutput {
                ok: false,
                content: format!("cannot read {}: {e}", path.display()),
            }),
        }
    }
}

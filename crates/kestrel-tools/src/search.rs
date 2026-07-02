//! search 工具：内容搜索（子串）与文件名匹配合一。
//!
//! 合一的理由：省一个工具 schema 的 token（原则 2）。
//! M1 用无依赖的递归遍历 + 子串匹配；正则/ignore 规则留待后续。

use std::path::Path;

use async_trait::async_trait;
use kestrel_core::CoreError;
use kestrel_core::ports::{Tool, ToolCtx, ToolOutput};
use kestrel_protocol::{RiskLevel, ToolSpec};

use crate::util::{str_arg, truncate_head_tail};

const MAX_HITS: usize = 100;
const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", ".venv", "dist"];

/// 在工作区内搜索包含指定子串的行。
pub struct SearchTool {
    spec: ToolSpec,
}

impl Default for SearchTool {
    fn default() -> Self {
        Self {
            spec: ToolSpec {
                name: "search".to_owned(),
                description: "Search file contents for a literal substring across the workdir. \
                              Arg: pattern. Returns up to 100 path:line: matches."
                    .to_owned(),
                parameters: serde_json::json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["pattern"],
                    "properties": {
                        "pattern": { "type": "string", "description": "literal substring to find" }
                    }
                }),
            },
        }
    }
}

#[async_trait]
impl Tool for SearchTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn risk(&self, _args: &serde_json::Value) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    async fn call(&self, args: serde_json::Value, ctx: &ToolCtx) -> Result<ToolOutput, CoreError> {
        let pattern = match str_arg(&args, "pattern") {
            Ok(p) => p.to_owned(),
            Err(e) => {
                return Ok(ToolOutput {
                    ok: false,
                    content: e,
                });
            }
        };
        let workdir = ctx.workdir.clone();
        let max = ctx.max_output_bytes;

        // 文件遍历用阻塞线程，避免占用异步 runtime。
        let result = tokio::task::spawn_blocking(move || {
            let mut hits = Vec::new();
            walk(&workdir, &workdir, &pattern, &mut hits);
            hits
        })
        .await
        .map_err(|e| CoreError::Tool(format!("search task: {e}")))?;

        if result.is_empty() {
            return Ok(ToolOutput {
                ok: true,
                content: "no matches".to_owned(),
            });
        }
        let mut body = result.join("\n");
        if result.len() >= MAX_HITS {
            use std::fmt::Write as _;
            let _ = write!(body, "\n... [capped at {MAX_HITS} matches]");
        }
        Ok(ToolOutput {
            ok: true,
            content: truncate_head_tail(&body, max),
        })
    }
}

fn walk(root: &Path, dir: &Path, pattern: &str, hits: &mut Vec<String>) {
    if hits.len() >= MAX_HITS {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if hits.len() >= MAX_HITS {
            return;
        }
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref()) {
                continue;
            }
            walk(root, &path, pattern, hits);
        } else if let Ok(text) = std::fs::read_to_string(&path) {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            for (i, line) in text.lines().enumerate() {
                if line.contains(pattern) {
                    hits.push(format!("{}:{}: {}", rel.display(), i + 1, line.trim()));
                    if hits.len() >= MAX_HITS {
                        return;
                    }
                }
            }
        }
    }
}

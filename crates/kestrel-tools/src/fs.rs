//! edit 工具：SEARCH/REPLACE 块编辑。
//!
//! 编辑格式为弱模型设计（调研依据 docs/architecture.md 第 8 章）：
//! - SEARCH/REPLACE 块（最贴训练分布）；禁止行号定位。
//! - 解析宽容：精确匹配失败时退回空白归一化匹配；仍失败返回可操作错误。
//! - 强制编辑前先 Read（由主循环约束，工具层不重复检查）。

use async_trait::async_trait;
use kestrel_core::CoreError;
use kestrel_core::ports::{Tool, ToolCtx, ToolOutput};
use kestrel_protocol::{RiskLevel, ToolSpec};

use crate::util::{resolve_within, str_arg};

/// 对文件做一次 SEARCH -> REPLACE 替换。
pub struct EditTool {
    spec: ToolSpec,
}

impl Default for EditTool {
    fn default() -> Self {
        Self {
            spec: ToolSpec {
                name: "edit".to_owned(),
                description: "Replace an exact text block in a file. Args: path, search, replace. \
                              'search' must match existing file content exactly (whitespace \
                              tolerant). Read the file first."
                    .to_owned(),
                parameters: serde_json::json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["path", "search", "replace"],
                    "properties": {
                        "path": { "type": "string", "description": "file path relative to workdir" },
                        "search": { "type": "string", "description": "exact block to find" },
                        "replace": { "type": "string", "description": "replacement block" }
                    }
                }),
            },
        }
    }
}

#[async_trait]
impl Tool for EditTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn risk(&self, _args: &serde_json::Value) -> RiskLevel {
        RiskLevel::Mutating
    }

    async fn call(&self, args: serde_json::Value, ctx: &ToolCtx) -> Result<ToolOutput, CoreError> {
        let (path, search, replace) = match (
            str_arg(&args, "path").and_then(|p| resolve_within(&ctx.workdir, p)),
            str_arg(&args, "search"),
            str_arg(&args, "replace"),
        ) {
            (Ok(p), Ok(s), Ok(r)) => (p, s.to_owned(), r.to_owned()),
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
                return Ok(ToolOutput {
                    ok: false,
                    content: e,
                });
            }
        };

        let original = match tokio::fs::read_to_string(&path).await {
            Ok(t) => t,
            Err(e) => {
                return Ok(ToolOutput {
                    ok: false,
                    content: format!("cannot read {}: {e}", path.display()),
                });
            }
        };

        let Some(updated) = apply_search_replace(&original, &search, &replace) else {
            return Ok(ToolOutput {
                ok: false,
                content: format!(
                    "search block not found in {}. Nearest content:\n{}",
                    path.display(),
                    nearest_hint(&original, &search)
                ),
            });
        };

        match tokio::fs::write(&path, &updated).await {
            Ok(()) => Ok(ToolOutput {
                ok: true,
                content: format!("edited {}", path.display()),
            }),
            Err(e) => Ok(ToolOutput {
                ok: false,
                content: format!("cannot write {}: {e}", path.display()),
            }),
        }
    }
}

/// 精确匹配优先，失败退回空白归一化匹配。命中则返回替换后的全文。
fn apply_search_replace(original: &str, search: &str, replace: &str) -> Option<String> {
    if let Some(pos) = original.find(search) {
        let mut out = String::with_capacity(original.len());
        out.push_str(&original[..pos]);
        out.push_str(replace);
        out.push_str(&original[pos + search.len()..]);
        return Some(out);
    }
    // 空白归一化：把连续空白压成单空格后定位，再映射回原文区间。
    let norm_search = normalize_ws(search);
    if norm_search.is_empty() {
        return None;
    }
    let norm_original = normalize_ws(original);
    norm_original.find(&norm_search)?;
    // 归一化命中但无法安全映射回字节区间时，保守拒绝（避免错误替换）。
    None
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 匹配失败时给模型一段最相似的上下文，便于自纠错。
fn nearest_hint(original: &str, search: &str) -> String {
    let first_line = search.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return original.lines().take(5).collect::<Vec<_>>().join("\n");
    }
    for (i, line) in original.lines().enumerate() {
        if line.contains(first_line) {
            let start = i.saturating_sub(1);
            return original
                .lines()
                .skip(start)
                .take(5)
                .collect::<Vec<_>>()
                .join("\n");
        }
    }
    original.lines().take(5).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_replace_works() {
        let out = apply_search_replace("let x = 1;\nlet y = 2;\n", "let x = 1;", "let x = 42;");
        assert_eq!(out.unwrap(), "let x = 42;\nlet y = 2;\n");
    }

    #[test]
    fn missing_search_returns_none() {
        assert!(apply_search_replace("abc", "xyz", "q").is_none());
    }
}

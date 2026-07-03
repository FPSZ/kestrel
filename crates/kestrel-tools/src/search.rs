//! search 工具：内容搜索（字面/正则）与文件名匹配（glob）合一。
//!
//! 合一的理由：省一个工具 schema 的 token（原则 2）。
//! - `pattern` 默认字面子串；`regex: true` 时按正则匹配（编译失败给可操作错误）。
//! - `glob` 可选：按文件相对路径的通配（`*` / `?`）过滤搜索范围；`pattern` 省略时
//!   退化为纯文件名查找（列出匹配 glob 的文件）。

use std::path::Path;

use async_trait::async_trait;
use kestrel_core::CoreError;
use kestrel_core::ports::{Tool, ToolCtx, ToolOutput};
use kestrel_protocol::{RiskLevel, ToolSpec};
use regex::Regex;

use crate::util::{opt_str_arg, str_arg, truncate_head_tail};

const MAX_HITS: usize = 100;
const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", ".venv", "dist"];

/// 匹配器：字面子串或已编译正则。
enum Matcher {
    Literal(String),
    Regex(Box<Regex>),
}

impl Matcher {
    fn is_match(&self, line: &str) -> bool {
        match self {
            Matcher::Literal(s) => line.contains(s.as_str()),
            Matcher::Regex(re) => re.is_match(line),
        }
    }
}

/// 在工作区内搜索内容（字面/正则）或文件名（glob）。
pub struct SearchTool {
    spec: ToolSpec,
}

impl Default for SearchTool {
    fn default() -> Self {
        Self {
            spec: ToolSpec {
                name: "search".to_owned(),
                description: "Search the workdir. 'pattern' matches file contents (literal \
                              substring; set regex=true for regular expressions). Optional \
                              'glob' filters files by relative path (* and ? wildcards); with \
                              no pattern it just lists matching files. Returns up to 100 \
                              path:line: matches."
                    .to_owned(),
                parameters: serde_json::json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "pattern": { "type": "string", "description": "text or regex to find in file contents" },
                        "regex": { "type": "boolean", "description": "treat pattern as a regular expression (default false)" },
                        "glob": { "type": "string", "description": "filter files by relative path, e.g. src/*.rs" }
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
        let pattern = opt_str_arg(&args, "pattern").map(str::to_owned);
        let glob = opt_str_arg(&args, "glob").map(str::to_owned);
        let use_regex = args
            .get("regex")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        // pattern 与 glob 至少一个必须给（否则无从搜起）。
        if pattern.is_none() && glob.is_none() {
            return Ok(ToolOutput {
                ok: false,
                content: str_arg(&args, "pattern")
                    .err()
                    .unwrap_or_else(|| "provide 'pattern' and/or 'glob'".to_owned()),
            });
        }

        // 构建内容匹配器（若给了 pattern）。正则编译失败 -> 可操作错误。
        let matcher = match &pattern {
            Some(p) if use_regex => match Regex::new(p) {
                Ok(re) => Some(Matcher::Regex(Box::new(re))),
                Err(e) => {
                    return Ok(ToolOutput {
                        ok: false,
                        content: format!("invalid regex '{p}': {e}"),
                    });
                }
            },
            Some(p) => Some(Matcher::Literal(p.clone())),
            None => None,
        };

        let workdir = ctx.workdir.clone();
        let max = ctx.max_output_bytes;

        // 文件遍历用阻塞线程，避免占用异步 runtime。
        let result = tokio::task::spawn_blocking(move || {
            let mut hits = Vec::new();
            walk(
                &workdir,
                &workdir,
                matcher.as_ref(),
                glob.as_deref(),
                &mut hits,
            );
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

fn walk(
    root: &Path,
    dir: &Path,
    matcher: Option<&Matcher>,
    glob: Option<&str>,
    hits: &mut Vec<String>,
) {
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
            walk(root, &path, matcher, glob, hits);
            continue;
        }
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        // glob 过滤：只搜路径匹配的文件。
        if let Some(g) = glob
            && !glob_match(g, &rel_str)
        {
            continue;
        }
        match matcher {
            // 无内容匹配器：纯文件名查找，列出匹配 glob 的文件。
            None => hits.push(rel_str),
            Some(m) => {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    for (i, line) in text.lines().enumerate() {
                        if m.is_match(line) {
                            hits.push(format!("{rel_str}:{}: {}", i + 1, line.trim()));
                            if hits.len() >= MAX_HITS {
                                return;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 极简 glob：支持 `*`（任意段内字符，含空）与 `?`（单个字符）。
/// 不支持 `**` / 字符类——够覆盖 `src/*.rs` 这类常见过滤，保持零依赖。
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    glob_rec(&p, &t)
}

fn glob_rec(p: &[char], t: &[char]) -> bool {
    match p.first() {
        None => t.is_empty(),
        Some('*') => {
            // 匹配零个或多个字符：尝试消费 0..=len。
            glob_rec(&p[1..], t) || (!t.is_empty() && glob_rec(p, &t[1..]))
        }
        Some('?') => !t.is_empty() && glob_rec(&p[1..], &t[1..]),
        Some(&c) => !t.is_empty() && t[0] == c && glob_rec(&p[1..], &t[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_star_matches_within_and_across() {
        assert!(glob_match("src/*.rs", "src/main.rs"));
        assert!(glob_match("*.rs", "lib.rs"));
        assert!(!glob_match("src/*.rs", "tests/main.rs"));
        assert!(glob_match("a*b", "aXYZb"));
        assert!(glob_match("a*b", "ab"));
    }

    #[test]
    fn glob_question_matches_single() {
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "ac"));
    }

    #[test]
    fn literal_matcher_finds_substring() {
        let m = Matcher::Literal("fn ".to_owned());
        assert!(m.is_match("pub fn foo()"));
        assert!(!m.is_match("let x = 1;"));
    }

    #[test]
    fn regex_matcher_matches_pattern() {
        let m = Matcher::Regex(Box::new(Regex::new(r"fn\s+\w+").unwrap()));
        assert!(m.is_match("pub fn  foo()"));
        assert!(!m.is_match("let x = 1;"));
    }
}

//! 工具共用的小工具：路径安全解析、输出截断。

use std::path::{Component, Path, PathBuf};

/// 摄入即截断（ARCHITECTURE.md §5.2）：超预算时保头保尾、中间折叠。
///
/// 工具输出是头号 token 杀手，在返回那一刻就截断，而非等压缩时处理。
pub(crate) fn truncate_head_tail(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_owned();
    }
    let half = max_bytes / 2;
    let head_end = floor_char_boundary(s, half);
    let tail_start = ceil_char_boundary(s, s.len() - half);
    let omitted = s[head_end..tail_start].lines().count();
    format!(
        "{}\n... [truncated {omitted} lines] ...\n{}",
        &s[..head_end],
        &s[tail_start..]
    )
}

fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// 把相对路径解析到工作目录内，拒绝逃逸（`..` 越过 workdir）。
///
/// 权限护栏的一部分：工具的文件操作不得越出 workdir。
pub(crate) fn resolve_within(workdir: &Path, rel: &str) -> Result<PathBuf, String> {
    let candidate = Path::new(rel);
    if candidate.is_absolute() {
        return Err(format!("path must be relative to workdir: {rel}"));
    }
    let mut resolved = workdir.to_path_buf();
    for comp in candidate.components() {
        match comp {
            Component::Normal(part) => resolved.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !resolved.pop() || !resolved.starts_with(workdir) {
                    return Err(format!("path escapes workdir: {rel}"));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("path must be relative to workdir: {rel}"));
            }
        }
    }
    if resolved.starts_with(workdir) {
        Ok(resolved)
    } else {
        Err(format!("path escapes workdir: {rel}"))
    }
}

/// 从 JSON 参数取一个必填字符串字段。
pub(crate) fn str_arg<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("missing required string arg: {key}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_keeps_head_and_tail() {
        let s = "a".repeat(1000);
        let out = truncate_head_tail(&s, 100);
        assert!(out.len() < s.len());
        assert!(out.contains("truncated"));
        assert!(out.starts_with('a'));
        assert!(out.ends_with('a'));
    }

    #[test]
    fn resolve_rejects_escape() {
        let wd = Path::new("/work");
        assert!(resolve_within(wd, "../etc/passwd").is_err());
        assert!(resolve_within(wd, "/etc/passwd").is_err());
        assert!(resolve_within(wd, "src/main.rs").is_ok());
    }
}

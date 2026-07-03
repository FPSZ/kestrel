//! 宿主工具探测：委托模式下判断 `ollama` / `lms` 等已在 `PATH` 里，用于给出
//! 可操作的提示（「装了但没起」）。零依赖 `PATH` 扫描，不 spawn、不联网。

use std::path::Path;

/// 可执行文件在各平台可能的后缀（Windows 需带扩展名才可执行）。
#[cfg(windows)]
const EXE_SUFFIXES: &[&str] = &[".exe", ".cmd", ".bat", ""];
#[cfg(not(windows))]
const EXE_SUFFIXES: &[&str] = &[""];

/// 名为 `name` 的可执行文件是否在 `PATH` 中可见（best-effort，纯查找不执行）。
///
/// 仅用于诊断提示，绝不据此自动 spawn 任何东西（安全：委托只连不代起）。
#[must_use]
pub fn host_tool_available(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| exe_exists_in(&dir, name))
}

/// `dir` 下是否存在 `name`（含平台可执行后缀）的文件。
fn exe_exists_in(dir: &Path, name: &str) -> bool {
    EXE_SUFFIXES
        .iter()
        .any(|suf| dir.join(format!("{name}{suf}")).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_tool_is_not_available() {
        assert!(!host_tool_available("kestrel-no-such-host-tool-xyz"));
    }

    #[test]
    fn empty_name_does_not_panic() {
        // 不应因空名或诡异输入 panic（防御式，返回 false 即可）。
        let _ = host_tool_available("");
    }
}

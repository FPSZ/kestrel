//! # kestrel-tools
//!
//! 内置工具集：[`kestrel_core::ports::Tool`] 的实现（docs/architecture.md 第 8 章）。
//!
//! ## 职责边界
//!
//! - 实现 core 的 Tool 端口；禁止依赖其他适配器 crate 与前端 crate。
//! - 工具数量纪律：内置工具 <= 10 个，每个 schema 都吃前缀预算。
//! - 每个工具的 spec 完全静态；schema 逐 token 手工优化，
//!   全部工具总预算 <= 1400 token（原则 2）。
//!
//! ## 工具清单（M1）
//!
//! | 模块 | 工具 | 风险基线 |
//! | --- | --- | --- |
//! | [`read`] | 读取文件 | ReadOnly |
//! | [`search`] | 子串内容搜索 | ReadOnly |
//! | [`fs`] | edit（SEARCH-REPLACE，宽容解析，编辑前必须 Read） | Mutating |
//! | [`shell`] | 执行命令 | Mutating 起步，按命令内容升级 |
//!
//! browser（CDP）与 process（系统管理）规划于 M4。

pub mod fs;
pub mod read;
pub mod registry;
pub mod search;
pub mod shell;
mod util;

pub use fs::EditTool;
pub use read::ReadTool;
pub use registry::builtin;
pub use search::SearchTool;
pub use shell::ShellTool;

use std::path::Path;

/// 主机环境块，追加到 system prompt 的稳定前缀末尾（组装根调用）。
///
/// 让模型知道自己身处什么 OS、`shell` 工具用哪个解释器执行命令、以及工作目录，
/// 从而生成该平台原生的命令（Windows 上不要再吐 Linux/bash 指令）。
///
/// OS 名取编译期常量 [`std::env::consts::OS`]，shell 描述取 [`shell::SHELL_DESC`]
/// （与真实 `build_command` 同源）。三项在进程生命周期内均不变，故并入稳定前缀
/// 不破坏 KV 缓存（前缀字节稳定铁律）。
pub fn environment_block(workdir: &Path) -> String {
    // 展示用：去掉 Windows 扩展长度路径前缀 \\?\。
    let dir = workdir.display().to_string();
    let dir = dir.trim_start_matches(r"\\?\");
    format!(
        "\n\nEnvironment:\n\
         - OS: {}\n\
         - The shell tool executes commands with: {}\n\
         - Working directory: {dir}\n\
         Use commands, flags, and path separators native to this OS and shell; \
         do not assume Linux or bash.",
        std::env::consts::OS,
        shell::SHELL_DESC,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_block_states_host_os_shell_and_dir() {
        let block = environment_block(Path::new("/work/dir"));
        // 声明的 OS/shell 必须与真实执行同源，且带上工作目录。
        assert!(block.contains(std::env::consts::OS));
        assert!(block.contains(shell::SHELL_DESC));
        assert!(block.contains("/work/dir"));
    }

    #[cfg(windows)]
    #[test]
    fn environment_block_trims_windows_verbatim_prefix() {
        let block = environment_block(Path::new(r"\\?\D:\AI\Agent-local"));
        assert!(block.contains(r"D:\AI\Agent-local"));
        assert!(!block.contains(r"\\?\"));
    }
}

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

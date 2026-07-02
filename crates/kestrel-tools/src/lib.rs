//! # kestrel-tools
//!
//! 内置工具集：[`kestrel_core::ports::Tool`] 的实现（ARCHITECTURE.md 第 8 章）。
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
//! | [`shell`] | 执行 PowerShell/命令 | Mutating 起步，按命令内容升级 |
//! | [`fs`] | read / edit（SEARCH-REPLACE 为主，宽容解析，编辑前必须 Read） | read=ReadOnly, edit=Mutating |
//! | [`search`] | grep + glob 合一 | `ReadOnly` |
//!
//! browser（CDP）与 process（系统管理）规划于 M4。

pub mod fs;
pub mod search;
pub mod shell;

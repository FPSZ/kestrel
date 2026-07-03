//! # kestrel-store
//!
//! 存储层：[`kestrel_core::ports::Store`] 的实现 + 配置 + 模型 profile。
//!
//! ## 职责边界
//!
//! - 实现 core 的 Store 端口；禁止依赖其他适配器 crate 与前端 crate。
//! - 存储格式纪律：事件日志 = JSONL（append-only，一个机制同时买到
//!   持久化/审计/崩溃恢复/回放测试，ADR-002）；配置与 profile = TOML
//!   （单文件，拒绝 OpenHands 式 140+ 字段配置，§2.2）。
//! - 不引入数据库。需要时走 ADR。
//!
//! ## 模块导览
//!
//! | 模块 | 职责 |
//! | --- | --- |
//! | [`jsonl`] | JSONL 事件日志（Store 端口实现） |
//! | [`config`] | kestrel.toml 单文件配置 |
//! | [`loadout`] | Loadout 声明式清单（ADR-0006 + ADR-0010 §3） |
//! | [`paths`] | 数据/配置目录解析 + 版本化迁移（ADR-0009） |
//! | [`profile`] | 模型 profile 读写（能力探针的产物） |

pub mod config;
pub mod jsonl;
pub mod loadout;
pub mod paths;
pub mod profile;

pub use config::{BackendConfig, Config};
pub use jsonl::JsonlStore;
pub use loadout::{Loadout, LoadoutMetadata, ModelSpec};
pub use paths::Layout;

/// 存储层错误。
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// 配置解析错误。
    #[error("config: {0}")]
    Config(String),
    /// 文件 IO 错误。
    #[error("io: {0}")]
    Io(String),
}

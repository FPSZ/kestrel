//! # kestrel-runtime
//!
//! 模型启动器 / 监督器（ADR-0010）：把模型**作为 agent 的一部分**来启动并监督。
//!
//! ## 职责边界
//!
//! - 落在适配器层，`kestrel-core` **不依赖**本 crate（ADR-0010 §6）；组装根
//!   （cli / server）调用本 crate 得到一个就绪的 `base_url`，再交给
//!   [`kestrel_core::ports::LlmBackend`] 去连。
//! - 禁止依赖其他适配器 crate（backend / store / tools）与前端 crate——与
//!   `kestrel-backend` 同纪律。Loadout -> [`LaunchSpec`] 的映射发生在组装根。
//! - 只做「为 agent 而启动并监督 llama.cpp，并能委托已有宿主」，**不做**模型
//!   发现 / 下载 / registry（留给 Ollama / LM Studio / HuggingFace）。
//!
//! ## 三种来源（ADR-0010 §2，都是一等公民）
//!
//! | 来源 | [`EngineSource`] | 行为 |
//! | --- | --- | --- |
//! | 自启 llama.cpp | [`EngineSource::SelfLaunch`] | spawn `llama-server … --jinja`，轮询 `/health` 就绪 |
//! | 委托已有宿主 | [`EngineSource::Delegate`] | 连一个已在跑的 server（lms/ollama/手起），可达才用 |
//! | 纯连接（现状） | [`EngineSource::Connect`] | 连指定 `base_url`，零启动 |
//!
//! ## 安全（ADR-0010 §5，铁律不可削弱）
//!
//! - 引擎二进制走**白名单 / 显式配置**：`SelfLaunch.bin` 必须是**存在的绝对路径**，
//!   否则 [`launch`] 拒绝（防任意路径 spawn 越权）。
//! - 自启进程只绑 `127.0.0.1`（[`LaunchSpec::self_launch`] 强制 `--host 127.0.0.1`），
//!   不自动联网拉模型。
//! - spawn / stop 走**结构化 `tracing` 审计轨**（英文、可 grep），是「配置即授权 +
//!   可审计」姿态的落点。（升级为 `EventPayload` 事件待协议稳定后接入。）

mod detect;
mod error;
mod spec;
mod supervisor;

pub use detect::host_tool_available;
pub use error::RuntimeError;
pub use spec::{EngineSource, LaunchSpec};
pub use supervisor::{EngineHandle, launch};

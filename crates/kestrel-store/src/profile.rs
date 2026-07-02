//! 模型 profile：能力探针（§5.4）的产物，按模型缓存。
//!
//! - 内置 profile 随仓库分发（profiles/*.toml）。
//! - 探针实测生成的本地覆盖存为 profiles/*.local.toml（不入库）。
//! - 内容：工具调用协议档位（native / hermes-xml / json-schema）、
//!   编辑格式（search-replace / whole-file）、建议采样参数。

// TODO(M3): pub struct ModelProfile + load/save

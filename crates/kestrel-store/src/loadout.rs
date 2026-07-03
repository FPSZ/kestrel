//! Loadout 声明式清单（ADR-0006 + ADR-0010 §3）。
//!
//! 一份 Loadout 声明「带什么进战场」：模型 @ 量化 + 引擎参数 + 工具 + 权限 + 人设 +
//! 机组 + 记忆种子。全部是**数据、无可执行代码**（ADR-0006 安全模型）。
//!
//! ## 本阶段范围（诚实标注）
//!
//! ADR-0006 的**成本感知编译器**（token 预算强制、能力分层、机组降级）依赖 ADR-0005
//! 能力分层，属 M4 及之后。本模块**只落 `[model]` 引擎/启动维度**（ADR-0010 §3 新引入的
//! 「模型@量化+参数」），供模型启动器 [`kestrel_runtime`] 消费；其余维度
//! （`persona` / `tools` / `permission` / `crew` / `memory` / `adapter`）按 ADR-0006
//! 「先固化格式草案、预留扩展点」**解析但暂不编译**，字段只加不改保证前向兼容。
//!
//! ## 安全（ADR-0006 分发信任模型）
//!
//! 一份共享的 Loadout 可携带能力组合 + 人设，但**权限策略永不随导入继承**——导入他人
//! Loadout 时本地回落最严档，权限须由本机用户重新确认。故 `permission` 维度此处仅存原样
//! 数据、**不**在加载时生效（守 AGENTS.md 第 7 节红线）。

use std::path::{Path, PathBuf};

use kestrel_protocol::SecretString;
use serde::{Deserialize, Serialize};

use crate::StoreError;

/// 一份 Loadout 清单（TOML）。未知顶层键被 serde 容忍（前向兼容）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Loadout {
    /// 元数据：分发与检索用。
    pub metadata: LoadoutMetadata,
    /// 引擎 / 模型启动维度（ADR-0010 §3）——本阶段唯一被编译消费的维度。
    pub model: ModelSpec,

    // --- 以下为 ADR-0006 预留维度：解析但暂不编译（M4 接入编译器）。---
    /// 人设 / system prompt 片段（进冻结前缀，计入 token 预算——M4 强制）。
    pub persona: Option<toml::Value>,
    /// 工具分层（core / cataloged，见 ADR-0005）。
    pub tools: Option<toml::Value>,
    /// 建议权限档位。**导入不自动继承**，仅存数据（见模块级安全说明）。
    pub permission: Option<toml::Value>,
    /// 机组花名册（lead / copilot / librarian / critic 各用哪个模型）。
    pub crew: Option<toml::Value>,
    /// 记忆种子 / 领域知识（进可检索存储，不进前缀）。
    pub memory: Option<toml::Value>,
    /// 预留：未来挂载 LoRA（ADR-0006 分期方案 b），现在恒空。
    pub adapter: Option<toml::Value>,
}

/// Loadout 元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LoadoutMetadata {
    /// 名称。
    pub name: String,
    /// 版本。
    pub version: String,
    /// 作者。
    pub author: String,
    /// 目标人群（`ctf` / `gov` / `home` / ...）。
    pub audience: String,
    /// 描述。
    pub description: String,
}

/// 引擎 / 模型启动规格（喂给模型启动器）。
///
/// 字段克制（配置纪律铁律）：自启用 `bin` / `model_path` / `gpu_layers` / `extra_args`；
/// 委托 / 连接用 `base_url`；连接层用 `kind` / `model` / `n_ctx` / `api_key`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelSpec {
    /// 引擎来源：`self`（自启 llama.cpp）| `delegate`（委托已跑宿主）| `connect`（纯连接）。
    pub source: String,
    /// 后端连接层类型：`llamacpp` | `lmstudio` | `openai`（自启恒按 llamacpp 连）。
    pub kind: String,
    /// 自启：引擎二进制**绝对路径**（白名单=显式配置，ADR-0010 §5）。
    pub bin: Option<PathBuf>,
    /// 自启：gguf 模型**绝对路径**。
    pub model_path: Option<PathBuf>,
    /// 模型标识（连接层 `model` 名；自启时可空，llama-server 任意）。
    pub model: String,
    /// 端口（自启：`--port`；连接：合成 `base_url` 用）。
    pub port: u16,
    /// 委托 / 连接：显式 `base_url`（不含 `/v1`）；空则用回环 `127.0.0.1:port` 合成。
    pub base_url: Option<String>,
    /// 上下文长度（自启 `-c`；probe 成功以实测覆盖）。
    pub n_ctx: u32,
    /// GPU 卸载层数：`auto`（默认，防 OOM）| `max` | 数字。
    pub gpu_layers: String,
    /// 追加透传给 `llama-server` 的参数（高级逃生舱）。
    pub extra_args: Vec<String>,
    /// API key（委托宿主需 token 时）。脱敏类型：不入事件日志 / 审计 / UI / 提交（地基 #7）。
    pub api_key: SecretString,
}

impl Default for ModelSpec {
    fn default() -> Self {
        Self {
            source: "connect".to_owned(),
            kind: "openai".to_owned(),
            bin: None,
            model_path: None,
            model: "local".to_owned(),
            port: 8080,
            base_url: None,
            n_ctx: 16_384,
            gpu_layers: "auto".to_owned(),
            extra_args: Vec::new(),
            api_key: SecretString::default(),
        }
    }
}

impl Loadout {
    /// 从 TOML 文件加载。与 [`crate::Config::load`] 不同：Loadout 是**显式指定**的，
    /// 文件缺失即配置错误（不静默兜底），把「指了个不存在的 loadout」当场点出来。
    pub fn load(path: &Path) -> Result<Self, StoreError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| StoreError::Io(format!("read loadout {}: {e}", path.display())))?;
        toml::from_str(&text).map_err(|e| StoreError::Config(format!("loadout: {e}")))
    }

    /// 后端连接层类型：自启恒按 `llamacpp` 连（我们起的就是 llama-server），
    /// 否则用 `model.kind`。供组装根选 backend 实现。
    #[must_use]
    pub fn backend_kind(&self) -> &str {
        if self.is_self_launch() {
            "llamacpp"
        } else {
            &self.model.kind
        }
    }

    /// 是否自启模式。
    #[must_use]
    pub fn is_self_launch(&self) -> bool {
        matches!(
            self.model.source.trim().to_ascii_lowercase().as_str(),
            "self" | "self-launch" | "self_launch" | "launch"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_pure_connect() {
        let lo = Loadout::default();
        assert_eq!(lo.model.source, "connect");
        assert!(!lo.is_self_launch());
        assert_eq!(lo.backend_kind(), "openai");
    }

    #[test]
    fn parse_self_launch_loadout() {
        let toml = r#"
[metadata]
name = "ctf-pwn"
audience = "ctf"

[model]
source = "self"
bin = "/opt/llama/llama-server"
model_path = "/models/qwen3-8b-q4.gguf"
model = "qwen3-8b"
port = 8099
n_ctx = 32768
gpu_layers = "max"
extra_args = ["--flash-attn"]
"#;
        let lo: Loadout = toml::from_str(toml).unwrap();
        assert_eq!(lo.metadata.name, "ctf-pwn");
        assert!(lo.is_self_launch());
        assert_eq!(lo.backend_kind(), "llamacpp"); // 自启恒 llamacpp
        assert_eq!(lo.model.port, 8099);
        assert_eq!(lo.model.gpu_layers, "max");
        assert_eq!(
            lo.model.bin.as_deref(),
            Some(Path::new("/opt/llama/llama-server"))
        );
    }

    #[test]
    fn reserved_dimensions_parse_but_are_inert() {
        // ADR-0006 预留维度：给了也能解析，不报错（前向兼容）。
        let toml = r#"
[model]
source = "connect"
base_url = "http://127.0.0.1:1234"

[persona]
system = "you are a pwn specialist"

[crew]
lead = "qwen3-8b"

[permission]
policy = "strict"

[some_future_section]
whatever = 42
"#;
        let lo: Loadout = toml::from_str(toml).unwrap();
        assert!(lo.persona.is_some());
        assert!(lo.crew.is_some());
        assert!(lo.permission.is_some()); // 存了数据，但加载不生效（信任模型）
        assert_eq!(lo.model.base_url.as_deref(), Some("http://127.0.0.1:1234"));
    }

    #[test]
    fn load_missing_file_is_error_not_default() {
        let path = std::env::temp_dir().join("kestrel-no-such-loadout-xyz.toml");
        assert!(Loadout::load(&path).is_err());
    }

    #[test]
    fn shipped_example_loadout_parses() {
        // 守住随仓库分发的示例：改坏格式会当场红，别让文档漂移。
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../kestrel.example.loadout.toml");
        let lo = Loadout::load(&path).expect("example loadout must parse");
        assert!(lo.is_self_launch());
        assert_eq!(lo.backend_kind(), "llamacpp");
    }
}

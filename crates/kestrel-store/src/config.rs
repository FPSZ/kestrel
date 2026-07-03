//! kestrel.toml 单文件配置。
//!
//! 配置纪律（OpenHands 140+ 字段的反面教训，§2.2）：字段总数保持个位数
//! 到低两位数；每加一个字段先问能不能用约定代替。

use std::path::{Path, PathBuf};

use kestrel_protocol::SecretString;
use serde::{Deserialize, Serialize};

use crate::StoreError;

/// 顶层配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// 后端连接。
    pub backend: BackendConfig,
    /// 确认策略：`on-request` | `auto` | `strict`。
    pub approval_policy: String,
    /// 全局禁用的工具名（deny 优先，组装时从工具列表预过滤——见权限门 §5.3）。
    /// 默认空。例：`deny_tools = ["shell"]` 起一个纯只读、绝不执行命令的 agent。
    pub deny_tools: Vec<String>,
    /// agent 工作目录（工具的文件操作以此为界）。默认当前目录。
    pub workdir: PathBuf,
    /// 会话事件日志目录的**显式覆盖**。默认 `None`：走 OS 标准数据目录
    /// （[`crate::Layout`]，ADR-0009）。设置后优先于标准目录（尊重"数据在手边"）。
    #[serde(default)]
    pub sessions_dir: Option<PathBuf>,
    /// Loadout 清单路径（ADR-0006 + ADR-0010）。默认 `None`：走 `[backend]` 纯连接。
    /// 设置后由模型启动器按清单的 `[model]` 维度自启 / 委托 / 连接引擎，`[model]`
    /// 覆盖 `[backend]`。相对路径按配置文件所在目录解析（见组装根）。
    #[serde(default)]
    pub loadout: Option<PathBuf>,
    /// 单个回合内模型↔工具往返的上限。到顶即以 `max_iterations` 收尾。agentic 任务
    /// （如 CTF：读二进制→反汇编→试 payload）需要更多轮，故默认给足。
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
}

fn default_max_iterations() -> u32 {
    25
}

/// 后端连接配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BackendConfig {
    /// 后端类型：`openai`（兜底，默认）| `llamacpp` | `lmstudio`。
    /// 选 llamacpp/lmstudio 可在启动时探测真实上下文长度（覆盖 `n_ctx`）。
    pub kind: String,
    /// OpenAI 兼容端点基址（llama-server 或 LM Studio），不含 `/v1`。
    pub base_url: String,
    /// API key（本地后端通常留空）。脱敏类型：不入 Debug / 序列化 / 日志 / UI（地基 #7）。
    pub api_key: SecretString,
    /// 模型名（llama-server 可任意；LM Studio 需匹配已加载模型）。
    pub model: String,
    /// 上下文长度兜底值。探针（llamacpp/lmstudio）成功时以实测值覆盖。
    pub n_ctx: u32,
    /// 单次生成的 token 上限（`0`=不设限）。掐断失控生成——推理模型在难题上
    /// 常陷入思考死循环、狂吐 reasoning 却不收敛，此上限交给 llama-server 掐断。
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_max_tokens() -> u32 {
    8192
}

impl Default for Config {
    fn default() -> Self {
        Self {
            backend: BackendConfig::default(),
            approval_policy: "on-request".to_owned(),
            deny_tools: Vec::new(),
            workdir: PathBuf::from("."),
            sessions_dir: None,
            loadout: None,
            max_iterations: default_max_iterations(),
        }
    }
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            kind: "openai".to_owned(),
            base_url: "http://127.0.0.1:8080".to_owned(),
            api_key: SecretString::default(),
            model: "local".to_owned(),
            n_ctx: 16_384,
            max_tokens: default_max_tokens(),
        }
    }
}

impl Config {
    /// 从 TOML 文件加载；文件不存在时返回默认配置（首跑零配置）。
    pub fn load(path: &Path) -> Result<Self, StoreError> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|e| StoreError::Config(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(StoreError::Io(e.to_string())),
        }
    }
}

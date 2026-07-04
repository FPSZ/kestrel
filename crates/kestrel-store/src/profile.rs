//! 模型 profile：每个模型的可调参数档案（`profiles/<key>.toml`）。
//!
//! M1 落地「用户可调参数」维度（启动器 UI 写入，ADR-0010 L6）：
//! - 启动参数（`n_ctx` / `gpu_layers` / `port` / `extra_args`）：启动器 spawn 引擎时当场生效。
//! - `max_tokens`：连该模型时经启动器写入 agent 的实时生成上限。
//!
//! 能力探针（§5.4 / M3）的产物（工具协议档位、编辑格式、建议采样）后续追加进同一 profile，
//! 字段只增不改（前向兼容）。全部字段可选：只存用户真正设过的。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::StoreError;

/// 单个模型的可调参数档案。空字段表示「未设，用默认」。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelProfile {
    /// 上下文长度（llama-server `-c`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_ctx: Option<u32>,
    /// GPU 卸载层：`auto` | `max` | 数字。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_layers: Option<String>,
    /// 监听端口（llama-server `--port`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// 追加透传给 llama-server 的启动参数（逃生舱：`--flash-attn` 等）。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extra_args: Vec<String>,
    /// 单次生成 token 上限（`None`/`0`=不限）。掐断推理模型的思考死循环。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

impl ModelProfile {
    /// 读一个模型的 profile；文件缺失 / 解析失败都回默认（宽容，不因坏档卡住 UI）。
    #[must_use]
    pub fn load(profiles_dir: &Path, model: &str) -> Self {
        let path = profile_path(profiles_dir, model);
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// 写一个模型的 profile（`profiles/` 目录按需创建）。
    pub fn save(&self, profiles_dir: &Path, model: &str) -> Result<(), StoreError> {
        std::fs::create_dir_all(profiles_dir)
            .map_err(|e| StoreError::Io(format!("create profiles dir: {e}")))?;
        let text = toml::to_string_pretty(self).map_err(|e| StoreError::Config(e.to_string()))?;
        std::fs::write(profile_path(profiles_dir, model), text)
            .map_err(|e| StoreError::Io(format!("write profile: {e}")))?;
        Ok(())
    }
}

/// 把模型名/路径清洗成安全的文件名 key（**防路径穿越**：只留字母数字与 `-_.`，其余成 `_`；
/// 折叠连续 `_`；去掉前导 `.`；限长）。空输入回 `default`。
#[must_use]
pub fn profile_key(model: &str) -> String {
    let mut key = String::with_capacity(model.len().min(96));
    let mut prev_us = false;
    for c in model.chars() {
        let ok = c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.';
        if ok && !(c == '.' && key.is_empty()) {
            key.push(c);
            prev_us = false;
        } else if !prev_us {
            key.push('_');
            prev_us = true;
        }
        if key.len() >= 96 {
            break;
        }
    }
    let key = key.trim_matches(['_', '.']).to_owned();
    if key.is_empty() {
        "default".to_owned()
    } else {
        key
    }
}

fn profile_path(profiles_dir: &Path, model: &str) -> PathBuf {
    profiles_dir.join(format!("{}.toml", profile_key(model)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_path_traversal_safe() {
        assert_eq!(profile_key("../../etc/passwd"), "etc_passwd");
        assert_eq!(profile_key("qwen3-8b-q4_k_m.gguf"), "qwen3-8b-q4_k_m.gguf");
        assert_eq!(profile_key("C:\\models\\x.gguf"), "C_models_x.gguf");
        assert_eq!(profile_key(""), "default");
        assert!(!profile_key("..").contains('.') || profile_key("..") == "default");
    }

    #[test]
    fn save_then_load_roundtrips_only_set_fields() {
        let dir = std::env::temp_dir().join(format!("kestrel-prof-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = ModelProfile {
            n_ctx: Some(65536),
            gpu_layers: Some("max".to_owned()),
            max_tokens: Some(8192),
            ..Default::default()
        };
        p.save(&dir, "qwen3").unwrap();
        let back = ModelProfile::load(&dir, "qwen3");
        assert_eq!(back, p);
        assert!(back.port.is_none() && back.extra_args.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_profile_loads_default() {
        let dir = std::env::temp_dir().join("kestrel-prof-nope");
        assert_eq!(ModelProfile::load(&dir, "ghost"), ModelProfile::default());
    }
}

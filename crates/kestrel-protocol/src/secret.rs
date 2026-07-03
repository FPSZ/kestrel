//! 敏感字符串（地基 #7 / AGENTS.md §5、§7 安全红线）。
//!
//! `api_key` 之类的密钥一旦以裸 `String` 流经带 `#[derive(Debug)]` 的配置 / 后端 /
//! 引擎结构，就可能经 tracing、事件日志、错误信息、UI、提交泄漏。用类型把它焊死：
//!
//! - `Debug` 一律脱敏（打印 `SecretString(***)`），故容器结构照常 `#[derive(Debug)]` 也不漏。
//! - **不实现 `Display`**：`format!("{secret}")` 直接编译失败，堵掉最常见的意外插值。
//! - `Serialize` 脱敏输出（空值仍为空）：即便配置结构被序列化写回 / 进日志也不带出明文。
//! - `Deserialize` 透明（配置文件是密钥的合法来源之一），照常从 TOML/JSON 的裸字符串读入。
//! - 明文只经 [`SecretString::expose`] 取得——调用点即审计点（目前仅后端 `bearer_auth`）。

use serde::{Deserialize, Serialize, Serializer};

/// 脱敏字符串。见模块文档。
#[derive(Clone, Default, Deserialize)]
#[serde(transparent)]
pub struct SecretString(String);

impl SecretString {
    /// 从任意可转 `String` 的值构造。
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// 取明文引用。**这是唯一的泄漏出口**——只在真正需要密钥的边缘调用
    /// （如 HTTP `bearer_auth`），并视调用点为审计点。
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// 是否为空（本地后端通常无 key）。不暴露内容，可安全用于分支判断。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.is_empty() {
            f.write_str("SecretString(empty)")
        } else {
            f.write_str("SecretString(***)")
        }
    }
}

impl Serialize for SecretString {
    /// 脱敏序列化：空值保持空，非空一律输出 `***`——序列化路径永不带出明文。
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(if self.0.is_empty() { "" } else { "***" })
    }
}

impl From<String> for SecretString {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SecretString {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_but_keeps_empty_signal() {
        let s = SecretString::new("sk-supersecret-123");
        assert_eq!(format!("{s:?}"), "SecretString(***)");
        assert!(!format!("{s:?}").contains("supersecret"));
        assert_eq!(
            format!("{:?}", SecretString::default()),
            "SecretString(empty)"
        );
    }

    #[test]
    fn serialize_redacts_nonempty_preserves_empty() {
        assert_eq!(
            serde_json::to_string(&SecretString::new("sk-abc")).unwrap(),
            "\"***\""
        );
        assert_eq!(
            serde_json::to_string(&SecretString::default()).unwrap(),
            "\"\""
        );
    }

    #[test]
    fn deserialize_is_transparent_and_expose_returns_plaintext() {
        let s: SecretString = serde_json::from_str("\"sk-real-key\"").unwrap();
        assert_eq!(s.expose(), "sk-real-key");
        assert!(!s.is_empty());
    }
}

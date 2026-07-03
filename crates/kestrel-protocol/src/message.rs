//! 对话消息：主循环维护的历史单位，也是发往后端的载荷。
//!
//! 序列化为 OpenAI 兼容的 role/content 结构（backend 直接透传）。
//! 前缀稳定性（原则 1）：历史 append-only，消息一旦入列不再改写。

use serde::{Deserialize, Serialize};

/// 消息角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// 系统提示（静态，前缀稳定）。
    System,
    /// 用户输入。
    User,
    /// 模型回复。
    Assistant,
    /// 工具执行结果。
    Tool,
}

/// 模型发起的一次工具调用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// 调用标识（OpenAI 的 `tool_calls[].id`，工具结果借此回填）。
    pub id: String,
    /// 工具名。
    pub name: String,
    /// 调用参数（已从流式分片拼装完整）。
    pub arguments: serde_json::Value,
}

/// 一条对话消息。
///
/// 字段布局贴合 OpenAI chat 消息：`assistant` 消息可带 `tool_calls`；
/// `tool` 消息必须带 `tool_call_id`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// 角色。
    pub role: Role,
    /// 文本内容（assistant 纯工具调用时可为空串）。
    pub content: String,
    /// 本消息发起的工具调用（仅 assistant）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// 本消息回应的工具调用标识（仅 tool 角色）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// 随 user 消息附带的图片（`data:...;base64,...` URL）。后端据此把 content
    /// 编成 OpenAI 多模态数组；文本模型忽略。派生序列化跳过（走 backend 自定义映射）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
}

impl Message {
    /// 构造纯文本消息（system / user / assistant）。
    pub fn text(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            images: Vec::new(),
        }
    }

    /// 构造 user 消息，可带粘贴图片（多模态）。`images` 为 `data:...;base64,...` URL。
    pub fn user(content: impl Into<String>, images: Vec<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            images,
        }
    }

    /// 构造带工具调用的 assistant 消息。
    #[must_use]
    pub fn assistant_calls(content: String, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content,
            tool_calls,
            tool_call_id: None,
            images: Vec::new(),
        }
    }

    /// 构造工具结果消息。
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
            images: Vec::new(),
        }
    }
}

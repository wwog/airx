//! Chat 消息类型。

use airx_core::json::Extensions;
use serde::{Deserialize, Serialize};

use super::tool::ToolCall;

/// 用户消息内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserContent {
    /// 纯文本。
    Text(String),
    /// 多模态片段。
    Parts(Vec<ContentPart>),
}

impl UserContent {
    /// 文本内容（多模态时返回 None）。
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Parts(_) => None,
        }
    }
}

/// 多模态片段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// 文本。
    Text {
        /// 文本内容。
        text: String,
    },
    /// 图片 URL。
    ImageUrl {
        /// 图片地址。
        image_url: ImageUrl,
    },
}

/// 图片地址。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageUrl {
    /// URL 或 data URI。
    pub url: String,
    /// 细节级别（`low` / `high` / `auto`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 请求消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum ChatMessage {
    /// 系统指令。
    System {
        /// 文本内容。
        content: String,
    },
    /// 用户输入。
    User {
        /// 文本或多模态内容。
        content: UserContent,
    },
    /// 助手历史回复。
    Assistant {
        /// 文本内容（工具调用时可能为 null）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        /// 历史工具调用。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<ToolCall>>,
    },
    /// 工具执行结果。
    Tool {
        /// 对应的调用 ID。
        tool_call_id: String,
        /// 结果文本。
        content: String,
    },
}

impl ChatMessage {
    /// 构造 system 消息。
    pub fn system(content: impl Into<String>) -> Self {
        Self::System {
            content: content.into(),
        }
    }

    /// 构造 user 文本消息。
    pub fn user(content: impl Into<String>) -> Self {
        Self::User {
            content: UserContent::Text(content.into()),
        }
    }

    /// 构造 user 多模态消息。
    pub fn user_parts(parts: Vec<ContentPart>) -> Self {
        Self::User {
            content: UserContent::Parts(parts),
        }
    }

    /// 构造 assistant 文本消息。
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::Assistant {
            content: Some(content.into()),
            tool_calls: None,
        }
    }

    /// 构造 assistant 工具调用消息。
    pub fn assistant_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self::Assistant {
            content: None,
            tool_calls: Some(tool_calls),
        }
    }

    /// 构造工具结果消息。
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::Tool {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
        }
    }

    /// 文本内容。
    pub fn content(&self) -> Option<&str> {
        match self {
            Self::System { content } => Some(content),
            Self::User { content } => content.as_text(),
            Self::Assistant { content, .. } => content.as_deref(),
            Self::Tool { content, .. } => Some(content),
        }
    }

    /// 角色名。
    pub fn role(&self) -> &'static str {
        match self {
            Self::System { .. } => "system",
            Self::User { .. } => "user",
            Self::Assistant { .. } => "assistant",
            Self::Tool { .. } => "tool",
        }
    }
}

/// 响应消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseMessage {
    /// 角色。
    pub role: String,
    /// 文本内容（工具调用时可能为 null）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// 拒答内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    /// 工具调用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ResponseMessage {
    /// 文本内容（若存在）。
    pub fn text(&self) -> Option<&str> {
        self.content.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_message_keeps_plain_string_shape() {
        let message = ChatMessage::user("hello");
        let value = serde_json::to_value(&message).expect("serialize");
        assert_eq!(value, json!({"role": "user", "content": "hello"}));
        assert_eq!(message.content(), Some("hello"));
    }

    #[test]
    fn multimodal_message_uses_parts() {
        let message = ChatMessage::user_parts(vec![
            ContentPart::Text {
                text: "what is this?".to_owned(),
            },
            ContentPart::ImageUrl {
                image_url: ImageUrl {
                    url: "https://example.com/a.png".to_owned(),
                    detail: None,
                    extensions: Extensions::new(),
                },
            },
        ]);
        let value = serde_json::to_value(&message).expect("serialize");
        assert_eq!(value["content"][0]["type"], "text");
        assert_eq!(
            value["content"][1]["image_url"]["url"],
            "https://example.com/a.png"
        );
        assert_eq!(message.content(), None);
    }

    #[test]
    fn assistant_tool_calls_and_tool_result_roundtrip() {
        let history = vec![
            ChatMessage::assistant_tool_calls(vec![ToolCall::function("call_1", "lookup", "{}")]),
            ChatMessage::tool_result("call_1", "42"),
        ];

        let value = serde_json::to_value(&history).expect("serialize");
        assert_eq!(value[0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(value[1]["role"], "tool");
        assert_eq!(value[1]["tool_call_id"], "call_1");
    }

    #[test]
    fn response_message_keeps_unknown_fields() {
        let json = r#"{"role":"assistant","content":null,"audio":{"id":"a1"}}"#;
        let message: ResponseMessage = serde_json::from_str(json).expect("deserialize");
        assert_eq!(message.content, None);
        assert!(message.extensions.contains_key("audio"));
    }
}

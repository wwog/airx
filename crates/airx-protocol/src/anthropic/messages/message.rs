//! Anthropic 消息与内容块。
//!
//! 未知内容块保留为 `Other(Value)`，不因新增块类型而解码失败。

use airx_core::json::Extensions;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 消息角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// 用户。
    User,
    /// 助手。
    Assistant,
}

/// 消息内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// 纯文本。
    Text(String),
    /// 内容块。
    Blocks(Vec<ContentBlock>),
}

/// 内容块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentBlock {
    /// 文本。
    Text(TextBlock),
    /// 图片。
    Image(ImageBlock),
    /// 工具调用。
    ToolUse(ToolUseBlock),
    /// 工具结果。
    ToolResult(ToolResultBlock),
    /// thinking。
    Thinking(ThinkingBlock),
    /// 加密 thinking。
    RedactedThinking(RedactedThinkingBlock),
    /// 未识别类型。
    Other(Value),
}

/// 文本块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "text")]
pub struct TextBlock {
    /// 文本。
    pub text: String,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 图片块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "image")]
pub struct ImageBlock {
    /// 图片来源（base64 / url）。
    pub source: Value,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 工具调用块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "tool_use")]
pub struct ToolUseBlock {
    /// 调用 ID。
    pub id: String,
    /// 工具名。
    pub name: String,
    /// 输入参数。
    #[serde(default)]
    pub input: Value,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 工具结果块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "tool_result")]
pub struct ToolResultBlock {
    /// 对应的调用 ID。
    pub tool_use_id: String,
    /// 结果内容（字符串或内容块数组）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
    /// 是否错误。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// thinking 块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "thinking")]
pub struct ThinkingBlock {
    /// 思考文本。
    pub thinking: String,
    /// 签名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 加密 thinking 块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "redacted_thinking")]
pub struct RedactedThinkingBlock {
    /// 加密数据。
    pub data: String,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_blocks_are_preserved() {
        let blocks: Vec<ContentBlock> = serde_json::from_value(json!([
            {"type": "text", "text": "hi"},
            {"type": "server_tool_use", "id": "srv_1"},
            {"type": "thinking", "thinking": "..."}
        ]))
        .expect("blocks");

        assert!(matches!(blocks[0], ContentBlock::Text(_)));
        assert!(matches!(blocks[1], ContentBlock::Other(_)));
        assert!(matches!(blocks[2], ContentBlock::Thinking(_)));

        let value = serde_json::to_value(&blocks).expect("serialize");
        assert_eq!(value[1]["id"], "srv_1");
    }

    #[test]
    fn text_message_serializes_as_plain_string() {
        let content = MessageContent::Text("hello".to_owned());
        assert_eq!(
            serde_json::to_value(content).expect("serialize"),
            json!("hello")
        );
    }
}

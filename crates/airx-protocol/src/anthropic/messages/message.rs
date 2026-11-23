//! Anthropic 消息与内容块。
//!
//! 未知内容块保留为 `Other(Value)`，不因新增块类型而解码失败。

use airx_core::json::Extensions;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
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
#[derive(Debug, Clone, PartialEq)]
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
    /// 服务端工具调用（联网搜索 / 抓取 / 代码执行等，由 Anthropic 后台执行）。
    ServerToolUse(ServerToolUseBlock),
    /// 服务端联网搜索结果。
    WebSearchToolResult(WebSearchToolResultBlock),
    /// 未识别类型。
    Other(Value),
}

/// 文本块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextBlock {
    /// 文本。
    pub text: String,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 图片块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageBlock {
    /// 图片来源（base64 / url）。
    pub source: Value,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 工具调用块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
pub struct RedactedThinkingBlock {
    /// 加密数据。
    pub data: String,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 服务端工具调用块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerToolUseBlock {
    /// 调用 ID。
    pub id: String,
    /// 工具名（`web_search` / `web_fetch` / `code_execution` 等）。
    pub name: String,
    /// 输入参数。
    #[serde(default)]
    pub input: Value,
    /// 调用来源（直接调用 / 其他服务端工具）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 服务端联网搜索结果块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchToolResultBlock {
    /// 对应的服务端调用 ID。
    pub tool_use_id: String,
    /// 结果数组或错误对象（结构随官方演进，原样保留）。
    #[serde(default)]
    pub content: Value,
    /// 调用来源。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<Value>,
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
            {"type": "future_block", "id": "future_1"},
            {"type": "thinking", "thinking": "..."}
        ]))
        .expect("blocks");

        assert!(matches!(blocks[0], ContentBlock::Text(_)));
        assert!(matches!(blocks[1], ContentBlock::Other(_)));
        assert!(matches!(blocks[2], ContentBlock::Thinking(_)));

        let value = serde_json::to_value(&blocks).expect("serialize");
        assert_eq!(value[1]["id"], "future_1");
        assert_eq!(value[1]["type"], "future_block");
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

#[cfg(test)]
mod server_tool_block_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn server_tool_use_and_web_search_result_are_typed() {
        let blocks: Vec<ContentBlock> = serde_json::from_value(json!([
            {"type": "server_tool_use", "id": "srv_1", "name": "web_search", "input": {"query": "rust"}},
            {"type": "web_search_tool_result", "tool_use_id": "srv_1", "content": [
                {"type": "web_search_result", "url": "https://example.com", "title": "Rust"}
            ]}
        ]))
        .expect("blocks");

        assert!(
            matches!(blocks[0], ContentBlock::ServerToolUse(_)),
            "block0 = {:?}",
            blocks[0]
        );
        assert!(matches!(blocks[1], ContentBlock::WebSearchToolResult(_)));

        let value = serde_json::to_value(&blocks).expect("serialize");
        assert_eq!(value[0]["name"], "web_search");
        assert_eq!(value[1]["content"][0]["type"], "web_search_result");
    }
}

impl Serialize for ContentBlock {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (kind, value) = match self {
            Self::Text(block) => ("text", serde_json::to_value(block)),
            Self::Image(block) => ("image", serde_json::to_value(block)),
            Self::ToolUse(block) => ("tool_use", serde_json::to_value(block)),
            Self::ToolResult(block) => ("tool_result", serde_json::to_value(block)),
            Self::Thinking(block) => ("thinking", serde_json::to_value(block)),
            Self::RedactedThinking(block) => ("redacted_thinking", serde_json::to_value(block)),
            Self::ServerToolUse(block) => ("server_tool_use", serde_json::to_value(block)),
            Self::WebSearchToolResult(block) => {
                ("web_search_tool_result", serde_json::to_value(block))
            }
            Self::Other(value) => return value.serialize(serializer),
        };
        let mut value = value.map_err(serde::ser::Error::custom)?;
        if let Value::Object(map) = &mut value {
            map.insert("type".to_owned(), Value::String(kind.to_owned()));
        }
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ContentBlock {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();

        macro_rules! parse {
            ($ty:ty, $variant:ident) => {{
                if let Value::Object(map) = &mut value {
                    map.remove("type");
                }
                return serde_json::from_value::<$ty>(value)
                    .map(Self::$variant)
                    .map_err(serde::de::Error::custom);
            }};
        }

        match kind.as_str() {
            "text" => parse!(TextBlock, Text),
            "image" => parse!(ImageBlock, Image),
            "tool_use" => parse!(ToolUseBlock, ToolUse),
            "tool_result" => parse!(ToolResultBlock, ToolResult),
            "thinking" => parse!(ThinkingBlock, Thinking),
            "redacted_thinking" => parse!(RedactedThinkingBlock, RedactedThinking),
            "server_tool_use" => parse!(ServerToolUseBlock, ServerToolUse),
            "web_search_tool_result" => parse!(WebSearchToolResultBlock, WebSearchToolResult),
            _ => Ok(Self::Other(value)),
        }
    }
}

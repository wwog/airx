//! Anthropic Messages 流式事件。

use airx_core::json::Extensions;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::anthropic::common::Usage;

use super::message::ContentBlock;
use super::response::MessagesResponse;

/// Messages 流事件。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum MessagesEvent {
    /// `message_start`
    MessageStart(MessageStart),
    /// `content_block_start`
    ContentBlockStart(ContentBlockStart),
    /// `content_block_delta`
    ContentBlockDelta(ContentBlockDelta),
    /// `content_block_stop`
    ContentBlockStop(ContentBlockStop),
    /// `message_delta`
    MessageDelta(MessageDeltaEvent),
    /// `message_stop`
    MessageStop(MessageStop),
    /// `ping`
    Ping(Ping),
    /// `error`
    Error(ErrorEvent),
    /// 未识别事件。
    Unknown(Value),
}

impl<'de> Deserialize<'de> for MessagesEvent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();

        macro_rules! parse {
            ($ty:ty, $variant:ident) => {
                return serde_json::from_value::<$ty>(value)
                    .map(Self::$variant)
                    .map_err(serde::de::Error::custom)
            };
        }

        match kind {
            "message_start" => parse!(MessageStart, MessageStart),
            "content_block_start" => parse!(ContentBlockStart, ContentBlockStart),
            "content_block_delta" => parse!(ContentBlockDelta, ContentBlockDelta),
            "content_block_stop" => parse!(ContentBlockStop, ContentBlockStop),
            "message_delta" => parse!(MessageDeltaEvent, MessageDelta),
            "message_stop" => parse!(MessageStop, MessageStop),
            "ping" => parse!(Ping, Ping),
            "error" => parse!(ErrorEvent, Error),
            _ => Ok(Self::Unknown(value)),
        }
    }
}

/// `message_start`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "message_start")]
pub struct MessageStart {
    /// 初始消息。
    pub message: MessagesResponse,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `content_block_start`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "content_block_start")]
pub struct ContentBlockStart {
    /// 块序号。
    pub index: u32,
    /// 初始内容块。
    pub content_block: ContentBlock,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `content_block_delta`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "content_block_delta")]
pub struct ContentBlockDelta {
    /// 块序号。
    pub index: u32,
    /// 增量。
    pub delta: Delta,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 内容块增量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Delta {
    /// 文本增量。
    Text(TextDelta),
    /// 工具参数增量。
    InputJson(InputJsonDelta),
    /// thinking 增量。
    Thinking(ThinkingDelta),
    /// 签名增量。
    Signature(SignatureDelta),
    /// 未识别增量。
    Other(Value),
}

/// 文本增量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "text_delta")]
pub struct TextDelta {
    /// 文本。
    pub text: String,
}

/// 工具参数增量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "input_json_delta")]
pub struct InputJsonDelta {
    /// JSON 片段。
    pub partial_json: String,
}

/// thinking 增量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "thinking_delta")]
pub struct ThinkingDelta {
    /// 思考文本。
    pub thinking: String,
}

/// 签名增量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "signature_delta")]
pub struct SignatureDelta {
    /// 签名片段。
    pub signature: String,
}

/// `content_block_stop`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "content_block_stop")]
pub struct ContentBlockStop {
    /// 块序号。
    pub index: u32,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `message_delta`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "message_delta")]
pub struct MessageDeltaEvent {
    /// 增量。
    pub delta: MessageDelta,
    /// 用量增量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// message 级增量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageDelta {
    /// 停止原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// 命中的停止序列。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
}

/// `message_stop`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "message_stop")]
pub struct MessageStop {
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `ping`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "ping")]
pub struct Ping {
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `error`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "error")]
pub struct ErrorEvent {
    /// 错误主体。
    pub error: ErrorBody,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 流错误主体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    /// 错误类型。
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    /// 错误描述。
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decodes_delta_variants_and_unknown_events() {
        let delta: MessagesEvent = serde_json::from_value(json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "input_json_delta", "partial_json": "{\"q\":"}
        }))
        .expect("delta");
        match delta {
            MessagesEvent::ContentBlockDelta(delta) => match delta.delta {
                Delta::InputJson(json) => assert_eq!(json.partial_json, "{\"q\":"),
                other => panic!("unexpected delta: {other:?}"),
            },
            other => panic!("unexpected event: {other:?}"),
        }

        let unknown: MessagesEvent = serde_json::from_value(json!({
            "type": "future_event", "payload": 1
        }))
        .expect("unknown");
        assert!(matches!(unknown, MessagesEvent::Unknown(_)));
    }
}

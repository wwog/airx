//! Anthropic Messages 流式事件。

use airx_core::json::Extensions;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::anthropic::common::Usage;

use super::message::ContentBlock;
use super::response::{Container, MessagesResponse};

/// Messages 流事件。
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
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
        let mut value = Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();

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
pub struct MessageStart {
    /// 初始消息。
    pub message: MessagesResponse,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `content_block_start`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[non_exhaustive]
pub enum Delta {
    /// 文本增量。
    Text(TextDelta),
    /// 工具参数增量。
    InputJson(InputJsonDelta),
    /// thinking 增量。
    Thinking(ThinkingDelta),
    /// 签名增量。
    Signature(SignatureDelta),
    /// 引用增量。
    Citations(CitationsDelta),
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

/// 引用增量（`citations_delta`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "citations_delta")]
pub struct CitationsDelta {
    /// 单条引用（`char_location` / `page_location` / `content_block_location`
    /// / `web_search_result_location` / `search_result_location`）。
    pub citation: Value,
}

/// `content_block_stop`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentBlockStop {
    /// 块序号。
    pub index: u32,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `message_delta`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageDeltaEvent {
    /// 增量。
    pub delta: MessageDelta,
    /// 用量增量（累计值）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// message 级增量。
///
/// 官方还带 `container` 与 `stop_details`；两者曾在这一层被丢弃，导致流式路径无法
/// 识别 refusal 细类与容器续跑信息。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MessageDelta {
    /// 停止原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// 命中的停止序列。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    /// 停止详情（refusal 分类）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_details: Option<super::response::RefusalStopDetails>,
    /// 容器信息（使用容器类工具时非空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<Container>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `message_stop`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageStop {
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `ping`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ping {
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `error`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

impl Serialize for MessagesEvent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (kind, value) = match self {
            Self::MessageStart(event) => ("message_start", serde_json::to_value(event)),
            Self::ContentBlockStart(event) => ("content_block_start", serde_json::to_value(event)),
            Self::ContentBlockDelta(event) => ("content_block_delta", serde_json::to_value(event)),
            Self::ContentBlockStop(event) => ("content_block_stop", serde_json::to_value(event)),
            Self::MessageDelta(event) => ("message_delta", serde_json::to_value(event)),
            Self::MessageStop(event) => ("message_stop", serde_json::to_value(event)),
            Self::Ping(event) => ("ping", serde_json::to_value(event)),
            Self::Error(event) => ("error", serde_json::to_value(event)),
            Self::Unknown(value) => return value.serialize(serializer),
        };
        let mut value = value.map_err(serde::ser::Error::custom)?;
        if let Value::Object(map) = &mut value {
            map.insert("type".to_owned(), Value::String(kind.to_owned()));
        }
        value.serialize(serializer)
    }
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

    #[test]
    fn citations_delta_is_typed() {
        let event: MessagesEvent = serde_json::from_value(json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {
                "type": "citations_delta",
                "citation": {"type": "char_location", "cited_text": "hi", "start_char_index": 0}
            }
        }))
        .expect("event");

        match event {
            MessagesEvent::ContentBlockDelta(event) => match event.delta {
                Delta::Citations(delta) => {
                    assert_eq!(delta.citation["cited_text"], "hi");
                }
                other => panic!("unexpected delta: {other:?}"),
            },
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn message_delta_keeps_container_and_stop_details() {
        let event: MessagesEvent = serde_json::from_value(json!({
            "type": "message_delta",
            "delta": {
                "stop_reason": "refusal",
                "stop_sequence": null,
                "stop_details": {"type": "refusal", "category": "bio"},
                "container": {"id": "container_1", "expires_at": "2026-11-24T00:00:00Z"}
            },
            "usage": {"output_tokens": 3, "input_tokens": null}
        }))
        .expect("event");

        match &event {
            MessagesEvent::MessageDelta(delta) => {
                assert_eq!(delta.delta.stop_reason.as_deref(), Some("refusal"));
                assert_eq!(
                    delta
                        .delta
                        .stop_details
                        .as_ref()
                        .and_then(|details| details.category.as_deref()),
                    Some("bio")
                );
                assert_eq!(
                    delta
                        .delta
                        .container
                        .as_ref()
                        .and_then(|container| container.id.as_deref()),
                    Some("container_1")
                );
                assert_eq!(delta.usage.as_ref().and_then(|u| u.input_tokens), None);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let value = serde_json::to_value(&event).expect("serialize");
        assert_eq!(value["delta"]["stop_details"]["category"], "bio");
        assert_eq!(value["delta"]["container"]["id"], "container_1");
    }
}

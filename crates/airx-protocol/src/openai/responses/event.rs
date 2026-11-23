//! OpenAI Responses 流式事件。
//!
//! 未知事件保留为 `Unknown(Value)`：Provider 新增事件类型时不会导致解码失败。

use airx_core::json::Extensions;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use super::response::{OutputItem, ResponsesResponse};

/// Responses 流事件。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ResponsesEvent {
    /// `response.created`
    Created(CreatedEvent),
    /// `response.output_item.added`
    OutputItemAdded(OutputItemAdded),
    /// `response.output_item.done`
    OutputItemDone(OutputItemDone),
    /// `response.output_text.delta`
    OutputTextDelta(OutputTextDelta),
    /// `response.output_text.done`
    OutputTextDone(OutputTextDone),
    /// `response.function_call_arguments.delta`
    FunctionCallArgumentsDelta(ArgumentsDelta),
    /// `response.function_call_arguments.done`
    FunctionCallArgumentsDone(ArgumentsDone),
    /// `response.completed`
    Completed(CompletedEvent),
    /// `response.failed`
    Failed(FailedEvent),
    /// `response.incomplete`
    Incomplete(IncompleteEvent),
    /// `error`
    Error(ErrorEvent),
    /// 未识别事件。
    Unknown(Value),
}

impl<'de> Deserialize<'de> for ResponsesEvent {
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
            "response.created" => parse!(CreatedEvent, Created),
            "response.output_item.added" => parse!(OutputItemAdded, OutputItemAdded),
            "response.output_item.done" => parse!(OutputItemDone, OutputItemDone),
            "response.output_text.delta" => parse!(OutputTextDelta, OutputTextDelta),
            "response.output_text.done" => parse!(OutputTextDone, OutputTextDone),
            "response.function_call_arguments.delta" => {
                parse!(ArgumentsDelta, FunctionCallArgumentsDelta)
            }
            "response.function_call_arguments.done" => {
                parse!(ArgumentsDone, FunctionCallArgumentsDone)
            }
            "response.completed" => parse!(CompletedEvent, Completed),
            "response.failed" => parse!(FailedEvent, Failed),
            "response.incomplete" => parse!(IncompleteEvent, Incomplete),
            "error" => parse!(ErrorEvent, Error),
            _ => Ok(Self::Unknown(value)),
        }
    }
}

/// `response.created`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "response.created")]
pub struct CreatedEvent {
    /// 序号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    /// 初始响应。
    pub response: ResponsesResponse,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `response.output_item.added`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "response.output_item.added")]
pub struct OutputItemAdded {
    /// 序号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    /// 输出序号。
    pub output_index: u32,
    /// 新增 item。
    pub item: OutputItem,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `response.output_item.done`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "response.output_item.done")]
pub struct OutputItemDone {
    /// 序号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    /// 输出序号。
    pub output_index: u32,
    /// 完成 item。
    pub item: OutputItem,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `response.output_text.delta`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "response.output_text.delta")]
pub struct OutputTextDelta {
    /// item ID。
    pub item_id: String,
    /// 输出序号。
    pub output_index: u32,
    /// 内容序号。
    #[serde(default)]
    pub content_index: u32,
    /// 文本增量。
    pub delta: String,
    /// 序号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `response.output_text.done`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "response.output_text.done")]
pub struct OutputTextDone {
    /// item ID。
    pub item_id: String,
    /// 输出序号。
    pub output_index: u32,
    /// 内容序号。
    #[serde(default)]
    pub content_index: u32,
    /// 完整文本。
    pub text: String,
    /// 序号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `response.function_call_arguments.delta`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "response.function_call_arguments.delta")]
pub struct ArgumentsDelta {
    /// item ID。
    pub item_id: String,
    /// 输出序号。
    pub output_index: u32,
    /// 参数增量。
    pub delta: String,
    /// 序号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `response.function_call_arguments.done`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "response.function_call_arguments.done")]
pub struct ArgumentsDone {
    /// item ID。
    pub item_id: String,
    /// 输出序号。
    pub output_index: u32,
    /// 完整参数。
    pub arguments: String,
    /// 序号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `response.completed`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "response.completed")]
pub struct CompletedEvent {
    /// 序号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    /// 完整响应。
    pub response: ResponsesResponse,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `response.failed`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "response.failed")]
pub struct FailedEvent {
    /// 序号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    /// 失败响应。
    pub response: ResponsesResponse,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `response.incomplete`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "response.incomplete")]
pub struct IncompleteEvent {
    /// 序号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    /// 不完整响应。
    pub response: ResponsesResponse,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `error`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "error")]
pub struct ErrorEvent {
    /// 错误码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// 错误描述。
    pub message: String,
    /// 相关参数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn typed_and_unknown_events_are_decoded() {
        let delta: ResponsesEvent = serde_json::from_value(json!({
            "type": "response.output_text.delta",
            "item_id": "item_1",
            "output_index": 0,
            "content_index": 0,
            "delta": "hello"
        }))
        .expect("delta");
        match delta {
            ResponsesEvent::OutputTextDelta(delta) => assert_eq!(delta.delta, "hello"),
            other => panic!("unexpected: {other:?}"),
        }

        let unknown: ResponsesEvent = serde_json::from_value(json!({
            "type": "response.future_event",
            "payload": {"x": 1}
        }))
        .expect("unknown");
        match unknown {
            ResponsesEvent::Unknown(value) => assert_eq!(value["payload"]["x"], 1),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn events_serialize_back_with_type_tag() {
        let event = ResponsesEvent::OutputTextDelta(OutputTextDelta {
            item_id: "item_1".to_owned(),
            output_index: 0,
            content_index: 0,
            delta: "hi".to_owned(),
            sequence_number: None,
            extensions: Extensions::new(),
        });
        let value = serde_json::to_value(&event).expect("serialize");
        assert_eq!(value["type"], "response.output_text.delta");
    }
}

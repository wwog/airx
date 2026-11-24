//! Responses 流式聚合器。

use std::collections::BTreeMap;

use airx_core::AirxError;
use airx_core::error::StreamError;
use airx_core::json::Extensions;
use airx_core::stream::StreamStatus;
use serde_json::Value;

use super::common::ReasoningSummary;
use super::event::ResponsesEvent;
use super::response::{
    FunctionCallItem, MessageItem, OutputContent, OutputItem, OutputText, ReasoningItem,
    RefusalContent, ResponsesResponse,
};

#[derive(Debug)]
enum ItemState {
    Message {
        id: Option<String>,
        role: String,
        text: String,
        refusal: Option<String>,
    },
    FunctionCall {
        id: Option<String>,
        call_id: String,
        name: String,
        arguments: String,
    },
    Reasoning {
        id: Option<String>,
        summary: Vec<ReasoningSummary>,
        encrypted_content: Option<String>,
        extensions: Extensions,
    },
    Other(serde_json::Value),
}

impl ItemState {
    fn from_item(item: OutputItem) -> Self {
        match item {
            OutputItem::Message(message) => Self::Message {
                id: message.id,
                role: message.role,
                text: message
                    .content
                    .iter()
                    .filter_map(|content| match content {
                        OutputContent::OutputText(text) => Some(text.text.as_str()),
                        _ => None,
                    })
                    .collect(),
                refusal: message.content.iter().find_map(|content| match content {
                    OutputContent::Refusal(refusal) => Some(refusal.refusal.clone()),
                    _ => None,
                }),
            },
            OutputItem::FunctionCall(call) => Self::FunctionCall {
                id: call.id,
                call_id: call.call_id,
                name: call.name,
                arguments: call.arguments,
            },
            OutputItem::Reasoning(item) => Self::Reasoning {
                id: item.id,
                summary: item.summary,
                encrypted_content: item.encrypted_content,
                extensions: item.extensions,
            },
            item @ (OutputItem::WebSearchCall(_)
            | OutputItem::FileSearchCall(_)
            | OutputItem::CodeInterpreterCall(_)
            | OutputItem::ImageGenerationCall(_)
            | OutputItem::McpCall(_)
            | OutputItem::CustomToolCall(_)
            | OutputItem::Compaction(_)) => {
                Self::Other(serde_json::to_value(item).unwrap_or(Value::Null))
            }
            OutputItem::Other(value) => Self::Other(value),
        }
    }

    fn into_item(self) -> OutputItem {
        match self {
            Self::Message {
                id,
                role,
                text,
                refusal,
            } => {
                let mut content = Vec::new();
                if !text.is_empty() {
                    content.push(OutputContent::OutputText(OutputText {
                        text,
                        annotations: Vec::new(),
                        extensions: Extensions::new(),
                    }));
                }
                if let Some(refusal) = refusal {
                    content.push(OutputContent::Refusal(RefusalContent {
                        refusal,
                        extensions: Extensions::new(),
                    }));
                }
                OutputItem::Message(MessageItem {
                    id,
                    role,
                    content,
                    status: None,
                    extensions: Extensions::new(),
                })
            }
            Self::FunctionCall {
                id,
                call_id,
                name,
                arguments,
            } => OutputItem::FunctionCall(FunctionCallItem {
                id,
                call_id,
                name,
                arguments,
                status: None,
                extensions: Extensions::new(),
            }),
            Self::Reasoning {
                id,
                summary,
                encrypted_content,
                extensions,
            } => OutputItem::Reasoning(ReasoningItem {
                id,
                summary,
                encrypted_content,
                status: None,
                extensions,
            }),
            Self::Other(value) => OutputItem::Other(value),
        }
    }
}

/// Responses 流式聚合器。
#[derive(Debug, Default)]
pub struct ResponsesStreamAccumulator {
    response: Option<ResponsesResponse>,
    items: BTreeMap<u32, ItemState>,
    status: StreamStatus,
    failure: Option<String>,
}

impl ResponsesStreamAccumulator {
    /// 创建空聚合器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前状态。
    pub fn status(&self) -> StreamStatus {
        self.status
    }

    /// 应用一个事件。
    pub fn apply(&mut self, event: ResponsesEvent) -> Result<(), AirxError> {
        if self.status.is_terminal() {
            return Ok(());
        }

        match event {
            ResponsesEvent::Created(event) => {
                self.response = Some(event.response);
            }
            ResponsesEvent::OutputItemAdded(event) => {
                self.items
                    .insert(event.output_index, ItemState::from_item(event.item));
            }
            ResponsesEvent::OutputItemDone(event) => {
                self.items
                    .insert(event.output_index, ItemState::from_item(event.item));
            }
            ResponsesEvent::OutputTextDelta(event) => {
                let state =
                    self.items
                        .entry(event.output_index)
                        .or_insert_with(|| ItemState::Message {
                            id: Some(event.item_id.clone()),
                            role: "assistant".to_owned(),
                            text: String::new(),
                            refusal: None,
                        });
                if let ItemState::Message { text, .. } = state {
                    text.push_str(&event.delta);
                }
            }
            ResponsesEvent::OutputTextDone(event) => {
                if let Some(ItemState::Message { text, .. }) =
                    self.items.get_mut(&event.output_index)
                {
                    *text = event.text;
                }
            }
            ResponsesEvent::RefusalDelta(event) => {
                let state =
                    self.items
                        .entry(event.output_index)
                        .or_insert_with(|| ItemState::Message {
                            id: Some(event.item_id.clone()),
                            role: "assistant".to_owned(),
                            text: String::new(),
                            refusal: None,
                        });
                if let ItemState::Message { refusal, .. } = state {
                    refusal
                        .get_or_insert_with(String::new)
                        .push_str(&event.delta);
                }
            }
            ResponsesEvent::RefusalDone(event) => {
                let state =
                    self.items
                        .entry(event.output_index)
                        .or_insert_with(|| ItemState::Message {
                            id: Some(event.item_id.clone()),
                            role: "assistant".to_owned(),
                            text: String::new(),
                            refusal: None,
                        });
                if let ItemState::Message { refusal, .. } = state {
                    *refusal = Some(event.refusal);
                }
            }
            ResponsesEvent::ReasoningSummaryTextDelta(event) => {
                let state =
                    self.items
                        .entry(event.output_index)
                        .or_insert_with(|| ItemState::Reasoning {
                            id: Some(event.item_id.clone()),
                            summary: Vec::new(),
                            encrypted_content: None,
                            extensions: Extensions::new(),
                        });
                if let ItemState::Reasoning { summary, .. } = state {
                    let index = event.summary_index as usize;
                    while summary.len() <= index {
                        summary.push(ReasoningSummary::text(""));
                    }
                    summary[index].text.push_str(&event.delta);
                }
            }
            ResponsesEvent::ReasoningSummaryTextDone(event) => {
                let state =
                    self.items
                        .entry(event.output_index)
                        .or_insert_with(|| ItemState::Reasoning {
                            id: Some(event.item_id.clone()),
                            summary: Vec::new(),
                            encrypted_content: None,
                            extensions: Extensions::new(),
                        });
                if let ItemState::Reasoning { summary, .. } = state {
                    let index = event.summary_index as usize;
                    while summary.len() <= index {
                        summary.push(ReasoningSummary::text(""));
                    }
                    summary[index].text = event.text;
                }
            }
            ResponsesEvent::ContentPartAdded(_)
            | ResponsesEvent::ContentPartDone(_)
            | ResponsesEvent::ServerToolCall(_)
            | ResponsesEvent::InProgress(_) => {}
            ResponsesEvent::FunctionCallArgumentsDelta(event) => {
                let state = self.items.entry(event.output_index).or_insert_with(|| {
                    ItemState::FunctionCall {
                        id: Some(event.item_id.clone()),
                        call_id: String::new(),
                        name: String::new(),
                        arguments: String::new(),
                    }
                });
                if let ItemState::FunctionCall { arguments, .. } = state {
                    arguments.push_str(&event.delta);
                }
            }
            ResponsesEvent::FunctionCallArgumentsDone(event) => {
                if let Some(ItemState::FunctionCall { arguments, .. }) =
                    self.items.get_mut(&event.output_index)
                {
                    *arguments = event.arguments;
                }
            }
            ResponsesEvent::Completed(event) => {
                self.response = Some(event.response);
                self.status = StreamStatus::Completed;
            }
            ResponsesEvent::Failed(event) => {
                self.failure = event
                    .response
                    .error
                    .as_ref()
                    .and_then(|error| error.get("message"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
                self.response = Some(event.response);
                self.status = StreamStatus::Failed;
            }
            ResponsesEvent::Incomplete(event) => {
                self.response = Some(event.response);
                self.status = StreamStatus::Incomplete;
            }
            ResponsesEvent::Error(event) => {
                self.failure = Some(event.message);
                self.status = StreamStatus::Failed;
            }
            ResponsesEvent::Unknown(_) => {}
        }

        Ok(())
    }

    /// 当前部分结果。
    pub fn snapshot(&self) -> ResponsesResponse {
        self.build(Some("in_progress"))
    }

    /// 结束并返回完整响应。
    pub fn finish(self) -> Result<ResponsesResponse, AirxError> {
        match self.status {
            StreamStatus::Completed => Ok(self.build(None)),
            StreamStatus::Failed => Err(AirxError::Stream(StreamError::failed(
                self.failure.unwrap_or_else(|| "响应流失败".to_owned()),
            ))),
            StreamStatus::Incomplete => {
                Err(AirxError::Stream(StreamError::incomplete("响应流被截断")))
            }
            _ => Err(AirxError::Stream(StreamError::incomplete(
                "未收到 response.completed 事件",
            ))),
        }
    }

    /// 结束并返回当前累积结果，不判断完成状态。
    ///
    /// 与 [`ResponsesStreamAccumulator::finish`] 的严格语义不同：即使流被截断
    /// 或失败，也把已累积的内容返回给调用方（用于“截断也要展示”的场景）。
    pub fn finish_partial(self) -> (ResponsesResponse, StreamStatus) {
        let status = self.status;
        let response = self.build(None);
        (response, status)
    }

    fn build(&self, status: Option<&str>) -> ResponsesResponse {
        let mut response = self.response.clone().unwrap_or_else(|| ResponsesResponse {
            id: String::new(),
            object: None,
            created_at: None,
            completed_at: None,
            status: None,
            model: None,
            output: Vec::new(),
            usage: None,
            prompt_cache_diagnostics: None,
            tool_usage: None,
            incomplete_details: None,
            error: None,
            extensions: Extensions::new(),
        });

        if response.output.is_empty() {
            response.output = self
                .items
                .values()
                .map(|state| match state {
                    ItemState::Message {
                        id,
                        role,
                        text,
                        refusal,
                    } => ItemState::Message {
                        id: id.clone(),
                        role: role.clone(),
                        text: text.clone(),
                        refusal: refusal.clone(),
                    }
                    .into_item(),
                    ItemState::FunctionCall {
                        id,
                        call_id,
                        name,
                        arguments,
                    } => ItemState::FunctionCall {
                        id: id.clone(),
                        call_id: call_id.clone(),
                        name: name.clone(),
                        arguments: arguments.clone(),
                    }
                    .into_item(),
                    ItemState::Reasoning {
                        id,
                        summary,
                        encrypted_content,
                        extensions,
                    } => ItemState::Reasoning {
                        id: id.clone(),
                        summary: summary.clone(),
                        encrypted_content: encrypted_content.clone(),
                        extensions: extensions.clone(),
                    }
                    .into_item(),
                    ItemState::Other(value) => OutputItem::Other(value.clone()),
                })
                .collect();
        }

        if response.status.is_none() {
            response.status = status.map(str::to_owned);
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airx_core::error::StreamErrorKind;
    use serde_json::json;

    fn event(value: serde_json::Value) -> ResponsesEvent {
        serde_json::from_value(value).expect("event")
    }

    #[test]
    fn accumulates_text_and_function_call() {
        let mut acc = ResponsesStreamAccumulator::new();
        acc.apply(event(json!({
            "type": "response.created",
            "response": {"id": "resp_1", "status": "in_progress", "output": []}
        })))
        .expect("created");
        acc.apply(event(json!({
            "type": "response.output_item.added",
            "output_index": 0,
            "item": {"type": "message", "role": "assistant", "content": []}
        })))
        .expect("item");
        acc.apply(event(json!({
            "type": "response.output_text.delta",
            "item_id": "msg_1", "output_index": 0, "content_index": 0, "delta": "你"
        })))
        .expect("delta 1");
        acc.apply(event(json!({
            "type": "response.output_text.delta",
            "item_id": "msg_1", "output_index": 0, "content_index": 0, "delta": "好"
        })))
        .expect("delta 2");
        acc.apply(event(json!({
            "type": "response.function_call_arguments.delta",
            "item_id": "fc_1", "output_index": 1, "delta": "{\"q\":1}"
        })))
        .expect("args");
        acc.apply(event(json!({
            "type": "response.completed",
            "response": {"id": "resp_1", "status": "completed", "output": []}
        })))
        .expect("completed");

        // 服务端最终响应为空 output 时，使用累积的 item。
        let response = acc.finish().expect("finish");
        assert_eq!(response.output_text(), "你好");
        match &response.output[1] {
            OutputItem::FunctionCall(call) => {
                assert_eq!(call.arguments, "{\"q\":1}");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn missing_completion_is_incomplete() {
        let mut acc = ResponsesStreamAccumulator::new();
        acc.apply(event(json!({
            "type": "response.output_text.delta",
            "item_id": "msg", "output_index": 0, "delta": "x"
        })))
        .expect("delta");

        match acc.finish().expect_err("incomplete") {
            AirxError::Stream(error) => assert_eq!(error.kind, StreamErrorKind::Incomplete),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn failed_event_carries_reason() {
        let mut acc = ResponsesStreamAccumulator::new();
        acc.apply(event(json!({
            "type": "response.failed",
            "response": {"id": "r", "output": [], "error": {"message": "boom"}}
        })))
        .expect("failed");

        match acc.finish().expect_err("failed") {
            AirxError::Stream(error) => {
                assert_eq!(error.kind, StreamErrorKind::Failed);
                assert!(error.message.contains("boom"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn finish_partial_keeps_partial_text_after_incomplete() {
        let mut acc = ResponsesStreamAccumulator::new();
        acc.apply(event(json!({
            "type": "response.created",
            "response": {"id": "resp_1", "status": "in_progress", "output": []}
        })))
        .expect("created");
        acc.apply(event(json!({
            "type": "response.output_text.delta",
            "item_id": "msg_1", "output_index": 0, "delta": "半截"
        })))
        .expect("delta");
        acc.apply(event(json!({
            "type": "response.incomplete",
            "response": {"id": "resp_1", "status": "incomplete", "output": []}
        })))
        .expect("incomplete");

        let (response, status) = acc.finish_partial();
        assert_eq!(status, StreamStatus::Incomplete);
        assert_eq!(response.output_text(), "半截");
    }
}

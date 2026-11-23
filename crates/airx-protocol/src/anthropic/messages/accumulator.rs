//! Anthropic Messages 流式聚合器。

use std::collections::BTreeMap;

use airx_core::AirxError;
use airx_core::error::{DeserializationError, StreamError};
use airx_core::json::Extensions;
use airx_core::stream::StreamStatus;
use serde_json::Value;

use crate::anthropic::common::Usage;

use super::event::{Delta, MessagesEvent};
use super::message::{ContentBlock, TextBlock, ThinkingBlock, ToolUseBlock};
use super::response::MessagesResponse;

#[derive(Debug, Clone)]
enum BlockState {
    Text(String),
    Thinking {
        thinking: String,
        signature: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
        partial: String,
    },
    Other(Value),
}

impl BlockState {
    fn from_block(block: ContentBlock) -> Self {
        match block {
            ContentBlock::Text(block) => Self::Text(block.text),
            ContentBlock::Thinking(block) => Self::Thinking {
                thinking: block.thinking,
                signature: block.signature.unwrap_or_default(),
            },
            ContentBlock::ToolUse(block) => Self::ToolUse {
                id: block.id,
                name: block.name,
                input: block.input,
                partial: String::new(),
            },
            other => Self::Other(serde_json::to_value(other).unwrap_or(Value::Null)),
        }
    }

    fn into_block(self, strict: bool) -> Result<ContentBlock, AirxError> {
        match self {
            Self::Text(text) => Ok(ContentBlock::Text(TextBlock {
                text,
                extensions: Extensions::new(),
            })),
            Self::Thinking {
                thinking,
                signature,
            } => Ok(ContentBlock::Thinking(ThinkingBlock {
                thinking,
                signature: (!signature.is_empty()).then_some(signature),
                extensions: Extensions::new(),
            })),
            Self::ToolUse {
                id,
                name,
                input,
                partial,
            } => {
                let input = if partial.trim().is_empty() {
                    input
                } else {
                    match serde_json::from_str(&partial) {
                        Ok(value) => value,
                        Err(error) if !strict => {
                            let _ = error;
                            Value::String(partial)
                        }
                        Err(error) => {
                            return Err(AirxError::Deserialization(DeserializationError::from(
                                error,
                            )));
                        }
                    }
                };
                Ok(ContentBlock::ToolUse(ToolUseBlock {
                    id,
                    name,
                    input,
                    extensions: Extensions::new(),
                }))
            }
            Self::Other(value) => Ok(ContentBlock::Other(value)),
        }
    }
}

/// Messages 流式聚合器。
#[derive(Debug, Default)]
pub struct MessagesStreamAccumulator {
    id: String,
    model: String,
    blocks: BTreeMap<u32, BlockState>,
    stop_reason: Option<String>,
    stop_sequence: Option<String>,
    usage: Option<Usage>,
    status: StreamStatus,
    failure: Option<String>,
    extensions: Extensions,
}

impl MessagesStreamAccumulator {
    /// 创建空聚合器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前状态。
    pub fn status(&self) -> StreamStatus {
        self.status
    }

    /// 应用一个事件。
    pub fn apply(&mut self, event: MessagesEvent) -> Result<(), AirxError> {
        if self.status.is_terminal() {
            return Ok(());
        }

        match event {
            MessagesEvent::MessageStart(event) => {
                let message = event.message;
                self.id = message.id;
                self.model = message.model;
                self.usage = message.usage;
                for (index, block) in message.content.into_iter().enumerate() {
                    self.blocks
                        .insert(index as u32, BlockState::from_block(block));
                }
                for (key, value) in message.extensions {
                    self.extensions.entry(key).or_insert(value);
                }
                for (key, value) in event.extensions {
                    self.extensions.entry(key).or_insert(value);
                }
            }
            MessagesEvent::ContentBlockStart(event) => {
                self.blocks
                    .insert(event.index, BlockState::from_block(event.content_block));
            }
            MessagesEvent::ContentBlockDelta(event) => {
                let Some(state) = self.blocks.get_mut(&event.index) else {
                    return Err(AirxError::Stream(StreamError::invalid_state(
                        "content_block_delta 指向不存在的块",
                    )));
                };
                match (state, event.delta) {
                    (BlockState::Text(text), Delta::Text(delta)) => text.push_str(&delta.text),
                    (BlockState::Thinking { thinking, .. }, Delta::Thinking(delta)) => {
                        thinking.push_str(&delta.thinking);
                    }
                    (BlockState::Thinking { signature, .. }, Delta::Signature(delta)) => {
                        signature.push_str(&delta.signature);
                    }
                    (BlockState::ToolUse { partial, .. }, Delta::InputJson(delta)) => {
                        partial.push_str(&delta.partial_json);
                    }
                    (_, Delta::Other(_)) => {}
                    _ => {
                        return Err(AirxError::Stream(StreamError::invalid_state(
                            "delta 类型与内容块不匹配",
                        )));
                    }
                }
            }
            MessagesEvent::ContentBlockStop(_) => {}
            MessagesEvent::MessageDelta(event) => {
                if event.delta.stop_reason.is_some() {
                    self.stop_reason = event.delta.stop_reason;
                }
                if event.delta.stop_sequence.is_some() {
                    self.stop_sequence = event.delta.stop_sequence;
                }
                if let Some(usage) = event.usage {
                    let target = self.usage.get_or_insert_with(Usage::default);
                    target.output_tokens = usage.output_tokens;
                    if usage.cache_creation_input_tokens.is_some() {
                        target.cache_creation_input_tokens = usage.cache_creation_input_tokens;
                    }
                    if usage.cache_read_input_tokens.is_some() {
                        target.cache_read_input_tokens = usage.cache_read_input_tokens;
                    }
                    for (key, value) in usage.extensions {
                        target.extensions.entry(key).or_insert(value);
                    }
                }
            }
            MessagesEvent::MessageStop(_) => {
                self.status = StreamStatus::Completed;
            }
            MessagesEvent::Error(event) => {
                self.failure = Some(event.error.message);
                self.status = StreamStatus::Failed;
            }
            MessagesEvent::Ping(_) | MessagesEvent::Unknown(_) => {}
        }

        Ok(())
    }

    /// 当前部分结果。
    pub fn snapshot(&self) -> MessagesResponse {
        self.build(false).unwrap_or_else(|_| self.empty())
    }

    /// 结束并返回完整响应。
    pub fn finish(self) -> Result<MessagesResponse, AirxError> {
        match self.status {
            StreamStatus::Completed => self.build(true),
            StreamStatus::Failed => Err(AirxError::Stream(StreamError::failed(
                self.failure.unwrap_or_else(|| "消息流失败".to_owned()),
            ))),
            StreamStatus::Incomplete => {
                Err(AirxError::Stream(StreamError::incomplete("消息流被截断")))
            }
            _ => Err(AirxError::Stream(StreamError::incomplete(
                "未收到 message_stop 事件",
            ))),
        }
    }

    fn build(&self, strict: bool) -> Result<MessagesResponse, AirxError> {
        let mut content = Vec::with_capacity(self.blocks.len());
        for state in self.blocks.values() {
            content.push(state.clone().into_block(strict)?);
        }

        Ok(MessagesResponse {
            id: self.id.clone(),
            kind: Some("message".to_owned()),
            role: Some("assistant".to_owned()),
            model: self.model.clone(),
            content,
            stop_reason: self.stop_reason.clone(),
            stop_sequence: self.stop_sequence.clone(),
            usage: self.usage.clone(),
            extensions: self.extensions.clone(),
        })
    }

    fn empty(&self) -> MessagesResponse {
        MessagesResponse {
            id: self.id.clone(),
            kind: Some("message".to_owned()),
            role: Some("assistant".to_owned()),
            model: self.model.clone(),
            content: Vec::new(),
            stop_reason: None,
            stop_sequence: None,
            usage: self.usage.clone(),
            extensions: self.extensions.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airx_core::error::StreamErrorKind;
    use serde_json::json;

    fn event(value: Value) -> MessagesEvent {
        serde_json::from_value(value).expect("event")
    }

    #[test]
    fn accumulates_text_and_tool_use() {
        let mut acc = MessagesStreamAccumulator::new();
        acc.apply(event(json!({
            "type": "message_start",
            "message": {"id": "msg_1", "model": "claude", "content": [], "usage": {"input_tokens": 10, "output_tokens": 0}}
        })))
        .expect("start");
        acc.apply(event(json!({
            "type": "content_block_start", "index": 0,
            "content_block": {"type": "text", "text": ""}
        })))
        .expect("block");
        acc.apply(event(json!({
            "type": "content_block_delta", "index": 0,
            "delta": {"type": "text_delta", "text": "你好"}
        })))
        .expect("text");
        acc.apply(event(json!({
            "type": "content_block_start", "index": 1,
            "content_block": {"type": "tool_use", "id": "toolu_1", "name": "lookup", "input": {}}
        })))
        .expect("tool");
        acc.apply(event(json!({
            "type": "content_block_delta", "index": 1,
            "delta": {"type": "input_json_delta", "partial_json": "{\"q\":\"sg\"}"}
        })))
        .expect("json");
        acc.apply(event(json!({
            "type": "message_delta",
            "delta": {"stop_reason": "tool_use"},
            "usage": {"output_tokens": 7}
        })))
        .expect("message delta");
        acc.apply(event(json!({"type": "message_stop"})))
            .expect("stop");

        let response = acc.finish().expect("response");
        assert_eq!(response.text(), "你好");
        assert_eq!(response.stop_reason.as_deref(), Some("tool_use"));
        assert_eq!(response.usage.as_ref().map(|u| u.output_tokens), Some(7));
        let calls = response.tool_uses();
        assert_eq!(calls[0].input["q"], "sg");
    }

    #[test]
    fn missing_stop_is_incomplete() {
        let mut acc = MessagesStreamAccumulator::new();
        acc.apply(event(json!({
            "type": "message_start",
            "message": {"id": "msg_1", "model": "claude", "content": []}
        })))
        .expect("start");

        match acc.finish().expect_err("incomplete") {
            AirxError::Stream(error) => assert_eq!(error.kind, StreamErrorKind::Incomplete),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn error_event_fails_the_stream() {
        let mut acc = MessagesStreamAccumulator::new();
        acc.apply(event(json!({
            "type": "error",
            "error": {"type": "overloaded_error", "message": "overloaded"}
        })))
        .expect("error");
        match acc.finish().expect_err("failed") {
            AirxError::Stream(error) => assert_eq!(error.kind, StreamErrorKind::Failed),
            other => panic!("unexpected: {other:?}"),
        }
    }
}

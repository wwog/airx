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
use super::response::{Container, Diagnostics, MessagesResponse, RefusalStopDetails};

#[derive(Debug, Clone)]
enum BlockState {
    Text {
        text: String,
        extensions: Extensions,
    },
    Thinking {
        thinking: String,
        signature: String,
        extensions: Extensions,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
        partial: String,
        caller: Option<Value>,
        toolset_name: Option<String>,
        extensions: Extensions,
    },
    Other(Value),
}

impl BlockState {
    /// 保留块起始事件的完整形状：未知字段进入 `extensions`，
    /// `citations` 等追加型字段在后续 delta 中继续累加。
    fn from_block(block: ContentBlock) -> Self {
        match block {
            ContentBlock::Text(block) => Self::Text {
                text: block.text,
                extensions: block.extensions,
            },
            ContentBlock::Thinking(block) => Self::Thinking {
                thinking: block.thinking,
                signature: block.signature.unwrap_or_default(),
                extensions: block.extensions,
            },
            ContentBlock::ToolUse(block) => Self::ToolUse {
                id: block.id,
                name: block.name,
                input: block.input,
                partial: String::new(),
                caller: block.caller,
                toolset_name: block.toolset_name,
                extensions: block.extensions,
            },
            other => Self::Other(serde_json::to_value(other).unwrap_or(Value::Null)),
        }
    }

    fn into_block(self, strict: bool) -> Result<ContentBlock, AirxError> {
        match self {
            Self::Text { text, extensions } => {
                Ok(ContentBlock::Text(TextBlock { text, extensions }))
            }
            Self::Thinking {
                thinking,
                signature,
                extensions,
            } => Ok(ContentBlock::Thinking(ThinkingBlock {
                thinking,
                signature: (!signature.is_empty()).then_some(signature),
                extensions,
            })),
            Self::ToolUse {
                id,
                name,
                input,
                partial,
                caller,
                toolset_name,
                extensions,
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
                    caller,
                    toolset_name,
                    extensions,
                }))
            }
            Self::Other(value) => Ok(ContentBlock::Other(value)),
        }
    }
}

/// 往 `extensions[key]` 追加一个数组元素（`citations` 等流式累加字段）。
fn push_extension_array(extensions: &mut Extensions, key: &str, value: Value) {
    let entry = extensions
        .entry(key.to_owned())
        .or_insert_with(|| Value::Array(Vec::new()));
    if !entry.is_array() {
        *entry = Value::Array(Vec::new());
    }
    if let Value::Array(items) = entry {
        items.push(value);
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
    stop_details: Option<RefusalStopDetails>,
    container: Option<Container>,
    diagnostics: Option<Diagnostics>,
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
                self.stop_details = message.stop_details;
                self.container = message.container;
                self.diagnostics = message.diagnostics;
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
                    (BlockState::Text { text, .. }, Delta::Text(delta)) => {
                        text.push_str(&delta.text);
                    }
                    (BlockState::Text { extensions, .. }, Delta::Citations(delta)) => {
                        push_extension_array(extensions, "citations", delta.citation);
                    }
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
                if event.delta.stop_details.is_some() {
                    self.stop_details = event.delta.stop_details;
                }
                if event.delta.container.is_some() {
                    self.container = event.delta.container;
                }
                for (key, value) in event.delta.extensions {
                    self.extensions.entry(key).or_insert(value);
                }
                if let Some(usage) = event.usage {
                    let target = self.usage.get_or_insert_with(Usage::default);
                    // `message_delta.usage` 是累计值：有值就覆盖，缺省 / null 保持旧值。
                    target.output_tokens = usage.output_tokens;
                    if usage.input_tokens.is_some() {
                        target.input_tokens = usage.input_tokens;
                    }
                    if usage.cache_creation_input_tokens.is_some() {
                        target.cache_creation_input_tokens = usage.cache_creation_input_tokens;
                    }
                    if usage.cache_read_input_tokens.is_some() {
                        target.cache_read_input_tokens = usage.cache_read_input_tokens;
                    }
                    if usage.cache_creation.is_some() {
                        target.cache_creation = usage.cache_creation;
                    }
                    if usage.output_tokens_details.is_some() {
                        target.output_tokens_details = usage.output_tokens_details;
                    }
                    if usage.server_tool_use.is_some() {
                        target.server_tool_use = usage.server_tool_use;
                    }
                    if usage.service_tier.is_some() {
                        target.service_tier = usage.service_tier;
                    }
                    if usage.speed.is_some() {
                        target.speed = usage.speed;
                    }
                    if usage.inference_geo.is_some() {
                        target.inference_geo = usage.inference_geo;
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

    /// 结束并返回已累积的部分结果（不因流内错误 / 截断而丢弃内容）。
    ///
    /// 官方明确流内错误不遵循 HTTP 语义；业务常常需要「部分结果 + 错误」。
    /// 错误本身由调用方从事件流里取得，这里只负责保留内容。
    pub fn finish_partial(self) -> MessagesResponse {
        self.build(false).unwrap_or_else(|_| self.empty())
    }

    /// 失败原因（若流内 `error` 事件已到达）。
    pub fn failure_message(&self) -> Option<&str> {
        self.failure.as_deref()
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
            stop_details: self.stop_details.clone(),
            container: self.container.clone(),
            diagnostics: self.diagnostics.clone(),
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
            stop_details: None,
            container: None,
            diagnostics: None,
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
        assert!(response.is_tool_use());
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

    #[test]
    fn finish_partial_keeps_content_after_error() {
        let mut acc = MessagesStreamAccumulator::new();
        acc.apply(event(json!({
            "type": "message_start",
            "message": {"id": "msg_1", "model": "claude", "content": []}
        })))
        .expect("start");
        acc.apply(event(json!({
            "type": "content_block_start", "index": 0,
            "content_block": {"type": "text", "text": ""}
        })))
        .expect("block");
        acc.apply(event(json!({
            "type": "content_block_delta", "index": 0,
            "delta": {"type": "text_delta", "text": "partial"}
        })))
        .expect("text");
        acc.apply(event(json!({
            "type": "error",
            "error": {"type": "overloaded_error", "message": "overloaded"}
        })))
        .expect("error");

        assert_eq!(acc.failure_message(), Some("overloaded"));
        let response = acc.finish_partial();
        assert_eq!(response.text(), "partial");
    }

    #[test]
    fn message_delta_keeps_stop_details_container_and_usage_details() {
        let mut acc = MessagesStreamAccumulator::new();
        acc.apply(event(json!({
            "type": "message_start",
            "message": {"id": "msg_1", "model": "claude", "content": [], "usage": {"input_tokens": 10, "output_tokens": 0}}
        })))
        .expect("start");
        acc.apply(event(json!({
            "type": "message_delta",
            "delta": {
                "stop_reason": "refusal",
                "stop_details": {"type": "refusal", "category": "cyber", "explanation": "policy"},
                "container": {"id": "container_1", "expires_at": "2026-11-24T00:00:00Z"}
            },
            "usage": {
                "input_tokens": 11,
                "output_tokens": 5,
                "cache_creation": {"ephemeral_1h_input_tokens": 2, "ephemeral_5m_input_tokens": 3},
                "output_tokens_details": {"thinking_tokens": 4},
                "server_tool_use": {"web_search_requests": 1},
                "service_tier": "priority",
                "speed": "fast"
            }
        })))
        .expect("delta");
        acc.apply(event(json!({"type": "message_stop"})))
            .expect("stop");

        let response = acc.finish().expect("response");
        assert!(response.is_refusal());
        assert_eq!(
            response.stop_details().and_then(|d| d.category.as_deref()),
            Some("cyber")
        );
        assert_eq!(response.container_id(), Some("container_1"));
        assert_eq!(response.thinking_tokens(), 4);

        let usage = response.usage.expect("usage");
        assert_eq!(usage.input_tokens, Some(11));
        assert_eq!(
            usage
                .cache_creation
                .expect("cache")
                .ephemeral_1h_input_tokens,
            2
        );
        assert_eq!(
            usage.server_tool_use.expect("server").web_search_requests,
            1
        );
        assert_eq!(
            usage.service_tier,
            Some(crate::anthropic::common::ServiceTier::Priority)
        );
        assert_eq!(usage.speed, Some(crate::anthropic::common::Speed::Fast));
    }

    #[test]
    fn block_extensions_and_citations_survive_aggregation() {
        let mut acc = MessagesStreamAccumulator::new();
        acc.apply(event(json!({
            "type": "message_start",
            "message": {"id": "msg_1", "model": "claude", "content": []}
        })))
        .expect("start");
        acc.apply(event(json!({
            "type": "content_block_start", "index": 0,
            "content_block": {
                "type": "text", "text": "", "citations": null,
                "cache_control": {"type": "ephemeral"},
                "future_field": 1
            }
        })))
        .expect("block");
        acc.apply(event(json!({
            "type": "content_block_delta", "index": 0,
            "delta": {"type": "citations_delta", "citation": {"type": "char_location", "cited_text": "a"}}
        })))
        .expect("citation 1");
        acc.apply(event(json!({
            "type": "content_block_delta", "index": 0,
            "delta": {"type": "citations_delta", "citation": {"type": "char_location", "cited_text": "b"}}
        })))
        .expect("citation 2");
        acc.apply(event(json!({
            "type": "content_block_delta", "index": 0,
            "delta": {"type": "text_delta", "text": "answer"}
        })))
        .expect("text");
        acc.apply(event(json!({
            "type": "content_block_start", "index": 1,
            "content_block": {"type": "tool_use", "id": "toolu_1", "name": "lookup", "input": {}, "caller": {"type": "direct"}, "toolset_name": "crm"}
        })))
        .expect("tool");
        acc.apply(event(json!({
            "type": "content_block_delta", "index": 1,
            "delta": {"type": "input_json_delta", "partial_json": "{\"q\":1}"}
        })))
        .expect("json");
        acc.apply(event(json!({"type": "message_stop"})))
            .expect("stop");

        let response = acc.finish().expect("response");
        match &response.content[0] {
            ContentBlock::Text(block) => {
                assert_eq!(block.text, "answer");
                assert_eq!(block.citations().len(), 2);
                assert_eq!(block.extensions["future_field"], 1);
                assert_eq!(block.extensions["cache_control"]["type"], "ephemeral");
            }
            other => panic!("unexpected: {other:?}"),
        }
        match &response.content[1] {
            ContentBlock::ToolUse(block) => {
                assert_eq!(block.caller, Some(json!({"type": "direct"})));
                assert_eq!(block.toolset_name.as_deref(), Some("crm"));
                assert_eq!(block.input["q"], 1);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn null_input_tokens_in_delta_does_not_break_the_stream() {
        let mut acc = MessagesStreamAccumulator::new();
        acc.apply(event(json!({
            "type": "message_start",
            "message": {"id": "msg_1", "model": "claude", "content": [], "usage": {"input_tokens": 10, "output_tokens": 0}}
        })))
        .expect("start");
        acc.apply(event(json!({
            "type": "message_delta",
            "delta": {"stop_reason": "end_turn"},
            "usage": {"input_tokens": null, "output_tokens": 2}
        })))
        .expect("delta");
        acc.apply(event(json!({"type": "message_stop"})))
            .expect("stop");

        let response = acc.finish().expect("response");
        let usage = response.usage.expect("usage");
        assert_eq!(usage.input_tokens, Some(10));
        assert_eq!(usage.output_tokens, 2);
    }
}

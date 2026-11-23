//! Chat 流式聚合器。
//!
//! 把增量 chunk 累积为完整响应；`finish()` 只有在收到 `[DONE]` 后才返回成功，
//! 网络中断或未收到结束标记一律报 `Incomplete`。

use std::collections::BTreeMap;

use airx_core::AirxError;
use airx_core::error::StreamError;
use airx_core::json::Extensions;
use airx_core::stream::StreamStatus;

use crate::openai::common::usage::Usage;

use super::chunk::{ChatCompletionChunk, ChunkChoice};
use super::event::ChatStreamEvent;
use super::message::ResponseMessage;
use super::response::{ChatChoice, ChatCompletionResponse};
use super::tool::{FunctionCall, ToolCall};

#[derive(Debug, Default)]
struct ToolCallState {
    id: Option<String>,
    kind: Option<String>,
    name: Option<String>,
    arguments: String,
    extensions: Extensions,
}

#[derive(Debug, Default)]
struct ChoiceState {
    role: Option<String>,
    content: String,
    has_content: bool,
    reasoning: String,
    annotations: Option<Vec<serde_json::Value>>,
    server_tool_calls: Option<Vec<serde_json::Value>>,
    refusal: Option<String>,
    tool_calls: BTreeMap<u32, ToolCallState>,
    finish_reason: Option<String>,
    logprobs: Option<serde_json::Value>,
    message_extensions: Extensions,
    extensions: Extensions,
}

/// Chat 流式聚合器。
#[derive(Debug, Default)]
pub struct ChatStreamAccumulator {
    id: String,
    object: Option<String>,
    created: Option<u64>,
    model: String,
    choices: BTreeMap<u32, ChoiceState>,
    usage: Option<Usage>,
    extensions: Extensions,
    status: StreamStatus,
}

impl ChatStreamAccumulator {
    /// 创建空聚合器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前状态。
    pub fn status(&self) -> StreamStatus {
        self.status
    }

    /// 应用一个流事件。
    pub fn apply(&mut self, event: ChatStreamEvent) -> Result<(), AirxError> {
        if self.status.is_terminal() {
            if matches!(event, ChatStreamEvent::Done) {
                return Ok(());
            }
            return Err(AirxError::Stream(StreamError::invalid_state(
                "流已结束，不能继续应用 chunk",
            )));
        }

        match event {
            ChatStreamEvent::Chunk(chunk) => self.apply_chunk(chunk),
            ChatStreamEvent::Done => {
                self.status = StreamStatus::Completed;
                Ok(())
            }
        }
    }

    /// 当前累积结果（用于中途观察，不判断完成状态）。
    pub fn snapshot(&self) -> ChatCompletionResponse {
        self.build()
    }

    /// 结束并返回完整响应。
    ///
    /// 未收到 `[DONE]` 时返回 `StreamError::Incomplete`，不把断流当成功。
    pub fn finish(self) -> Result<ChatCompletionResponse, AirxError> {
        if !self.status.is_success() {
            return Err(AirxError::Stream(StreamError::incomplete(
                "Chat 流在收到 [DONE] 之前终止",
            )));
        }
        Ok(self.build())
    }

    fn apply_chunk(&mut self, chunk: ChatCompletionChunk) -> Result<(), AirxError> {
        if self.id.is_empty() {
            self.id = chunk.id;
        }
        if self.model.is_empty() {
            self.model = chunk.model;
        }
        if self.object.is_none() {
            self.object = chunk.object;
        }
        if self.created.is_none() {
            self.created = chunk.created;
        }
        if let Some(usage) = chunk.usage {
            self.usage = Some(usage);
        }
        for (key, value) in chunk.extensions {
            self.extensions.entry(key).or_insert(value);
        }

        for choice in chunk.choices {
            let state = self.choices.entry(choice.index).or_default();
            apply_choice(state, choice);
        }
        Ok(())
    }

    fn build(&self) -> ChatCompletionResponse {
        let choices = self
            .choices
            .iter()
            .map(|(index, state)| {
                let mut message_extensions = state.message_extensions.clone();
                if !state.reasoning.is_empty() {
                    message_extensions.insert(
                        "reasoning_content".to_owned(),
                        serde_json::Value::String(state.reasoning.clone()),
                    );
                }
                ChatChoice {
                    index: *index,
                    message: ResponseMessage {
                        role: state.role.clone().unwrap_or_else(|| "assistant".to_owned()),
                        content: state.has_content.then(|| state.content.clone()),
                        refusal: state.refusal.clone(),
                        tool_calls: build_tool_calls(&state.tool_calls),
                        annotations: state.annotations.clone(),
                        server_tool_calls: state.server_tool_calls.clone(),
                        extensions: message_extensions,
                    },
                    finish_reason: state.finish_reason.clone(),
                    logprobs: state.logprobs.clone(),
                    extensions: state.extensions.clone(),
                }
            })
            .collect();

        ChatCompletionResponse {
            id: self.id.clone(),
            object: self.object.clone(),
            created: self.created,
            model: self.model.clone(),
            choices,
            usage: self.usage.clone(),
            system_fingerprint: None,
            extensions: self.extensions.clone(),
        }
    }
}

fn apply_choice(state: &mut ChoiceState, choice: ChunkChoice) {
    if let Some(role) = choice.delta.role {
        state.role = Some(role);
    }
    if let Some(content) = choice.delta.content {
        state.content.push_str(&content);
        state.has_content = true;
    }
    if let Some(reasoning) = choice.delta.reasoning_content {
        state.reasoning.push_str(&reasoning);
    }
    if choice.delta.annotations.is_some() {
        state.annotations = choice.delta.annotations;
    }
    if choice.delta.server_tool_calls.is_some() {
        state.server_tool_calls = choice.delta.server_tool_calls;
    }
    if let Some(tool_calls) = choice.delta.tool_calls {
        for call in tool_calls {
            let target = state.tool_calls.entry(call.index).or_default();
            if let Some(id) = call.id {
                target.id = Some(id);
            }
            if let Some(kind) = call.kind {
                target.kind = Some(kind);
            }
            if let Some(function) = call.function {
                if let Some(name) = function.name {
                    target.name = Some(name);
                }
                if let Some(arguments) = function.arguments {
                    target.arguments.push_str(&arguments);
                }
                for (key, value) in function.extensions {
                    target.extensions.entry(key).or_insert(value);
                }
            }
            for (key, value) in call.extensions {
                target.extensions.entry(key).or_insert(value);
            }
        }
    }
    if choice.finish_reason.is_some() {
        state.finish_reason = choice.finish_reason;
    }
    if choice.logprobs.is_some() {
        state.logprobs = choice.logprobs;
    }
    for (key, value) in choice.delta.extensions {
        state.message_extensions.entry(key).or_insert(value);
    }
    for (key, value) in choice.extensions {
        state.extensions.entry(key).or_insert(value);
    }
}

fn build_tool_calls(calls: &BTreeMap<u32, ToolCallState>) -> Option<Vec<ToolCall>> {
    if calls.is_empty() {
        return None;
    }

    Some(
        calls
            .values()
            .map(|state| ToolCall {
                id: state.id.clone().unwrap_or_default(),
                kind: state.kind.clone().unwrap_or_else(|| "function".to_owned()),
                function: FunctionCall {
                    name: state.name.clone().unwrap_or_default(),
                    arguments: state.arguments.clone(),
                    extensions: Extensions::new(),
                },
                extensions: state.extensions.clone(),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use airx_core::error::StreamErrorKind;
    use serde_json::{Value, json};

    fn chunk(value: Value) -> ChatCompletionChunk {
        serde_json::from_value(value).expect("chunk")
    }

    #[test]
    fn accumulates_text_usage_and_extensions() {
        let mut acc = ChatStreamAccumulator::new();
        acc.apply(ChatStreamEvent::Chunk(chunk(json!({
            "id": "chatcmpl-1",
            "model": "gpt-4o-mini",
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": "你"}}]
        }))))
        .expect("chunk 1");
        acc.apply(ChatStreamEvent::Chunk(chunk(json!({
            "id": "chatcmpl-1",
            "model": "gpt-4o-mini",
            "choices": [{"index": 0, "delta": {"content": "好"}, "finish_reason": "stop"}],
            "vendor": {"trace": "x"}
        }))))
        .expect("chunk 2");
        acc.apply(ChatStreamEvent::Chunk(chunk(json!({
            "id": "chatcmpl-1",
            "model": "gpt-4o-mini",
            "choices": [],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3}
        }))))
        .expect("usage");
        acc.apply(ChatStreamEvent::Done).expect("done");

        let response = acc.finish().expect("response");
        assert_eq!(response.first_content(), Some("你好"));
        assert_eq!(response.choices[0].finish_reason.as_deref(), Some("stop"));
        assert_eq!(response.usage.as_ref().map(|u| u.total_tokens), Some(3));
        assert!(response.extensions.contains_key("vendor"));
    }

    #[test]
    fn merges_tool_call_arguments_across_chunks() {
        let mut acc = ChatStreamAccumulator::new();
        for delta in [
            json!({"tool_calls": [{"index": 0, "id": "call_1", "type": "function", "function": {"name": "lookup", "arguments": "{\"q\":"}}]}),
            json!({"tool_calls": [{"index": 0, "function": {"arguments": "\"singapore\""}}]}),
            json!({"tool_calls": [{"index": 0, "function": {"arguments": "}"}}]}),
        ] {
            acc.apply(ChatStreamEvent::Chunk(chunk(json!({
                "id": "chatcmpl-1",
                "model": "gpt-4o-mini",
                "choices": [{"index": 0, "delta": delta}]
            }))))
            .expect("chunk");
        }
        acc.apply(ChatStreamEvent::Done).expect("done");

        let response = acc.finish().expect("response");
        let calls = response.choices[0]
            .message
            .tool_calls
            .as_ref()
            .expect("tool calls");
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].function.name, "lookup");
        assert_eq!(calls[0].function.arguments, r#"{"q":"singapore"}"#);
        assert_eq!(response.choices[0].message.content, None);
    }

    #[test]
    fn missing_done_is_incomplete() {
        let mut acc = ChatStreamAccumulator::new();
        acc.apply(ChatStreamEvent::Chunk(chunk(json!({
            "id": "1",
            "model": "m",
            "choices": [{"index": 0, "delta": {"content": "x"}}]
        }))))
        .expect("chunk");

        let error = acc.finish().expect_err("incomplete");
        match error {
            AirxError::Stream(error) => assert_eq!(error.kind, StreamErrorKind::Incomplete),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn chunk_after_done_is_rejected() {
        let mut acc = ChatStreamAccumulator::new();
        acc.apply(ChatStreamEvent::Done).expect("done");
        let error = acc
            .apply(ChatStreamEvent::Chunk(chunk(json!({
                "id": "1", "model": "m", "choices": []
            }))))
            .expect_err("invalid state");
        assert!(matches!(error, AirxError::Stream(_)));
    }

    #[test]
    fn accumulates_reasoning_content_into_message_extensions() {
        let mut acc = ChatStreamAccumulator::new();
        for delta in [
            json!({"reasoning_content": "思"}),
            json!({"reasoning_content": "考"}),
        ] {
            acc.apply(ChatStreamEvent::Chunk(chunk(json!({
                "id": "1",
                "model": "m",
                "choices": [{"index": 0, "delta": delta}]
            }))))
            .expect("chunk");
        }
        acc.apply(ChatStreamEvent::Done).expect("done");

        let response = acc.finish().expect("response");
        assert_eq!(
            response.choices[0]
                .message
                .extensions
                .get("reasoning_content")
                .and_then(serde_json::Value::as_str),
            Some("思考")
        );
    }
}

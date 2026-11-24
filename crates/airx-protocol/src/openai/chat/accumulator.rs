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
use super::tool::{CustomToolCall, FunctionCall, ToolCall};

#[derive(Debug, Default)]
struct ToolCallState {
    id: Option<String>,
    kind: Option<String>,
    name: Option<String>,
    arguments: String,
    custom_name: Option<String>,
    custom_input: String,
    custom_extensions: Extensions,
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
    function_call: Option<super::chunk::ChunkFunctionCall>,
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
    system_fingerprint: Option<String>,
    service_tier: Option<String>,
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

    /// 结束并返回当前累积结果，不判断完成状态。
    ///
    /// 用于“截断也要展示半截内容”的场景；需要严格语义时仍用 [`Self::finish`]。
    pub fn finish_partial(self) -> (ChatCompletionResponse, StreamStatus) {
        let status = self.status;
        (self.build(), status)
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
        if chunk.system_fingerprint.is_some() {
            self.system_fingerprint = chunk.system_fingerprint;
        }
        if chunk.service_tier.is_some() {
            self.service_tier = chunk.service_tier;
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
                if let Some(function_call) = &state.function_call {
                    if let Ok(value) = serde_json::to_value(function_call) {
                        message_extensions.insert("function_call".to_owned(), value);
                    }
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

        let mut extensions = self.extensions.clone();
        // 非流式响应的 service_tier 现行落在 extensions；流式保持同形。
        if let Some(service_tier) = &self.service_tier {
            extensions
                .entry("service_tier".to_owned())
                .or_insert_with(|| serde_json::Value::String(service_tier.clone()));
        }

        ChatCompletionResponse {
            id: self.id.clone(),
            object: self.object.clone(),
            created: self.created,
            model: self.model.clone(),
            choices,
            usage: self.usage.clone(),
            system_fingerprint: self.system_fingerprint.clone(),
            extensions,
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
    if let Some(refusal) = choice.delta.refusal {
        state
            .refusal
            .get_or_insert_with(String::new)
            .push_str(&refusal);
    }
    if let Some(function_call) = choice.delta.function_call {
        let target = state.function_call.get_or_insert_with(Default::default);
        if let Some(name) = function_call.name {
            target.name = Some(name);
        }
        if let Some(arguments) = function_call.arguments {
            target
                .arguments
                .get_or_insert_with(String::new)
                .push_str(&arguments);
        }
        for (key, value) in function_call.extensions {
            target.extensions.entry(key).or_insert(value);
        }
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
            if let Some(custom) = call.custom {
                if let Some(name) = custom.name {
                    target.custom_name = Some(name);
                }
                if let Some(input) = custom.input {
                    target.custom_input.push_str(&input);
                }
                for (key, value) in custom.extensions {
                    target.custom_extensions.entry(key).or_insert(value);
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
            .map(|state| {
                let has_custom = state.custom_name.is_some()
                    || !state.custom_input.is_empty()
                    || state.kind.as_deref() == Some("custom");
                let has_function = state.name.is_some()
                    || !state.arguments.is_empty()
                    || (!has_custom && state.kind.as_deref() != Some("custom"));
                ToolCall {
                    id: state.id.clone().unwrap_or_default(),
                    kind: state.kind.clone().unwrap_or_else(|| {
                        if has_custom && !has_function {
                            "custom".to_owned()
                        } else {
                            "function".to_owned()
                        }
                    }),
                    function: has_function.then(|| FunctionCall {
                        name: state.name.clone().unwrap_or_default(),
                        arguments: state.arguments.clone(),
                        extensions: Extensions::new(),
                    }),
                    custom: has_custom.then(|| CustomToolCall {
                        name: state.custom_name.clone().unwrap_or_default(),
                        input: state.custom_input.clone(),
                        extensions: state.custom_extensions.clone(),
                    }),
                    extensions: state.extensions.clone(),
                }
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
        let function = calls[0].function.as_ref().expect("function");
        assert_eq!(function.name, "lookup");
        assert_eq!(function.arguments, r#"{"q":"singapore"}"#);
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

    #[test]
    fn streaming_refusal_fingerprint_and_tier_match_non_streaming() {
        let mut acc = ChatStreamAccumulator::new();
        acc.apply(ChatStreamEvent::Chunk(chunk(json!({
            "id": "chatcmpl-1",
            "model": "gpt-4o-mini",
            "system_fingerprint": "fp_1",
            "service_tier": "default",
            "choices": [{"index": 0, "delta": {"role": "assistant", "refusal": "no"}}]
        }))))
        .expect("chunk 1");
        acc.apply(ChatStreamEvent::Chunk(chunk(json!({
            "id": "chatcmpl-1",
            "model": "gpt-4o-mini",
            "choices": [{"index": 0, "delta": {"refusal": "pe"}}]
        }))))
        .expect("chunk 2");
        acc.apply(ChatStreamEvent::Done).expect("done");

        let response = acc.finish().expect("response");
        assert_eq!(response.system_fingerprint.as_deref(), Some("fp_1"));
        assert_eq!(response.choices[0].message.refusal.as_deref(), Some("nope"));
        assert_eq!(
            response
                .extensions
                .get("service_tier")
                .and_then(serde_json::Value::as_str),
            Some("default")
        );
    }

    #[test]
    fn legacy_function_call_delta_is_preserved_in_message_extensions() {
        let mut acc = ChatStreamAccumulator::new();
        acc.apply(ChatStreamEvent::Chunk(chunk(json!({
            "id": "1",
            "model": "m",
            "choices": [{"index": 0, "delta": {"function_call": {"name": "lookup", "arguments": "{\"q\":"}}}]
        }))))
        .expect("chunk 1");
        acc.apply(ChatStreamEvent::Chunk(chunk(json!({
            "id": "1",
            "model": "m",
            "choices": [{"index": 0, "delta": {"function_call": {"arguments": "1}"}}}]
        }))))
        .expect("chunk 2");
        acc.apply(ChatStreamEvent::Done).expect("done");

        let response = acc.finish().expect("response");
        let function_call = &response.choices[0].message.extensions["function_call"];
        assert_eq!(function_call["name"], "lookup");
        assert_eq!(function_call["arguments"], "{\"q\":1}");
    }

    #[test]
    fn finish_partial_returns_truncated_content() {
        let mut acc = ChatStreamAccumulator::new();
        acc.apply(ChatStreamEvent::Chunk(chunk(json!({
            "id": "1",
            "model": "m",
            "choices": [{"index": 0, "delta": {"content": "half"}}]
        }))))
        .expect("chunk");

        let (response, status) = acc.finish_partial();
        assert_eq!(response.first_content(), Some("half"));
        assert_eq!(status, airx_core::stream::StreamStatus::Active);
    }

    #[test]
    fn custom_tool_call_deltas_do_not_synthesize_function() {
        let mut acc = ChatStreamAccumulator::new();
        acc.apply(ChatStreamEvent::Chunk(chunk(json!({
            "id": "1",
            "model": "m",
            "choices": [{"index": 0, "delta": {"tool_calls": [
                {"index": 0, "id": "c1", "type": "custom", "custom": {"name": "code"}}
            ]}}]
        }))))
        .expect("chunk 1");
        acc.apply(ChatStreamEvent::Chunk(chunk(json!({
            "id": "1",
            "model": "m",
            "choices": [{"index": 0, "delta": {"tool_calls": [
                {"index": 0, "custom": {"input": "print(1)"}}
            ]}, "finish_reason": "tool_calls"}]
        }))))
        .expect("chunk 2");
        acc.apply(ChatStreamEvent::Done).expect("done");

        let response = acc.finish().expect("response");
        let call = &response.choices[0]
            .message
            .tool_calls
            .as_ref()
            .expect("calls")[0];
        assert_eq!(call.kind, "custom");
        assert!(call.function.is_none());
        assert_eq!(call.custom.as_ref().map(|c| c.name.as_str()), Some("code"));
        assert_eq!(
            call.custom.as_ref().map(|c| c.input.as_str()),
            Some("print(1)")
        );
        assert_eq!(call.name(), Some("code"));
        assert_eq!(call.arguments(), Some("print(1)"));
    }
}

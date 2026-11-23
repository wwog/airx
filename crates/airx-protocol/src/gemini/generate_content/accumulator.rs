//! Gemini 流式聚合器。
//!
//! Gemini 的 SSE 没有 `[DONE]` 哨兵：流在 EOF 结束，完成信号是候选的
//! `finishReason`（被安全策略拦截的输入则由 `promptFeedback.blockReason` 表示）。
//! 两者都没有的流按 `Incomplete` 处理，避免把截断当作成功。

use std::collections::BTreeMap;

use airx_core::AirxError;
use airx_core::error::StreamError;
use airx_core::json::Extensions;
use airx_core::stream::StreamStatus;

use crate::gemini::common::UsageMetadata;

use super::content::{Content, Part};
use super::response::{Candidate, GenerateContentResponse, PromptFeedback};

/// Gemini 流式聚合器。
#[derive(Debug, Default)]
pub struct GenerateContentStreamAccumulator {
    candidates: BTreeMap<u32, Candidate>,
    prompt_feedback: Option<PromptFeedback>,
    usage_metadata: Option<UsageMetadata>,
    model_version: Option<String>,
    response_id: Option<String>,
    status: StreamStatus,
    extensions: Extensions,
}

impl GenerateContentStreamAccumulator {
    /// 创建空聚合器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前状态。
    pub fn status(&self) -> StreamStatus {
        self.status
    }

    /// 应用一个响应增量。
    pub fn apply(&mut self, response: GenerateContentResponse) -> Result<(), AirxError> {
        if self.status.is_terminal() {
            // 终态之后只吸收可能迟到的用量与扩展字段，不再追加内容。
            if response.usage_metadata.is_some() {
                self.usage_metadata = response.usage_metadata;
            }
            for (key, value) in response.extensions {
                self.extensions.entry(key).or_insert(value);
            }
            return Ok(());
        }

        let GenerateContentResponse {
            candidates,
            prompt_feedback,
            usage_metadata,
            model_version,
            response_id,
            extensions,
        } = response;

        if prompt_feedback.is_some() {
            self.prompt_feedback = prompt_feedback;
        }
        if usage_metadata.is_some() {
            self.usage_metadata = usage_metadata;
        }
        if model_version.is_some() {
            self.model_version = model_version;
        }
        if response_id.is_some() {
            self.response_id = response_id;
        }
        for (key, value) in extensions {
            self.extensions.entry(key).or_insert(value);
        }

        for candidate in candidates {
            match self.candidates.get_mut(&candidate.index) {
                Some(existing) => merge_candidate(existing, candidate),
                None => {
                    self.candidates.insert(candidate.index, candidate);
                }
            }
        }

        let blocked = self
            .prompt_feedback
            .as_ref()
            .is_some_and(|feedback| feedback.block_reason.is_some());
        let finished = self
            .candidates
            .values()
            .any(|candidate| is_terminal_finish(candidate.finish_reason.as_deref()));

        if blocked || finished {
            self.status = StreamStatus::Completed;
        }

        Ok(())
    }

    /// 当前部分结果。
    pub fn snapshot(&self) -> GenerateContentResponse {
        self.build()
    }

    /// 结束并返回完整响应。
    pub fn finish(self) -> Result<GenerateContentResponse, AirxError> {
        match self.status {
            StreamStatus::Completed => Ok(self.build()),
            StreamStatus::Failed => Err(AirxError::Stream(StreamError::failed("Gemini 流失败"))),
            _ => Err(AirxError::Stream(StreamError::incomplete(
                "Gemini 流在 finishReason / blockReason 之前结束",
            ))),
        }
    }

    fn build(&self) -> GenerateContentResponse {
        GenerateContentResponse {
            candidates: self.candidates.values().cloned().collect(),
            prompt_feedback: self.prompt_feedback.clone(),
            usage_metadata: self.usage_metadata.clone(),
            model_version: self.model_version.clone(),
            response_id: self.response_id.clone(),
            extensions: self.extensions.clone(),
        }
    }
}

fn is_terminal_finish(reason: Option<&str>) -> bool {
    matches!(reason, Some(reason) if !reason.is_empty() && reason != "FINISH_REASON_UNSPECIFIED")
}

fn merge_candidate(target: &mut Candidate, source: Candidate) {
    if let Some(source_content) = source.content {
        let content = target.content.get_or_insert_with(Content::default);
        if source_content.role.is_some() {
            content.role = source_content.role;
        }
        merge_parts(&mut content.parts, source_content.parts);
        for (key, value) in source_content.extensions {
            content.extensions.entry(key).or_insert(value);
        }
    }

    if source.finish_reason.is_some() {
        target.finish_reason = source.finish_reason;
    }
    if source.finish_message.is_some() {
        target.finish_message = source.finish_message;
    }
    if source.safety_ratings.is_some() {
        target.safety_ratings = source.safety_ratings;
    }
    if source.citation_metadata.is_some() {
        target.citation_metadata = source.citation_metadata;
    }
    if source.grounding_metadata.is_some() {
        target.grounding_metadata = source.grounding_metadata;
    }
    if source.url_context_metadata.is_some() {
        target.url_context_metadata = source.url_context_metadata;
    }
    if source.token_count.is_some() {
        target.token_count = source.token_count;
    }
    if source.avg_logprobs.is_some() {
        target.avg_logprobs = source.avg_logprobs;
    }
    if source.logprobs_result.is_some() {
        target.logprobs_result = source.logprobs_result;
    }
    for (key, value) in source.extensions {
        target.extensions.entry(key).or_insert(value);
    }
}

/// 文本增量按 thinking 标记合并；其余片段按出现顺序追加。
fn merge_parts(target: &mut Vec<Part>, incoming: Vec<Part>) {
    for part in incoming {
        match part {
            Part::Text(text) => {
                if let Some(Part::Text(last)) = target.last_mut() {
                    if last.is_thought() == text.is_thought() {
                        last.text.push_str(&text.text);
                        if text.thought_signature.is_some() {
                            last.thought_signature = text.thought_signature;
                        }
                        for (key, value) in text.extensions {
                            last.extensions.entry(key).or_insert(value);
                        }
                        continue;
                    }
                }
                target.push(Part::Text(text));
            }
            other => target.push(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airx_core::error::StreamErrorKind;
    use serde_json::{Value, json};

    fn chunk(value: Value) -> GenerateContentResponse {
        serde_json::from_value(value).expect("chunk")
    }

    #[test]
    fn accumulates_text_thought_and_usage() {
        let mut acc = GenerateContentStreamAccumulator::new();
        acc.apply(chunk(json!({
            "candidates": [{
                "index": 0,
                "content": {"role": "model", "parts": [{"text": "你"}]}
            }],
            "modelVersion": "gemini-2.5-flash",
            "responseId": "resp-1"
        })))
        .expect("chunk 1");
        acc.apply(chunk(json!({
            "candidates": [{
                "index": 0,
                "content": {"parts": [{"text": "好", "thought": false}, {"text": "想", "thought": true}]}
            }]
        })))
        .expect("chunk 2");
        acc.apply(chunk(json!({
            "candidates": [{
                "index": 0,
                "content": {"parts": [{"text": "一下", "thought": true}]},
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 3,
                "candidatesTokenCount": 4,
                "totalTokenCount": 7
            }
        })))
        .expect("chunk 3");

        assert!(acc.status().is_success());
        let response = acc.finish().expect("response");
        assert_eq!(response.text(), "你好");
        assert_eq!(response.thought_text(), "想一下");
        assert_eq!(response.finish_reason(), Some("STOP"));
        assert_eq!(response.model_version.as_deref(), Some("gemini-2.5-flash"));
        assert_eq!(
            response
                .usage_metadata
                .as_ref()
                .map(|u| u.total_token_count),
            Some(7)
        );
    }

    #[test]
    fn truncated_stream_is_incomplete() {
        let mut acc = GenerateContentStreamAccumulator::new();
        acc.apply(chunk(json!({
            "candidates": [{
                "index": 0,
                "content": {"parts": [{"text": "partial"}]}
            }]
        })))
        .expect("chunk");

        match acc.finish().expect_err("incomplete") {
            AirxError::Stream(error) => assert_eq!(error.kind, StreamErrorKind::Incomplete),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn unspecified_finish_reason_is_not_completion() {
        let mut acc = GenerateContentStreamAccumulator::new();
        acc.apply(chunk(json!({
            "candidates": [{"index": 0, "finishReason": "FINISH_REASON_UNSPECIFIED"}]
        })))
        .expect("chunk");

        assert!(!acc.status().is_success());
        assert!(acc.finish().is_err());
    }

    #[test]
    fn blocked_prompt_is_completed_and_keeps_feedback() {
        let mut acc = GenerateContentStreamAccumulator::new();
        acc.apply(chunk(json!({
            "promptFeedback": {
                "blockReason": "SAFETY",
                "safetyRatings": [{"category": "HARM_CATEGORY_HARASSMENT"}]
            }
        })))
        .expect("chunk");

        assert!(acc.status().is_success());
        let response = acc.finish().expect("response");
        assert!(response.candidates.is_empty());
        assert_eq!(
            response
                .prompt_feedback
                .as_ref()
                .and_then(|feedback| feedback.block_reason.as_deref()),
            Some("SAFETY")
        );
    }

    #[test]
    fn function_call_parts_are_preserved() {
        let mut acc = GenerateContentStreamAccumulator::new();
        acc.apply(chunk(json!({
            "candidates": [{
                "index": 0,
                "content": {"role": "model", "parts": [
                    {"functionCall": {"name": "lookup", "args": {"q": "rust"}}}
                ]},
                "finishReason": "STOP"
            }]
        })))
        .expect("chunk");

        let response = acc.finish().expect("response");
        let calls = response.function_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].args["q"], "rust");
    }

    #[test]
    fn snapshot_does_not_require_completion() {
        let mut acc = GenerateContentStreamAccumulator::new();
        acc.apply(chunk(json!({
            "candidates": [{"index": 0, "content": {"parts": [{"text": "hi"}]}}]
        })))
        .expect("chunk");

        assert_eq!(acc.snapshot().text(), "hi");
    }
}

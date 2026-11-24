//! Gemini 流式聚合器。
//!
//! Gemini 的 SSE 没有 `[DONE]` 哨兵：流在 EOF 结束，完成信号是候选的
//! `finishReason`（被安全策略拦截的输入则由 `promptFeedback.blockReason` 表示）。
//! 两者都没有的流按 `Incomplete` 处理，避免把截断当作成功。
//!
//! 完成判定与官方语义对齐：
//! - `CONTINUATION` 不是完成态（需要携带 `continuationToken` 续写）；
//! - `candidateCount > 1` 时等所有候选都进入终态才算完成，终态之后到达的候选
//!   内容也不会被丢弃。

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
    create_time: Option<String>,
    model_status: Option<serde_json::Value>,
    status: StreamStatus,
    extensions: Extensions,
    inline_data_merge: bool,
}

impl GenerateContentStreamAccumulator {
    /// 创建空聚合器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 是否合并连续 `inlineData` 增量（默认关闭）。
    ///
    /// 开启后，连续的同类 `inlineData` 片段会按 base64 拼接（前一段没有结尾
    /// 填充时）。官方快照未规定 generateContent 的图片分片规则，若上游按
    /// "每个 chunk 一张完整图片"返回，拼接会破坏数据，因此默认关闭。
    #[must_use]
    pub fn with_inline_data_merge(mut self, enabled: bool) -> Self {
        self.inline_data_merge = enabled;
        self
    }

    /// 当前状态。
    pub fn status(&self) -> StreamStatus {
        self.status
    }

    /// 应用一个响应增量。
    pub fn apply(&mut self, response: GenerateContentResponse) -> Result<(), AirxError> {
        let GenerateContentResponse {
            candidates,
            prompt_feedback,
            usage_metadata,
            model_version,
            response_id,
            create_time,
            model_status,
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
        if create_time.is_some() {
            self.create_time = create_time;
        }
        if model_status.is_some() {
            self.model_status = model_status;
        }
        for (key, value) in extensions {
            self.extensions.entry(key).or_insert(value);
        }

        for candidate in candidates {
            match self.candidates.get_mut(&candidate.index) {
                Some(existing) => merge_candidate(existing, candidate, self.inline_data_merge),
                None => {
                    self.candidates.insert(candidate.index, candidate);
                }
            }
        }

        self.refresh_status();
        Ok(())
    }

    /// 当前部分结果。
    pub fn snapshot(&self) -> GenerateContentResponse {
        self.build()
    }

    /// 当前结果是否需要续写（`CONTINUATION`），返回续写游标。
    pub fn requires_continuation(&self) -> Option<String> {
        self.build().requires_continuation().map(str::to_owned)
    }

    /// 结束并返回完整响应。
    pub fn finish(self) -> Result<GenerateContentResponse, AirxError> {
        if self.status.is_success() {
            return Ok(self.build());
        }
        Err(AirxError::Stream(StreamError::incomplete(
            "Gemini 流在 finishReason / blockReason 之前结束（CONTINUATION 需要续写）",
        )))
    }

    /// 部分结果收尾：不要求完成信号，返回当前快照与状态。
    pub fn finish_partial(self) -> (GenerateContentResponse, StreamStatus) {
        let status = self.status;
        (self.build(), status)
    }

    fn refresh_status(&mut self) {
        let blocked = self
            .prompt_feedback
            .as_ref()
            .is_some_and(|feedback| feedback.block_reason.is_some());
        let all_terminal =
            !self.candidates.is_empty() && self.candidates.values().all(Candidate::is_terminal);

        // 允许回退：终态之后又出现未完成的候选时，重新视为进行中，
        // 避免多候选流被提前判定为成功。
        self.status = if blocked || all_terminal {
            StreamStatus::Completed
        } else {
            StreamStatus::Active
        };
    }

    fn build(&self) -> GenerateContentResponse {
        GenerateContentResponse {
            candidates: self.candidates.values().cloned().collect(),
            prompt_feedback: self.prompt_feedback.clone(),
            usage_metadata: self.usage_metadata.clone(),
            model_version: self.model_version.clone(),
            response_id: self.response_id.clone(),
            create_time: self.create_time.clone(),
            model_status: self.model_status.clone(),
            extensions: self.extensions.clone(),
        }
    }
}

fn merge_candidate(target: &mut Candidate, source: Candidate, merge_inline: bool) {
    if let Some(source_content) = source.content {
        let content = target.content.get_or_insert_with(Content::default);
        if source_content.role.is_some() {
            content.role = source_content.role;
        }
        merge_parts(&mut content.parts, source_content.parts, merge_inline);
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
    if source.continuation_token.is_some() {
        target.continuation_token = source.continuation_token;
    }
    for (key, value) in source.extensions {
        target.extensions.entry(key).or_insert(value);
    }
}

/// 文本增量按 thinking 标记合并；其余片段按出现顺序追加。
fn merge_parts(target: &mut Vec<Part>, incoming: Vec<Part>, merge_inline: bool) {
    for part in incoming {
        match part {
            Part::Text(text) => {
                if let Some(Part::Text(last)) = target.last_mut() {
                    if can_merge_text(last, &text) {
                        last.text.push_str(&text.text);
                        if text.thought_signature.is_some() {
                            last.thought_signature = text.thought_signature;
                        }
                        continue;
                    }
                }
                target.push(Part::Text(text));
            }
            Part::InlineData(data) => {
                if merge_inline {
                    if let Some(Part::InlineData(last)) = target.last_mut() {
                        if can_merge_inline(last, &data) {
                            last.data.push_str(&data.data);
                            continue;
                        }
                    }
                }
                target.push(Part::InlineData(data));
            }
            other => target.push(other),
        }
    }
}

/// 仅在同 thinking 标记、且双方都没有 Part 级元数据（含未知兄弟键）时拼接文本。
///
/// `thoughtSignature` 是唯一例外：它通常只出现在最后一段，可并入已累积文本。
fn can_merge_text(last: &super::content::TextPart, next: &super::content::TextPart) -> bool {
    last.is_thought() == next.is_thought()
        && last.video_metadata.is_none()
        && next.video_metadata.is_none()
        && last.part_metadata.is_none()
        && next.part_metadata.is_none()
        && last.media_resolution.is_none()
        && next.media_resolution.is_none()
        && last.extensions.is_empty()
        && next.extensions.is_empty()
}

/// 仅在同类型、同 `displayName`、双方都没有 part 级元数据、且前一段没有
/// base64 结尾填充时拼接。
fn can_merge_inline(
    last: &super::content::InlineDataPart,
    next: &super::content::InlineDataPart,
) -> bool {
    last.mime_type == next.mime_type
        && last.display_name == next.display_name
        && !last.data.is_empty()
        && !next.data.is_empty()
        && !last.data.ends_with('=')
        && !last.has_part_metadata()
        && !next.has_part_metadata()
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
    fn continuation_is_not_completion() {
        let mut acc = GenerateContentStreamAccumulator::new();
        acc.apply(chunk(json!({
            "candidates": [{
                "index": 0,
                "content": {"parts": [{"text": "long output"}]},
                "finishReason": "CONTINUATION",
                "continuationToken": "next-token"
            }]
        })))
        .expect("chunk");

        assert!(!acc.status().is_success());
        assert_eq!(acc.requires_continuation().as_deref(), Some("next-token"));
        assert_eq!(acc.snapshot().text(), "long output");

        match acc.finish().expect_err("continuation") {
            AirxError::Stream(error) => assert_eq!(error.kind, StreamErrorKind::Incomplete),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn blocked_prompt_is_completed_and_keeps_feedback() {
        let mut acc = GenerateContentStreamAccumulator::new();
        acc.apply(chunk(json!({
            "promptFeedback": {
                "blockReason": "SAFETY",
                "blockReasonMessage": "blocked",
                "safetyRatings": [{"category": "HARM_CATEGORY_HARASSMENT"}]
            }
        })))
        .expect("chunk");

        assert!(acc.status().is_success());
        let response = acc.finish().expect("response");
        assert!(response.candidates.is_empty());
        assert_eq!(response.block_message(), Some("blocked"));
    }

    #[test]
    fn function_call_parts_are_preserved() {
        let mut acc = GenerateContentStreamAccumulator::new();
        acc.apply(chunk(json!({
            "candidates": [{
                "index": 0,
                "content": {"role": "model", "parts": [
                    {"functionCall": {"name": "lookup", "args": {"q": "rust"}},
                     "thoughtSignature": "sig-1"}
                ]},
                "finishReason": "STOP"
            }]
        })))
        .expect("chunk");

        let response = acc.finish().expect("response");
        let calls = response.function_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].args["q"], "rust");
        assert_eq!(calls[0].thought_signature(), Some("sig-1"));
    }

    #[test]
    fn multi_candidate_stream_waits_for_all_candidates() {
        let mut acc = GenerateContentStreamAccumulator::new();
        acc.apply(chunk(json!({
            "candidates": [
                {"index": 0, "content": {"parts": [{"text": "a"}]}},
                {"index": 1, "content": {"parts": [{"text": "b"}]}}
            ]
        })))
        .expect("chunk 1");

        acc.apply(chunk(json!({
            "candidates": [
                {"index": 0, "finishReason": "STOP"},
                {"index": 1, "content": {"parts": [{"text": "b2"}]}}
            ]
        })))
        .expect("chunk 2");

        assert!(!acc.status().is_success(), "second candidate still active");
        assert_eq!(acc.snapshot().candidates[1].text(), "bb2");

        acc.apply(chunk(json!({
            "candidates": [
                {"index": 0, "finishReason": "STOP"},
                {"index": 1, "finishReason": "STOP"}
            ]
        })))
        .expect("chunk 3");

        assert!(acc.status().is_success());
        let response = acc.finish().expect("response");
        assert_eq!(response.candidate_count(), 2);
        assert_eq!(response.candidates[0].text(), "a");
        assert_eq!(response.candidates[1].text(), "bb2");
    }

    #[test]
    fn later_unfinished_candidate_demotes_completion() {
        let mut acc = GenerateContentStreamAccumulator::new();
        acc.apply(chunk(json!({
            "candidates": [{"index": 0, "finishReason": "STOP"}]
        })))
        .expect("chunk 1");
        assert!(acc.status().is_success());

        acc.apply(chunk(json!({
            "candidates": [{
                "index": 1,
                "content": {"parts": [{"text": "late"}]}
            }]
        })))
        .expect("chunk 2");

        assert!(!acc.status().is_success());
        assert_eq!(acc.snapshot().candidates[1].text(), "late");
        assert!(acc.finish().is_err(), "未完成的候选不算成功");
    }

    #[test]
    fn late_candidate_content_is_not_dropped() {
        let mut acc = GenerateContentStreamAccumulator::new();
        acc.apply(chunk(json!({
            "candidates": [{
                "index": 0,
                "content": {"parts": [{"text": "a"}]},
                "finishReason": "STOP"
            }]
        })))
        .expect("chunk 1");
        assert!(acc.status().is_success());

        acc.apply(chunk(json!({
            "candidates": [{
                "index": 1,
                "content": {"parts": [{"text": "late"}]},
                "finishReason": "STOP"
            }]
        })))
        .expect("chunk 2");

        let response = acc.snapshot();
        assert_eq!(response.candidate_count(), 2);
        assert_eq!(response.candidates[1].text(), "late");
    }

    #[test]
    fn inline_data_is_appended_by_default() {
        let mut acc = GenerateContentStreamAccumulator::new();
        for data in ["AAA", "BBB"] {
            acc.apply(chunk(json!({
                "candidates": [{
                    "index": 0,
                    "content": {"parts": [{"inlineData": {"mimeType": "image/png", "data": data}}]}
                }]
            })))
            .expect("chunk");
        }

        let response = acc.snapshot();
        let parts = &response.candidates[0]
            .content
            .as_ref()
            .expect("content")
            .parts;
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn inline_data_merge_joins_base64_chunks() {
        let mut acc = GenerateContentStreamAccumulator::new().with_inline_data_merge(true);
        for data in ["QUJD", "REVG"] {
            acc.apply(chunk(json!({
                "candidates": [{
                    "index": 0,
                    "content": {"parts": [{"inlineData": {"mimeType": "image/png", "data": data}}]}
                }]
            })))
            .expect("chunk");
        }

        let response = acc.snapshot();
        let parts = &response.candidates[0]
            .content
            .as_ref()
            .expect("content")
            .parts;
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            Part::InlineData(data) => assert_eq!(data.data, "QUJDREVG"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn inline_data_merge_keeps_padded_chunks_separate() {
        let mut acc = GenerateContentStreamAccumulator::new().with_inline_data_merge(true);
        // 前一段已带结尾填充 → 视为一张完整图片，不再拼接。
        for data in ["QUJDREVG==", "SELL"] {
            acc.apply(chunk(json!({
                "candidates": [{
                    "index": 0,
                    "content": {"parts": [{"inlineData": {"mimeType": "image/png", "data": data}}]}
                }]
            })))
            .expect("chunk");
        }

        let response = acc.snapshot();
        let parts = &response.candidates[0]
            .content
            .as_ref()
            .expect("content")
            .parts;
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn inline_data_merge_accepts_final_padded_chunk() {
        let mut acc = GenerateContentStreamAccumulator::new().with_inline_data_merge(true);
        for data in ["QUJD", "REVG=="] {
            acc.apply(chunk(json!({
                "candidates": [{
                    "index": 0,
                    "content": {"parts": [{"inlineData": {"mimeType": "image/png", "data": data}}]}
                }]
            })))
            .expect("chunk");
        }

        let response = acc.snapshot();
        let parts = &response.candidates[0]
            .content
            .as_ref()
            .expect("content")
            .parts;
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            Part::InlineData(data) => assert_eq!(data.data, "QUJDREVG=="),
            other => panic!("unexpected: {other:?}"),
        }
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

    #[test]
    fn text_with_part_metadata_is_not_merged() {
        let mut acc = GenerateContentStreamAccumulator::new();
        acc.apply(chunk(json!({
            "candidates": [{"index": 0, "content": {"parts": [{"text": "a"}]}}]
        })))
        .expect("chunk");
        acc.apply(chunk(json!({
            "candidates": [{
                "index": 0,
                "content": {"parts": [
                    {"text": "b", "partMetadata": {"trace": "x"}}
                ]}
            }]
        })))
        .expect("chunk");

        let response = acc.snapshot();
        let parts = &response.candidates[0]
            .content
            .as_ref()
            .expect("content")
            .parts;
        assert_eq!(parts.len(), 2, "带 Part 级元数据的文本不能并入前一段");
        match &parts[1] {
            Part::Text(part) => assert_eq!(part.part_metadata, Some(json!({"trace": "x"}))),
            other => panic!("unexpected: {other:?}"),
        }
    }
}

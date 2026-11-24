//! Gemini `generateContent` 响应。
//!
//! 流式响应（`alt=sse`）的每个 SSE data 也是同一种
//! [`GenerateContentResponse`] 增量结构，因此事件类型直接复用它。

use airx_core::json::Extensions;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::gemini::common::{SafetyRating, UsageMetadata};

use super::content::{Content, FunctionCallPart};

/// `generateContent` 响应。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentResponse {
    /// 候选结果。
    #[serde(default)]
    pub candidates: Vec<Candidate>,
    /// 输入侧的反馈（例如安全拦截）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_feedback: Option<PromptFeedback>,
    /// Token 用量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_metadata: Option<UsageMetadata>,
    /// 实际模型版本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_version: Option<String>,
    /// 响应 ID（部分模型返回）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    /// 创建时间（原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create_time: Option<String>,
    /// 模型状态（原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_status: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl GenerateContentResponse {
    /// 第一个候选。
    pub fn first_candidate(&self) -> Option<&Candidate> {
        self.candidates.first()
    }

    /// 第一个候选的可见文本。
    pub fn text(&self) -> String {
        self.first_candidate()
            .map(|candidate| candidate.text())
            .unwrap_or_default()
    }

    /// 第一个候选的 thinking 文本。
    pub fn thought_text(&self) -> String {
        self.first_candidate()
            .map(|candidate| candidate.thought_text())
            .unwrap_or_default()
    }

    /// 第一个候选的结束原因（原始字符串）。
    pub fn finish_reason(&self) -> Option<&str> {
        self.first_candidate()?.finish_reason.as_deref()
    }

    /// 第一个候选的结束原因（分类枚举）。
    pub fn finish_reason_typed(&self) -> Option<FinishReason> {
        self.finish_reason().map(FinishReason::from_wire)
    }

    /// 所有候选的工具调用。
    pub fn function_calls(&self) -> Vec<&FunctionCallPart> {
        self.candidates
            .iter()
            .flat_map(Candidate::function_calls)
            .collect()
    }

    /// 候选数量。
    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    /// 是否被安全策略拦截（输入 `blockReason` 或候选终态为拦截类）。
    ///
    /// 被拦截是 200 响应，`text()` 可能为空，因此要先看这里。
    pub fn is_blocked(&self) -> bool {
        let prompt_blocked = self
            .prompt_feedback
            .as_ref()
            .is_some_and(|feedback| feedback.block_reason.is_some());
        prompt_blocked
            || self
                .candidates
                .iter()
                .filter_map(|candidate| candidate.finish_reason.as_deref())
                .map(FinishReason::from_wire)
                .any(|reason| reason.is_blocked())
    }

    /// 是否因 `MAX_TOKENS` 截断。
    pub fn is_truncated(&self) -> bool {
        self.candidates
            .iter()
            .filter_map(|candidate| candidate.finish_reason.as_deref())
            .map(FinishReason::from_wire)
            .any(|reason| reason.is_truncated())
    }

    /// 是否需要续写（`finishReason=CONTINUATION`）。
    ///
    /// 返回续写游标：优先取候选上的 `continuationToken`，否则返回空字符串，
    /// 表示需要把 `continuationToken` 从别处补全（airx 不自动重发）。
    pub fn requires_continuation(&self) -> Option<&str> {
        self.candidates
            .iter()
            .find(|candidate| {
                candidate
                    .finish_reason
                    .as_deref()
                    .is_some_and(|reason| FinishReason::from_wire(reason).requires_continuation())
            })
            .map(|candidate| candidate.continuation_token.as_deref().unwrap_or_default())
    }

    /// 输入侧拦截的可读原因（`promptFeedback.blockReasonMessage`）。
    pub fn block_message(&self) -> Option<&str> {
        self.prompt_feedback
            .as_ref()
            .and_then(|feedback| feedback.block_reason_message.as_deref())
    }
}

/// 结束原因分类（覆盖官方 24 个值中的常见集合）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishReason {
    /// 正常结束。
    Stop,
    /// 达到输出上限。
    MaxTokens,
    /// 内容安全拦截。
    Safety,
    /// 复述拦截。
    Recitation,
    /// 黑名单拦截。
    Blocklist,
    /// 禁止内容。
    ProhibitedContent,
    /// 敏感个人信息。
    Spii,
    /// 图片安全拦截。
    ImageSafety,
    /// 没有生成图片。
    NoImage,
    /// 函数调用格式错误。
    MalformedFunctionCall,
    /// 响应格式错误。
    MalformedResponse,
    /// 缺少思考签名。
    MissingThoughtSignature,
    /// 意外的工具调用。
    UnexpectedToolCall,
    /// 工具调用过多。
    TooManyToolCalls,
    /// 模型防护拦截。
    ModelArmor,
    /// 需要续写（`continuationToken`）。
    Continuation,
    /// 未识别或其它终态。
    Other(String),
}

impl FinishReason {
    /// 由线上字符串解析。
    pub fn from_wire(reason: &str) -> Self {
        match reason {
            "STOP" => Self::Stop,
            "MAX_TOKENS" => Self::MaxTokens,
            "SAFETY" => Self::Safety,
            "RECITATION" => Self::Recitation,
            "BLOCKLIST" => Self::Blocklist,
            "PROHIBITED_CONTENT" => Self::ProhibitedContent,
            "SPII" => Self::Spii,
            "IMAGE_SAFETY" => Self::ImageSafety,
            "NO_IMAGE" => Self::NoImage,
            "MALFORMED_FUNCTION_CALL" => Self::MalformedFunctionCall,
            "MALFORMED_RESPONSE" => Self::MalformedResponse,
            "MISSING_THOUGHT_SIGNATURE" => Self::MissingThoughtSignature,
            "UNEXPECTED_TOOL_CALL" => Self::UnexpectedToolCall,
            "TOO_MANY_TOOL_CALLS" => Self::TooManyToolCalls,
            "MODEL_ARMOR" => Self::ModelArmor,
            "CONTINUATION" => Self::Continuation,
            other => Self::Other(other.to_owned()),
        }
    }

    /// 是否为"调用完成但结果不可用"的拦截类终态。
    pub fn is_blocked(&self) -> bool {
        matches!(
            self,
            Self::Safety
                | Self::Recitation
                | Self::Blocklist
                | Self::ProhibitedContent
                | Self::Spii
                | Self::ImageSafety
                | Self::NoImage
                | Self::MalformedFunctionCall
                | Self::MalformedResponse
                | Self::MissingThoughtSignature
                | Self::UnexpectedToolCall
                | Self::TooManyToolCalls
                | Self::ModelArmor
        )
    }

    /// 是否因输出上限截断。
    pub fn is_truncated(&self) -> bool {
        matches!(self, Self::MaxTokens)
    }

    /// 是否需要续写。
    pub fn requires_continuation(&self) -> bool {
        matches!(self, Self::Continuation)
    }

    /// 是否是可用的正常终态。
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Stop)
    }
}

/// 单个候选。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// 候选序号。
    #[serde(default)]
    pub index: u32,
    /// 生成内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    /// 结束原因（`STOP` / `MAX_TOKENS` / `SAFETY` …）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// 结束原因说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_message: Option<String>,
    /// 安全评级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_ratings: Option<Vec<SafetyRating>>,
    /// 引用信息（结构随官方演进，原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citation_metadata: Option<Value>,
    /// 接地 / 搜索来源信息（原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grounding_metadata: Option<Value>,
    /// URL 上下文信息（原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_context_metadata: Option<Value>,
    /// 该候选的 token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_count: Option<u32>,
    /// 平均对数概率。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_logprobs: Option<f64>,
    /// logprobs 结果（原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs_result: Option<Value>,
    /// 续写游标（`finishReason=CONTINUATION` 时用于下一次请求）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_token: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl Candidate {
    /// 可见文本。
    pub fn text(&self) -> String {
        self.content.as_ref().map(Content::text).unwrap_or_default()
    }

    /// thinking 文本。
    pub fn thought_text(&self) -> String {
        self.content
            .as_ref()
            .map(Content::thought_text)
            .unwrap_or_default()
    }

    /// 工具调用。
    pub fn function_calls(&self) -> Vec<&FunctionCallPart> {
        self.content
            .as_ref()
            .map(Content::function_calls)
            .unwrap_or_default()
    }

    /// 是否已进入终态（`CONTINUATION` 不算终态）。
    pub fn is_terminal(&self) -> bool {
        match self.finish_reason.as_deref() {
            Some(reason) if !reason.is_empty() && reason != "FINISH_REASON_UNSPECIFIED" => {
                !FinishReason::from_wire(reason).requires_continuation()
            }
            _ => false,
        }
    }
}

/// 输入侧反馈。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptFeedback {
    /// 拦截原因（`SAFETY` / `OTHER` / `BLOCKLIST` …）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<String>,
    /// 拦截原因的可读说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_reason_message: Option<String>,
    /// 安全评级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_ratings: Option<Vec<SafetyRating>>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl PromptFeedback {
    /// 拦截原因的可读说明。
    pub fn block_message(&self) -> Option<&str> {
        self.block_reason_message.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn response_helpers_extract_text_and_calls() {
        let response: GenerateContentResponse = serde_json::from_value(json!({
            "candidates": [{
                "index": 0,
                "content": {
                    "role": "model",
                    "parts": [
                        {"text": "先思考", "thought": true},
                        {"text": "答案是 42"},
                        {"functionCall": {"name": "lookup", "args": {"q": "42"}}}
                    ]
                },
                "finishReason": "STOP",
                "safetyRatings": [{"category": "HARM_CATEGORY_HARASSMENT", "probability": "NEGLIGIBLE"}]
            }],
            "usageMetadata": {
                "promptTokenCount": 3,
                "candidatesTokenCount": 5,
                "totalTokenCount": 8,
                "thoughtsTokenCount": 2
            },
            "modelVersion": "gemini-2.5-flash",
            "responseId": "resp-1"
        }))
        .expect("response");

        assert_eq!(response.text(), "答案是 42");
        assert_eq!(response.thought_text(), "先思考");
        assert_eq!(response.finish_reason(), Some("STOP"));
        assert_eq!(response.finish_reason_typed(), Some(FinishReason::Stop));
        assert_eq!(response.function_calls()[0].name, "lookup");
        assert_eq!(
            response
                .usage_metadata
                .as_ref()
                .map(|u| u.total_token_count),
            Some(8)
        );
        assert!(!response.is_blocked());
        assert!(!response.is_truncated());
        assert!(response.requires_continuation().is_none());
        assert_eq!(response.candidate_count(), 1);
    }

    #[test]
    fn blocked_prompt_has_no_candidates() {
        let response: GenerateContentResponse = serde_json::from_value(json!({
            "promptFeedback": {
                "blockReason": "SAFETY",
                "blockReasonMessage": "Input blocked by safety filter",
                "safetyRatings": [{"category": "HARM_CATEGORY_DANGEROUS_CONTENT", "blocked": true}]
            }
        }))
        .expect("response");

        assert!(response.candidates.is_empty());
        assert_eq!(
            response
                .prompt_feedback
                .as_ref()
                .and_then(|feedback| feedback.block_reason.as_deref()),
            Some("SAFETY")
        );
        assert_eq!(
            response.block_message(),
            Some("Input blocked by safety filter")
        );
        assert!(response.is_blocked());
    }

    #[test]
    fn safety_finish_reason_is_blocked() {
        let response: GenerateContentResponse = serde_json::from_value(json!({
            "candidates": [{"index": 0, "finishReason": "SAFETY"}]
        }))
        .expect("response");

        assert!(response.is_blocked());
        assert_eq!(response.finish_reason_typed(), Some(FinishReason::Safety));
    }

    #[test]
    fn max_tokens_is_truncated_but_not_blocked() {
        let response: GenerateContentResponse = serde_json::from_value(json!({
            "candidates": [{"index": 0, "finishReason": "MAX_TOKENS"}]
        }))
        .expect("response");

        assert!(response.is_truncated());
        assert!(!response.is_blocked());
    }

    #[test]
    fn continuation_reason_exposes_token() {
        let response: GenerateContentResponse = serde_json::from_value(json!({
            "candidates": [{
                "index": 0,
                "finishReason": "CONTINUATION",
                "continuationToken": "next-token"
            }]
        }))
        .expect("response");

        assert_eq!(response.requires_continuation(), Some("next-token"));
        assert!(!response.is_truncated());
        assert!(!response.is_blocked());
        assert!(!response.candidates[0].is_terminal());
    }

    #[test]
    fn malformed_function_call_is_blocked() {
        let response: GenerateContentResponse = serde_json::from_value(json!({
            "candidates": [{"index": 0, "finishReason": "MALFORMED_FUNCTION_CALL"}]
        }))
        .expect("response");

        assert!(response.is_blocked());
        assert_eq!(
            response.finish_reason_typed(),
            Some(FinishReason::MalformedFunctionCall)
        );
    }

    #[test]
    fn unknown_fields_roundtrip() {
        let response: GenerateContentResponse = serde_json::from_value(json!({
            "candidates": [{"index": 0, "futureCandidateField": 1}],
            "futureTopField": true
        }))
        .expect("response");

        assert!(response.extensions.contains_key("futureTopField"));
        assert!(
            response.candidates[0]
                .extensions
                .contains_key("futureCandidateField")
        );

        let value = serde_json::to_value(&response).expect("serialize");
        assert_eq!(value["futureTopField"], true);
        assert_eq!(value["candidates"][0]["futureCandidateField"], 1);
    }
}

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

    /// 第一个候选的结束原因。
    pub fn finish_reason(&self) -> Option<&str> {
        self.first_candidate()?.finish_reason.as_deref()
    }

    /// 所有候选的工具调用。
    pub fn function_calls(&self) -> Vec<&FunctionCallPart> {
        self.candidates
            .iter()
            .flat_map(Candidate::function_calls)
            .collect()
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
}

/// 输入侧反馈。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptFeedback {
    /// 拦截原因（`SAFETY` / `OTHER` / `BLOCKLIST` …）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<String>,
    /// 安全评级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_ratings: Option<Vec<SafetyRating>>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
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
        assert_eq!(response.function_calls()[0].name, "lookup");
        assert_eq!(
            response
                .usage_metadata
                .as_ref()
                .map(|u| u.total_token_count),
            Some(8)
        );
    }

    #[test]
    fn blocked_prompt_has_no_candidates() {
        let response: GenerateContentResponse = serde_json::from_value(json!({
            "promptFeedback": {
                "blockReason": "SAFETY",
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

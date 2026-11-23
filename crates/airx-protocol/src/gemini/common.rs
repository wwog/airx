//! Gemini 错误、用量与安全评级的共享类型。

use airx_core::AirxError;
use airx_core::error::{HttpError, ProviderError};
use airx_core::http::{HeaderMap, StatusCode};
use airx_core::json::{Extensions, parse_raw};
use airx_core::model::ProviderId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Gemini 错误响应。
#[derive(Debug, Clone, Deserialize)]
pub struct GeminiErrorResponse {
    /// 错误主体。
    pub error: GeminiErrorBody,
}

/// Gemini 错误主体。
#[derive(Debug, Clone, Deserialize)]
pub struct GeminiErrorBody {
    /// HTTP 状态码。
    #[serde(default)]
    pub code: Option<u16>,
    /// 错误描述。
    pub message: String,
    /// 规范错误状态（如 `INVALID_ARGUMENT`）。
    #[serde(default)]
    pub status: Option<String>,
    /// 结构化错误详情。
    #[serde(default)]
    pub details: Option<Value>,
}

/// 把非 2xx 响应映射为 Provider 错误或通用 HTTP 错误。
pub(crate) fn error_from_response(
    provider: &ProviderId,
    status: StatusCode,
    headers: &HeaderMap,
    body: &[u8],
) -> AirxError {
    let text = std::str::from_utf8(body).ok();
    let raw_body = text.and_then(|text| parse_raw(text).ok());

    if let Some(text) = text {
        if let Ok(envelope) = serde_json::from_str::<GeminiErrorResponse>(text) {
            let code = error_code(&envelope.error);
            let mut error = ProviderError::new(provider.clone(), envelope.error.message);
            error.status = Some(status.as_u16());
            error.code = code;
            error.request_id = request_id(headers);
            error.headers = headers.clone();
            error.raw_body = raw_body;
            return AirxError::Provider(Box::new(error));
        }
    }

    let mut error = HttpError::new(
        status.as_u16(),
        status.canonical_reason().unwrap_or("request failed"),
    );
    error.body = raw_body;
    AirxError::Http(error)
}

/// 流内错误对象（部分网关在 SSE 中直接返回错误体）。
pub(crate) fn error_from_stream_value(provider: &ProviderId, value: Value) -> AirxError {
    let raw_body = serde_json::value::to_raw_value(&value).ok();
    if let Ok(envelope) = serde_json::from_value::<GeminiErrorResponse>(value) {
        let code = error_code(&envelope.error);
        let mut error = ProviderError::new(provider.clone(), envelope.error.message);
        error.code = code;
        error.raw_body = raw_body;
        return AirxError::Provider(Box::new(error));
    }
    AirxError::Deserialization(airx_core::error::DeserializationError::new(
        "无法解析 Gemini 流错误事件",
    ))
}

fn error_code(body: &GeminiErrorBody) -> Option<String> {
    body.status
        .clone()
        .or_else(|| body.code.map(|code| code.to_string()))
}

fn request_id(headers: &HeaderMap) -> Option<String> {
    ["x-goog-request-id", "x-request-id"]
        .iter()
        .find_map(|name| {
            headers
                .get(*name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        })
}

/// Gemini token 用量。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetadata {
    /// 输入 token。
    #[serde(default)]
    pub prompt_token_count: u32,
    /// 输出 token（含 thinking）。
    #[serde(default)]
    pub candidates_token_count: u32,
    /// 总计。
    #[serde(default)]
    pub total_token_count: u32,
    /// 命中缓存的内容 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_content_token_count: Option<u32>,
    /// thinking token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thoughts_token_count: Option<u32>,
    /// 工具声明消耗的输入 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use_prompt_token_count: Option<u32>,
    /// 按模态拆分的输入 token（官方结构随版本演进，原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<Value>,
    /// 按模态拆分的输出 token（原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidates_tokens_details: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 安全评级。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyRating {
    /// 安全类别（如 `HARM_CATEGORY_HARASSMENT`）。
    pub category: String,
    /// 风险概率（如 `NEGLIGIBLE`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probability: Option<String>,
    /// 风险概率分值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probability_score: Option<f32>,
    /// 风险严重程度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    /// 风险严重程度分值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity_score: Option<f32>,
    /// 是否被拦截。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<bool>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_body_maps_to_provider_error_with_status_and_code() {
        let body = br#"{"error":{"code":400,"message":"bad schema","status":"INVALID_ARGUMENT"}}"#;
        let error = error_from_response(
            &ProviderId::gemini(),
            StatusCode::BAD_REQUEST,
            &HeaderMap::new(),
            body,
        );

        match error {
            AirxError::Provider(error) => {
                assert_eq!(error.status, Some(400));
                assert_eq!(error.code.as_deref(), Some("INVALID_ARGUMENT"));
                assert_eq!(error.message, "bad schema");
                assert!(error.raw_body.is_some());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn request_id_header_is_preserved() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-goog-request-id",
            airx_core::http::HeaderValue::from_static("req-1"),
        );
        let body = br#"{"error":{"code":429,"message":"quota","status":"RESOURCE_EXHAUSTED"}}"#;

        match error_from_response(
            &ProviderId::gemini(),
            StatusCode::TOO_MANY_REQUESTS,
            &headers,
            body,
        ) {
            AirxError::Provider(error) => assert_eq!(error.request_id.as_deref(), Some("req-1")),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn non_json_error_falls_back_to_http_error() {
        let error = error_from_response(
            &ProviderId::gemini(),
            StatusCode::BAD_GATEWAY,
            &HeaderMap::new(),
            b"upstream down",
        );
        assert!(matches!(error, AirxError::Http(_)));
    }

    #[test]
    fn usage_metadata_decodes_camel_case() {
        let usage: UsageMetadata = serde_json::from_value(serde_json::json!({
            "promptTokenCount": 10,
            "candidatesTokenCount": 20,
            "totalTokenCount": 35,
            "thoughtsTokenCount": 5,
            "cachedContentTokenCount": 2
        }))
        .expect("usage");

        assert_eq!(usage.prompt_token_count, 10);
        assert_eq!(usage.candidates_token_count, 20);
        assert_eq!(usage.total_token_count, 35);
        assert_eq!(usage.thoughts_token_count, Some(5));
        assert_eq!(usage.cached_content_token_count, Some(2));
    }

    #[test]
    fn safety_rating_keeps_unknown_fields() {
        let rating: SafetyRating = serde_json::from_value(serde_json::json!({
            "category": "HARM_CATEGORY_HATE_SPEECH",
            "probability": "NEGLIGIBLE",
            "futureField": true
        }))
        .expect("rating");

        assert_eq!(rating.category, "HARM_CATEGORY_HATE_SPEECH");
        assert!(rating.extensions.contains_key("futureField"));
    }
}

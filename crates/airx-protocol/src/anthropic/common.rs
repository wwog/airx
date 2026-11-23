//! Anthropic 错误与用量。

use airx_core::AirxError;
use airx_core::error::{HttpError, ProviderError};
use airx_core::http::{HeaderMap, StatusCode};
use airx_core::json::{Extensions, parse_raw};
use airx_core::model::ProviderId;
use serde::Deserialize;
use serde_json::Value;

/// Anthropic 错误响应。
#[derive(Debug, Clone, Deserialize)]
pub struct AnthropicErrorResponse {
    /// 固定为 `error`。
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    /// 错误主体。
    pub error: AnthropicErrorBody,
}

/// Anthropic 错误主体。
#[derive(Debug, Clone, Deserialize)]
pub struct AnthropicErrorBody {
    /// 错误类型（如 `overloaded_error`）。
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    /// 错误描述。
    pub message: String,
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
        if let Ok(envelope) = serde_json::from_str::<AnthropicErrorResponse>(text) {
            let mut error = ProviderError::new(provider.clone(), envelope.error.message);
            error.status = Some(status.as_u16());
            error.code = envelope.error.kind;
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

/// 流内错误事件。
pub(crate) fn error_from_event_value(provider: &ProviderId, value: Value) -> AirxError {
    let raw_body = serde_json::value::to_raw_value(&value).ok();
    if let Ok(envelope) = serde_json::from_value::<AnthropicErrorResponse>(value) {
        let mut error = ProviderError::new(provider.clone(), envelope.error.message);
        error.code = envelope.error.kind;
        error.raw_body = raw_body;
        return AirxError::Provider(Box::new(error));
    }
    AirxError::Deserialization(airx_core::error::DeserializationError::new(
        "无法解析 Anthropic 流错误事件",
    ))
}

fn request_id(headers: &HeaderMap) -> Option<String> {
    ["request-id", "x-request-id"].iter().find_map(|name| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    })
}

/// Anthropic token 用量。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Usage {
    /// 输入 token。
    #[serde(default)]
    pub input_tokens: u32,
    /// 输出 token。
    #[serde(default)]
    pub output_tokens: u32,
    /// 缓存写入 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u32>,
    /// 缓存命中 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u32>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

//! OpenAI 错误响应体。
//!
//! 错误体是协议的一部分：保留 message / type / code / param，
//! 并映射为 `airx_core::error::ProviderError`，而不是折叠成字符串。

use airx_core::AirxError;
use airx_core::error::{DeserializationError, HttpError, ProviderError};
use airx_core::http::{HeaderMap, StatusCode};
use airx_core::json::parse_raw;
use airx_core::model::ProviderId;
use serde::Deserialize;
use serde_json::Value;

/// OpenAI 错误信封。
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiErrorResponse {
    /// 错误主体。
    pub error: OpenAiErrorBody,
}

/// OpenAI 错误主体。
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiErrorBody {
    /// 错误描述。
    pub message: String,
    /// 错误类型（如 `invalid_request_error`）。
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    /// 相关参数。
    #[serde(default)]
    pub param: Option<Value>,
    /// 错误码（可能是字符串或数字）。
    #[serde(default)]
    pub code: Option<Value>,
}

impl OpenAiErrorBody {
    fn code_as_string(&self) -> Option<String> {
        match self.code.as_ref()? {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        }
    }
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
        if let Ok(envelope) = serde_json::from_str::<OpenAiErrorResponse>(text) {
            let body = envelope.error;
            let code = body.code_as_string().or(body.kind);
            let mut error = ProviderError::new(provider.clone(), body.message);
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

/// 把 200 响应中的错误事件（OpenAI 兼容格式）映射为错误。
pub(crate) fn error_from_stream_value(provider: &ProviderId, value: Value) -> AirxError {
    let raw_body = serde_json::value::to_raw_value(&value).ok();

    if let Ok(envelope) = serde_json::from_value::<OpenAiErrorResponse>(value) {
        let body = envelope.error;
        let code = body.code_as_string().or(body.kind);
        let mut error = ProviderError::new(provider.clone(), body.message);
        error.code = code;
        error.raw_body = raw_body;
        return AirxError::Provider(Box::new(error));
    }

    AirxError::Deserialization(DeserializationError::new("无法解析流式错误事件"))
}

/// 从常见 Header 中提取 request id。
fn request_id(headers: &HeaderMap) -> Option<String> {
    ["x-request-id", "request-id", "x-amzn-requestid"]
        .iter()
        .find_map(|name| {
            headers
                .get(*name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        })
}

//! Anthropic 错误与用量。

use airx_core::AirxError;
use airx_core::error::{HttpError, ProviderError};
use airx_core::http::{HeaderMap, StatusCode};
use airx_core::json::{Extensions, parse_raw};
use airx_core::model::ProviderId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Anthropic 错误响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnthropicErrorResponse {
    /// 固定为 `error`。
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    /// 错误主体。
    pub error: AnthropicErrorBody,
    /// 错误体顶层的 `request_id`（Header 被网关吞掉时的兜底）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// Anthropic 错误主体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnthropicErrorBody {
    /// 错误类型（如 `overloaded_error`）。
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    /// 错误描述。
    pub message: String,
    /// 稳定错误码（`error.details.error_code`，如 `storage_quota_exceeded`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// 未归一的细节（`error.details`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl AnthropicErrorBody {
    /// 归一化后的稳定错误码：优先 `error.details.error_code`，其次同名平铺字段，
    /// 最后回退到 `error.type`。
    pub fn code(&self) -> Option<String> {
        self.details
            .as_ref()
            .and_then(|details| details.get("error_code"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| self.error_code.clone())
            .or_else(|| self.kind.clone())
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
        if let Ok(envelope) = serde_json::from_str::<AnthropicErrorResponse>(text) {
            let code = envelope.error.code();
            let mut error = ProviderError::new(provider.clone(), envelope.error.message);
            error.status = Some(status.as_u16());
            error.code = code;
            error.request_id = request_id(headers).or(envelope.request_id);
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
        let code = envelope.error.code();
        let mut error = ProviderError::new(provider.clone(), envelope.error.message);
        error.code = code;
        error.request_id = envelope.request_id;
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

/// 缓存写入按 TTL 的拆分。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheCreation {
    /// 1 小时缓存写入 token。
    #[serde(default)]
    pub ephemeral_1h_input_tokens: u32,
    /// 5 分钟缓存写入 token。
    #[serde(default)]
    pub ephemeral_5m_input_tokens: u32,
}

/// 输出 token 的分类拆分。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputTokensDetails {
    /// 内部推理消耗的 token（含 thinking 块定界符，按 output 计费）。
    #[serde(default)]
    pub thinking_tokens: u32,
}

/// 服务端工具调用次数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerToolUsage {
    /// 联网搜索次数。
    #[serde(default)]
    pub web_search_requests: u32,
    /// 网页抓取次数。
    #[serde(default)]
    pub web_fetch_requests: u32,
}

/// 请求的服务档位（`service_tier`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RequestServiceTier {
    /// 由服务端自动选择。
    Auto,
    /// 只用标准档。
    StandardOnly,
}

/// 实际生效的服务档位（响应 `usage.service_tier`）。
///
/// 官方会新增取值，未识别值保留在 [`ServiceTier::Other`]。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ServiceTier {
    /// 标准档。
    Standard,
    /// 优先档。
    Priority,
    /// 批处理档。
    Batch,
    /// 未识别的取值。
    Other(String),
}

impl ServiceTier {
    /// 字符串形式。
    pub fn as_str(&self) -> &str {
        match self {
            Self::Standard => "standard",
            Self::Priority => "priority",
            Self::Batch => "batch",
            Self::Other(value) => value,
        }
    }
}

impl Serialize for ServiceTier {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ServiceTier {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Ok(match text.as_str() {
            "standard" => Self::Standard,
            "priority" => Self::Priority,
            "batch" => Self::Batch,
            _ => Self::Other(text),
        })
    }
}

/// 推理速度模式（`standard` / `fast`）。
///
/// 官方会新增取值，未识别值保留在 [`Speed::Other`]。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Speed {
    /// 标准速度。
    Standard,
    /// 高吞吐（部分模型需 `fast-mode-2026-02-01` beta 头）。
    Fast,
    /// 未识别的取值。
    Other(String),
}

impl Speed {
    /// 字符串形式。
    pub fn as_str(&self) -> &str {
        match self {
            Self::Standard => "standard",
            Self::Fast => "fast",
            Self::Other(value) => value,
        }
    }
}

impl Serialize for Speed {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Speed {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Ok(match text.as_str() {
            "standard" => Self::Standard,
            "fast" => Self::Fast,
            _ => Self::Other(text),
        })
    }
}

/// Anthropic token 用量。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    /// 输入 token。
    ///
    /// 流式 `message_delta` 的 `usage.input_tokens` 允许为 `null`，
    /// 因此这里是 `Option`；缺失与 `null` 都不再让整条流反序列化失败。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u32>,
    /// 输出 token。
    #[serde(default)]
    pub output_tokens: u32,
    /// 缓存写入 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u32>,
    /// 缓存命中 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u32>,
    /// 缓存写入按 TTL 的拆分（5m / 1h 单价不同）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation: Option<CacheCreation>,
    /// 输出 token 的分类拆分。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens_details: Option<OutputTokensDetails>,
    /// 服务端工具调用次数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_tool_use: Option<ServerToolUsage>,
    /// 实际生效的服务档位。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    /// 本次请求的推理速度模式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<Speed>,
    /// 本次推理所在区域。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_geo: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl Usage {
    /// 内部推理消耗的 token。
    pub fn thinking_tokens(&self) -> u32 {
        self.output_tokens_details
            .map(|details| details.thinking_tokens)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn usage_decodes_full_shape() {
        let usage: Usage = serde_json::from_value(json!({
            "input_tokens": 12,
            "output_tokens": 34,
            "cache_creation_input_tokens": 5,
            "cache_read_input_tokens": 6,
            "cache_creation": {"ephemeral_1h_input_tokens": 1, "ephemeral_5m_input_tokens": 4},
            "output_tokens_details": {"thinking_tokens": 20},
            "server_tool_use": {"web_search_requests": 2, "web_fetch_requests": 1},
            "service_tier": "standard",
            "speed": "fast",
            "inference_geo": "us"
        }))
        .expect("usage");

        assert_eq!(usage.input_tokens, Some(12));
        assert_eq!(usage.thinking_tokens(), 20);
        assert_eq!(
            usage
                .cache_creation
                .expect("cache creation")
                .ephemeral_5m_input_tokens,
            4
        );
        assert_eq!(
            usage
                .server_tool_use
                .expect("server tool")
                .web_search_requests,
            2
        );
        assert_eq!(usage.service_tier, Some(ServiceTier::Standard));
        assert_eq!(usage.speed, Some(Speed::Fast));
        assert_eq!(usage.inference_geo.as_deref(), Some("us"));
    }

    #[test]
    fn usage_accepts_null_input_tokens() {
        let usage: Usage = serde_json::from_value(json!({
            "input_tokens": null,
            "output_tokens": 3
        }))
        .expect("usage");
        assert_eq!(usage.input_tokens, None);
        assert_eq!(usage.output_tokens, 3);
    }

    #[test]
    fn usage_tolerates_unknown_service_tier() {
        let usage: Usage = serde_json::from_value(json!({
            "output_tokens": 1,
            "service_tier": "flex"
        }))
        .expect("usage");
        assert_eq!(
            usage.service_tier,
            Some(ServiceTier::Other("flex".to_owned()))
        );
    }

    #[test]
    fn error_body_request_id_and_error_code() {
        let error = error_from_response(
            &ProviderId::anthropic(),
            StatusCode::BAD_REQUEST,
            &HeaderMap::new(),
            br#"{"type":"error","request_id":"req_body","error":{"type":"invalid_request_error","message":"nope","details":{"error_code":"storage_quota_exceeded"}}}"#,
        );

        match error {
            AirxError::Provider(error) => {
                assert_eq!(error.code.as_deref(), Some("storage_quota_exceeded"));
                assert_eq!(error.request_id.as_deref(), Some("req_body"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn error_header_request_id_wins_over_body() {
        let mut headers = HeaderMap::new();
        headers.insert("request-id", "req_header".parse().expect("header"));
        let error = error_from_response(
            &ProviderId::anthropic(),
            StatusCode::BAD_REQUEST,
            &headers,
            br#"{"type":"error","request_id":"req_body","error":{"type":"invalid_request_error","message":"nope"}}"#,
        );

        match error {
            AirxError::Provider(error) => {
                assert_eq!(error.request_id.as_deref(), Some("req_header"));
                assert_eq!(error.code.as_deref(), Some("invalid_request_error"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}

//! 错误分类与错误形状。
//!
//! 本模块只定义与后端无关的错误**数据形状**：不引用 `reqwest` / `gloo` 等具体实现。
//! `airx-transport` 负责把后端错误映射为 [`TransportError`]，
//! `airx-protocol` 负责把错误响应体解析为 [`ProviderError`]。
//!
//! 判定（如是否可重试）只提供中立规则，最终策略由传输层结合幂等性决定。

use std::error::Error as StdError;
use std::fmt;
use std::time::Duration;

use crate::http::HeaderMap;
use crate::json::raw::RawJson;
use crate::model::ProviderId;

/// 被包装的底层错误。
///
/// WASM 下浏览器错误的 Future / Error 不保证 `Send + Sync`，
/// 因此这里按目标平台放宽约束，而不是强迫所有平台接受同一套约束。
#[cfg(not(target_arch = "wasm32"))]
pub type BoxError = Box<dyn StdError + Send + Sync + 'static>;
/// 被包装的底层错误（WASM：不要求 `Send + Sync`）。
#[cfg(target_arch = "wasm32")]
pub type BoxError = Box<dyn StdError + 'static>;

/// airx 的统一错误类型。
///
/// 每个变体保留原始信息（状态码、错误码、request id、原始响应体、底层 source），
/// 不做“把所有 API 失败折叠成一个字符串”的降级。
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AirxError {
    /// 连接、TLS、超时、取消等传输层失败。
    #[error(transparent)]
    Transport(#[from] TransportError),

    /// 非 2xx 且无法解析为 Provider 错误体的响应。
    #[error(transparent)]
    Http(#[from] HttpError),

    /// 请求序列化失败。
    #[error(transparent)]
    Serialization(#[from] SerializationError),

    /// 响应或事件反序列化失败。
    #[error(transparent)]
    Deserialization(#[from] DeserializationError),

    /// Provider 返回的可识别错误。
    ///
    /// 装箱以避免撑大整个 [`AirxError`]（Provider 错误携带 Header 与原始 body）。
    #[error(transparent)]
    Provider(Box<ProviderError>),

    /// SSE / 流状态错误。
    #[error(transparent)]
    Stream(#[from] StreamError),

    /// 客户端配置错误。
    #[error(transparent)]
    Configuration(#[from] ConfigurationError),
}

impl From<ProviderError> for AirxError {
    fn from(error: ProviderError) -> Self {
        Self::Provider(Box::new(error))
    }
}

impl AirxError {
    /// HTTP 状态码（若此错误由 HTTP 响应产生）。
    pub fn http_status(&self) -> Option<u16> {
        match self {
            Self::Http(err) => Some(err.status),
            Self::Provider(err) => err.status,
            _ => None,
        }
    }

    /// 中立的重试判定。
    ///
    /// 仅覆盖与幂等性无关的部分：连接失败、超时、429、5xx。
    /// 非幂等请求是否允许重试由 `airx-transport` 结合方法与业务语义决定。
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Transport(err) => err.is_retryable(),
            Self::Http(err) => is_retryable_status(err.status),
            Self::Provider(err) => err.status.is_some_and(is_retryable_status),
            _ => false,
        }
    }

    /// Provider 请求 ID（若有）。
    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::Provider(err) => err.request_id.as_deref(),
            _ => None,
        }
    }

    /// 是否被限流（HTTP 429）。
    pub fn is_rate_limit(&self) -> bool {
        self.http_status() == Some(429)
    }

    /// Provider 返回的 `Retry-After`（仅支持秒数形式）。
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Provider(err) => err.retry_after(),
            _ => None,
        }
    }

    /// Provider 错误响应头（若有）。
    pub fn response_headers(&self) -> Option<&HeaderMap> {
        match self {
            Self::Provider(err) => Some(&err.headers),
            _ => None,
        }
    }
}

fn is_retryable_status(status: u16) -> bool {
    status == 429 || (500..=599).contains(&status)
}

/// 传输层错误。
#[derive(Debug)]
pub struct TransportError {
    /// 失败类别。
    pub kind: TransportErrorKind,
    /// 人类可读描述（来自后端）。
    pub message: String,
    /// 底层错误，保留完整 source 链。
    pub source: Option<BoxError>,
}

impl TransportError {
    /// 创建传输错误。
    pub fn new(kind: TransportErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            source: None,
        }
    }

    /// 附加底层错误。
    #[must_use]
    pub fn with_source(mut self, source: BoxError) -> Self {
        self.source = Some(source);
        self
    }

    /// 连接失败或超时通常可重试。
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.kind,
            TransportErrorKind::Connect | TransportErrorKind::Timeout
        )
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl StdError for TransportError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_deref()
            .map(|err| err as &(dyn StdError + 'static))
    }
}

/// 传输失败类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TransportErrorKind {
    /// 建立连接失败。
    #[error("connection failed")]
    Connect,
    /// 请求超时。
    #[error("request timed out")]
    Timeout,
    /// TLS 握手或证书校验失败。
    #[error("TLS failure")]
    Tls,
    /// 代理配置或代理连接失败。
    #[error("proxy failure")]
    Proxy,
    /// 读取响应体失败。
    #[error("response body failure")]
    Body,
    /// 请求被调用方取消。
    #[error("request cancelled")]
    Cancelled,
    /// 其他传输失败。
    #[error("transport failure")]
    Other,
}

/// HTTP 层错误：响应非 2xx 且无法解析为 Provider 错误体。
#[derive(Debug)]
pub struct HttpError {
    /// HTTP 状态码。
    pub status: u16,
    /// 人类可读描述。
    pub message: String,
    /// 原始响应体，便于排查。
    pub body: Option<RawJson>,
}

impl HttpError {
    /// 创建 HTTP 错误。
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            body: None,
        }
    }

    /// 附加原始响应体。
    #[must_use]
    pub fn with_body(mut self, body: RawJson) -> Self {
        self.body = Some(body);
        self
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HTTP {}: {}", self.status, self.message)
    }
}

impl StdError for HttpError {}

/// Provider 返回的可识别错误。
///
/// 保留 `status` / `code` / `request_id` / 响应头 / 原始响应体，
/// 使调用方可以区分限流、参数错误、服务端故障与网络问题。
#[derive(Debug)]
pub struct ProviderError {
    /// 产生错误的 Provider。
    pub provider: ProviderId,
    /// HTTP 状态码（若可获取）。
    pub status: Option<u16>,
    /// Provider 错误码（若有）。
    pub code: Option<String>,
    /// 错误描述。
    pub message: String,
    /// Provider 请求 ID，便于对账。
    pub request_id: Option<String>,
    /// 错误响应头（限流信息、request id 等）。
    pub headers: HeaderMap,
    /// 原始错误响应体。
    pub raw_body: Option<RawJson>,
}

impl ProviderError {
    /// 创建 Provider 错误。
    pub fn new(provider: ProviderId, message: impl Into<String>) -> Self {
        Self {
            provider,
            status: None,
            code: None,
            message: message.into(),
            request_id: None,
            headers: HeaderMap::new(),
            raw_body: None,
        }
    }

    /// 是否被限流。
    pub fn is_rate_limited(&self) -> bool {
        self.status == Some(429)
    }

    /// `Retry-After`（仅支持秒数形式）。
    pub fn retry_after(&self) -> Option<Duration> {
        self.headers
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(Duration::from_secs)
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} error", self.provider)?;
        if let Some(status) = self.status {
            write!(f, " (HTTP {status})")?;
        }
        if let Some(code) = &self.code {
            write!(f, " [{code}]")?;
        }
        write!(f, ": {}", self.message)
    }
}

impl StdError for ProviderError {}

/// 请求序列化失败。
#[derive(Debug)]
pub struct SerializationError {
    /// 人类可读描述。
    pub message: String,
    /// 原始 serde 错误。
    pub source: Option<serde_json::Error>,
}

impl SerializationError {
    /// 以自定义描述创建。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }
}

impl From<serde_json::Error> for SerializationError {
    fn from(error: serde_json::Error) -> Self {
        Self {
            message: error.to_string(),
            source: Some(error),
        }
    }
}

impl fmt::Display for SerializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "serialization failed: {}", self.message)
    }
}

impl StdError for SerializationError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_ref()
            .map(|err| err as &(dyn StdError + 'static))
    }
}

/// 响应或事件反序列化失败。
#[derive(Debug)]
pub struct DeserializationError {
    /// 人类可读描述。
    pub message: String,
    /// 原始 serde 错误。
    pub source: Option<serde_json::Error>,
}

impl DeserializationError {
    /// 以自定义描述创建。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }
}

impl From<serde_json::Error> for DeserializationError {
    fn from(error: serde_json::Error) -> Self {
        Self {
            message: error.to_string(),
            source: Some(error),
        }
    }
}

impl fmt::Display for DeserializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "deserialization failed: {}", self.message)
    }
}

impl StdError for DeserializationError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_ref()
            .map(|err| err as &(dyn StdError + 'static))
    }
}

/// 流错误。
#[derive(Debug)]
pub struct StreamError {
    /// 错误类别。
    pub kind: StreamErrorKind,
    /// 人类可读描述。
    pub message: String,
}

impl StreamError {
    /// 以类别与描述创建。
    pub fn new(kind: StreamErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// SSE 语法错误。
    pub fn syntax(message: impl Into<String>) -> Self {
        Self::new(StreamErrorKind::Syntax, message)
    }

    /// 非法 UTF-8。
    pub fn utf8(error: std::str::Utf8Error) -> Self {
        Self::new(StreamErrorKind::Utf8, error.to_string())
    }

    /// 流被截断或事件未完整结束。
    pub fn incomplete(message: impl Into<String>) -> Self {
        Self::new(StreamErrorKind::Incomplete, message)
    }

    /// 协议层明确报告失败。
    pub fn failed(message: impl Into<String>) -> Self {
        Self::new(StreamErrorKind::Failed, message)
    }

    /// 状态机非预期转换。
    pub fn invalid_state(message: impl Into<String>) -> Self {
        Self::new(StreamErrorKind::InvalidState, message)
    }
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl StdError for StreamError {}

/// 流错误类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum StreamErrorKind {
    /// SSE 语法错误。
    #[error("SSE syntax error")]
    Syntax,
    /// 非法 UTF-8。
    #[error("invalid UTF-8")]
    Utf8,
    /// 流不完整。
    #[error("incomplete stream")]
    Incomplete,
    /// 协议层报告失败。
    #[error("stream failed")]
    Failed,
    /// 状态机处于非法状态。
    #[error("invalid stream state")]
    InvalidState,
}

/// 客户端配置错误。
#[derive(Debug, thiserror::Error)]
#[error("configuration error: {message}")]
pub struct ConfigurationError {
    /// 配置问题描述。
    pub message: String,
}

impl ConfigurationError {
    /// 创建配置错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_status_is_preserved_and_classified() {
        let err = AirxError::from(HttpError::new(429, "rate limited"));
        assert_eq!(err.http_status(), Some(429));
        assert!(err.is_retryable());

        let err = AirxError::from(HttpError::new(400, "bad request"));
        assert_eq!(err.http_status(), Some(400));
        assert!(!err.is_retryable());
    }

    #[test]
    fn provider_error_display_keeps_status_and_code() {
        let mut err = ProviderError::new(ProviderId::openai(), "invalid schema");
        err.status = Some(400);
        err.code = Some("invalid_request_error".to_owned());

        let text = AirxError::from(err).to_string();
        assert!(text.contains("openai"));
        assert!(text.contains("HTTP 400"));
        assert!(text.contains("invalid_request_error"));
        assert!(text.contains("invalid schema"));
    }

    #[test]
    fn transport_source_chain_is_kept() {
        let source = std::io::Error::other("connection reset");
        let err = TransportError::new(TransportErrorKind::Connect, "failed to connect")
            .with_source(Box::new(source));

        assert!(err.is_retryable());
        assert!(StdError::source(&err).is_some());
        assert!(err.to_string().contains("connection failed"));
    }

    #[test]
    fn cancellation_is_not_retryable() {
        let err = AirxError::from(TransportError::new(
            TransportErrorKind::Cancelled,
            "cancelled by caller",
        ));
        assert!(!err.is_retryable());
        assert_eq!(err.http_status(), None);
    }

    #[test]
    fn stream_error_kind_is_visible() {
        let err = StreamError::incomplete("event not terminated");
        assert_eq!(err.kind, StreamErrorKind::Incomplete);
        assert!(err.to_string().contains("incomplete stream"));
    }
}

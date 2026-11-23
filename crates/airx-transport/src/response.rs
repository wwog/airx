//! 流式响应与响应元数据。

use std::fmt;

use airx_core::http::{HeaderMap, StatusCode};

use crate::stream::ByteStream;

/// 已建立连接的流式响应。
///
/// 状态码与响应头在建立连接时即可读取；错误响应体仍由协议层解码，
/// 因此非 2xx 也返回流，而不是在传输层丢弃内容。
pub struct HttpStreamResponse {
    /// 状态码。
    pub status: StatusCode,
    /// 响应头。
    pub headers: HeaderMap,
    /// 响应体流。
    pub body: ByteStream,
}

impl HttpStreamResponse {
    /// 创建流式响应。
    pub fn new(status: StatusCode, headers: HeaderMap, body: ByteStream) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

    /// 是否为 2xx。
    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }
}

impl fmt::Debug for HttpStreamResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpStreamResponse")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("body", &"<stream>")
            .finish()
    }
}

/// 响应的 HTTP 元数据。
///
/// 保留状态码、响应头与 request id，便于限流处理与问题追踪。
#[derive(Debug, Clone)]
pub struct ResponseMetadata {
    /// 状态码。
    pub status: StatusCode,
    /// 响应头。
    pub headers: HeaderMap,
    /// 从常见 Header 中提取的 request id。
    pub request_id: Option<String>,
}

impl ResponseMetadata {
    /// 从状态码与响应头构建。
    pub fn new(status: StatusCode, headers: HeaderMap) -> Self {
        let request_id = extract_request_id(&headers);
        Self {
            status,
            headers,
            request_id,
        }
    }
}

/// 依次尝试常见 request id Header。
///
/// Provider 的专属 Header 由各协议层自行解析；这里只覆盖通用命名。
pub fn extract_request_id(headers: &HeaderMap) -> Option<String> {
    const CANDIDATES: [&str; 3] = ["x-request-id", "request-id", "x-amzn-requestid"];

    CANDIDATES.iter().find_map(|name| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use airx_core::http::HeaderValue;

    #[test]
    fn request_id_prefers_x_request_id() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", HeaderValue::from_static("req_123"));
        headers.insert("request-id", HeaderValue::from_static("other"));

        let metadata = ResponseMetadata::new(StatusCode::OK, headers);
        assert_eq!(metadata.request_id.as_deref(), Some("req_123"));
    }

    #[test]
    fn missing_request_id_is_none() {
        let metadata = ResponseMetadata::new(StatusCode::OK, HeaderMap::new());
        assert_eq!(metadata.request_id, None);
    }

    #[test]
    fn stream_response_reports_success() {
        let response = HttpStreamResponse::new(
            StatusCode::OK,
            HeaderMap::new(),
            crate::stream::byte_stream(vec![]),
        );
        assert!(response.is_success());
    }
}

//! 协议无关的 HTTP 数据结构。
//!
//! 这里只定义**数据形状**，不包含客户端、连接池、TLS 或超时逻辑：
//! `airx-protocol` 把请求编码成 [`HttpRequest`]，
//! `airx-transport` 读取 [`HttpRequest`] 并产出 [`HttpResponse`]。
//!
//! 两者因此只共享数据契约，不产生依赖关系。

use bytes::Bytes;

pub use ::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri};

/// 待执行的 HTTP 请求。
#[derive(Debug, Clone)]
pub struct HttpRequest {
    /// 请求方法。
    pub method: Method,
    /// 完整请求 URI。
    pub uri: Uri,
    /// 请求头。
    pub headers: HeaderMap,
    /// 请求体。
    pub body: Bytes,
}

impl HttpRequest {
    /// 创建空 body 的请求。
    pub fn new(method: Method, uri: Uri) -> Self {
        Self {
            method,
            uri,
            headers: HeaderMap::new(),
            body: Bytes::new(),
        }
    }

    /// 创建 GET 请求。
    pub fn get(uri: Uri) -> Self {
        Self::new(Method::GET, uri)
    }

    /// 创建 POST 请求。
    pub fn post(uri: Uri) -> Self {
        Self::new(Method::POST, uri)
    }

    /// 追加 / 覆盖请求头。
    #[must_use]
    pub fn with_header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.headers.insert(name, value);
        self
    }

    /// 设置请求体。
    #[must_use]
    pub fn with_body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = body.into();
        self
    }
}

/// 已完成的 HTTP 响应。
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// 状态码。
    pub status: StatusCode,
    /// 响应头。
    pub headers: HeaderMap,
    /// 响应体。
    pub body: Bytes,
}

impl HttpResponse {
    /// 创建响应。
    pub fn new(status: StatusCode, headers: HeaderMap, body: impl Into<Bytes>) -> Self {
        Self {
            status,
            headers,
            body: body.into(),
        }
    }

    /// 是否为 2xx。
    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_carries_method_headers_and_body() {
        let uri: Uri = "https://api.example.com/v1/messages".parse().expect("uri");
        let request = HttpRequest::post(uri)
            .with_header(
                HeaderName::from_static("content-type"),
                HeaderValue::from_static("application/json"),
            )
            .with_body(r#"{"model":"m"}"#);

        assert_eq!(request.method, Method::POST);
        assert_eq!(
            request.headers.get("content-type").expect("header"),
            "application/json"
        );
        assert_eq!(&request.body[..], br#"{"model":"m"}"#);
    }

    #[test]
    fn response_classifies_success() {
        let response = HttpResponse::new(StatusCode::OK, HeaderMap::new(), Bytes::new());
        assert!(response.is_success());

        let response = HttpResponse::new(
            StatusCode::TOO_MANY_REQUESTS,
            HeaderMap::new(),
            Bytes::new(),
        );
        assert!(!response.is_success());
    }
}

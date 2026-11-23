//! Mock 传输：用于单元测试与端到端竖切。
//!
//! 特点：
//! - 预置响应队列，按顺序消费；
//! - 记录收到的请求，便于断言 URL / Header / Body；
//! - 支持完整响应、流式响应与错误注入；
//! - 克隆共享同一份状态，测试可同时持有客户端与断言句柄。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use airx_core::error::{TransportError, TransportErrorKind};
use airx_core::http::{HeaderMap, HeaderName, HeaderValue, HttpRequest, HttpResponse, StatusCode};
use bytes::{Bytes, BytesMut};

use crate::response::HttpStreamResponse;
use crate::stream::byte_stream;
use crate::transport::{BoxFuture, Transport};

/// 预置的传输响应。
#[derive(Debug)]
enum MockReply {
    Response(MockResponse),
    Error {
        kind: TransportErrorKind,
        message: String,
    },
}

#[derive(Debug)]
struct MockResponse {
    status: StatusCode,
    headers: HeaderMap,
    chunks: Vec<Bytes>,
}

#[derive(Debug, Default)]
struct MockState {
    replies: VecDeque<MockReply>,
    requests: Vec<HttpRequest>,
}

/// 可脚本化的传输实现。
#[derive(Debug, Clone, Default)]
pub struct MockTransport {
    state: Arc<Mutex<MockState>>,
}

impl MockTransport {
    /// 创建空 Mock。
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一个 JSON 响应。
    ///
    /// # Panics
    ///
    /// `status` 不是合法 HTTP 状态码时 panic（测试辅助，提前暴露用例错误）。
    pub fn push_json(&self, status: u16, body: impl Into<String>) -> &Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        );
        self.push_response(status, headers, Bytes::from(body.into()))
    }

    /// 追加一个完整响应。
    ///
    /// # Panics
    ///
    /// `status` 不是合法 HTTP 状态码时 panic。
    pub fn push_response(&self, status: u16, headers: HeaderMap, body: impl Into<Bytes>) -> &Self {
        self.lock()
            .replies
            .push_back(MockReply::Response(MockResponse {
                status: status_code(status),
                headers,
                chunks: vec![body.into()],
            }));
        self
    }

    /// 追加一个流式响应。
    ///
    /// # Panics
    ///
    /// `status` 不是合法 HTTP 状态码时 panic。
    pub fn push_stream(
        &self,
        status: u16,
        headers: HeaderMap,
        chunks: impl IntoIterator<Item = Bytes>,
    ) -> &Self {
        self.lock()
            .replies
            .push_back(MockReply::Response(MockResponse {
                status: status_code(status),
                headers,
                chunks: chunks.into_iter().collect(),
            }));
        self
    }

    /// 追加一个传输错误。
    pub fn push_error(&self, kind: TransportErrorKind, message: impl Into<String>) -> &Self {
        self.lock().replies.push_back(MockReply::Error {
            kind,
            message: message.into(),
        });
        self
    }

    /// 已收到的请求快照。
    pub fn requests(&self) -> Vec<HttpRequest> {
        self.lock().requests.clone()
    }

    /// 已收到的请求数量。
    pub fn request_count(&self) -> usize {
        self.lock().requests.len()
    }

    fn lock(&self) -> MutexGuard<'_, MockState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn next_reply(&self) -> Result<MockReply, TransportError> {
        self.lock()
            .replies
            .pop_front()
            .ok_or_else(|| TransportError::new(TransportErrorKind::Other, "Mock 未预置响应"))
    }
}

impl Transport for MockTransport {
    fn execute(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
        self.lock().requests.push(request);
        Box::pin(async move {
            match self.next_reply()? {
                MockReply::Error { kind, message } => Err(TransportError::new(kind, message)),
                MockReply::Response(response) => {
                    let mut body = BytesMut::new();
                    for chunk in response.chunks {
                        body.extend_from_slice(&chunk);
                    }
                    Ok(HttpResponse::new(
                        response.status,
                        response.headers,
                        body.freeze(),
                    ))
                }
            }
        })
    }

    fn execute_stream(
        &self,
        request: HttpRequest,
    ) -> BoxFuture<'_, Result<HttpStreamResponse, TransportError>> {
        self.lock().requests.push(request);
        Box::pin(async move {
            match self.next_reply()? {
                MockReply::Error { kind, message } => Err(TransportError::new(kind, message)),
                MockReply::Response(response) => Ok(HttpStreamResponse::new(
                    response.status,
                    response.headers,
                    byte_stream(response.chunks),
                )),
            }
        })
    }
}

fn status_code(status: u16) -> StatusCode {
    StatusCode::from_u16(status).expect("测试用例使用了非法 HTTP 状态码")
}

#[cfg(test)]
mod tests {
    use super::*;
    use airx_core::http::Uri;

    fn request() -> HttpRequest {
        HttpRequest::get("https://example.com/v1/chat".parse::<Uri>().expect("uri"))
    }

    #[tokio::test]
    async fn returns_scripted_responses_in_order_and_records_requests() {
        let mock = MockTransport::new();
        mock.push_json(200, r#"{"ok":true}"#);
        mock.push_json(200, r#"{"ok":false}"#);

        let first = mock.execute(request()).await.expect("first");
        assert_eq!(&first.body[..], br#"{"ok":true}"#);
        assert_eq!(mock.request_count(), 1);

        let second = mock.execute(request()).await.expect("second");
        assert_eq!(&second.body[..], br#"{"ok":false}"#);
        assert_eq!(mock.request_count(), 2);

        assert_eq!(mock.requests()[0].uri.path(), "/v1/chat",);
    }

    #[tokio::test]
    async fn stream_response_yields_chunks() {
        use futures_util::StreamExt;

        let mock = MockTransport::new();
        mock.push_stream(
            200,
            HeaderMap::new(),
            [
                Bytes::from_static(b"data: 1\n\n"),
                Bytes::from_static(b"data: 2\n\n"),
            ],
        );

        let response = mock.execute_stream(request()).await.expect("stream");
        let chunks: Vec<_> = response.body.collect().await;
        assert_eq!(chunks.len(), 2);
        assert_eq!(&chunks[1].as_ref().expect("chunk")[..], b"data: 2\n\n");
    }

    #[tokio::test]
    async fn error_injection_is_returned() {
        let mock = MockTransport::new();
        mock.push_error(TransportErrorKind::Timeout, "timed out");

        let error = mock.execute(request()).await.expect_err("error");
        assert_eq!(error.kind, TransportErrorKind::Timeout);
    }

    #[tokio::test]
    async fn empty_queue_is_an_error() {
        let mock = MockTransport::new();
        let error = mock.execute(request()).await.expect_err("error");
        assert!(error.to_string().contains("未预置响应"));
    }
}

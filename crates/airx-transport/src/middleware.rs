//! 中间件。
//!
//! 当前只支持同步中间件：Header 注入、日志、脱敏等。需要异步操作
//! （例如刷新 Token）时应放在自定义 [`Transport`] 中。
//!
//! 流式响应的 `on_response` 不会被调用：响应体尚未读取，
//! 对 Header / 状态码的处理请在 `on_request` 或协议层完成。

use std::sync::Arc;

use airx_core::error::TransportError;
use airx_core::http::{HttpRequest, HttpResponse};

use crate::response::HttpStreamResponse;
use crate::transport::{BoxFuture, Transport};

/// 请求 / 响应中间件。
pub trait Middleware: Send + Sync {
    /// 发送前修改请求；按注册顺序调用。
    fn on_request(&self, request: &mut HttpRequest) -> Result<(), TransportError> {
        let _ = request;
        Ok(())
    }

    /// 收到完整响应后修改响应；按注册的逆序调用。
    fn on_response(&self, response: &mut HttpResponse) -> Result<(), TransportError> {
        let _ = response;
        Ok(())
    }
}

/// 应用中间件链的传输包装。
pub struct MiddlewareTransport<T> {
    inner: T,
    middleware: Vec<Arc<dyn Middleware>>,
}

impl<T> MiddlewareTransport<T> {
    /// 以给定顺序包装中间件。
    pub fn new(inner: T, middleware: Vec<Arc<dyn Middleware>>) -> Self {
        Self { inner, middleware }
    }

    /// 追加一个中间件。
    #[must_use]
    pub fn with(mut self, middleware: Arc<dyn Middleware>) -> Self {
        self.middleware.push(middleware);
        self
    }
}

impl<T: Transport> Transport for MiddlewareTransport<T> {
    fn execute(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
        Box::pin(async move {
            let mut request = request;
            for middleware in &self.middleware {
                middleware.on_request(&mut request)?;
            }

            let mut response = self.inner.execute(request).await?;

            for middleware in self.middleware.iter().rev() {
                middleware.on_response(&mut response)?;
            }
            Ok(response)
        })
    }

    fn execute_stream(
        &self,
        request: HttpRequest,
    ) -> BoxFuture<'_, Result<HttpStreamResponse, TransportError>> {
        Box::pin(async move {
            let mut request = request;
            for middleware in &self.middleware {
                middleware.on_request(&mut request)?;
            }
            self.inner.execute_stream(request).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airx_core::http::{HeaderName, HeaderValue, StatusCode, Uri};

    #[derive(Default)]
    struct Tag {
        name: &'static str,
        log: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl Middleware for Tag {
        fn on_request(&self, request: &mut HttpRequest) -> Result<(), TransportError> {
            request.headers.insert(
                HeaderName::from_static("x-middleware"),
                HeaderValue::from_static("applied"),
            );
            self.log
                .lock()
                .expect("lock")
                .push(format!("request:{}", self.name));
            Ok(())
        }

        fn on_response(&self, _response: &mut HttpResponse) -> Result<(), TransportError> {
            self.log
                .lock()
                .expect("lock")
                .push(format!("response:{}", self.name));
            Ok(())
        }
    }

    #[tokio::test]
    async fn middleware_runs_in_order_and_reverse_for_response() {
        use crate::mock::MockTransport;

        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mock = MockTransport::new();
        mock.push_json(200, "{}");
        let recorder = mock.clone();

        let transport = MiddlewareTransport::new(
            mock,
            vec![
                Arc::new(Tag {
                    name: "a",
                    log: log.clone(),
                }),
                Arc::new(Tag {
                    name: "b",
                    log: log.clone(),
                }),
            ],
        );

        let request = HttpRequest::get("https://example.com".parse::<Uri>().expect("uri"));

        let response = transport.execute(request).await.expect("response");
        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(
            log.lock().expect("lock").as_slice(),
            ["request:a", "request:b", "response:b", "response:a"]
        );

        // Header 修改确实作用到了下游 Transport 收到的请求上。
        let requests = recorder.requests();
        assert_eq!(requests[0].headers["x-middleware"], "applied");
    }
}

//! Transport 契约。
//!
//! 设计要点：
//!
//! - **对象安全**：`Client` 需要在运行时持有一个可替换的传输实现，
//!   因此方法返回 `BoxFuture`。只有传输边界付出一次装箱，协议与解析仍是纯函数。
//! - **不强制 `Send`**：Native 下要求 `Send + Sync`；WASM 下浏览器 Future
//!   不保证 `Send`，因此按目标平台放宽约束。
//! - **固定错误形状**：实现只负责把后端错误映射为
//!   [`airx_core::error::TransportError`]，不引入第二套错误体系。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use airx_core::error::TransportError;
use airx_core::http::{HttpRequest, HttpResponse};

use crate::response::HttpStreamResponse;

/// 对象安全的 boxed future。
#[cfg(not(target_arch = "wasm32"))]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// 对象安全的 boxed future（WASM：不要求 `Send`）。
#[cfg(target_arch = "wasm32")]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// 可替换的传输实现。
#[cfg(not(target_arch = "wasm32"))]
pub trait Transport: Send + Sync {
    /// 执行一次完整请求。
    fn execute(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>>;

    /// 执行一次流式请求，返回已建立连接的响应流。
    fn execute_stream(
        &self,
        request: HttpRequest,
    ) -> BoxFuture<'_, Result<HttpStreamResponse, TransportError>>;
}

/// 可替换的传输实现（WASM：不要求 `Send + Sync`）。
#[cfg(target_arch = "wasm32")]
pub trait Transport {
    /// 执行一次完整请求。
    fn execute(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>>;

    /// 执行一次流式请求，返回已建立连接的响应流。
    fn execute_stream(
        &self,
        request: HttpRequest,
    ) -> BoxFuture<'_, Result<HttpStreamResponse, TransportError>>;
}

impl<T: Transport + ?Sized> Transport for &T {
    fn execute(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
        T::execute(*self, request)
    }

    fn execute_stream(
        &self,
        request: HttpRequest,
    ) -> BoxFuture<'_, Result<HttpStreamResponse, TransportError>> {
        T::execute_stream(*self, request)
    }
}

impl<T: Transport + ?Sized> Transport for Arc<T> {
    fn execute(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
        T::execute(self.as_ref(), request)
    }

    fn execute_stream(
        &self,
        request: HttpRequest,
    ) -> BoxFuture<'_, Result<HttpStreamResponse, TransportError>> {
        T::execute_stream(self.as_ref(), request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airx_core::error::TransportErrorKind;
    use airx_core::http::{HeaderMap, StatusCode, Uri};

    struct EchoTransport;

    impl Transport for EchoTransport {
        fn execute(
            &self,
            _request: HttpRequest,
        ) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
            Box::pin(async {
                Ok(HttpResponse::new(
                    StatusCode::OK,
                    HeaderMap::new(),
                    b"ok".as_slice(),
                ))
            })
        }

        fn execute_stream(
            &self,
            _request: HttpRequest,
        ) -> BoxFuture<'_, Result<HttpStreamResponse, TransportError>> {
            Box::pin(async { Err(TransportError::new(TransportErrorKind::Other, "not used")) })
        }
    }

    fn request() -> HttpRequest {
        HttpRequest::get("https://example.com".parse::<Uri>().expect("uri"))
    }

    #[tokio::test]
    async fn transport_can_be_used_through_references_and_arc() {
        let transport = EchoTransport;

        let direct = Transport::execute(&&transport, request())
            .await
            .expect("direct");
        assert_eq!(&direct.body[..], b"ok");

        let shared = Arc::new(transport);
        let response = Transport::execute(&shared, request()).await.expect("arc");
        assert_eq!(&response.body[..], b"ok");
    }
}

//! 协议端点的执行组合。
//!
//! 这是 Protocol / Transport / Client 三个契约的汇合点：
//! 编码与解码来自 `airx-protocol`，IO 来自 `airx-transport`，
//! 本类型只负责顺序与错误转换。

use std::sync::Arc;

use airx_core::AirxError;
use airx_core::http::{HttpRequest, HttpResponse};
use airx_protocol::Endpoint;
#[cfg(feature = "stream")]
use airx_protocol::StreamingEndpoint;

use crate::executor::Executor;
#[cfg(feature = "stream")]
use crate::stream::{ProtocolStream, collect_body};

/// 绑定一个协议端点与执行器。
pub struct ApiClient<P> {
    endpoint: P,
    executor: Arc<Executor>,
}

impl<P> ApiClient<P> {
    #[cfg(any(feature = "openai", feature = "anthropic", feature = "gemini"))]
    pub(crate) fn new(endpoint: P, executor: Arc<Executor>) -> Self {
        Self { endpoint, executor }
    }

    /// 端点引用。
    pub fn endpoint(&self) -> &P {
        &self.endpoint
    }

    /// 直接发送预编码请求（逃生通道）。
    pub async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, AirxError> {
        self.executor.execute(request).await
    }
}

impl<P: Endpoint> ApiClient<P> {
    /// 编码 → 执行 → 解码。
    pub async fn create(&self, request: &P::Request) -> Result<P::Response, AirxError> {
        let http_request = self.endpoint.encode(request)?;
        let response = self.executor.execute(http_request).await?;
        self.endpoint.decode(response)
    }
}

#[cfg(feature = "stream")]
impl<P: StreamingEndpoint + Clone> ApiClient<P> {
    /// 编码 → 建立流 → 逐事件解码。
    ///
    /// 非 2xx 响应会先读完 body 并交给协议层解码，保证错误信息不丢失。
    pub async fn stream(&self, request: &P::Request) -> Result<ProtocolStream<P>, AirxError> {
        let http_request = self.endpoint.encode(request)?;
        let response = self.executor.execute_stream(http_request).await?;

        let status = response.status;
        let headers = response.headers;
        let body = response.body;

        if status.is_success() {
            return Ok(ProtocolStream::new(self.endpoint.clone(), body));
        }

        let body = collect_body(body).await?;
        let synthetic = HttpResponse::new(status, headers, body);
        match self.endpoint.decode(synthetic) {
            Ok(_) => Err(AirxError::Configuration(
                airx_core::error::ConfigurationError::new(
                    "非 2xx 流式响应被协议层意外解析为成功响应",
                ),
            )),
            Err(error) => Err(error),
        }
    }
}

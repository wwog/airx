//! 协议端点的执行组合。
//!
//! 这是 Protocol / Transport / Client 三个契约的汇合点：
//! 编码与解码来自 `airx-protocol`，IO 来自 `airx-transport`，
//! 本类型只负责顺序、Header、元数据与错误转换。

use std::sync::Arc;

use airx_core::AirxError;
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, header};
use airx_protocol::Endpoint;
#[cfg(feature = "stream")]
use airx_protocol::StreamingEndpoint;

use crate::executor::Executor;
use crate::options::RequestOptions;
#[cfg(feature = "stream")]
use crate::response::StreamMeta;
use crate::response::{ResponseMeta, WithMeta};
#[cfg(feature = "stream")]
use crate::stream::{ProtocolStream, collect_body};

/// 绑定一个协议端点与执行器。
#[derive(Clone)]
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

    pub(crate) fn executor(&self) -> Arc<Executor> {
        Arc::clone(&self.executor)
    }

    /// 直接发送预编码请求（逃生通道）。
    pub async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, AirxError> {
        self.executor.execute(request).await
    }
}

impl<P: Endpoint> ApiClient<P> {
    /// 编码 → 执行 → 解码。
    pub async fn create(&self, request: &P::Request) -> Result<P::Response, AirxError> {
        self.create_with(request, &RequestOptions::default()).await
    }

    /// 编码 → 执行 → 解码，并应用单次请求选项。
    pub async fn create_with(
        &self,
        request: &P::Request,
        options: &RequestOptions,
    ) -> Result<P::Response, AirxError> {
        let mut http_request = self.endpoint.encode(request)?;
        accept(&mut http_request, "application/json");
        let response = self.executor.execute_with(http_request, options).await?;
        self.endpoint.decode(response)
    }

    /// 与 [`ApiClient::create_with`] 相同，同时返回 HTTP 元数据。
    pub async fn create_with_meta(
        &self,
        request: &P::Request,
        options: &RequestOptions,
    ) -> Result<WithMeta<P::Response>, AirxError> {
        let mut http_request = self.endpoint.encode(request)?;
        accept(&mut http_request, "application/json");
        let response = self.executor.execute_with(http_request, options).await?;
        let meta = ResponseMeta::from_response(&response);
        let data = self.endpoint.decode(response)?;
        Ok(WithMeta { data, meta })
    }
}

#[cfg(feature = "stream")]
impl<P: StreamingEndpoint + Clone> ApiClient<P> {
    /// 编码 → 建立流 → 逐事件解码。
    ///
    /// 非 2xx 响应会先读完 body 并交给协议层解码，保证错误信息不丢失。
    pub async fn stream(&self, request: &P::Request) -> Result<ProtocolStream<P>, AirxError> {
        self.stream_with(request, &RequestOptions::default()).await
    }

    /// 与 [`ApiClient::stream`] 相同，并应用单次请求选项。
    pub async fn stream_with(
        &self,
        request: &P::Request,
        options: &RequestOptions,
    ) -> Result<ProtocolStream<P>, AirxError> {
        let mut http_request = self.endpoint.encode(request)?;
        accept(&mut http_request, "text/event-stream");
        let response = self
            .executor
            .execute_stream_with(http_request, options)
            .await?;

        let meta = StreamMeta::from_response(&response);
        let status = response.status;
        let headers = response.headers;
        let body = response.body;

        if status.is_success() {
            return Ok(ProtocolStream::new(self.endpoint.clone(), body, meta));
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

/// 不覆盖协议层或调用方已设置的 Accept。
fn accept(request: &mut HttpRequest, value: &'static str) {
    request
        .headers
        .entry(header::ACCEPT)
        .or_insert_with(|| HeaderValue::from_static(value));
}

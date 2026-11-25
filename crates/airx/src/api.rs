//! 协议端点的执行组合。
//!
//! 这是 Protocol / Transport / Client 三个契约的汇合点：
//! 编码与解码来自 `airx-protocol`，IO 来自 `airx-transport`，
//! 本类型只负责顺序、Header、元数据与错误转换。

use std::sync::Arc;

use airx_core::AirxError;
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, Method, StatusCode, header};
use airx_core::log::{LogLevel, Scope, Timer};
use airx_protocol::Endpoint;
#[cfg(feature = "stream")]
use airx_protocol::StreamingEndpoint;

use crate::executor::{Executor, path_of};
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

    #[cfg(any(feature = "openai", feature = "anthropic"))]
    pub(crate) fn executor(&self) -> Arc<Executor> {
        Arc::clone(&self.executor)
    }

    /// 直接发送预编码请求（逃生通道）。
    pub async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, AirxError> {
        self.executor.execute(request).await
    }

    /// 直接发送预编码请求，并应用单次请求选项。
    pub async fn execute_with(
        &self,
        request: HttpRequest,
        options: &RequestOptions,
    ) -> Result<HttpResponse, AirxError> {
        self.executor.execute_with(request, options).await
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
        let mut http_request = self.encode(request)?;
        accept(&mut http_request, "application/json");
        let context = CallContext::new(&http_request);

        let response = match self.executor.execute_with(http_request, options).await {
            Ok(response) => response,
            Err(error) => {
                self.log_call_failure(&context, &error);
                return Err(error);
            }
        };

        let status = response.status;
        let request_id = airx_transport::extract_request_id(&response.headers);
        match self.endpoint.decode(response) {
            Ok(data) => {
                self.log_call_success(&context, status, request_id.as_deref());
                Ok(data)
            }
            Err(error) => {
                self.log_protocol("decode", &error);
                self.log_call_failure(&context, &error);
                Err(error)
            }
        }
    }

    /// 与 [`ApiClient::create_with`] 相同，同时返回 HTTP 元数据。
    pub async fn create_with_meta(
        &self,
        request: &P::Request,
        options: &RequestOptions,
    ) -> Result<WithMeta<P::Response>, AirxError> {
        let mut http_request = self.encode(request)?;
        accept(&mut http_request, "application/json");
        let context = CallContext::new(&http_request);

        let response = match self.executor.execute_with(http_request, options).await {
            Ok(response) => response,
            Err(error) => {
                self.log_call_failure(&context, &error);
                return Err(error);
            }
        };

        let status = response.status;
        let meta = ResponseMeta::from_response(&response);
        match self.endpoint.decode(response) {
            Ok(data) => {
                self.log_call_success(&context, status, meta.request_id.as_deref());
                Ok(WithMeta { data, meta })
            }
            Err(error) => {
                self.log_protocol("decode", &error);
                self.log_call_failure(&context, &error);
                Err(error)
            }
        }
    }

    /// 编码请求；失败时记录 `protocol` 日志。
    fn encode(&self, request: &P::Request) -> Result<HttpRequest, AirxError> {
        self.endpoint.encode(request).inspect_err(|error| {
            self.log_protocol("encode", error);
        })
    }

    fn log_protocol(&self, phase: &'static str, error: &AirxError) {
        airx_core::airx_log!(
            self.executor.log(),
            Scope::PROTOCOL,
            LogLevel::Error,
            "protocol encode/decode failed",
            phase = phase,
            error = error,
        );
    }

    fn log_call_success(
        &self,
        context: &CallContext,
        status: StatusCode,
        request_id: Option<&str>,
    ) {
        airx_core::airx_log!(
            self.executor.log(),
            Scope::CALL,
            LogLevel::Info,
            "call completed",
            method = context.method,
            path = context.path,
            status = status.as_u16(),
            request_id = request_id.unwrap_or("-"),
            stream = false,
            elapsed = context.timer.elapsed(),
        );
    }

    fn log_call_failure(&self, context: &CallContext, error: &AirxError) {
        airx_core::airx_log!(
            self.executor.log(),
            Scope::CALL,
            LogLevel::Error,
            "call failed",
            method = context.method,
            path = context.path,
            stream = false,
            error = error,
            elapsed = context.timer.elapsed(),
        );
    }
}

/// 一次逻辑调用的日志上下文。
struct CallContext {
    method: Method,
    path: String,
    timer: Timer,
}

impl CallContext {
    fn new(request: &HttpRequest) -> Self {
        Self {
            method: request.method.clone(),
            path: path_of(request),
            timer: Timer::start(),
        }
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
        let mut http_request = self.encode_stream(request)?;
        accept(&mut http_request, "text/event-stream");
        let context = CallContext::new(&http_request);

        let response = match self
            .executor
            .execute_stream_with(http_request, options)
            .await
        {
            Ok(response) => response,
            Err(error) => {
                self.log_stream_call_failure(&context, &error);
                return Err(error);
            }
        };

        let meta = StreamMeta::from_response(&response);
        let status = response.status;
        let headers = response.headers;
        let body = response.body;

        if status.is_success() {
            airx_core::airx_log!(
                self.executor.log(),
                Scope::STREAM,
                LogLevel::Info,
                "stream opened",
                method = context.method,
                path = context.path,
                status = status.as_u16(),
                request_id = meta.request_id.as_deref().unwrap_or("-"),
            );
            return Ok(ProtocolStream::new(
                self.endpoint.clone(),
                body,
                meta,
                self.executor.log().clone(),
            ));
        }

        let body = collect_body(body).await?;
        let synthetic = HttpResponse::new(status, headers, body);
        self.executor.log_response_details(&synthetic);
        match self.endpoint.decode(synthetic) {
            Ok(_) => {
                let error = AirxError::Configuration(airx_core::error::ConfigurationError::new(
                    "非 2xx 流式响应被协议层意外解析为成功响应",
                ));
                self.log_stream_call_failure(&context, &error);
                Err(error)
            }
            Err(error) => {
                self.log_stream_call_failure(&context, &error);
                Err(error)
            }
        }
    }

    fn log_stream_call_failure(&self, context: &CallContext, error: &AirxError) {
        airx_core::airx_log!(
            self.executor.log(),
            Scope::CALL,
            LogLevel::Error,
            "call failed",
            method = context.method,
            path = context.path,
            stream = true,
            error = error,
            elapsed = context.timer.elapsed(),
        );
    }

    /// 编码流式请求；失败时记录 `protocol` 日志。
    fn encode_stream(&self, request: &P::Request) -> Result<HttpRequest, AirxError> {
        self.endpoint.encode_stream(request).inspect_err(|error| {
            self.log_protocol("encode_stream", error);
        })
    }
}

/// 不覆盖协议层或调用方已设置的 Accept。
fn accept(request: &mut HttpRequest, value: &'static str) {
    request
        .headers
        .entry(header::ACCEPT)
        .or_insert_with(|| HeaderValue::from_static(value));
}

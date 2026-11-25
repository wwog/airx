//! 请求执行器：组合 Transport、执行级默认 Header、单次请求选项与重试。

use std::sync::Arc;

use airx_core::AirxError;
#[cfg(target_arch = "wasm32")]
use airx_core::error::ConfigurationError;
use airx_core::error::TransportError;
use airx_core::http::{HeaderMap, HeaderName, HeaderValue, HttpRequest, HttpResponse, Method};
use airx_core::log::{LogHandle, LogLevel, Scope, Timer};
#[cfg(feature = "stream")]
use airx_transport::HttpStreamResponse;
use airx_transport::{RetryPolicy, Transport};

use crate::options::RequestOptions;

pub(crate) struct Executor {
    transport: Arc<dyn Transport>,
    default_headers: HeaderMap,
    retry_policy: RetryPolicy,
    log: LogHandle,
}

impl Executor {
    pub(crate) fn new(
        transport: Arc<dyn Transport>,
        default_headers: HeaderMap,
        retry_policy: RetryPolicy,
        log: LogHandle,
    ) -> Self {
        Self {
            transport,
            default_headers,
            retry_policy,
            log,
        }
    }

    /// 日志句柄；`call` / `protocol` / `stream` 日志点共用同一份配置。
    pub(crate) fn log(&self) -> &LogHandle {
        &self.log
    }

    pub(crate) async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, AirxError> {
        self.execute_with(request, &RequestOptions::default()).await
    }

    pub(crate) async fn execute_with(
        &self,
        request: HttpRequest,
        options: &RequestOptions,
    ) -> Result<HttpResponse, AirxError> {
        let request = self.prepare(request, options)?;
        let policy = options.get_retry_policy().unwrap_or(self.retry_policy);
        let future = self.execute_transport(request, policy);

        match options.get_timeout() {
            Some(timeout) => Ok(airx_transport::with_timeout(timeout, future).await??),
            None => future.await,
        }
    }

    #[cfg(feature = "stream")]
    pub(crate) async fn execute_stream_with(
        &self,
        request: HttpRequest,
        options: &RequestOptions,
    ) -> Result<HttpStreamResponse, AirxError> {
        let request = self.prepare(request, options)?;
        let method = request.method.clone();
        let path = path_of(&request);
        let timer = Timer::start();

        airx_core::airx_log!(
            self.log,
            Scope::NET,
            LogLevel::Info,
            "opening stream",
            method = method,
            path = path,
        );

        // 流式请求不重试；超时只覆盖建连阶段。
        let future = self.transport.execute_stream(request);

        let outcome = match options.get_timeout() {
            Some(timeout) => airx_transport::with_timeout(timeout, future).await?,
            None => future.await,
        };

        match &outcome {
            Ok(response) if response.status.is_success() => {
                airx_core::airx_log!(
                    self.log,
                    Scope::NET,
                    LogLevel::Info,
                    "stream established",
                    method = method,
                    path = path,
                    status = response.status.as_u16(),
                    elapsed = timer.elapsed(),
                );
            }
            Ok(response) => {
                airx_core::airx_log!(
                    self.log,
                    Scope::NET,
                    LogLevel::Warn,
                    "non-success stream response",
                    method = method,
                    path = path,
                    status = response.status.as_u16(),
                    elapsed = timer.elapsed(),
                );
            }
            Err(error) => {
                airx_core::airx_log!(
                    self.log,
                    Scope::NET,
                    LogLevel::Error,
                    "stream request failed",
                    method = method,
                    path = path,
                    kind = error.kind,
                    error = error,
                    elapsed = timer.elapsed(),
                );
            }
        }

        Ok(outcome?)
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn execute_transport(
        &self,
        request: HttpRequest,
        policy: RetryPolicy,
    ) -> Result<HttpResponse, AirxError> {
        let method = request.method.clone();
        let path = path_of(&request);
        let timer = Timer::start();

        airx_core::airx_log!(
            self.log,
            Scope::NET,
            LogLevel::Info,
            "sending request",
            method = method,
            path = path,
        );

        let outcome = if policy.is_enabled() {
            let retry = airx_transport::RetryTransport::new(self.transport.as_ref(), policy)
                .with_log(self.log.clone());
            retry.execute(request).await
        } else {
            self.transport.execute(request).await
        };

        self.log_net_outcome(&method, &path, timer, &outcome);
        Ok(outcome?)
    }

    #[cfg(target_arch = "wasm32")]
    async fn execute_transport(
        &self,
        request: HttpRequest,
        policy: RetryPolicy,
    ) -> Result<HttpResponse, AirxError> {
        if policy.is_enabled() {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "WASM 目标暂不支持自动重试",
            )));
        }

        let method = request.method.clone();
        let path = path_of(&request);
        let timer = Timer::start();

        airx_core::airx_log!(
            self.log,
            Scope::NET,
            LogLevel::Info,
            "sending request",
            method = method,
            path = path,
        );

        let outcome = self.transport.execute(request).await;
        self.log_net_outcome(&method, &path, timer, &outcome);
        Ok(outcome?)
    }

    /// 记录一次完整请求的结果。
    fn log_net_outcome(
        &self,
        method: &Method,
        path: &str,
        timer: Timer,
        outcome: &Result<HttpResponse, TransportError>,
    ) {
        match outcome {
            Ok(response) if response.status.is_success() => {
                airx_core::airx_log!(
                    self.log,
                    Scope::NET,
                    LogLevel::Info,
                    "response received",
                    method = method,
                    path = path,
                    status = response.status.as_u16(),
                    elapsed = timer.elapsed(),
                );
            }
            Ok(response) => {
                airx_core::airx_log!(
                    self.log,
                    Scope::NET,
                    LogLevel::Warn,
                    "non-success response",
                    method = method,
                    path = path,
                    status = response.status.as_u16(),
                    elapsed = timer.elapsed(),
                );
            }
            Err(error) => {
                airx_core::airx_log!(
                    self.log,
                    Scope::NET,
                    LogLevel::Error,
                    "request failed",
                    method = method,
                    path = path,
                    kind = error.kind,
                    error = error,
                    elapsed = timer.elapsed(),
                );
            }
        }
    }

    /// 应用 Header 的顺序：
    /// 1. 协议层 Header（已由 endpoint 写入）；
    /// 2. 执行级默认 Header（不覆盖已有值）；
    /// 3. 单次请求选项（覆盖）；
    /// 4. 缺省 User-Agent。
    fn prepare(
        &self,
        mut request: HttpRequest,
        options: &RequestOptions,
    ) -> Result<HttpRequest, AirxError> {
        for (name, value) in &self.default_headers {
            request
                .headers
                .entry(name.clone())
                .or_insert_with(|| value.clone());
        }

        options.apply(&mut request)?;

        request
            .headers
            .entry(HeaderName::from_static("user-agent"))
            .or_insert_with(|| {
                HeaderValue::from_static(concat!("airx/", env!("CARGO_PKG_VERSION")))
            });

        Ok(request)
    }
}

/// 只记录 path，避免把 query 中的密钥写进日志。
pub(crate) fn path_of(request: &HttpRequest) -> String {
    request.uri.path().to_owned()
}

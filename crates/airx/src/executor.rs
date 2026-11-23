//! 请求执行器：组合 Transport、执行级默认 Header 与单次请求选项。

use std::sync::Arc;

use airx_core::AirxError;
use airx_core::http::{HeaderMap, HeaderName, HeaderValue, HttpRequest, HttpResponse};
#[cfg(feature = "stream")]
use airx_transport::HttpStreamResponse;
use airx_transport::Transport;

use crate::options::RequestOptions;

pub(crate) struct Executor {
    transport: Arc<dyn Transport>,
    default_headers: HeaderMap,
}

impl Executor {
    pub(crate) fn new(transport: Arc<dyn Transport>, default_headers: HeaderMap) -> Self {
        Self {
            transport,
            default_headers,
        }
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
        Ok(self.transport.execute(request).await?)
    }

    #[cfg(feature = "stream")]
    pub(crate) async fn execute_stream_with(
        &self,
        request: HttpRequest,
        options: &RequestOptions,
    ) -> Result<HttpStreamResponse, AirxError> {
        let request = self.prepare(request, options)?;
        Ok(self.transport.execute_stream(request).await?)
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

//! 请求执行器：组合 Transport 与执行级默认 Header。

use std::sync::Arc;

use airx_core::AirxError;
use airx_core::http::{HeaderMap, HttpRequest, HttpResponse};
#[cfg(feature = "stream")]
use airx_transport::HttpStreamResponse;
use airx_transport::Transport;

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
        let request = self.apply_headers(request);
        Ok(self.transport.execute(request).await?)
    }

    #[cfg(feature = "stream")]
    pub(crate) async fn execute_stream(
        &self,
        request: HttpRequest,
    ) -> Result<HttpStreamResponse, AirxError> {
        let request = self.apply_headers(request);
        Ok(self.transport.execute_stream(request).await?)
    }

    /// 执行级默认 Header 不覆盖协议层写入的 Header。
    fn apply_headers(&self, mut request: HttpRequest) -> HttpRequest {
        for (name, value) in &self.default_headers {
            request
                .headers
                .entry(name.clone())
                .or_insert_with(|| value.clone());
        }
        request
    }
}

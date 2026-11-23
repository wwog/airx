//! Reqwest 传输实现（Native）。
//!
//! 只做 IO 与错误映射：不感知任何 Provider 语义，不构造认证 Header。

use std::error::Error as StdError;
use std::time::Duration;

use airx_core::error::{TransportError, TransportErrorKind};
use airx_core::http::{HttpRequest, HttpResponse};
use futures_util::StreamExt;

use crate::response::HttpStreamResponse;
use crate::transport::{BoxFuture, Transport};

/// 基于 Reqwest 的 Native 传输。
#[derive(Debug, Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// 使用默认配置创建。
    pub fn new() -> Result<Self, TransportError> {
        Self::builder().build()
    }

    /// 创建配置构建器。
    pub fn builder() -> ReqwestTransportBuilder {
        ReqwestTransportBuilder::default()
    }

    /// 使用外部构造好的 Reqwest Client（共享连接池 / 代理配置）。
    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }
}

/// [`ReqwestTransport`] 构建器。
#[derive(Debug, Default)]
pub struct ReqwestTransportBuilder {
    timeout: Option<Duration>,
}

impl ReqwestTransportBuilder {
    /// 设置整体请求超时。
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// 构建传输实现。
    pub fn build(self) -> Result<ReqwestTransport, TransportError> {
        let mut builder = reqwest::Client::builder();
        if let Some(timeout) = self.timeout {
            builder = builder.timeout(timeout);
        }
        let client = builder.build().map_err(map_reqwest_error)?;
        Ok(ReqwestTransport { client })
    }
}

impl Transport for ReqwestTransport {
    fn execute(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
        Box::pin(async move {
            tracing::debug!(
                method = %request.method,
                uri = %request.uri,
                "airx: sending request"
            );

            let response = self
                .client
                .execute(to_reqwest_request(request)?)
                .await
                .map_err(map_reqwest_error)?;

            let status = response.status();
            tracing::debug!(status = status.as_u16(), "airx: response received");
            let headers = response.headers().clone();
            let body = response.bytes().await.map_err(map_reqwest_error)?;

            Ok(HttpResponse::new(status, headers, body))
        })
    }

    fn execute_stream(
        &self,
        request: HttpRequest,
    ) -> BoxFuture<'_, Result<HttpStreamResponse, TransportError>> {
        Box::pin(async move {
            tracing::debug!(
                method = %request.method,
                uri = %request.uri,
                "airx: opening stream"
            );

            let response = self
                .client
                .execute(to_reqwest_request(request)?)
                .await
                .map_err(map_reqwest_error)?;

            let status = response.status();
            tracing::debug!(status = status.as_u16(), "airx: stream established");
            let headers = response.headers().clone();
            let stream = response
                .bytes_stream()
                .map(|chunk| chunk.map_err(map_reqwest_error));

            Ok(HttpStreamResponse::new(status, headers, Box::pin(stream)))
        })
    }
}

fn to_reqwest_request(request: HttpRequest) -> Result<reqwest::Request, TransportError> {
    let method = reqwest::Method::from_bytes(request.method.as_str().as_bytes())
        .map_err(|error| TransportError::new(TransportErrorKind::Other, error.to_string()))?;

    let url = reqwest::Url::parse(&request.uri.to_string())
        .map_err(|error| TransportError::new(TransportErrorKind::Other, error.to_string()))?;

    let mut outgoing = reqwest::Request::new(method, url);
    *outgoing.headers_mut() = request.headers;
    if !request.body.is_empty() {
        *outgoing.body_mut() = Some(reqwest::Body::from(request.body));
    }
    Ok(outgoing)
}

fn map_reqwest_error(error: reqwest::Error) -> TransportError {
    let kind = if error.is_timeout() {
        TransportErrorKind::Timeout
    } else if error.is_connect() {
        TransportErrorKind::Connect
    } else if error.is_body() {
        TransportErrorKind::Body
    } else if looks_like_tls(&error) {
        TransportErrorKind::Tls
    } else {
        TransportErrorKind::Other
    };

    let message = error.to_string();
    TransportError::new(kind, message).with_source(Box::new(error))
}

fn looks_like_tls(error: &reqwest::Error) -> bool {
    let mut current: Option<&(dyn StdError + 'static)> = Some(error);
    while let Some(source) = current {
        let text = source.to_string().to_ascii_lowercase();
        if text.contains("certificate") || text.contains("handshake") || text.contains("tls") {
            return true;
        }
        current = source.source();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_default_client() {
        assert!(ReqwestTransport::new().is_ok());
    }

    #[test]
    fn builds_with_timeout() {
        let transport = ReqwestTransport::builder()
            .timeout(Duration::from_secs(3))
            .build();
        assert!(transport.is_ok());
    }
}

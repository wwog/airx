//! 浏览器 Fetch 传输（WASM）。
//!
//! 与 Native 实现共享同一套数据契约；浏览器限制（CORS、受限 Header、
//! 部分 API 不支持直连）由服务端与调用方负责，本层不做规避。
//!
//! 流式实现基于 `fetch` 的 `ReadableStream`。

use airx_core::error::{TransportError, TransportErrorKind};
use airx_core::http::{HeaderMap, HeaderName, HeaderValue, HttpRequest, HttpResponse, StatusCode};
use bytes::Bytes;
use futures_util::StreamExt;

use crate::response::HttpStreamResponse;
use crate::transport::{BoxFuture, Transport};

/// 基于 `fetch` 的 WASM 传输。
#[derive(Debug, Clone, Copy, Default)]
pub struct WasmTransport;

impl WasmTransport {
    /// 创建传输实例。
    pub fn new() -> Self {
        Self
    }
}

impl Transport for WasmTransport {
    fn execute(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
        Box::pin(async move {
            let response = send(request).await?;
            let status = status_code(response.status())?;
            let headers = convert_headers(&response.headers());
            let body = response.binary().await.map_err(|error| {
                TransportError::new(TransportErrorKind::Body, format!("{error:?}"))
            })?;

            Ok(HttpResponse::new(status, headers, body))
        })
    }

    fn execute_stream(
        &self,
        request: HttpRequest,
    ) -> BoxFuture<'_, Result<HttpStreamResponse, TransportError>> {
        Box::pin(async move {
            let response = send(request).await?;
            let status = status_code(response.status())?;
            let headers = convert_headers(&response.headers());

            let Some(raw) = response.body() else {
                return Err(TransportError::new(
                    TransportErrorKind::Body,
                    "fetch 响应没有可读取的 body stream",
                ));
            };

            let stream = wasm_streams::ReadableStream::from_raw(raw)
                .into_stream()
                .map(|item| match item {
                    Ok(value) => {
                        let array = js_sys::Uint8Array::new(&value);
                        Ok(Bytes::from(array.to_vec()))
                    }
                    Err(error) => Err(TransportError::new(
                        TransportErrorKind::Body,
                        format!("fetch stream error: {error:?}"),
                    )),
                });

            Ok(HttpStreamResponse::new(status, headers, Box::pin(stream)))
        })
    }
}

async fn send(request: HttpRequest) -> Result<gloo_net::http::Response, TransportError> {
    let method = request
        .method
        .as_str()
        .parse::<gloo_net::http::Method>()
        .map_err(|error| {
            TransportError::new(
                TransportErrorKind::Other,
                format!("不支持的 HTTP 方法: {error}"),
            )
        })?;

    let mut builder = gloo_net::http::RequestBuilder::new(&request.uri.to_string()).method(method);

    for (name, value) in &request.headers {
        let value = value.to_str().map_err(|error| {
            TransportError::new(
                TransportErrorKind::Other,
                format!("Header 不是合法字符串: {error}"),
            )
        })?;
        builder = builder.header(name.as_str(), value);
    }

    let array = js_sys::Uint8Array::from(request.body.as_ref());
    builder
        .body(array)
        .map_err(|error| TransportError::new(TransportErrorKind::Other, format!("{error:?}")))?
        .send()
        .await
        .map_err(|error| TransportError::new(TransportErrorKind::Connect, format!("{error:?}")))
}

fn status_code(status: u16) -> Result<StatusCode, TransportError> {
    StatusCode::from_u16(status).map_err(|error| {
        TransportError::new(
            TransportErrorKind::Other,
            format!("非法状态码 {status}: {error}"),
        )
    })
}

fn convert_headers(headers: &gloo_net::http::Headers) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in headers.entries() {
        let Ok(name) = HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        let Ok(value) = HeaderValue::from_str(&value) else {
            continue;
        };
        map.append(name, value);
    }
    map
}

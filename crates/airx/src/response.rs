//! SDK 层响应元数据。
//!
//! 类型化响应只保留协议语义；状态码、响应头、request id 与原始 body
//! 通过 [`ResponseMeta`] 单独暴露，等价于官方 SDK 的 `withResponse()`。

use airx_core::AirxError;
use airx_core::error::DeserializationError;
use airx_core::http::{HeaderMap, HttpResponse, StatusCode};
#[cfg(feature = "stream")]
use airx_transport::HttpStreamResponse;
use airx_transport::extract_request_id;
use bytes::Bytes;
use serde::de::DeserializeOwned;

/// 非流式响应的 HTTP 元数据。
#[derive(Debug, Clone)]
pub struct ResponseMeta {
    /// 状态码。
    pub status: StatusCode,
    /// 响应头。
    pub headers: HeaderMap,
    /// 从常见 Header 中提取的 request id。
    pub request_id: Option<String>,
    /// 原始响应体。
    pub raw_body: Bytes,
}

impl ResponseMeta {
    pub(crate) fn from_response(response: &HttpResponse) -> Self {
        Self {
            status: response.status,
            headers: response.headers.clone(),
            request_id: extract_request_id(&response.headers),
            raw_body: response.body.clone(),
        }
    }
}

/// 流式响应建立连接时的元数据。
#[derive(Debug, Clone)]
pub struct StreamMeta {
    /// 状态码。
    pub status: StatusCode,
    /// 响应头。
    pub headers: HeaderMap,
    /// request id。
    pub request_id: Option<String>,
}

#[cfg(feature = "stream")]
impl StreamMeta {
    pub(crate) fn from_response(response: &HttpStreamResponse) -> Self {
        Self {
            status: response.status,
            headers: response.headers.clone(),
            request_id: extract_request_id(&response.headers),
        }
    }
}

/// 类型化数据 + HTTP 元数据。
#[derive(Debug, Clone)]
pub struct WithMeta<T> {
    /// 解码后的协议响应。
    pub data: T,
    /// 原始 HTTP 元数据。
    pub meta: ResponseMeta,
}

impl<T> WithMeta<T> {
    /// 只取类型化数据。
    pub fn into_data(self) -> T {
        self.data
    }
}

/// 把模型输出的文本解析为 `T`。
pub(crate) fn parse_json<T: DeserializeOwned>(text: &str) -> Result<T, AirxError> {
    if text.trim().is_empty() {
        return Err(AirxError::Deserialization(DeserializationError::new(
            "模型返回了空内容，无法解析为 JSON",
        )));
    }
    serde_json::from_str(text)
        .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))
}

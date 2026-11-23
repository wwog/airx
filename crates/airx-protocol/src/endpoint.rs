//! 协议端点契约。
//!
//! 只约定“如何编码 / 如何解码”，不包含 IO，也不绑定传输实现。
//!
//! 非流式与流式分成两个 trait：只有支持流式的协议才实现
//! [`StreamingEndpoint`]，避免给所有 API 挂上未使用的 `Event` 关联类型。

use airx_core::AirxError;
use airx_core::http::{HttpRequest, HttpResponse};
use airx_core::stream::RawEvent;

/// 一个可编码、可解码的协议调用。
pub trait Endpoint {
    /// 原生协议请求类型。
    type Request;
    /// 原生协议响应类型。
    type Response;

    /// 编码为 HTTP 请求（纯函数）。
    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError>;

    /// 从 HTTP 响应解码（纯函数）。
    fn decode(&self, response: HttpResponse) -> Result<Self::Response, AirxError>;
}

/// 支持流式的协议端点。
pub trait StreamingEndpoint: Endpoint {
    /// 原生事件类型。
    type Event;

    /// 解码单个原始 SSE 事件。
    ///
    /// 返回 `Ok(None)` 表示该事件在协议层可忽略（例如 ping / 注释）。
    fn decode_event(&self, event: RawEvent) -> Result<Option<Self::Event>, AirxError>;
}

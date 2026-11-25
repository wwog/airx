//! 协议端点契约。
//!
//! 只约定“如何编码 / 如何解码”，不包含 IO，也不绑定传输实现。
//!
//! 非流式与流式分成两个 trait：只有支持流式的协议才实现
//! [`StreamingEndpoint`]，避免给所有 API 挂上未使用的 `Event` 关联类型。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, SerializationError};
use airx_core::http::{HttpRequest, HttpResponse};
use airx_core::stream::RawEvent;
use serde::Serialize;
use serde_json::Value;

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
    ///
    /// 要求 `Debug`：`ProtocolStream` 在 `Trace` 级别会逐事件记录解析结果。
    type Event: std::fmt::Debug;

    /// 编码流式请求。
    ///
    /// 默认与非流式编码相同。流式标记写在请求体里的协议（OpenAI / Anthropic
    /// 的 `"stream": true`）在此覆盖；流式只由「调用哪个入口」决定，
    /// 请求类型本身不暴露 `stream` 字段。
    fn encode_stream(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        self.encode(request)
    }

    /// 解码单个原始 SSE 事件。
    ///
    /// 返回 `Ok(None)` 表示该事件在协议层可忽略（例如 ping / 注释）。
    fn decode_event(&self, event: RawEvent) -> Result<Option<Self::Event>, AirxError>;
}

/// 序列化请求体。
pub fn encode_json_body<T: Serialize>(request: &T) -> Result<Vec<u8>, AirxError> {
    serde_json::to_vec(request)
        .map_err(|error| AirxError::Serialization(SerializationError::from(error)))
}

/// 序列化请求体，并注入 wire 标记 `"stream": true`。
pub fn encode_json_stream_body<T: Serialize>(request: &T) -> Result<Vec<u8>, AirxError> {
    let mut value = serde_json::to_value(request)
        .map_err(|error| AirxError::Serialization(SerializationError::from(error)))?;
    let Value::Object(map) = &mut value else {
        return Err(AirxError::Configuration(ConfigurationError::new(
            "流式请求体必须是 JSON 对象",
        )));
    };
    map.insert("stream".to_owned(), Value::Bool(true));
    serde_json::to_vec(&value)
        .map_err(|error| AirxError::Serialization(SerializationError::from(error)))
}

//! 响应字节流类型。

use std::pin::Pin;

use airx_core::error::TransportError;
use bytes::Bytes;
use futures_core::Stream;

/// 流式响应体。
///
/// Native 目标要求 `Send`，便于在运行时中跨任务传递；
/// WASM 下浏览器流不保证 `Send`，因此不加该约束。
#[cfg(not(target_arch = "wasm32"))]
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, TransportError>> + Send + 'static>>;

/// 流式响应体（WASM：不要求 `Send`）。
#[cfg(target_arch = "wasm32")]
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, TransportError>> + 'static>>;

/// 把一组字节块包装成 [`ByteStream`]。
///
/// 供 Mock 与自定义 Transport 复用；真实实现应直接映射后端流。
pub fn byte_stream(chunks: Vec<Bytes>) -> ByteStream {
    Box::pin(futures_util::stream::iter(
        chunks.into_iter().map(Ok::<Bytes, TransportError>),
    ))
}

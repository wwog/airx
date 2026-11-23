//! airx 传输层。
//!
//! 唯一的 IO 层：HTTP 执行、流式读取、超时、取消、重试与中间件。
//!
//! 本 crate 不感知任何具体协议：认证 Header 由 `airx-protocol` 在编码阶段写入，
//! 错误响应体也由协议层解码。

pub mod middleware;
pub mod mock;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod native;
pub mod response;
pub mod retry;
pub mod stream;
pub mod timeout;
pub mod transport;
#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
pub mod wasm;

pub use middleware::{Middleware, MiddlewareTransport};
pub use mock::MockTransport;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub use native::{DEFAULT_CONNECT_TIMEOUT, ReqwestTransport, ReqwestTransportBuilder};
pub use response::{HttpStreamResponse, ResponseMetadata, extract_request_id};
pub use retry::RetryPolicy;
#[cfg(not(target_arch = "wasm32"))]
pub use retry::RetryTransport;
pub use stream::{ByteStream, byte_stream};
pub use timeout::with_timeout;
pub use transport::{BoxFuture, Transport};
#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
pub use wasm::WasmTransport;

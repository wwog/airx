//! airx 原生协议层。
//!
//! 按 `provider/api` 划分模块，提供强类型请求、响应、事件模型与纯编解码逻辑。
//!
//! 本 crate 不执行任何网络 IO；编码产物是
//! `airx_core::http::HttpRequest`，由 `airx-transport` 执行。

pub mod endpoint;

#[cfg(feature = "anthropic")]
pub mod anthropic;

#[cfg(feature = "gemini")]
pub mod gemini;

#[cfg(feature = "openai")]
pub mod openai;

pub use endpoint::{Endpoint, StreamingEndpoint};

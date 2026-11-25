//! airx 门面。
//!
//! 组合 `airx-protocol` 与 `airx-transport`，
//! 对外提供 Client、Builder、typed endpoint 与 prelude。

// 未启用任何 Provider 时，门面的大部分类型不会被构造；这是预期状态。
#![cfg_attr(
    not(any(feature = "openai", feature = "anthropic", feature = "gemini")),
    allow(dead_code)
)]

#[cfg(feature = "anthropic")]
mod anthropic;
mod api;
mod builder;
mod client;
mod executor;
#[cfg(feature = "gemini")]
mod gemini;
#[cfg(any(feature = "openai", feature = "anthropic"))]
mod multipart;
#[cfg(feature = "openai")]
mod openai;
mod options;
pub mod prelude;
mod response;
#[cfg(feature = "stream")]
mod stream;

pub use airx_core::AirxError;
pub use airx_core::airx_log;
pub use airx_core::http;
pub use airx_core::log::{LogConfig, LogHandle, LogLevel, Logger, Scope};
pub use airx_protocol::{Endpoint, StreamingEndpoint};
pub use airx_transport::{RetryPolicy, Transport};
// 公开 API 中直接出现的外部类型：统一从门面重导出，
// 调用方无需再直接依赖 `bytes` / `futures-core`。
pub use bytes::Bytes;
pub use futures_core::Stream;

/// 分级、按 scope 过滤的日志类型。
pub mod log {
    pub use airx_core::log::*;
}

#[cfg(feature = "anthropic")]
pub use anthropic::{Anthropic, AnthropicBatches, AnthropicFiles, AnthropicModels, Messages};
pub use api::ApiClient;
#[cfg(feature = "anthropic")]
pub use builder::AnthropicBuilder;
pub use builder::ClientBuilder;
#[cfg(feature = "gemini")]
pub use builder::GeminiBuilder;
#[cfg(feature = "openai")]
pub use builder::OpenAiBuilder;
pub use client::Client;
#[cfg(feature = "gemini")]
pub use gemini::{Gemini, GeminiCaches, GeminiEmbeddings, GeminiModels, GenerateContent};
#[cfg(feature = "openai")]
pub use openai::{
    Chat, OpenAi, OpenAiBatches, OpenAiEmbeddings, OpenAiFiles, OpenAiModels, OpenAiModerations,
    Responses,
};
#[cfg(any(feature = "openai", feature = "anthropic", feature = "gemini"))]
pub use options::RequestOptions;
#[cfg(any(feature = "openai", feature = "anthropic", feature = "gemini"))]
pub use response::{ResponseMeta, StreamMeta, WithMeta};
#[cfg(feature = "stream")]
pub use stream::ProtocolStream;

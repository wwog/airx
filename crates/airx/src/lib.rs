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
#[cfg(feature = "openai")]
mod openai;
mod options;
pub mod prelude;
mod response;
#[cfg(feature = "stream")]
mod stream;

pub use airx_core::AirxError;
pub use airx_protocol::{Endpoint, StreamingEndpoint};
pub use airx_transport::{RetryPolicy, Transport};

#[cfg(feature = "anthropic")]
pub use anthropic::{Anthropic, AnthropicModels, Messages};
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
pub use gemini::{Gemini, GeminiModels, GenerateContent};
#[cfg(feature = "openai")]
pub use openai::{Chat, OpenAi, OpenAiModels, Responses};
#[cfg(any(feature = "openai", feature = "anthropic", feature = "gemini"))]
pub use options::RequestOptions;
#[cfg(any(feature = "openai", feature = "anthropic", feature = "gemini"))]
pub use response::{ResponseMeta, StreamMeta, WithMeta};
#[cfg(feature = "stream")]
pub use stream::ProtocolStream;

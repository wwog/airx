//! airx 门面。
//!
//! 组合 `airx-protocol` 与 `airx-transport`，
//! 对外提供 Client、Builder、typed endpoint 与 prelude。

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
pub mod prelude;
#[cfg(feature = "stream")]
mod stream;

pub use airx_core::AirxError;
pub use airx_protocol::{Endpoint, StreamingEndpoint};
pub use airx_transport::{RetryPolicy, Transport};

#[cfg(feature = "anthropic")]
pub use anthropic::{Anthropic, Messages};
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
pub use gemini::{Gemini, GenerateContent};
#[cfg(feature = "openai")]
pub use openai::{Chat, OpenAi, Responses};
#[cfg(feature = "stream")]
pub use stream::ProtocolStream;

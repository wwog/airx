//! 常用类型的统一入口。
//!
//! ```ignore
//! use airx::prelude::*;
//!
//! let client = Client::builder()
//!     .openai()
//!     .api_key("sk-...")
//!     .build()?;
//!
//! let response = client
//!     .openai()?
//!     .chat()
//!     .create(ChatCompletionRequest::builder()
//!         .model("gpt-4o-mini")
//!         .user("Hello")
//!         .build()?)
//!     .await?;
//! # Ok::<(), AirxError>(())
//! ```

pub use airx_core::{AirxError, ApiVersion, ModelId, ProviderId};
#[cfg(feature = "anthropic")]
pub use airx_protocol::anthropic::messages::{
    JsonOutputFormat, MessagesEvent, MessagesRequest, MessagesResponse, MessagesStreamAccumulator,
    OutputConfig, OutputEffort, Role, ThinkingConfig, ThinkingDisplay,
};
#[cfg(feature = "deepseek")]
pub use airx_protocol::deepseek::{CompatibilityMode, DeepSeekPolicy};
#[cfg(all(feature = "openai", feature = "stream"))]
pub use airx_protocol::openai::chat::ChatStreamAccumulator;
#[cfg(feature = "openai")]
pub use airx_protocol::openai::chat::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ChatStreamEvent, Tool, ToolCall,
    ToolChoice,
};
#[cfg(feature = "openai")]
pub use airx_protocol::openai::common::{ReasoningEffort, Verbosity};
#[cfg(feature = "openai")]
pub use airx_protocol::openai::responses::{
    ResponsesEvent, ResponsesRequest, ResponsesResponse, ResponsesStreamAccumulator, TextFormat,
};
pub use airx_transport::RetryPolicy;
#[cfg(feature = "unified")]
pub use airx_unified::{
    Adapted, GenerateRequest, GenerateResponse, GenerationOptions, ProviderExtensions,
    Warning as CompatibilityWarning,
};

#[cfg(feature = "openai")]
pub use crate::OpenAiBuilder;
#[cfg(feature = "stream")]
pub use crate::ProtocolStream;
pub use crate::{ApiClient, Client, ClientBuilder};

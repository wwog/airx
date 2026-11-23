//! 常用类型的统一入口。
//!
//! ```ignore
//! use airx::prelude::*;
//!
//! // build() 直接得到已定性的 OpenAI 客户端，可复用。
//! let client = Client::openai().api_key("sk-...").build()?;
//!
//! let response = client
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
#[cfg(feature = "gemini")]
pub use airx_protocol::gemini::generate_content::{
    Candidate, Content, FunctionCallPart, FunctionCallingConfig, FunctionCallingMode,
    FunctionDeclaration, GenerateContentRequest, GenerateContentResponse,
    GenerateContentStreamAccumulator, GenerationConfig, Part, PromptFeedback, Role as GeminiRole,
    SafetySetting, ThinkingConfig as GeminiThinkingConfig, Tool as GeminiTool, ToolConfig,
};
#[cfg(feature = "gemini")]
pub use airx_protocol::gemini::{SafetyRating, UsageMetadata};
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

#[cfg(feature = "stream")]
pub use crate::ProtocolStream;
#[cfg(feature = "anthropic")]
pub use crate::{Anthropic, AnthropicBuilder};
pub use crate::{ApiClient, Client, ClientBuilder};
#[cfg(feature = "gemini")]
pub use crate::{Gemini, GeminiBuilder};
#[cfg(feature = "openai")]
pub use crate::{OpenAi, OpenAiBuilder};

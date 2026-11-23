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
    CountTokensRequest, CountTokensResponse, JsonOutputFormat, MessagesEvent, MessagesRequest,
    MessagesResponse, MessagesStreamAccumulator, OutputConfig, OutputEffort, Role, ThinkingConfig,
    ThinkingDisplay,
};
#[cfg(feature = "anthropic")]
pub use airx_protocol::anthropic::models::{
    ListModelsRequest as AnthropicListModelsRequest, Model as AnthropicModel,
};
#[cfg(feature = "gemini")]
pub use airx_protocol::gemini::generate_content::{
    Candidate, Content, FunctionCallPart, FunctionCallingConfig, FunctionCallingMode,
    FunctionDeclaration, GenerateContentRequest, GenerateContentResponse,
    GenerateContentStreamAccumulator, GenerationConfig, Part, PromptFeedback, Role as GeminiRole,
    SafetySetting, ThinkingConfig as GeminiThinkingConfig, Tool as GeminiTool, ToolConfig,
};
#[cfg(feature = "gemini")]
pub use airx_protocol::gemini::generate_content::{
    CountTokensRequest as GeminiCountTokensRequest,
    CountTokensResponse as GeminiCountTokensResponse,
};
#[cfg(feature = "gemini")]
pub use airx_protocol::gemini::models::{
    ListModelsRequest as GeminiListModelsRequest, Model as GeminiModel,
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
pub use airx_protocol::openai::models::{Model as OpenAiModel, ModelList as OpenAiModelList};
#[cfg(feature = "openai")]
pub use airx_protocol::openai::responses::{
    ResponsesEvent, ResponsesRequest, ResponsesResponse, ResponsesStreamAccumulator, TextFormat,
};
pub use airx_transport::RetryPolicy;

#[cfg(feature = "stream")]
pub use crate::ProtocolStream;
#[cfg(feature = "anthropic")]
pub use crate::{Anthropic, AnthropicBuilder, AnthropicModels};
pub use crate::{ApiClient, Client, ClientBuilder};
#[cfg(feature = "gemini")]
pub use crate::{Gemini, GeminiBuilder, GeminiModels};
#[cfg(feature = "openai")]
pub use crate::{OpenAi, OpenAiBuilder, OpenAiModels};
#[cfg(any(feature = "openai", feature = "anthropic", feature = "gemini"))]
pub use crate::{RequestOptions, ResponseMeta, StreamMeta, WithMeta};

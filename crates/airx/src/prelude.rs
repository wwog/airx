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
pub use airx_protocol::anthropic::batches::{
    BatchList as AnthropicBatchList, BatchRequestItem,
    CreateBatchRequest as AnthropicCreateBatchRequest,
    ListBatchesRequest as AnthropicListBatchesRequest, MessageBatch,
};
#[cfg(feature = "anthropic")]
pub use airx_protocol::anthropic::batches::{BatchResultLine, DeletedBatch};
#[cfg(feature = "anthropic")]
pub use airx_protocol::anthropic::files::{
    FileDeleteResponse as AnthropicFileDeleteResponse, FileList as AnthropicFileList, FileMetadata,
    ListFilesRequest as AnthropicListFilesRequest,
};
#[cfg(feature = "anthropic")]
pub use airx_protocol::anthropic::messages::{
    CacheControl, CacheKind, CacheTtl, ContentBlock, DocumentBlock, DocumentSource, StopReason,
    SystemPrompt, TextBlock, Tool as AnthropicTool, ToolChoice as AnthropicToolChoice,
};
#[cfg(feature = "anthropic")]
pub use airx_protocol::anthropic::messages::{
    CountTokensRequest, CountTokensResponse, JsonOutputFormat, MessagesEvent, MessagesRequest,
    MessagesResponse, MessagesStreamAccumulator, OutputConfig, OutputEffort, Role, ThinkingConfig,
    ThinkingDisplay,
};
#[cfg(feature = "anthropic")]
pub use airx_protocol::anthropic::models::ModelLifecycle;
#[cfg(feature = "anthropic")]
pub use airx_protocol::anthropic::models::{
    ListModelsRequest as AnthropicListModelsRequest, Model as AnthropicModel,
};
#[cfg(feature = "anthropic")]
pub use airx_protocol::anthropic::{
    AuthScheme as AnthropicAuthScheme, ServiceTier as AnthropicServiceTier, Usage,
};
#[cfg(feature = "gemini")]
pub use airx_protocol::gemini::caches::{
    CachedContent, CachedContentDelete, CachedContentList, CreateCachedContentRequest,
    ListCachedContentsRequest, UpdateCachedContentRequest,
};
#[cfg(feature = "gemini")]
pub use airx_protocol::gemini::embeddings::{
    BatchEmbedContentsRequest, BatchEmbedContentsResponse, BatchEmbedItem, EmbedContentRequest,
    EmbedContentResponse, Embedding,
};
#[cfg(feature = "gemini")]
pub use airx_protocol::gemini::generate_content::{
    AudioTranscriptionConfig, Candidate, Content, FinishReason, FunctionCallPart,
    FunctionCallingConfig, FunctionCallingMode, FunctionDeclaration, FunctionResponseMedia,
    GenerateContentRequest, GenerateContentResponse, GenerateContentStreamAccumulator,
    GenerationConfig, ImageConfig, Part, PromptFeedback, Role as GeminiRole, SafetySetting,
    ServiceTier as GeminiServiceTier, SpeechConfig, ThinkingConfig as GeminiThinkingConfig,
    Tool as GeminiTool, ToolConfig, TranslationConfig, VideoMetadata,
};
#[cfg(feature = "gemini")]
pub use airx_protocol::gemini::generate_content::{
    CountTokensRequest as GeminiCountTokensRequest,
    CountTokensResponse as GeminiCountTokensResponse,
};
#[cfg(feature = "gemini")]
pub use airx_protocol::gemini::models::{
    ComputeTokensRequest, ComputeTokensResponse, ListModelsRequest as GeminiListModelsRequest,
    Model as GeminiModel, TokensInfo,
};
#[cfg(feature = "gemini")]
pub use airx_protocol::gemini::{SafetyRating, UsageMetadata};
#[cfg(feature = "openai")]
pub use airx_protocol::openai::batches::{
    BatchList, BatchObject, BatchUsage, CreateBatchRequest, ListBatchesRequest,
};
#[cfg(all(feature = "openai", feature = "stream"))]
pub use airx_protocol::openai::chat::ChatStreamAccumulator;
#[cfg(feature = "openai")]
pub use airx_protocol::openai::chat::{
    ChatCompletionChunk, ChatCompletionListRequest, ChatCompletionRequest, ChatCompletionResponse,
    ChatCompletionUpdateRequest, ChatMessage, ChatStreamEvent, ContentPart, CustomToolCall,
    FilePart, InputAudio, JsonSchemaSpec, PromptCacheOptions, ResponseFormat, StreamOptions, Tool,
    ToolCall, ToolChoice,
};
#[cfg(feature = "openai")]
pub use airx_protocol::openai::common::{ReasoningEffort, Verbosity};
#[cfg(feature = "openai")]
pub use airx_protocol::openai::embeddings::{EmbeddingRequest, EmbeddingResponse, EmbeddingVector};
#[cfg(feature = "openai")]
pub use airx_protocol::openai::files::{
    ExpiresAfter, FileDeleteResponse, FileList, FileObject, ListFilesRequest, UploadFileRequest,
};
#[cfg(feature = "openai")]
pub use airx_protocol::openai::models::{
    Model as OpenAiModel, ModelDeleted, ModelList as OpenAiModelList,
};
#[cfg(feature = "openai")]
pub use airx_protocol::openai::moderations::{ModerationRequest, ModerationResponse};
#[cfg(feature = "openai")]
pub use airx_protocol::openai::responses::PromptCacheOptions as ResponsesPromptCacheOptions;
#[cfg(feature = "openai")]
pub use airx_protocol::openai::responses::{
    Annotation, CompactRequest, ContextManagement, InputContent, InputItem, InputItemList,
    InputMessageContent, InputTokensCount, InputTokensCountRequest, ListInputItemsRequest,
    OutputContent, OutputItem, Prompt, PromptCacheDiagnostics, ReasoningSummary, ResponsesEvent,
    ResponsesFunctionTool, ResponsesRequest, ResponsesResponse, ResponsesStreamAccumulator,
    ResponsesStreamOptions, ResponsesTool, ResponsesToolChoice, RetrieveResponseOptions,
    TextConfig, TextFormat, ToolUsage,
};
pub use airx_transport::RetryPolicy;

#[cfg(feature = "stream")]
pub use crate::ProtocolStream;
#[cfg(feature = "anthropic")]
pub use crate::{Anthropic, AnthropicBatches, AnthropicBuilder, AnthropicFiles, AnthropicModels};
pub use crate::{ApiClient, Client, ClientBuilder};
#[cfg(feature = "gemini")]
pub use crate::{Gemini, GeminiBuilder, GeminiCaches, GeminiEmbeddings, GeminiModels};
#[cfg(feature = "openai")]
pub use crate::{
    OpenAi, OpenAiBatches, OpenAiBuilder, OpenAiEmbeddings, OpenAiFiles, OpenAiModels,
};
#[cfg(any(feature = "openai", feature = "anthropic", feature = "gemini"))]
pub use crate::{RequestOptions, ResponseMeta, StreamMeta, WithMeta};

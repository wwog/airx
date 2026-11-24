//! Gemini `generateContent` API。

mod accumulator;
mod content;
mod count_tokens;
mod endpoint;
mod generation;
mod request;
mod response;
mod tool;

pub use accumulator::GenerateContentStreamAccumulator;
pub use content::{
    CodeExecutionResultPart, Content, ExecutableCodePart, FileDataPart, FunctionCallPart,
    FunctionResponseBlob, FunctionResponseFileData, FunctionResponseMedia, FunctionResponsePart,
    InlineDataPart, Part, Role, TextPart, VideoMetadata,
};
pub use count_tokens::{
    CountTokensEndpoint, CountTokensRequest, CountTokensRequestBuilder, CountTokensResponse,
};
pub(crate) use endpoint::model_path;
pub use endpoint::{
    GENERATE_CONTENT_SUFFIX, GenerateContentEndpoint, GenerateContentMode,
    STREAM_GENERATE_CONTENT_SUFFIX,
};
pub use generation::{
    AudioTranscriptionConfig, GenerationConfig, ImageConfig, PrebuiltVoiceConfig, SafetySetting,
    ServiceTier, SpeechConfig, ThinkingConfig, TranslationConfig, VoiceConfig,
};
pub use request::{GenerateContentRequest, GenerateContentRequestBuilder};
pub use response::{Candidate, FinishReason, GenerateContentResponse, PromptFeedback};
pub use tool::{FunctionCallingConfig, FunctionCallingMode, FunctionDeclaration, Tool, ToolConfig};

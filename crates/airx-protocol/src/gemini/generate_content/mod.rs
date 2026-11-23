//! Gemini `generateContent` API。

mod accumulator;
mod content;
mod endpoint;
mod generation;
mod request;
mod response;
mod tool;

pub use accumulator::GenerateContentStreamAccumulator;
pub use content::{
    CodeExecutionResultPart, Content, ExecutableCodePart, FileDataPart, FunctionCallPart,
    FunctionResponsePart, InlineDataPart, Part, Role, TextPart,
};
pub use endpoint::{
    GENERATE_CONTENT_SUFFIX, GenerateContentEndpoint, GenerateContentMode,
    STREAM_GENERATE_CONTENT_SUFFIX,
};
pub use generation::{GenerationConfig, SafetySetting, ThinkingConfig};
pub use request::{GenerateContentRequest, GenerateContentRequestBuilder};
pub use response::{Candidate, GenerateContentResponse, PromptFeedback};
pub use tool::{FunctionCallingConfig, FunctionCallingMode, FunctionDeclaration, Tool, ToolConfig};

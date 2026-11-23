//! OpenAI Chat Completions。

mod accumulator;
mod chunk;
mod endpoint;
mod event;
mod message;
mod request;
mod response;
mod tool;

pub use accumulator::ChatStreamAccumulator;
pub use chunk::{ChatCompletionChunk, ChunkChoice, ChunkDelta, ChunkFunctionCall, ChunkToolCall};
pub use endpoint::{CHAT_COMPLETIONS_PATH, ChatEndpoint};
pub use event::ChatStreamEvent;
pub use message::{ChatMessage, ContentPart, ImageUrl, ResponseMessage, UserContent};
pub use request::{ChatCompletionRequest, ChatCompletionRequestBuilder, StreamOptions};
pub use response::{ChatChoice, ChatCompletionResponse};
pub use tool::{
    FunctionCall, FunctionDefinition, JsonSchemaSpec, ResponseFormat, Tool, ToolCall, ToolChoice,
    ToolChoiceFunction, ToolChoiceFunctionName, ToolChoiceMode,
};

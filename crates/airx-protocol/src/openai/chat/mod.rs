//! OpenAI Chat Completions。

mod accumulator;
mod chunk;
mod completions;
mod endpoint;
mod event;
mod message;
mod request;
mod response;
mod tool;

pub use accumulator::ChatStreamAccumulator;
pub use chunk::{ChatCompletionChunk, ChunkChoice, ChunkDelta, ChunkFunctionCall, ChunkToolCall};
pub use completions::{
    ChatCompletionDeleteResponse, ChatCompletionList, ChatCompletionListRequest,
    ChatCompletionUpdateRequest, DeleteChatCompletionEndpoint, ListChatCompletionsEndpoint,
    RetrieveChatCompletionEndpoint, UpdateChatCompletionEndpoint,
};
pub use endpoint::{CHAT_COMPLETIONS_PATH, ChatEndpoint};
pub use event::ChatStreamEvent;
pub use message::{
    ChatMessage, ContentPart, FilePart, ImageUrl, InputAudio, ResponseMessage, UserContent,
};
pub use request::{
    ChatCompletionRequest, ChatCompletionRequestBuilder, PromptCacheOptions, StreamOptions,
};
pub use response::{ChatChoice, ChatCompletionResponse};
pub use tool::{
    CustomToolCall, FunctionCall, FunctionDefinition, FunctionTool, JsonSchemaSpec, ResponseFormat,
    Tool, ToolCall, ToolChoice, ToolChoiceFunction, ToolChoiceFunctionName, ToolChoiceMode,
};

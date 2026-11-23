//! Anthropic Messages API。

mod accumulator;
mod endpoint;
mod event;
mod message;
mod request;
mod response;

pub use accumulator::MessagesStreamAccumulator;
pub use endpoint::{MESSAGES_PATH, MessagesEndpoint};
pub use event::{
    ContentBlockDelta, ContentBlockStart, ContentBlockStop, Delta, ErrorBody, ErrorEvent,
    InputJsonDelta, MessageDelta, MessageDeltaEvent, MessageStart, MessageStop, MessagesEvent,
    Ping, SignatureDelta, TextDelta, ThinkingDelta,
};
pub use message::{
    ContentBlock, ImageBlock, MessageContent, RedactedThinkingBlock, Role, ServerToolUseBlock,
    TextBlock, ThinkingBlock, ToolResultBlock, ToolUseBlock, WebSearchToolResultBlock,
};
pub use request::{
    ClientTool, JsonOutputFormat, Message, MessagesRequest, MessagesRequestBuilder, OutputConfig,
    OutputEffort, ThinkingConfig, ThinkingDisplay, Tool, ToolChoice, WebSearchKind, WebSearchTool,
};
pub use response::MessagesResponse;

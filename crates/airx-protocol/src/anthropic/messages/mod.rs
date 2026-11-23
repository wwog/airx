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
    ContentBlock, ImageBlock, MessageContent, RedactedThinkingBlock, Role, TextBlock,
    ThinkingBlock, ToolResultBlock, ToolUseBlock,
};
pub use request::{
    Message, MessagesRequest, MessagesRequestBuilder, ThinkingConfig, Tool, ToolChoice,
};
pub use response::MessagesResponse;

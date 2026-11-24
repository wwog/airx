//! Anthropic Messages API。

mod accumulator;
mod count_tokens;
mod endpoint;
mod event;
mod message;
mod request;
mod response;

pub use accumulator::MessagesStreamAccumulator;
pub use count_tokens::{
    CountTokensEndpoint, CountTokensRequest, CountTokensRequestBuilder, CountTokensResponse,
};
pub use endpoint::{MESSAGES_PATH, MessagesEndpoint};
pub use event::{
    CitationsDelta, ContentBlockDelta, ContentBlockStart, ContentBlockStop, Delta, ErrorBody,
    ErrorEvent, InputJsonDelta, MessageDelta, MessageDeltaEvent, MessageStart, MessageStop,
    MessagesEvent, Ping, SignatureDelta, TextDelta, ThinkingDelta,
};
pub use message::{
    CacheControl, CacheKind, CacheTtl, CitationsConfig, ContentBlock, DocumentBlock,
    DocumentSource, ImageBlock, MessageContent, RedactedThinkingBlock, Role, ServerToolUseBlock,
    TextBlock, ThinkingBlock, ToolResultBlock, ToolUseBlock, WebSearchToolResultBlock,
};
pub use request::{
    ClientTool, ContainerParam, ContainerRef, DiagnosticsParam, JsonOutputFormat, Message,
    MessagesRequest, MessagesRequestBuilder, Metadata, OutputConfig, OutputEffort, SystemPrompt,
    ThinkingConfig, ThinkingDisplay, Tool, ToolChoice, WebSearchKind, WebSearchTool,
};
pub use response::{Container, Diagnostics, MessagesResponse, RefusalStopDetails, StopReason};

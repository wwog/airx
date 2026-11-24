//! OpenAI Responses API。

mod accumulator;
mod common;
mod endpoint;
mod event;
mod request;
mod resources;
mod response;
mod tool;

pub use accumulator::ResponsesStreamAccumulator;
pub use common::ReasoningSummary;
pub use endpoint::{RESPONSES_PATH, ResponsesEndpoint};
pub use event::{
    ArgumentsDelta, ArgumentsDone, CompletedEvent, ContentPartAdded, ContentPartDone, CreatedEvent,
    ErrorEvent, FailedEvent, IncompleteEvent, OutputItemAdded, OutputItemDone, OutputTextDelta,
    OutputTextDone, ReasoningSummaryDelta, ReasoningSummaryDone, RefusalDelta, RefusalDone,
    ResponsesEvent, ServerToolCallEvent,
};
pub use request::{
    ContextManagement, InputContent, InputItem, InputMessageContent, Prompt, PromptCacheOptions,
    Reasoning, ResponsesInput, ResponsesRequest, ResponsesRequestBuilder, ResponsesStreamOptions,
    TextConfig, TextFormat,
};
pub use resources::{
    CancelResponseEndpoint, CompactRequest, CompactRequestBuilder, CompactResponseEndpoint,
    CountInputTokensEndpoint, DeleteResponse, DeleteResponseEndpoint, InputItemList,
    InputTokensCount, InputTokensCountRequest, InputTokensCountRequestBuilder,
    ListInputItemsEndpoint, ListInputItemsRequest, RetrieveResponseEndpoint,
    RetrieveResponseOptions, RetrieveResponseRequest,
};
pub use response::{
    Annotation, CodeInterpreterCallItem, CompactionItem, ContainerFileCitation, CustomToolCallItem,
    FileCitation, FilePath, FileSearchCallItem, FunctionCallItem, ImageGenerationCallItem,
    McpCallItem, MessageItem, OutputContent, OutputItem, OutputText, PromptCacheDiagnostics,
    ReasoningItem, RefusalContent, ResponsesResponse, ResponsesUsage, ToolUsage, ToolUsageImageGen,
    ToolUsageWebSearch, UrlCitation, WebSearchCallItem,
};
pub use tool::{
    ResponsesFunctionTool, ResponsesTool, ResponsesToolChoice, SearchContextSize, WebSearchFilters,
    WebSearchKind, WebSearchTool,
};

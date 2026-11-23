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
    InputContent, InputItem, InputMessageContent, Reasoning, ResponsesInput, ResponsesRequest,
    ResponsesRequestBuilder, TextConfig, TextFormat,
};
pub use resources::{
    CancelResponseEndpoint, CountInputTokensEndpoint, DeleteResponse, DeleteResponseEndpoint,
    InputItemList, InputTokensCount, InputTokensCountRequest, InputTokensCountRequestBuilder,
    ListInputItemsEndpoint, ListInputItemsRequest, RetrieveResponseEndpoint,
};
pub use response::{
    Annotation, CodeInterpreterCallItem, FileSearchCallItem, FunctionCallItem,
    ImageGenerationCallItem, McpCallItem, MessageItem, OutputContent, OutputItem, OutputText,
    ReasoningItem, RefusalContent, ResponsesResponse, ResponsesUsage, UrlCitation,
    WebSearchCallItem,
};
pub use tool::{ResponsesTool, SearchContextSize, WebSearchFilters, WebSearchKind, WebSearchTool};

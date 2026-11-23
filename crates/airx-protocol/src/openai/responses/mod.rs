//! OpenAI Responses API。

mod accumulator;
mod endpoint;
mod event;
mod request;
mod response;
mod tool;

pub use accumulator::ResponsesStreamAccumulator;
pub use endpoint::{RESPONSES_PATH, ResponsesEndpoint};
pub use event::{
    ArgumentsDelta, ArgumentsDone, CompletedEvent, CreatedEvent, ErrorEvent, FailedEvent,
    IncompleteEvent, OutputItemAdded, OutputItemDone, OutputTextDelta, OutputTextDone,
    ResponsesEvent,
};
pub use request::{
    InputItem, Reasoning, ResponsesInput, ResponsesRequest, ResponsesRequestBuilder, TextConfig,
    TextFormat,
};
pub use response::{
    Annotation, FunctionCallItem, MessageItem, OutputContent, OutputItem, OutputText,
    RefusalContent, ResponsesResponse, ResponsesUsage, UrlCitation, WebSearchCallItem,
};
pub use tool::{ResponsesTool, SearchContextSize, WebSearchFilters, WebSearchKind, WebSearchTool};

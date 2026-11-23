//! OpenAI Responses API。

mod accumulator;
mod endpoint;
mod event;
mod request;
mod response;

pub use accumulator::ResponsesStreamAccumulator;
pub use endpoint::{RESPONSES_PATH, ResponsesEndpoint};
pub use event::{
    ArgumentsDelta, ArgumentsDone, CompletedEvent, CreatedEvent, ErrorEvent, FailedEvent,
    IncompleteEvent, OutputItemAdded, OutputItemDone, OutputTextDelta, OutputTextDone,
    ResponsesEvent,
};
pub use request::{InputItem, ResponsesInput, ResponsesRequest, ResponsesRequestBuilder};
pub use response::{
    FunctionCallItem, MessageItem, OutputContent, OutputItem, OutputText, RefusalContent,
    ResponsesResponse, ResponsesUsage,
};

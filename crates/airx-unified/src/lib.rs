//! airx 统一层（可选）。
//!
//! 跨 Provider 的统一请求、响应与内容模型，以及 Native ⇄ Unified 的纯函数转换。
//!
//! 本 crate 不执行 IO，也不依赖传输层：adapter 只做数据转换，
//! 真正的发送由 `airx` 组合完成。

pub mod adapter;
pub mod capability;
pub mod compatibility;
pub mod content;
pub mod request;
pub mod response;

pub use capability::capabilities;
pub use compatibility::{Adapted, CompatibilityMode, Warning};
pub use content::{Content, ImageContent, Message, Role, TokenUsage, ToolCall, ToolResult};
pub use request::{
    GenerateRequest, GenerateRequestBuilder, GenerationOptions, ProviderExtensions, Tool,
};
pub use response::GenerateResponse;

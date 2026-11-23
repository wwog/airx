//! OpenAI 协议共享类型。

pub mod error;
pub mod usage;

pub use error::{OpenAiErrorBody, OpenAiErrorResponse};
pub use usage::Usage;

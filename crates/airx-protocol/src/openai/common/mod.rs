//! OpenAI 协议共享类型。

pub mod error;
pub mod reasoning;
pub mod usage;

pub use error::{OpenAiErrorBody, OpenAiErrorResponse};
pub use reasoning::{ReasoningEffort, Verbosity};
pub use usage::Usage;

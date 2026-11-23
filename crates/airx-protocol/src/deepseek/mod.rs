//! DeepSeek 原生协议。
//!
//! DeepSeek 的 Chat 接口与 OpenAI 结构兼容，但**不是别名**：
//! 独立配置、独立端点、独立错误映射与兼容策略；
//! 仅在语义一致的字段上复用 `openai::chat` 的类型。

pub mod chat;
pub mod compatibility;
pub mod config;

pub use compatibility::{CompatibilityMode, DeepSeekPolicy};
pub use config::DeepSeekConfig;

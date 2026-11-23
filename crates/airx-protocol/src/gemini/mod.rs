//! Gemini 原生协议（Generative Language API）。
//!
//! 当前覆盖 `generateContent` 与 `streamGenerateContent`：
//! 请求 / 响应 / `parts` / 工具声明 / `generationConfig` / SSE 增量与聚合器。
//!
//! 认证使用 `x-goog-api-key`，默认基地址为
//! `https://generativelanguage.googleapis.com/v1beta`；兼容网关通过
//! `base_url` 覆盖。

pub mod common;
pub mod config;
pub mod embeddings;
pub mod generate_content;
pub mod models;

pub use common::{GeminiErrorBody, GeminiErrorResponse, SafetyRating, UsageMetadata};
pub use config::GeminiConfig;

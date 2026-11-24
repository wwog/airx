//! Gemini 原生协议（Generative Language API）。
//!
//! 当前覆盖 `generateContent` / `streamGenerateContent`、`countTokens`、
//! `computeTokens`、`embedContent` / `batchEmbedContents`、`models` 列表与详情，
//! 以及 `cachedContents` 的创建 / 读取 / 续期 / 删除。
//!
//! 认证使用 `x-goog-api-key`，默认基地址为
//! `https://generativelanguage.googleapis.com/v1beta`；兼容网关通过
//! `base_url` 覆盖。

pub mod caches;
pub mod common;
pub mod config;
pub mod embeddings;
pub mod generate_content;
pub mod models;

pub use caches::{
    CachedContent, CachedContentDelete, CachedContentList, CachedContentUsageMetadata,
    CreateCachedContentEndpoint, CreateCachedContentRequest, CreateCachedContentRequestBuilder,
    DeleteCachedContentEndpoint, GetCachedContentEndpoint, ListCachedContentsEndpoint,
    ListCachedContentsRequest, ListCachedContentsRequestBuilder, UpdateCachedContentEndpoint,
    UpdateCachedContentRequest,
};
pub use common::{GeminiErrorBody, GeminiErrorResponse, SafetyRating, UsageMetadata};
pub use config::GeminiConfig;

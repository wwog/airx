//! OpenAI 原生协议。
//!
//! `chat` 与（后续的）`responses` 是独立协议模块：二者的事件模型与
//! item 生命周期不同，不共享请求 / 响应类型。

pub mod chat;
pub mod common;
pub mod config;
pub mod models;
pub mod responses;

pub use config::OpenAiConfig;

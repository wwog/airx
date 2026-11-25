//! airx 基础层。
//!
//! 提供与 Provider 无关的基础设施：
//!
//! - [`error`]：统一错误分类与错误形状
//! - [`model`]：Provider / 模型 / API 版本标识
//! - [`http`]：协议无关的 HTTP 数据结构（无客户端）
//! - [`json`]：JSON 保真工具与输出模式
//! - [`log`]：按 scope 过滤的分级日志抽象（业务层注入实现）
//! - [`stream`]：SSE 语法、事件装配、流状态、增量 JSON
//!
//! 本 crate 不执行 IO，也不感知任何具体协议。

pub mod error;
pub mod http;
pub mod json;
pub mod log;
pub mod model;
pub mod multipart;
pub mod stream;

pub use error::AirxError;
pub use json::{JsonMode, RawJson};
pub use log::{LogConfig, LogHandle, LogLevel, Logger, Scope};
pub use model::{ApiVersion, ModelId, ProviderId};
pub use multipart::{MultipartForm, MultipartPart};

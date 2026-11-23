//! 流式基础设施。
//!
//! 三层职责，不可合并：
//!
//! - SSE 语法（本模块）：字节 → 行 → 字段 → [`RawEvent`]
//! - 协议事件（`airx-protocol`）：[`RawEvent`] → Provider 原生事件
//! - 网络读取（`airx-transport`）：响应字节流与背压
//!
//! 本模块只认 SSE 语法，不认 `[DONE]` 之类的协议约定。

pub mod buffer;
pub mod decoder;
pub mod json_accumulator;
pub mod sse;
pub mod status;

pub use buffer::LineBuffer;
pub use decoder::SseDecoder;
pub use json_accumulator::{JsonAccumulator, JsonState};
pub use sse::{RawEvent, SseField, parse_field};
pub use status::StreamStatus;

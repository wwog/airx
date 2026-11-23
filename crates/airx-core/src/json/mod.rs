//! JSON 保真工具。
//!
//! 四种表示按场景分工，不用一种类型硬扛所有需求：
//!
//! | 表示 | 用途 |
//! | --- | --- |
//! | 强类型 struct | 协议请求 / 响应 |
//! | [`serde_json::Value`] | 动态字段、未知结构探索 |
//! | [`RawJson`] | 原样转发、字节级保真 |
//! | `bytes::Bytes` | 原始网络数据 |
//!
//! 必须区分三个概念：语义保真、未知字段保留、字节级保真。
//! `Value` 不保证原始数字格式、空白和字段顺序，
//! 需要字节级保真时必须使用 [`RawJson`]。

pub mod format;
pub mod raw;
pub mod value;

pub use format::{JsonMode, to_string};
pub use raw::{RawExtension, RawJson, parse_raw, raw_from_value};
pub use value::Extensions;

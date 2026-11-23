//! 统一层 ↔ 原生协议转换。

#[cfg(feature = "anthropic")]
pub mod anthropic;
#[cfg(feature = "deepseek")]
pub mod deepseek;
#[cfg(feature = "openai")]
pub mod openai;

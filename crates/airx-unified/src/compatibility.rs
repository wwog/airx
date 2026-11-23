//! 兼容模式与显式 Warning。

#[cfg(any(feature = "openai", feature = "anthropic"))]
use airx_core::AirxError;
#[cfg(any(feature = "openai", feature = "anthropic"))]
use airx_core::error::CompatibilityError;

/// 兼容模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompatibilityMode {
    /// 目标协议无法表达即报错。
    #[default]
    Strict,
    /// 允许降级，并返回显式 Warning。
    BestEffort,
}

/// 降级警告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// 被降级或丢弃的内容说明。
    pub message: String,
}

impl Warning {
    /// 构造警告。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// 转换结果：原生请求 + 警告。
#[derive(Debug, Clone, PartialEq)]
pub struct Adapted<T> {
    /// 目标协议请求。
    pub native: T,
    /// 降级警告。
    pub warnings: Vec<Warning>,
}

impl<T> Adapted<T> {
    /// 创建结果。
    pub fn new(native: T) -> Self {
        Self {
            native,
            warnings: Vec::new(),
        }
    }
}

/// 处理不可表达能力：严格模式报错，宽松模式记录警告。
#[cfg(any(feature = "openai", feature = "anthropic"))]
pub(crate) fn unsupported(
    mode: CompatibilityMode,
    warnings: &mut Vec<Warning>,
    message: impl Into<String>,
) -> Result<(), AirxError> {
    let message = message.into();
    match mode {
        CompatibilityMode::Strict => {
            Err(AirxError::Compatibility(CompatibilityError::new(message)))
        }
        CompatibilityMode::BestEffort => {
            warnings.push(Warning::new(message));
            Ok(())
        }
    }
}

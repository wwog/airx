//! OpenAI 推理与输出控制参数。

use serde::{Deserialize, Serialize};

/// 推理投入等级。
///
/// 取值集合来自官方 SDK：`none` / `minimal` / `low` / `medium` / `high` / `xhigh` / `max`。
/// 具体模型支持哪些值由 Provider 决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    /// 不推理。
    None,
    /// 最低。
    Minimal,
    /// 低。
    Low,
    /// 中。
    Medium,
    /// 高。
    High,
    /// 极高。
    XHigh,
    /// 最大。
    Max,
}

/// 输出详尽程度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verbosity {
    /// 低。
    Low,
    /// 中。
    Medium,
    /// 高。
    High,
}

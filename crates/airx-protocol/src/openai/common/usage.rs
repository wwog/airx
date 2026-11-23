//! Token 用量。

use airx_core::json::Extensions;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Token 用量。
///
/// 各 Provider 的扩展字段（如缓存命中 token）进入 `extensions`，不丢失。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    /// 输入 token。
    #[serde(default)]
    pub prompt_tokens: u32,
    /// 输出 token。
    #[serde(default)]
    pub completion_tokens: u32,
    /// 总计。
    #[serde(default)]
    pub total_tokens: u32,
    /// 输入明细（各 Provider 结构不同）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<Value>,
    /// 输出明细。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens_details: Option<Value>,
    /// 未识别的 Provider 扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

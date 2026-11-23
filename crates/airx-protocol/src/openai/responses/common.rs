//! Responses 请求与响应共享的 item 片段。

use airx_core::json::Extensions;
use serde::{Deserialize, Serialize};

/// reasoning summary 片段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReasoningSummary {
    /// 片段类型，通常为 `summary_text`。
    #[serde(rename = "type", default = "summary_kind")]
    pub kind: String,
    /// 文本。
    pub text: String,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ReasoningSummary {
    /// 创建 `summary_text` 片段。
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            kind: summary_kind(),
            text: text.into(),
            extensions: Extensions::new(),
        }
    }
}

fn summary_kind() -> String {
    "summary_text".to_owned()
}

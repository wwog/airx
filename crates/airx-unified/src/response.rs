//! 统一响应模型。

use airx_core::model::ProviderId;

use crate::compatibility::Warning;
use crate::content::{Content, TokenUsage};

/// 统一生成响应。
#[derive(Debug, Clone, PartialEq)]
pub struct GenerateResponse {
    /// 产生响应的 Provider。
    pub provider: ProviderId,
    /// 响应 ID。
    pub id: Option<String>,
    /// 实际模型。
    pub model: Option<String>,
    /// 内容。
    pub content: Vec<Content>,
    /// 用量。
    pub usage: Option<TokenUsage>,
    /// 结束原因。
    pub finish_reason: Option<String>,
    /// 降级警告（BestEffort 模式下可能非空）。
    pub warnings: Vec<Warning>,
}

impl GenerateResponse {
    /// 拼接文本内容。
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|content| match content {
                Content::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    /// 所有工具调用。
    pub fn tool_calls(&self) -> Vec<&crate::content::ToolCall> {
        self.content
            .iter()
            .filter_map(|content| match content {
                Content::ToolCall(call) => Some(call),
                _ => None,
            })
            .collect()
    }
}

//! Anthropic Messages 响应。

use airx_core::json::Extensions;
use serde::{Deserialize, Serialize};

use crate::anthropic::common::Usage;

use super::message::ContentBlock;

/// Messages 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessagesResponse {
    /// 响应 ID。
    pub id: String,
    /// 对象类型（`message`）。
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// 角色。
    #[serde(default)]
    pub role: Option<String>,
    /// 模型。
    pub model: String,
    /// 内容块。
    #[serde(default)]
    pub content: Vec<ContentBlock>,
    /// 停止原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// 命中的停止序列。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    /// 用量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl MessagesResponse {
    /// 拼接全部文本块。
    pub fn text(&self) -> String {
        let mut text = String::new();
        for block in &self.content {
            if let ContentBlock::Text(block) = block {
                text.push_str(&block.text);
            }
        }
        text
    }

    /// 所有工具调用。
    pub fn tool_uses(&self) -> Vec<&super::message::ToolUseBlock> {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolUse(call) => Some(call),
                _ => None,
            })
            .collect()
    }
}

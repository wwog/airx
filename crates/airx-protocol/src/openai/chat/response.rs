//! Chat Completions 响应。

use airx_core::json::Extensions;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::usage::Usage;

use super::message::ResponseMessage;

/// Chat Completions 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    /// 响应 ID。
    pub id: String,
    /// 对象类型（通常为 `chat.completion`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 创建时间（Unix 秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<u64>,
    /// 实际使用的模型。
    pub model: String,
    /// 候选结果。
    pub choices: Vec<ChatChoice>,
    /// Token 用量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    /// 系统指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
    /// 未识别的扩展字段（如 `service_tier`）。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ChatCompletionResponse {
    /// 第一个候选的文本内容。
    pub fn first_content(&self) -> Option<&str> {
        self.choices.first()?.message.text()
    }

    /// 第一个候选的结束原因（`stop` / `length` / `tool_calls` / `content_filter` …）。
    pub fn finish_reason(&self) -> Option<&str> {
        self.choices.first()?.finish_reason.as_deref()
    }

    /// 是否因输出长度上限截断。
    pub fn is_truncated(&self) -> bool {
        self.choices.first().is_some_and(ChatChoice::is_truncated)
    }

    /// 是否请求了工具调用。
    pub fn is_tool_use(&self) -> bool {
        self.choices.first().is_some_and(ChatChoice::is_tool_use)
    }

    /// 是否因内容过滤结束。
    pub fn is_content_filtered(&self) -> bool {
        self.choices
            .first()
            .is_some_and(ChatChoice::is_content_filtered)
    }

    /// 是否返回了拒答（`refusal`）。
    pub fn is_refusal(&self) -> bool {
        self.choices
            .first()
            .is_some_and(|choice| choice.message.refusal.is_some())
    }
}

/// 单个候选。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatChoice {
    /// 候选序号。
    pub index: u32,
    /// 响应消息。
    pub message: ResponseMessage,
    /// 结束原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// 对数概率（结构随 Provider 而异）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ChatChoice {
    /// 是否因输出长度上限截断。
    pub fn is_truncated(&self) -> bool {
        self.finish_reason.as_deref() == Some("length")
    }

    /// 是否请求了工具调用（`tool_calls` 或旧版 `function_call`）。
    pub fn is_tool_use(&self) -> bool {
        matches!(
            self.finish_reason.as_deref(),
            Some("tool_calls" | "function_call")
        )
    }

    /// 是否因内容过滤结束。
    pub fn is_content_filtered(&self) -> bool {
        self.finish_reason.as_deref() == Some("content_filter")
    }
}

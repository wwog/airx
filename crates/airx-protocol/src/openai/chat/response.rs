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

//! Anthropic Messages 响应。

use airx_core::json::Extensions;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::anthropic::common::Usage;

use super::message::{ContentBlock, ToolUseBlock};

/// 停止原因。
///
/// 官方会新增取值，未识别值保留在 [`StopReason::Other`]。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StopReason {
    /// 自然结束。
    EndTurn,
    /// 达到 `max_tokens`。
    MaxTokens,
    /// 命中自定义停止序列。
    StopSequence,
    /// 模型调用了工具。
    ToolUse,
    /// 长时间运行的服务端工具轮次被暂停，需要原样回传后续跑。
    PauseTurn,
    /// 分类器介入拦截。
    Refusal,
    /// 超出模型上下文窗口。
    ModelContextWindowExceeded,
    /// 未识别的取值。
    Other(String),
}

impl StopReason {
    /// 解析字符串。
    pub fn parse(text: &str) -> Self {
        match text {
            "end_turn" => Self::EndTurn,
            "max_tokens" => Self::MaxTokens,
            "stop_sequence" => Self::StopSequence,
            "tool_use" => Self::ToolUse,
            "pause_turn" => Self::PauseTurn,
            "refusal" => Self::Refusal,
            "model_context_window_exceeded" => Self::ModelContextWindowExceeded,
            _ => Self::Other(text.to_owned()),
        }
    }

    /// 字符串形式。
    pub fn as_str(&self) -> &str {
        match self {
            Self::EndTurn => "end_turn",
            Self::MaxTokens => "max_tokens",
            Self::StopSequence => "stop_sequence",
            Self::ToolUse => "tool_use",
            Self::PauseTurn => "pause_turn",
            Self::Refusal => "refusal",
            Self::ModelContextWindowExceeded => "model_context_window_exceeded",
            Self::Other(value) => value,
        }
    }

    /// 是否属于截断（`max_tokens` / `model_context_window_exceeded`）。
    pub fn is_truncation(&self) -> bool {
        matches!(self, Self::MaxTokens | Self::ModelContextWindowExceeded)
    }
}

impl std::fmt::Display for StopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for StopReason {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for StopReason {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Ok(Self::parse(&text))
    }
}

/// 拒绝详情（`stop_details`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefusalStopDetails {
    /// 触发拒绝的策略分类（`cyber` / `bio` / `frontier_llm` / `reasoning_extraction`
    /// / `general_harms`），未归类时为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// 人工可读解释。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    /// 固定为 `refusal`。
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 容器信息（代码执行 / skills）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Container {
    /// 容器 ID（续跑时原样回传）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 过期时间（RFC 3339）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// 容器内加载的 skills（结构随官方演进，原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 请求级缓存诊断结果（`diagnostics`）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Diagnostics {
    /// 缓存未命中的原因（`null` 表示仍在后台比对）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_miss_reason: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl Diagnostics {
    /// 缓存未命中的原因类型（`model_changed` / `system_changed` / `tools_changed`
    /// / `messages_changed` / `previous_message_not_found` / `unavailable`）。
    pub fn cache_miss_reason_type(&self) -> Option<&str> {
        self.cache_miss_reason
            .as_ref()
            .and_then(|reason| reason.get("type"))
            .and_then(Value::as_str)
    }
}

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
    /// 停止原因（字符串保留原始值，判定用 [`MessagesResponse::stop_reason`]）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// 命中的停止序列。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    /// 停止详情（refusal 分类）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_details: Option<RefusalStopDetails>,
    /// 容器信息（使用容器类工具时非空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<Container>,
    /// 请求级缓存诊断。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<Diagnostics>,
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
    pub fn tool_uses(&self) -> Vec<&ToolUseBlock> {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolUse(call) => Some(call),
                _ => None,
            })
            .collect()
    }

    /// 停止原因（枚举判定；字段本身保留原始字符串）。
    pub fn stop_reason(&self) -> Option<StopReason> {
        self.stop_reason.as_deref().map(StopReason::parse)
    }

    /// 是否因工具调用而停止。
    pub fn is_tool_use(&self) -> bool {
        self.stop_reason() == Some(StopReason::ToolUse)
    }

    /// 是否被服务端暂停（`pause_turn`，需要把内容原样回传后续跑）。
    pub fn is_paused(&self) -> bool {
        self.stop_reason() == Some(StopReason::PauseTurn)
    }

    /// 是否被分类器拒绝（`refusal`，详情见 [`MessagesResponse::stop_details`]）。
    pub fn is_refusal(&self) -> bool {
        self.stop_reason() == Some(StopReason::Refusal)
    }

    /// 是否被截断（`max_tokens` / `model_context_window_exceeded`）。
    pub fn is_truncated(&self) -> bool {
        self.stop_reason()
            .is_some_and(|reason| reason.is_truncation())
    }

    /// 停止详情。
    pub fn stop_details(&self) -> Option<&RefusalStopDetails> {
        self.stop_details.as_ref()
    }

    /// 内部推理消耗的 token。
    pub fn thinking_tokens(&self) -> u32 {
        self.usage
            .as_ref()
            .map(Usage::thinking_tokens)
            .unwrap_or_default()
    }

    /// 容器 ID（续跑容器类工具时原样回传）。
    pub fn container_id(&self) -> Option<&str> {
        self.container
            .as_ref()
            .and_then(|container| container.id.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn response(stop_reason: &str) -> MessagesResponse {
        serde_json::from_value(json!({
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "model": "claude",
            "content": [{"type": "text", "text": "hi"}],
            "stop_reason": stop_reason
        }))
        .expect("response")
    }

    #[test]
    fn stop_reason_covers_official_values() {
        assert_eq!(
            response("end_turn").stop_reason(),
            Some(StopReason::EndTurn)
        );
        assert!(response("tool_use").is_tool_use());
        assert!(response("pause_turn").is_paused());
        assert!(response("refusal").is_refusal());
        assert!(response("max_tokens").is_truncated());
        assert!(response("model_context_window_exceeded").is_truncated());
        assert_eq!(
            response("stop_sequence").stop_reason(),
            Some(StopReason::StopSequence)
        );
        assert_eq!(
            response("future_reason").stop_reason(),
            Some(StopReason::Other("future_reason".to_owned()))
        );
    }

    #[test]
    fn stop_details_and_container_are_typed() {
        let response: MessagesResponse = serde_json::from_value(json!({
            "id": "msg_1",
            "model": "claude",
            "content": [],
            "stop_reason": "refusal",
            "stop_details": {"type": "refusal", "category": "cyber", "explanation": "nope"},
            "container": {"id": "container_1", "expires_at": "2026-11-24T00:00:00Z"},
            "usage": {
                "input_tokens": 1,
                "output_tokens": 2,
                "output_tokens_details": {"thinking_tokens": 9}
            }
        }))
        .expect("response");

        assert_eq!(
            response.stop_details().and_then(|d| d.category.as_deref()),
            Some("cyber")
        );
        assert_eq!(response.container_id(), Some("container_1"));
        assert_eq!(response.thinking_tokens(), 9);

        let value = serde_json::to_value(&response).expect("serialize");
        assert_eq!(value["stop_details"]["category"], "cyber");
        assert_eq!(value["container"]["id"], "container_1");
    }

    #[test]
    fn diagnostics_cache_miss_reason_is_readable() {
        let response: MessagesResponse = serde_json::from_value(json!({
            "id": "msg_1",
            "model": "claude",
            "content": [],
            "diagnostics": {"cache_miss_reason": {"type": "tools_changed"}}
        }))
        .expect("response");

        assert_eq!(
            response
                .diagnostics
                .as_ref()
                .and_then(Diagnostics::cache_miss_reason_type),
            Some("tools_changed")
        );
    }
}

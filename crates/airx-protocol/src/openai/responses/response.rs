//! OpenAI Responses 响应与输出 item。
//!
//! 未知 item / content 类型进入 `Other(Value)`，保证 Provider 新增类型时
//! 仍可透传，而不是反序列化失败。

use airx_core::json::Extensions;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Responses 用量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponsesUsage {
    /// 输入 token。
    #[serde(default)]
    pub input_tokens: u32,
    /// 输出 token。
    #[serde(default)]
    pub output_tokens: u32,
    /// 总计。
    #[serde(default)]
    pub total_tokens: u32,
    /// 输入明细。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens_details: Option<Value>,
    /// 输出明细。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens_details: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// Responses 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponsesResponse {
    /// 响应 ID。
    pub id: String,
    /// 对象类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 创建时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    /// 状态（`completed` / `in_progress` / `incomplete` / `failed`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// 模型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 输出 item。
    #[serde(default)]
    pub output: Vec<OutputItem>,
    /// 用量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<ResponsesUsage>,
    /// 不完整原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incomplete_details: Option<Value>,
    /// 失败信息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ResponsesResponse {
    /// 拼接全部 `output_text`。
    pub fn output_text(&self) -> String {
        let mut text = String::new();
        for item in &self.output {
            if let OutputItem::Message(message) = item {
                for content in &message.content {
                    if let OutputContent::OutputText(output) = content {
                        text.push_str(&output.text);
                    }
                }
            }
        }
        text
    }
}

/// 输出 item。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OutputItem {
    /// 消息。
    Message(MessageItem),
    /// 函数调用。
    FunctionCall(FunctionCallItem),
    /// 未识别类型（原样保留）。
    Other(Value),
}

/// 消息 item。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "message")]
pub struct MessageItem {
    /// item ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 角色。
    pub role: String,
    /// 内容。
    pub content: Vec<OutputContent>,
    /// 状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 消息内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OutputContent {
    /// 输出文本。
    OutputText(OutputText),
    /// 拒答。
    Refusal(RefusalContent),
    /// 未识别类型。
    Other(Value),
}

/// 输出文本。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "output_text")]
pub struct OutputText {
    /// 文本。
    pub text: String,
    /// 引用标注。
    #[serde(default)]
    pub annotations: Vec<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 拒答内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "refusal")]
pub struct RefusalContent {
    /// 拒答文本。
    pub refusal: String,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 函数调用 item。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "function_call")]
pub struct FunctionCallItem {
    /// item ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 调用 ID（回传给模型时使用）。
    pub call_id: String,
    /// 函数名。
    pub name: String,
    /// 参数 JSON 字符串。
    pub arguments: String,
    /// 状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_items_and_contents_are_preserved() {
        let json = json!({
            "id": "resp_1",
            "output": [
                {
                    "type": "message",
                    "role": "assistant",
                    "content": [
                        {"type": "output_text", "text": "hi", "annotations": []},
                        {"type": "future_content", "payload": 1}
                    ]
                },
                {"type": "web_search_call", "id": "ws_1"}
            ]
        });

        let response: ResponsesResponse = serde_json::from_value(json.clone()).expect("response");
        assert_eq!(response.output_text(), "hi");
        assert_eq!(serde_json::to_value(&response).expect("serialize"), json);
    }

    #[test]
    fn function_call_items_are_typed() {
        let item: OutputItem = serde_json::from_value(json!({
            "type": "function_call",
            "call_id": "call_1",
            "name": "lookup",
            "arguments": "{\"q\":1}"
        }))
        .expect("item");

        match item {
            OutputItem::FunctionCall(call) => {
                assert_eq!(call.call_id, "call_1");
                assert_eq!(call.name, "lookup");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}

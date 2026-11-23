//! OpenAI Responses 响应与输出 item。
//!
//! 未知 item / content 类型进入 `Other(Value)`，保证 Provider 新增类型时
//! 仍可透传，而不是反序列化失败。

use airx_core::json::Extensions;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use super::common::ReasoningSummary;

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
#[derive(Debug, Clone, PartialEq)]
pub enum OutputItem {
    /// 消息。
    Message(MessageItem),
    /// 函数调用。
    FunctionCall(FunctionCallItem),
    /// reasoning item（需要原样回传以保持推理状态）。
    Reasoning(ReasoningItem),
    /// 服务端联网搜索调用（OpenAI 后台执行）。
    WebSearchCall(WebSearchCallItem),
    /// 服务端文件搜索调用。
    FileSearchCall(FileSearchCallItem),
    /// 服务端代码解释器调用。
    CodeInterpreterCall(CodeInterpreterCallItem),
    /// 服务端图片生成调用。
    ImageGenerationCall(ImageGenerationCallItem),
    /// 服务端 MCP 调用。
    McpCall(McpCallItem),
    /// 未识别类型（原样保留）。
    Other(Value),
}

/// reasoning item。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReasoningItem {
    /// item ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 摘要片段。
    #[serde(default)]
    pub summary: Vec<ReasoningSummary>,
    /// 加密内容（多轮回传时保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    /// 状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 文件搜索调用 item。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileSearchCallItem {
    /// item ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// 查询列表。
    #[serde(default)]
    pub queries: Vec<String>,
    /// 结果（结构随官方演进，原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 代码解释器调用 item。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeInterpreterCallItem {
    /// item ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// 代码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// 结果（结构随官方演进，原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 图片生成调用 item。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageGenerationCallItem {
    /// item ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// base64 图片结果。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// 修订后的提示词。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revised_prompt: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// MCP 调用 item。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpCallItem {
    /// item ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// MCP server 标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_label: Option<String>,
    /// 工具名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 参数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
    /// 输出。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    /// 错误。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 消息 item。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq)]
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
pub struct OutputText {
    /// 文本。
    pub text: String,
    /// 引用标注。
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 拒答内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefusalContent {
    /// 拒答文本。
    pub refusal: String,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 函数调用 item。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// 服务端联网搜索调用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchCallItem {
    /// item ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// 搜索动作（查询、来源等，随官方结构演进，原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 文本引用标注。
#[derive(Debug, Clone, PartialEq)]
pub enum Annotation {
    /// URL 引用。
    UrlCitation(UrlCitation),
    /// 未识别标注。
    Other(Value),
}

/// URL 引用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UrlCitation {
    /// 来源 URL。
    pub url: String,
    /// 标题。
    pub title: String,
    /// 在文本中的起始位置。
    #[serde(default)]
    pub start_index: u32,
    /// 在文本中的结束位置。
    #[serde(default)]
    pub end_index: u32,
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

#[cfg(test)]
mod server_tool_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn web_search_call_and_url_citation_are_typed() {
        let json = json!({
            "id": "resp_1",
            "output": [
                {
                    "type": "web_search_call",
                    "id": "ws_1",
                    "status": "completed",
                    "action": {"type": "search", "query": "rust news"}
                },
                {
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": "rust news",
                        "annotations": [{
                            "type": "url_citation",
                            "url": "https://example.com/rust",
                            "title": "Rust",
                            "start_index": 0,
                            "end_index": 4
                        }]
                    }]
                }
            ]
        });

        let response: ResponsesResponse = serde_json::from_value(json.clone()).expect("response");
        assert!(matches!(response.output[0], OutputItem::WebSearchCall(_)));

        match &response.output[1] {
            OutputItem::Message(message) => match &message.content[0] {
                OutputContent::OutputText(text) => match &text.annotations[0] {
                    Annotation::UrlCitation(citation) => {
                        assert_eq!(citation.url, "https://example.com/rust");
                        assert_eq!(citation.end_index, 4);
                    }
                    other => panic!("unexpected annotation: {other:?}"),
                },
                other => panic!("unexpected content: {other:?}"),
            },
            other => panic!("unexpected item: {other:?}"),
        }

        assert_eq!(serde_json::to_value(&response).expect("serialize"), json);
    }
}

impl Serialize for OutputItem {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (kind, value) = match self {
            Self::Message(item) => ("message", serde_json::to_value(item)),
            Self::FunctionCall(item) => ("function_call", serde_json::to_value(item)),
            Self::Reasoning(item) => ("reasoning", serde_json::to_value(item)),
            Self::WebSearchCall(item) => ("web_search_call", serde_json::to_value(item)),
            Self::FileSearchCall(item) => ("file_search_call", serde_json::to_value(item)),
            Self::CodeInterpreterCall(item) => {
                ("code_interpreter_call", serde_json::to_value(item))
            }
            Self::ImageGenerationCall(item) => {
                ("image_generation_call", serde_json::to_value(item))
            }
            Self::McpCall(item) => ("mcp_call", serde_json::to_value(item)),
            Self::Other(value) => return value.serialize(serializer),
        };
        inject_type(kind, value, serializer)
    }
}

impl<'de> Deserialize<'de> for OutputItem {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();

        macro_rules! parse {
            ($ty:ty, $variant:ident) => {{
                remove_type(&mut value);
                return serde_json::from_value::<$ty>(value)
                    .map(Self::$variant)
                    .map_err(serde::de::Error::custom);
            }};
        }

        match kind.as_str() {
            "message" => parse!(MessageItem, Message),
            "function_call" => parse!(FunctionCallItem, FunctionCall),
            "reasoning" => parse!(ReasoningItem, Reasoning),
            "web_search_call" => parse!(WebSearchCallItem, WebSearchCall),
            "file_search_call" => parse!(FileSearchCallItem, FileSearchCall),
            "code_interpreter_call" => parse!(CodeInterpreterCallItem, CodeInterpreterCall),
            "image_generation_call" => parse!(ImageGenerationCallItem, ImageGenerationCall),
            "mcp_call" => parse!(McpCallItem, McpCall),
            _ => Ok(Self::Other(value)),
        }
    }
}

impl Serialize for OutputContent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (kind, value) = match self {
            Self::OutputText(content) => ("output_text", serde_json::to_value(content)),
            Self::Refusal(content) => ("refusal", serde_json::to_value(content)),
            Self::Other(value) => return value.serialize(serializer),
        };
        inject_type(kind, value, serializer)
    }
}

impl<'de> Deserialize<'de> for OutputContent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();

        macro_rules! parse {
            ($ty:ty, $variant:ident) => {{
                remove_type(&mut value);
                return serde_json::from_value::<$ty>(value)
                    .map(Self::$variant)
                    .map_err(serde::de::Error::custom);
            }};
        }

        match kind.as_str() {
            "output_text" => parse!(OutputText, OutputText),
            "refusal" => parse!(RefusalContent, Refusal),
            _ => Ok(Self::Other(value)),
        }
    }
}

impl Serialize for Annotation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (kind, value) = match self {
            Self::UrlCitation(citation) => ("url_citation", serde_json::to_value(citation)),
            Self::Other(value) => return value.serialize(serializer),
        };
        inject_type(kind, value, serializer)
    }
}

impl<'de> Deserialize<'de> for Annotation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();

        if kind == "url_citation" {
            remove_type(&mut value);
            return serde_json::from_value::<UrlCitation>(value)
                .map(Self::UrlCitation)
                .map_err(serde::de::Error::custom);
        }
        Ok(Self::Other(value))
    }
}

fn remove_type(value: &mut Value) {
    if let Value::Object(map) = value {
        map.remove("type");
    }
}

fn inject_type<S: Serializer>(
    kind: &str,
    value: Result<Value, serde_json::Error>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut value = value.map_err(serde::ser::Error::custom)?;
    if let Value::Object(map) = &mut value {
        map.insert("type".to_owned(), Value::String(kind.to_owned()));
    }
    value.serialize(serializer)
}

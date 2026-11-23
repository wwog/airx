//! Gemini `contents` / `parts` 内容模型。
//!
//! Gemini 的 `Part` 没有 `type` 判别字段：JSON 中出现的键即决定变体
//! （`text` / `inlineData` / `functionCall` …）。未知 Part 保留为
//! `Other(Value)`，不因新增字段而解码失败。

use airx_core::json::Extensions;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

/// 消息角色。
///
/// Gemini 使用 `user` 与 `model`；未知角色原样保留。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// 用户。
    User,
    /// 模型。
    Model,
    /// 未识别角色。
    Other(String),
}

impl Role {
    /// 用户角色。
    pub fn user() -> Self {
        Self::User
    }

    /// 模型角色。
    pub fn model() -> Self {
        Self::Model
    }

    /// 以 `&str` 返回角色。
    pub fn as_str(&self) -> &str {
        match self {
            Self::User => "user",
            Self::Model => "model",
            Self::Other(role) => role,
        }
    }
}

impl From<&str> for Role {
    fn from(value: &str) -> Self {
        match value {
            "user" => Self::User,
            "model" => Self::Model,
            other => Self::Other(other.to_owned()),
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Role {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Role {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from(value.as_str()))
    }
}

/// 一轮内容。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Content {
    /// 角色。`systemInstruction` 通常省略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// 内容片段。
    #[serde(default)]
    pub parts: Vec<Part>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl Content {
    /// 用户文本内容。
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: Some(Role::User),
            parts: vec![Part::text(text)],
            extensions: Extensions::new(),
        }
    }

    /// 模型文本内容。
    pub fn model_text(text: impl Into<String>) -> Self {
        Self {
            role: Some(Role::Model),
            parts: vec![Part::text(text)],
            extensions: Extensions::new(),
        }
    }

    /// 由片段构造内容。
    pub fn parts(role: Role, parts: Vec<Part>) -> Self {
        Self {
            role: Some(role),
            parts,
            extensions: Extensions::new(),
        }
    }

    /// 拼接非 thinking 的文本片段。
    pub fn text(&self) -> String {
        self.parts.iter().filter_map(Part::visible_text).collect()
    }

    /// 拼接 thinking 文本片段。
    pub fn thought_text(&self) -> String {
        self.parts.iter().filter_map(Part::thought_text).collect()
    }

    /// 所有工具调用。
    pub fn function_calls(&self) -> Vec<&FunctionCallPart> {
        self.parts
            .iter()
            .filter_map(|part| match part {
                Part::FunctionCall(call) => Some(call),
                _ => None,
            })
            .collect()
    }
}

/// 内容片段。
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    /// 文本（可带 thinking 标记）。
    Text(TextPart),
    /// 内联二进制（base64）。
    InlineData(InlineDataPart),
    /// 文件引用（Files API / GCS）。
    FileData(FileDataPart),
    /// 模型发起的工具调用。
    FunctionCall(FunctionCallPart),
    /// 回传给模型的工具结果。
    FunctionResponse(FunctionResponsePart),
    /// 代码执行生成的代码。
    ExecutableCode(ExecutableCodePart),
    /// 代码执行结果。
    CodeExecutionResult(CodeExecutionResultPart),
    /// 未识别片段。
    Other(Value),
}

impl Part {
    /// 文本片段。
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(TextPart {
            text: text.into(),
            thought: None,
            thought_signature: None,
            extensions: Extensions::new(),
        })
    }

    /// thinking 文本片段。
    pub fn thought(text: impl Into<String>) -> Self {
        Self::Text(TextPart {
            text: text.into(),
            thought: Some(true),
            thought_signature: None,
            extensions: Extensions::new(),
        })
    }

    /// 内联图片片段。
    pub fn inline_image(mime_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self::InlineData(InlineDataPart {
            mime_type: mime_type.into(),
            data: data.into(),
            extensions: Extensions::new(),
        })
    }

    /// 文件引用片段。
    pub fn file(mime_type: impl Into<String>, file_uri: impl Into<String>) -> Self {
        Self::FileData(FileDataPart {
            mime_type: mime_type.into(),
            file_uri: file_uri.into(),
            extensions: Extensions::new(),
        })
    }

    /// 工具调用片段。
    pub fn function_call(name: impl Into<String>, args: Value) -> Self {
        Self::FunctionCall(FunctionCallPart {
            id: None,
            name: name.into(),
            args,
            thought_signature: None,
            extensions: Extensions::new(),
        })
    }

    /// 工具结果片段。
    pub fn function_response(name: impl Into<String>, response: Value) -> Self {
        Self::FunctionResponse(FunctionResponsePart {
            id: None,
            name: name.into(),
            response,
            extensions: Extensions::new(),
        })
    }

    /// 可见文本（thinking 片段返回 `None`）。
    pub fn visible_text(&self) -> Option<&str> {
        match self {
            Self::Text(part) if !part.is_thought() => Some(&part.text),
            _ => None,
        }
    }

    /// thinking 文本。
    pub fn thought_text(&self) -> Option<&str> {
        match self {
            Self::Text(part) if part.is_thought() => Some(&part.text),
            _ => None,
        }
    }
}

/// 文本片段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextPart {
    /// 文本。
    pub text: String,
    /// 是否为 thinking 内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thought: Option<bool>,
    /// 思考签名（多轮工具调用时需回传）。
    #[serde(
        rename = "thoughtSignature",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub thought_signature: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl TextPart {
    /// 是否为 thinking 内容。
    pub fn is_thought(&self) -> bool {
        self.thought == Some(true)
    }
}

/// 内联二进制片段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InlineDataPart {
    /// MIME 类型。
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    /// base64 数据。
    pub data: String,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 文件引用片段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileDataPart {
    /// MIME 类型。
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    /// 文件 URI。
    #[serde(rename = "fileUri")]
    pub file_uri: String,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 工具调用片段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCallPart {
    /// 调用 ID（部分模型返回）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 工具名。
    pub name: String,
    /// 参数。
    #[serde(default = "empty_object")]
    pub args: Value,
    /// 思考签名（Gemini 3 要求回传）。
    #[serde(
        rename = "thoughtSignature",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub thought_signature: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 工具结果片段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionResponsePart {
    /// 对应的调用 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 工具名。
    pub name: String,
    /// 结果对象。
    #[serde(default = "empty_object")]
    pub response: Value,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 代码执行片段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutableCodePart {
    /// 语言（目前为 `PYTHON`）。
    pub language: String,
    /// 代码。
    pub code: String,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 代码执行结果片段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeExecutionResultPart {
    /// 结果状态（如 `OUTCOME_OK`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    /// 输出。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

fn empty_object() -> Value {
    Value::Object(Map::new())
}

impl Serialize for Part {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Text(part) => part.serialize(serializer),
            Self::InlineData(part) => serialize_wrapped("inlineData", part, serializer),
            Self::FileData(part) => serialize_wrapped("fileData", part, serializer),
            Self::FunctionCall(part) => serialize_wrapped("functionCall", part, serializer),
            Self::FunctionResponse(part) => serialize_wrapped("functionResponse", part, serializer),
            Self::ExecutableCode(part) => serialize_wrapped("executableCode", part, serializer),
            Self::CodeExecutionResult(part) => {
                serialize_wrapped("codeExecutionResult", part, serializer)
            }
            Self::Other(value) => value.serialize(serializer),
        }
    }
}

/// 把内部结构序列化为 `{ "<kind>": { ... } }`。
fn serialize_wrapped<S: Serializer, T: Serialize>(
    kind: &str,
    part: &T,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let value = serde_json::to_value(part).map_err(serde::ser::Error::custom)?;
    let mut outer = Map::new();
    outer.insert(kind.to_owned(), value);
    Value::Object(outer).serialize(serializer)
}

impl<'de> Deserialize<'de> for Part {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;

        macro_rules! parse {
            ($ty:ty, $variant:ident) => {
                serde_json::from_value::<$ty>(value)
                    .map(Self::$variant)
                    .map_err(serde::de::Error::custom)
            };
        }

        if value.get("text").is_some() {
            return parse!(TextPart, Text);
        }
        if value.get("inlineData").is_some() {
            return Self::inline_data(value).map_err(serde::de::Error::custom);
        }
        if value.get("fileData").is_some() {
            return Self::file_data(value).map_err(serde::de::Error::custom);
        }
        if value.get("functionCall").is_some() {
            return Self::function_call_part(value).map_err(serde::de::Error::custom);
        }
        if value.get("functionResponse").is_some() {
            return Self::function_response_part(value).map_err(serde::de::Error::custom);
        }
        if value.get("executableCode").is_some() {
            return Self::executable_code(value).map_err(serde::de::Error::custom);
        }
        if value.get("codeExecutionResult").is_some() {
            return Self::code_execution_result(value).map_err(serde::de::Error::custom);
        }

        Ok(Self::Other(value))
    }
}

impl Part {
    fn inner(value: Value, kind: &str) -> Result<Value, serde_json::Error> {
        let Value::Object(mut map) = value else {
            return Err(serde::de::Error::custom(format!("{kind} 必须是对象")));
        };
        Ok(map.remove(kind).unwrap_or(Value::Null))
    }

    fn inline_data(value: Value) -> Result<Self, serde_json::Error> {
        let inner = Self::inner(value, "inlineData")?;
        Ok(Self::InlineData(serde_json::from_value(inner)?))
    }

    fn file_data(value: Value) -> Result<Self, serde_json::Error> {
        let inner = Self::inner(value, "fileData")?;
        Ok(Self::FileData(serde_json::from_value(inner)?))
    }

    fn function_call_part(value: Value) -> Result<Self, serde_json::Error> {
        let inner = Self::inner(value, "functionCall")?;
        Ok(Self::FunctionCall(serde_json::from_value(inner)?))
    }

    fn function_response_part(value: Value) -> Result<Self, serde_json::Error> {
        let inner = Self::inner(value, "functionResponse")?;
        Ok(Self::FunctionResponse(serde_json::from_value(inner)?))
    }

    fn executable_code(value: Value) -> Result<Self, serde_json::Error> {
        let inner = Self::inner(value, "executableCode")?;
        Ok(Self::ExecutableCode(serde_json::from_value(inner)?))
    }

    fn code_execution_result(value: Value) -> Result<Self, serde_json::Error> {
        let inner = Self::inner(value, "codeExecutionResult")?;
        Ok(Self::CodeExecutionResult(serde_json::from_value(inner)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parts_roundtrip_all_variants() {
        let value = json!([
            {"text": "你好"},
            {"text": "思考", "thought": true, "thoughtSignature": "sig"},
            {"inlineData": {"mimeType": "image/png", "data": "AAA"}},
            {"fileData": {"mimeType": "application/pdf", "fileUri": "gs://b/f"}},
            {"functionCall": {"name": "lookup", "args": {"q": "rust"}}},
            {"functionResponse": {"name": "lookup", "response": {"ok": true}}},
            {"executableCode": {"language": "PYTHON", "code": "print(1)"}},
            {"codeExecutionResult": {"outcome": "OUTCOME_OK", "output": "1"}},
            {"futurePart": {"x": 1}}
        ]);

        let parts: Vec<Part> = serde_json::from_value(value.clone()).expect("parts");
        assert_eq!(parts.len(), 9);
        assert!(matches!(parts[0], Part::Text(_)));
        assert_eq!(parts[0].visible_text(), Some("你好"));
        assert_eq!(parts[1].thought_text(), Some("思考"));
        assert!(matches!(parts[2], Part::InlineData(_)));
        assert!(matches!(parts[3], Part::FileData(_)));
        assert_eq!(parts[4].visible_text(), None);
        assert!(matches!(parts[4], Part::FunctionCall(_)));
        assert!(matches!(parts[5], Part::FunctionResponse(_)));
        assert!(matches!(parts[6], Part::ExecutableCode(_)));
        assert!(matches!(parts[7], Part::CodeExecutionResult(_)));
        assert!(matches!(parts[8], Part::Other(_)));

        assert_eq!(serde_json::to_value(&parts).expect("serialize"), value);
    }

    #[test]
    fn content_helpers_skip_thought_text() {
        let content = Content {
            role: Some(Role::Model),
            parts: vec![
                Part::thought("先想一下"),
                Part::text("答案"),
                Part::function_call("lookup", json!({"q": "x"})),
            ],
            extensions: Extensions::new(),
        };

        assert_eq!(content.text(), "答案");
        assert_eq!(content.thought_text(), "先想一下");
        assert_eq!(content.function_calls()[0].name, "lookup");
    }

    #[test]
    fn unknown_role_is_preserved() {
        let role: Role = serde_json::from_value(json!("system")).expect("role");
        assert_eq!(role, Role::Other("system".to_owned()));
        assert_eq!(serde_json::to_value(&role).expect("serialize"), "system");
    }
}

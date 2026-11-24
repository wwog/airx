//! Gemini `contents` / `parts` 内容模型。
//!
//! Gemini 的 `Part` 没有 `type` 判别字段：JSON 中出现的键即决定变体
//! （`text` / `inlineData` / `functionCall` …）。未知 Part 保留为
//! `Other(Value)`，不因新增字段而解码失败。
//!
//! 官方 `Part` 是"一个 part 级对象 + 互斥的 data 成员"：`thoughtSignature`、
//! `videoMetadata`、`partMetadata`、`mediaResolution` 是 **part 级兄弟键**，
//! 不在 `functionCall` / `inlineData` 内部。因此每个变体除自身 data 字段外，
//! 还有四个 typed 的 part 级字段与 `extensions`（收集剩余兄弟键），序列化时
//! 它们被写到兄弟层，反序列化时从兄弟层读回 —— 不再丢弃。

use airx_core::json::Extensions;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

/// 官方 Part 的 part 级字段（序列化时提到 data 成员的兄弟层）。
const PART_LEVEL_KEYS: [&str; 4] = [
    "thoughtSignature",
    "videoMetadata",
    "partMetadata",
    "mediaResolution",
];

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
            video_metadata: None,
            part_metadata: None,
            media_resolution: None,
            extensions: Extensions::new(),
        })
    }

    /// thinking 文本片段。
    pub fn thought(text: impl Into<String>) -> Self {
        Self::Text(TextPart {
            text: text.into(),
            thought: Some(true),
            thought_signature: None,
            video_metadata: None,
            part_metadata: None,
            media_resolution: None,
            extensions: Extensions::new(),
        })
    }

    /// 内联图片片段。
    pub fn inline_image(mime_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self::InlineData(InlineDataPart {
            mime_type: mime_type.into(),
            data: data.into(),
            display_name: None,
            thought_signature: None,
            video_metadata: None,
            part_metadata: None,
            media_resolution: None,
            extensions: Extensions::new(),
            part_extensions: Extensions::new(),
        })
    }

    /// 文件引用片段。
    pub fn file(mime_type: impl Into<String>, file_uri: impl Into<String>) -> Self {
        Self::FileData(FileDataPart {
            mime_type: mime_type.into(),
            file_uri: file_uri.into(),
            display_name: None,
            thought_signature: None,
            video_metadata: None,
            part_metadata: None,
            media_resolution: None,
            extensions: Extensions::new(),
            part_extensions: Extensions::new(),
        })
    }

    /// 工具调用片段。
    pub fn function_call(name: impl Into<String>, args: Value) -> Self {
        Self::FunctionCall(FunctionCallPart {
            id: None,
            name: name.into(),
            args,
            thought_signature: None,
            video_metadata: None,
            part_metadata: None,
            media_resolution: None,
            extensions: Extensions::new(),
            part_extensions: Extensions::new(),
        })
    }

    /// 工具结果片段。
    pub fn function_response(name: impl Into<String>, response: Value) -> Self {
        Self::FunctionResponse(FunctionResponsePart {
            id: None,
            name: name.into(),
            response,
            parts: None,
            thought_signature: None,
            video_metadata: None,
            part_metadata: None,
            media_resolution: None,
            extensions: Extensions::new(),
            part_extensions: Extensions::new(),
        })
    }

    /// 多模态工具结果片段（`functionResponse.parts`）。
    ///
    /// `parts` 里的每个附件都可以通过 `displayName` 被 `response` 里的
    /// `{"$ref": "<display_name>"}` 引用（官方 multimodal function response 语义）。
    pub fn function_response_parts(
        name: impl Into<String>,
        parts: impl IntoIterator<Item = FunctionResponseMedia>,
    ) -> Self {
        let parts: Vec<FunctionResponseMedia> = parts.into_iter().collect();
        let response = parts
            .iter()
            .find_map(|part| {
                part.display_name()
                    .map(|display_name| serde_json::json!({"$ref": display_name}))
            })
            .unwrap_or_else(empty_object);

        Self::FunctionResponse(FunctionResponsePart {
            id: None,
            name: name.into(),
            response,
            parts: Some(parts),
            thought_signature: None,
            video_metadata: None,
            part_metadata: None,
            media_resolution: None,
            extensions: Extensions::new(),
            part_extensions: Extensions::new(),
        })
    }

    /// `{"$ref": "<displayName>"}` 形式的工具结果（引用 `parts` 里的附件）。
    pub fn function_response_ref(name: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self::FunctionResponse(FunctionResponsePart {
            id: None,
            name: name.into(),
            response: serde_json::json!({"$ref": display_name.into()}),
            parts: None,
            thought_signature: None,
            video_metadata: None,
            part_metadata: None,
            media_resolution: None,
            extensions: Extensions::new(),
            part_extensions: Extensions::new(),
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

    /// Part 级 `thoughtSignature`。
    pub fn thought_signature(&self) -> Option<&str> {
        match self {
            Self::Text(part) => part.thought_signature.as_deref(),
            Self::InlineData(part) => part.thought_signature.as_deref(),
            Self::FileData(part) => part.thought_signature.as_deref(),
            Self::FunctionCall(part) => part.thought_signature.as_deref(),
            Self::FunctionResponse(part) => part.thought_signature.as_deref(),
            Self::ExecutableCode(part) => part.thought_signature.as_deref(),
            Self::CodeExecutionResult(part) => part.thought_signature.as_deref(),
            Self::Other(_) => None,
        }
    }

    /// 设置 Part 级 `thoughtSignature`（多轮工具调用需要原样回传）。
    #[must_use]
    pub fn with_thought_signature(self, signature: impl Into<String>) -> Self {
        let signature = signature.into();
        match self {
            Self::Text(mut part) => {
                part.thought_signature = Some(signature);
                Self::Text(part)
            }
            Self::InlineData(mut part) => {
                part.thought_signature = Some(signature);
                Self::InlineData(part)
            }
            Self::FileData(mut part) => {
                part.thought_signature = Some(signature);
                Self::FileData(part)
            }
            Self::FunctionCall(mut part) => {
                part.thought_signature = Some(signature);
                Self::FunctionCall(part)
            }
            Self::FunctionResponse(mut part) => {
                part.thought_signature = Some(signature);
                Self::FunctionResponse(part)
            }
            Self::ExecutableCode(mut part) => {
                part.thought_signature = Some(signature);
                Self::ExecutableCode(part)
            }
            Self::CodeExecutionResult(mut part) => {
                part.thought_signature = Some(signature);
                Self::CodeExecutionResult(part)
            }
            Self::Other(mut value) => {
                if let Value::Object(map) = &mut value {
                    map.insert("thoughtSignature".to_owned(), Value::String(signature));
                }
                Self::Other(value)
            }
        }
    }

    /// Part 级 `videoMetadata`。
    pub fn video_metadata(&self) -> Option<&VideoMetadata> {
        match self {
            Self::Text(part) => part.video_metadata.as_ref(),
            Self::InlineData(part) => part.video_metadata.as_ref(),
            Self::FileData(part) => part.video_metadata.as_ref(),
            Self::FunctionCall(part) => part.video_metadata.as_ref(),
            Self::FunctionResponse(part) => part.video_metadata.as_ref(),
            Self::ExecutableCode(part) => part.video_metadata.as_ref(),
            Self::CodeExecutionResult(part) => part.video_metadata.as_ref(),
            Self::Other(_) => None,
        }
    }

    /// Part 级 `partMetadata`（自由结构，原样保留）。
    pub fn part_metadata(&self) -> Option<&Value> {
        match self {
            Self::Text(part) => part.part_metadata.as_ref(),
            Self::InlineData(part) => part.part_metadata.as_ref(),
            Self::FileData(part) => part.part_metadata.as_ref(),
            Self::FunctionCall(part) => part.part_metadata.as_ref(),
            Self::FunctionResponse(part) => part.part_metadata.as_ref(),
            Self::ExecutableCode(part) => part.part_metadata.as_ref(),
            Self::CodeExecutionResult(part) => part.part_metadata.as_ref(),
            Self::Other(_) => None,
        }
    }

    /// Part 级 `mediaResolution`。
    pub fn media_resolution(&self) -> Option<&str> {
        match self {
            Self::Text(part) => part.media_resolution.as_deref(),
            Self::InlineData(part) => part.media_resolution.as_deref(),
            Self::FileData(part) => part.media_resolution.as_deref(),
            Self::FunctionCall(part) => part.media_resolution.as_deref(),
            Self::FunctionResponse(part) => part.media_resolution.as_deref(),
            Self::ExecutableCode(part) => part.media_resolution.as_deref(),
            Self::CodeExecutionResult(part) => part.media_resolution.as_deref(),
            Self::Other(_) => None,
        }
    }

    /// 该变体收集到的 part 级未知兄弟键（`thoughtSignature` 等已知字段有 typed 访问器）。
    pub fn extensions(&self) -> &Extensions {
        static EMPTY: std::sync::OnceLock<Extensions> = std::sync::OnceLock::new();
        match self {
            Self::Text(part) => &part.extensions,
            Self::InlineData(part) => &part.part_extensions,
            Self::FileData(part) => &part.part_extensions,
            Self::FunctionCall(part) => &part.part_extensions,
            Self::FunctionResponse(part) => &part.part_extensions,
            Self::ExecutableCode(part) => &part.part_extensions,
            Self::CodeExecutionResult(part) => &part.part_extensions,
            Self::Other(_) => EMPTY.get_or_init(Extensions::new),
        }
    }

    /// 该变体数据成员内部的未知键（例如 `functionResponse.willContinue`）。
    pub fn data_extensions(&self) -> &Extensions {
        match self {
            Self::Text(part) => &part.extensions,
            Self::InlineData(part) => &part.extensions,
            Self::FileData(part) => &part.extensions,
            Self::FunctionCall(part) => &part.extensions,
            Self::FunctionResponse(part) => &part.extensions,
            Self::ExecutableCode(part) => &part.extensions,
            Self::CodeExecutionResult(part) => &part.extensions,
            Self::Other(_) => self.extensions(),
        }
    }

    /// 未识别片段里的 `toolCall`（服务端工具调用，客户端应原样 echo 回 API）。
    pub fn tool_call(&self) -> Option<&Value> {
        self.other_field("toolCall")
    }

    /// 未识别片段里的 `toolResponse`（服务端工具结果）。
    pub fn tool_response(&self) -> Option<&Value> {
        self.other_field("toolResponse")
    }

    /// 未识别片段的原始 JSON。
    pub fn other(&self) -> Option<&Value> {
        match self {
            Self::Other(value) => Some(value),
            _ => None,
        }
    }

    fn other_field(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Other(value) => value.get(key),
            _ => None,
        }
    }
}

/// 视频时间轴信息（part 级 `videoMetadata`）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoMetadata {
    /// 起始偏移（如 `1s`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<String>,
    /// 结束偏移（如 `5s`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<String>,
    /// 采样帧率。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 文本片段（本身就是完整的 part 对象）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextPart {
    /// 文本。
    pub text: String,
    /// 是否为 thinking 内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thought: Option<bool>,
    /// Part 级思考签名（多轮工具调用时需回传）。
    #[serde(
        rename = "thoughtSignature",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub thought_signature: Option<String>,
    /// Part 级视频时间轴信息。
    #[serde(
        rename = "videoMetadata",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub video_metadata: Option<VideoMetadata>,
    /// Part 级自由元数据。
    #[serde(
        rename = "partMetadata",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub part_metadata: Option<Value>,
    /// Part 级媒体分辨率。
    #[serde(
        rename = "mediaResolution",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub media_resolution: Option<String>,
    /// 未识别的扩展字段（part 级兄弟键）。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl TextPart {
    /// 是否为 thinking 内容。
    pub fn is_thought(&self) -> bool {
        self.thought == Some(true)
    }
}

/// 内联二进制片段（对应 `inlineData` 数据成员）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InlineDataPart {
    /// MIME 类型。
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    /// base64 数据。
    pub data: String,
    /// 资源名（可用于 `functionResponse.response` 的 `$ref`）。
    #[serde(
        rename = "displayName",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub display_name: Option<String>,
    /// Part 级思考签名。
    #[serde(
        rename = "thoughtSignature",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub thought_signature: Option<String>,
    /// Part 级视频时间轴信息。
    #[serde(
        rename = "videoMetadata",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub video_metadata: Option<VideoMetadata>,
    /// Part 级自由元数据。
    #[serde(
        rename = "partMetadata",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub part_metadata: Option<Value>,
    /// Part 级媒体分辨率。
    #[serde(
        rename = "mediaResolution",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub media_resolution: Option<String>,
    /// 未识别的扩展字段（part 级兄弟键）。
    ///
    /// 序列化时写到 `inlineData` 的兄弟层；反序列化时把兄弟层未知键一并收在这里。
    #[serde(flatten)]
    pub extensions: Extensions,
    /// Part 级未知兄弟键（与数据成员内部的未知键分开保存）。
    #[serde(skip)]
    pub(crate) part_extensions: Extensions,
}

impl InlineDataPart {
    /// 是否带任何 part 级元数据（用于流式 base64 合并的保守判断）。
    pub(crate) fn has_part_metadata(&self) -> bool {
        self.thought_signature.is_some()
            || self.video_metadata.is_some()
            || self.part_metadata.is_some()
            || self.media_resolution.is_some()
            || !self.part_extensions.is_empty()
    }
}

/// 文件引用片段（对应 `fileData` 数据成员）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileDataPart {
    /// MIME 类型。
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    /// 文件 URI。
    #[serde(rename = "fileUri")]
    pub file_uri: String,
    /// 资源名（可用于 `functionResponse.response` 的 `$ref`）。
    #[serde(
        rename = "displayName",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub display_name: Option<String>,
    /// Part 级思考签名。
    #[serde(
        rename = "thoughtSignature",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub thought_signature: Option<String>,
    /// Part 级视频时间轴信息。
    #[serde(
        rename = "videoMetadata",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub video_metadata: Option<VideoMetadata>,
    /// Part 级自由元数据。
    #[serde(
        rename = "partMetadata",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub part_metadata: Option<Value>,
    /// Part 级媒体分辨率。
    #[serde(
        rename = "mediaResolution",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub media_resolution: Option<String>,
    /// 未识别的扩展字段（part 级兄弟键）。
    #[serde(flatten)]
    pub extensions: Extensions,
    /// Part 级未知兄弟键（与数据成员内部的未知键分开保存）。
    #[serde(skip)]
    pub(crate) part_extensions: Extensions,
}

/// 工具调用片段（对应 `functionCall` 数据成员）。
///
/// 官方 `FunctionCall` 接口只有 `args` / `id` / `name`（`partialArgs` / `willContinue`
/// 为 Vertex 专属）。`thoughtSignature` 是 **Part 级** 字段，序列化时写在
/// `functionCall` 的兄弟层。
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
    /// Part 级思考签名。
    #[serde(
        rename = "thoughtSignature",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub thought_signature: Option<String>,
    /// Part 级视频时间轴信息。
    #[serde(
        rename = "videoMetadata",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub video_metadata: Option<VideoMetadata>,
    /// Part 级自由元数据。
    #[serde(
        rename = "partMetadata",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub part_metadata: Option<Value>,
    /// Part 级媒体分辨率。
    #[serde(
        rename = "mediaResolution",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub media_resolution: Option<String>,
    /// 未识别的扩展字段（part 级兄弟键）。
    #[serde(flatten)]
    pub extensions: Extensions,
    /// Part 级未知兄弟键（与数据成员内部的未知键分开保存）。
    #[serde(skip)]
    pub(crate) part_extensions: Extensions,
}

impl FunctionCallPart {
    /// 该调用的思考签名。
    pub fn thought_signature(&self) -> Option<&str> {
        self.thought_signature.as_deref()
    }
}

/// 工具结果片段（对应 `functionResponse` 数据成员）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionResponsePart {
    /// 对应的调用 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 工具名。
    pub name: String,
    /// 结果对象。可用 `{"$ref": "<displayName>"}` 引用 `parts` 里的附件。
    #[serde(default = "empty_object")]
    pub response: Value,
    /// 多模态结果附件（官方 `FunctionResponsePart[]`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<FunctionResponseMedia>>,
    /// Part 级思考签名。
    #[serde(
        rename = "thoughtSignature",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub thought_signature: Option<String>,
    /// Part 级视频时间轴信息。
    #[serde(
        rename = "videoMetadata",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub video_metadata: Option<VideoMetadata>,
    /// Part 级自由元数据。
    #[serde(
        rename = "partMetadata",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub part_metadata: Option<Value>,
    /// Part 级媒体分辨率。
    #[serde(
        rename = "mediaResolution",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub media_resolution: Option<String>,
    /// 未识别的扩展字段（part 级兄弟键）。
    #[serde(flatten)]
    pub extensions: Extensions,
    /// Part 级未知兄弟键（与数据成员内部的未知键分开保存）。
    #[serde(skip)]
    pub(crate) part_extensions: Extensions,
}

/// 工具结果附件（官方类型名 `FunctionResponsePart`）。
///
/// 与 [`FunctionResponsePart`]（整个 `functionResponse` 成员）区分命名。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponseMedia {
    /// 内联二进制。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<FunctionResponseBlob>,
    /// 文件引用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_data: Option<FunctionResponseFileData>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl FunctionResponseMedia {
    /// 内联二进制附件（base64）。
    pub fn inline(mime_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            inline_data: Some(FunctionResponseBlob {
                mime_type: Some(mime_type.into()),
                data: Some(data.into()),
                display_name: None,
                extensions: Extensions::new(),
            }),
            file_data: None,
            extensions: Extensions::new(),
        }
    }

    /// 文件引用附件。
    pub fn file(file_uri: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self {
            inline_data: None,
            file_data: Some(FunctionResponseFileData {
                file_uri: file_uri.into(),
                mime_type: Some(mime_type.into()),
                display_name: None,
                extensions: Extensions::new(),
            }),
            extensions: Extensions::new(),
        }
    }

    /// 设置 `displayName`（供 `response` 的 `$ref` 引用）。
    #[must_use]
    pub fn with_display_name(mut self, display_name: impl Into<String>) -> Self {
        let display_name = display_name.into();
        match (&mut self.inline_data, &mut self.file_data) {
            (Some(blob), _) => blob.display_name = Some(display_name),
            (None, Some(file)) => file.display_name = Some(display_name),
            (None, None) => {}
        }
        self
    }

    /// 附件上的 `displayName`。
    pub fn display_name(&self) -> Option<&str> {
        self.inline_data
            .as_ref()
            .and_then(|blob| blob.display_name.as_deref())
            .or_else(|| {
                self.file_data
                    .as_ref()
                    .and_then(|file| file.display_name.as_deref())
            })
    }
}

/// 工具结果的内联二进制（官方 `FunctionResponseBlob`）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponseBlob {
    /// MIME 类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// base64 数据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// 资源名（`$ref` 目标）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 工具结果的文件引用（官方 `FunctionResponseFileData`）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponseFileData {
    /// 文件 URI。
    pub file_uri: String,
    /// MIME 类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// 资源名（`$ref` 目标）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
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
    /// Part 级思考签名。
    #[serde(
        rename = "thoughtSignature",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub thought_signature: Option<String>,
    /// Part 级视频时间轴信息。
    #[serde(
        rename = "videoMetadata",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub video_metadata: Option<VideoMetadata>,
    /// Part 级自由元数据。
    #[serde(
        rename = "partMetadata",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub part_metadata: Option<Value>,
    /// Part 级媒体分辨率。
    #[serde(
        rename = "mediaResolution",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub media_resolution: Option<String>,
    /// 未识别的扩展字段（part 级兄弟键）。
    #[serde(flatten)]
    pub extensions: Extensions,
    /// Part 级未知兄弟键（与数据成员内部的未知键分开保存）。
    #[serde(skip)]
    pub(crate) part_extensions: Extensions,
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
    /// Part 级思考签名。
    #[serde(
        rename = "thoughtSignature",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub thought_signature: Option<String>,
    /// Part 级视频时间轴信息。
    #[serde(
        rename = "videoMetadata",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub video_metadata: Option<VideoMetadata>,
    /// Part 级自由元数据。
    #[serde(
        rename = "partMetadata",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub part_metadata: Option<Value>,
    /// Part 级媒体分辨率。
    #[serde(
        rename = "mediaResolution",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub media_resolution: Option<String>,
    /// 未识别的扩展字段（part 级兄弟键）。
    #[serde(flatten)]
    pub extensions: Extensions,
    /// Part 级未知兄弟键（与数据成员内部的未知键分开保存）。
    #[serde(skip)]
    pub(crate) part_extensions: Extensions,
}

fn empty_object() -> Value {
    Value::Object(Map::new())
}

impl Serialize for Part {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Text(part) => part.serialize(serializer),
            Self::InlineData(part) => serialize_wrapped(
                "inlineData",
                part,
                &part.part_extensions,
                &part.extensions,
                serializer,
            ),
            Self::FileData(part) => serialize_wrapped(
                "fileData",
                part,
                &part.part_extensions,
                &part.extensions,
                serializer,
            ),
            Self::FunctionCall(part) => serialize_wrapped(
                "functionCall",
                part,
                &part.part_extensions,
                &part.extensions,
                serializer,
            ),
            Self::FunctionResponse(part) => serialize_wrapped(
                "functionResponse",
                part,
                &part.part_extensions,
                &part.extensions,
                serializer,
            ),
            Self::ExecutableCode(part) => serialize_wrapped(
                "executableCode",
                part,
                &part.part_extensions,
                &part.extensions,
                serializer,
            ),
            Self::CodeExecutionResult(part) => serialize_wrapped(
                "codeExecutionResult",
                part,
                &part.part_extensions,
                &part.extensions,
                serializer,
            ),
            Self::Other(value) => value.serialize(serializer),
        }
    }
}

/// 把内部结构序列化为 `{ "<kind>": { ...data 成员... }, ...part 级兄弟键 }`。
///
/// `part_extensions` 是反序列化时从兄弟层收集的未知键，只把这些写回兄弟层；
/// `data_extensions` 属于数据成员内部的未知键，保持在 `<kind>` 内部，不能提升。
fn serialize_wrapped<S: Serializer, T: Serialize>(
    kind: &str,
    part: &T,
    part_extensions: &Extensions,
    data_extensions: &Extensions,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let value = serde_json::to_value(part).map_err(serde::ser::Error::custom)?;
    let Value::Object(mut inner) = value else {
        return Err(serde::ser::Error::custom(format!(
            "{kind} 必须序列化为对象"
        )));
    };
    let _ = data_extensions; // flatten 已写入 inner；这里刻意不提升

    let mut outer = Map::new();
    // 已知 part 级字段：从 data 成员里提出来。
    for key in PART_LEVEL_KEYS {
        if let Some(value) = inner.remove(key) {
            outer.insert(key.to_owned(), value);
        }
    }
    // 反序列化时记录的未知兄弟键：写回兄弟层。
    for (key, value) in part_extensions {
        inner.remove(key);
        outer.insert(key.clone(), value.clone());
    }
    outer.insert(kind.to_owned(), Value::Object(inner));

    Value::Object(outer).serialize(serializer)
}

/// 把 `{ "<kind>": {...}, ...兄弟键 }` 摊平成 data 成员对象 + 未知兄弟键。
///
/// 已知 part 级字段合并进 data 成员（由 typed 字段承接）；未知兄弟键单独返回，
/// 避免与数据成员内部的未知键混在同一个 `extensions` 里。
fn flatten_variant(value: Value, kind: &str) -> Result<(Value, Extensions), serde_json::Error> {
    let Value::Object(mut map) = value else {
        return Err(serde::de::Error::custom(format!("{kind} 必须是对象")));
    };
    let mut inner = map
        .remove(kind)
        .ok_or_else(|| serde::de::Error::custom(format!("缺少 {kind}")))?;

    let mut part_extensions = Extensions::new();
    if let Value::Object(inner_map) = &mut inner {
        for (key, value) in map {
            if PART_LEVEL_KEYS.contains(&key.as_str()) {
                inner_map.insert(key, value);
            } else {
                part_extensions.insert(key, value);
            }
        }
    }

    Ok((inner, part_extensions))
}

impl<'de> Deserialize<'de> for Part {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;

        if value.get("text").is_some() {
            // TextPart 本身就是完整的 part 对象。
            return serde_json::from_value::<TextPart>(value)
                .map(Self::Text)
                .map_err(serde::de::Error::custom);
        }

        macro_rules! parse {
            ($kind:literal, $ty:ty, $variant:ident) => {{
                let (inner, part_extensions) =
                    flatten_variant(value, $kind).map_err(serde::de::Error::custom)?;
                let mut part: $ty =
                    serde_json::from_value(inner).map_err(serde::de::Error::custom)?;
                part.part_extensions = part_extensions;
                Ok(Self::$variant(part))
            }};
        }

        if value.get("inlineData").is_some() {
            return parse!("inlineData", InlineDataPart, InlineData);
        }
        if value.get("fileData").is_some() {
            return parse!("fileData", FileDataPart, FileData);
        }
        if value.get("functionCall").is_some() {
            return parse!("functionCall", FunctionCallPart, FunctionCall);
        }
        if value.get("functionResponse").is_some() {
            return parse!("functionResponse", FunctionResponsePart, FunctionResponse);
        }
        if value.get("executableCode").is_some() {
            return parse!("executableCode", ExecutableCodePart, ExecutableCode);
        }
        if value.get("codeExecutionResult").is_some() {
            return parse!(
                "codeExecutionResult",
                CodeExecutionResultPart,
                CodeExecutionResult
            );
        }

        Ok(Self::Other(value))
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
    fn thought_signature_is_a_part_level_sibling() {
        let value = json!([{
            "functionCall": {"name": "check_flight", "args": {"flight": "AA100"}},
            "thoughtSignature": "Signature A"
        }]);

        let parts: Vec<Part> = serde_json::from_value(value.clone()).expect("parts");
        assert_eq!(parts[0].thought_signature(), Some("Signature A"));
        match &parts[0] {
            Part::FunctionCall(call) => {
                assert_eq!(call.thought_signature(), Some("Signature A"));
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(serde_json::to_value(&parts).expect("serialize"), value);
    }

    #[test]
    fn function_call_never_nests_signature() {
        let part = Part::function_call("f", json!({"x": 1})).with_thought_signature("sig-1");
        let value = serde_json::to_value(&part).expect("serialize");

        assert_eq!(value["thoughtSignature"], "sig-1");
        assert!(value["functionCall"].get("thoughtSignature").is_none());
    }

    #[test]
    fn non_text_sibling_keys_roundtrip() {
        let value = json!([
            {
                "inlineData": {"mimeType": "video/mp4", "data": "AAA"},
                "videoMetadata": {"startOffset": "1s", "endOffset": "5s", "fps": 2.0},
                "mediaResolution": "MEDIA_RESOLUTION_LOW"
            },
            {
                "functionResponse": {"name": "render", "response": {"ok": true}},
                "partMetadata": {"trace": "abc"},
                "futureSibling": {"keep": true}
            }
        ]);

        let parts: Vec<Part> = serde_json::from_value(value.clone()).expect("parts");
        assert_eq!(
            parts[0]
                .video_metadata()
                .and_then(|video| video.start_offset.as_deref()),
            Some("1s")
        );
        assert_eq!(parts[0].media_resolution(), Some("MEDIA_RESOLUTION_LOW"));
        assert_eq!(parts[1].part_metadata(), Some(&json!({"trace": "abc"})));
        assert!(parts[1].extensions().contains_key("futureSibling"));

        // 兄弟键落在变体自己的 part_extensions 里（不再丢弃）。
        match &parts[1] {
            Part::FunctionResponse(response) => {
                assert!(response.part_extensions.contains_key("futureSibling"));
                assert!(response.extensions.is_empty());
            }
            other => panic!("unexpected: {other:?}"),
        }

        assert_eq!(serde_json::to_value(&parts).expect("serialize"), value);
    }

    #[test]
    fn function_response_inner_unknown_stays_inside_member() {
        let value = json!([{
            "functionResponse": {
                "name": "render",
                "response": {"ok": true},
                "willContinue": true
            },
            "partMetadata": {"trace": "abc"}
        }]);

        let parts: Vec<Part> = serde_json::from_value(value.clone()).expect("parts");
        match &parts[0] {
            Part::FunctionResponse(part) => {
                assert_eq!(
                    part.extensions.get("willContinue"),
                    Some(&Value::Bool(true))
                );
                assert!(part.part_extensions.is_empty());
                assert_eq!(part.part_metadata, Some(json!({"trace": "abc"})));
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert!(parts[0].data_extensions().contains_key("willContinue"));

        assert_eq!(serde_json::to_value(&parts).expect("serialize"), value);
    }

    #[test]
    fn untyped_part_is_still_raw_preserved() {
        let value = json!([{"toolCall": {"id": "1", "toolType": "GOOGLE_SEARCH"}}]);
        let parts: Vec<Part> = serde_json::from_value(value.clone()).expect("parts");

        assert!(matches!(parts[0], Part::Other(_)));
        assert_eq!(
            parts[0].tool_call(),
            Some(&json!({"id": "1", "toolType": "GOOGLE_SEARCH"}))
        );
        assert!(parts[0].tool_response().is_none());
        assert_eq!(serde_json::to_value(&parts).expect("serialize"), value);
    }

    #[test]
    fn function_response_parts_use_official_shape() {
        let part = Part::function_response_parts(
            "render",
            [FunctionResponseMedia::inline("image/png", "AAA").with_display_name("result_image")],
        );
        let value = serde_json::to_value(&part).expect("serialize");

        assert_eq!(
            value["functionResponse"]["parts"][0]["inlineData"],
            json!({"mimeType": "image/png", "data": "AAA", "displayName": "result_image"})
        );
        assert_eq!(
            value["functionResponse"]["response"],
            json!({"$ref": "result_image"})
        );

        let parsed: Part = serde_json::from_value(value.clone()).expect("part");
        assert_eq!(
            parsed,
            Part::function_response_parts(
                "render",
                [FunctionResponseMedia::inline("image/png", "AAA")
                    .with_display_name("result_image")]
            )
        );
        assert_eq!(serde_json::to_value(&parsed).expect("serialize"), value);
    }

    #[test]
    fn function_response_ref_serializes_dollar_ref() {
        let part = Part::function_response_ref("render", "result_image");
        let value = serde_json::to_value(&part).expect("serialize");
        assert_eq!(
            value["functionResponse"]["response"],
            json!({"$ref": "result_image"})
        );
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

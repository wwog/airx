//! OpenAI Responses 请求与输入模型。

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::json::Extensions;
use airx_core::model::ModelId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use super::common::ReasoningSummary;
use super::response::{OutputContent, OutputItem, ResponsesResponse};
use super::tool::{ResponsesTool, ResponsesToolChoice};
use crate::openai::common::reasoning::ReasoningEffort;

/// Responses 输入：纯文本或 item 列表。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponsesInput {
    /// 纯文本输入。
    Text(String),
    /// 结构化输入项。
    Items(Vec<InputItem>),
}

/// 输入消息内容：纯文本或多模态片段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InputMessageContent {
    /// 纯文本。
    Text(String),
    /// 多模态片段。
    Parts(Vec<InputContent>),
}

impl From<&str> for InputMessageContent {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for InputMessageContent {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl InputMessageContent {
    /// 纯文本内容（多模态时返回 None）。
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Parts(_) => None,
        }
    }
}

/// 输入消息片段。
#[derive(Debug, Clone, PartialEq)]
pub enum InputContent {
    /// `input_text`
    Text {
        /// 文本。
        text: String,
    },
    /// `input_image`
    Image {
        /// 图片 URL 或 data URI。
        image_url: Option<String>,
        /// 已上传文件 ID。
        file_id: Option<String>,
        /// 细节级别（`low` / `high` / `auto`）。
        detail: Option<String>,
        /// 未识别的扩展字段。
        extensions: Extensions,
    },
    /// `input_file`
    File {
        /// 已上传文件 ID。
        file_id: Option<String>,
        /// 内联文件数据（base64）。
        file_data: Option<String>,
        /// 文件名。
        filename: Option<String>,
        /// 未识别的扩展字段。
        extensions: Extensions,
    },
    /// 未识别片段。
    Other(Value),
}

impl Serialize for InputContent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Text { text } => {
                let mut map = Map::new();
                map.insert("type".to_owned(), Value::String("input_text".to_owned()));
                map.insert("text".to_owned(), Value::String(text.clone()));
                Value::Object(map).serialize(serializer)
            }
            Self::Image {
                image_url,
                file_id,
                detail,
                extensions,
            } => {
                let mut map = Map::new();
                map.insert("type".to_owned(), Value::String("input_image".to_owned()));
                insert_optional(&mut map, "image_url", image_url.as_deref());
                insert_optional(&mut map, "file_id", file_id.as_deref());
                insert_optional(&mut map, "detail", detail.as_deref());
                insert_extensions(&mut map, extensions);
                Value::Object(map).serialize(serializer)
            }
            Self::File {
                file_id,
                file_data,
                filename,
                extensions,
            } => {
                let mut map = Map::new();
                map.insert("type".to_owned(), Value::String("input_file".to_owned()));
                insert_optional(&mut map, "file_id", file_id.as_deref());
                insert_optional(&mut map, "file_data", file_data.as_deref());
                insert_optional(&mut map, "filename", filename.as_deref());
                insert_extensions(&mut map, extensions);
                Value::Object(map).serialize(serializer)
            }
            Self::Other(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for InputContent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();

        match kind.as_str() {
            "input_text" => {
                remove_type(&mut value);
                let payload: TextContent =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::Text { text: payload.text })
            }
            "input_image" => {
                remove_type(&mut value);
                let payload: ImageContent =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::Image {
                    image_url: payload.image_url,
                    file_id: payload.file_id,
                    detail: payload.detail,
                    extensions: payload.extensions,
                })
            }
            "input_file" => {
                remove_type(&mut value);
                let payload: FileContent =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::File {
                    file_id: payload.file_id,
                    file_data: payload.file_data,
                    filename: payload.filename,
                    extensions: payload.extensions,
                })
            }
            _ => Ok(Self::Other(value)),
        }
    }
}

#[derive(Deserialize)]
struct TextContent {
    text: String,
}

#[derive(Deserialize)]
struct ImageContent {
    #[serde(default)]
    image_url: Option<String>,
    #[serde(default)]
    file_id: Option<String>,
    #[serde(default)]
    detail: Option<String>,
    #[serde(flatten)]
    extensions: Extensions,
}

#[derive(Deserialize)]
struct FileContent {
    #[serde(default)]
    file_id: Option<String>,
    #[serde(default)]
    file_data: Option<String>,
    #[serde(default)]
    filename: Option<String>,
    #[serde(flatten)]
    extensions: Extensions,
}

/// 输入项。
#[derive(Debug, Clone, PartialEq)]
pub enum InputItem {
    /// 消息。
    Message {
        /// 角色。
        role: String,
        /// 文本或多模态内容。
        content: InputMessageContent,
    },
    /// 已有的函数调用 item（手动回传历史时使用）。
    FunctionCall {
        /// 调用 ID。
        call_id: String,
        /// 函数名。
        name: String,
        /// 参数 JSON 字符串。
        arguments: String,
    },
    /// 函数调用结果。
    FunctionCallOutput {
        /// 对应的调用 ID。
        call_id: String,
        /// 结果文本。
        output: String,
    },
    /// Reasoning item（原样回传以保留推理状态）。
    Reasoning {
        /// item ID。
        id: Option<String>,
        /// 摘要。
        summary: Option<Vec<ReasoningSummary>>,
        /// 加密内容。
        encrypted_content: Option<String>,
        /// 未识别的扩展字段。
        extensions: Extensions,
    },
    /// item 引用。
    ItemReference {
        /// item ID。
        id: String,
    },
    /// 未识别 item（原样保留）。
    Other(Value),
}

impl InputItem {
    /// 构造 user 文本消息。
    pub fn user(content: impl Into<String>) -> Self {
        Self::Message {
            role: "user".to_owned(),
            content: InputMessageContent::Text(content.into()),
        }
    }

    /// 构造 system / developer 文本消息。
    pub fn system(content: impl Into<String>) -> Self {
        Self::Message {
            role: "system".to_owned(),
            content: InputMessageContent::Text(content.into()),
        }
    }

    /// 构造 assistant 文本消息（回传历史时使用）。
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::Message {
            role: "assistant".to_owned(),
            content: InputMessageContent::Text(content.into()),
        }
    }

    /// 构造函数调用 item（回传历史时使用）。
    pub fn function_call(
        call_id: impl Into<String>,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        Self::FunctionCall {
            call_id: call_id.into(),
            name: name.into(),
            arguments: arguments.into(),
        }
    }

    /// 构造函数调用结果。
    pub fn function_call_output(call_id: impl Into<String>, output: impl Into<String>) -> Self {
        Self::FunctionCallOutput {
            call_id: call_id.into(),
            output: output.into(),
        }
    }
}

/// `OutputItem` → `InputItem`：直接复用上一次响应作为下一轮输入。
///
/// 映射规则：
/// - `message`：拼接全部 `output_text`（无文本时回退到拒答文本）；
/// - `function_call`：`call_id` / `name` / `arguments` 原样保留；
/// - `reasoning`：`id` / `summary` / `encrypted_content` 原样保留；
/// - 其他 item（服务端工具调用、custom tool call、compaction 等）：
///   保留完整 JSON，避免丢字段。
///
/// 注意：官方 item 的 `id` / `status` 不是输入必需字段；`message` 转换时
/// 会丢弃 `id` / `status`（官方也不要求回传），`reasoning` 则保留 `id`。
impl From<OutputItem> for InputItem {
    fn from(item: OutputItem) -> Self {
        match item {
            OutputItem::Message(message) => {
                // 只有「纯 output_text 且无标注」的消息才能无损压成字符串输入；
                // `input_text`（compact 结果）、refusal、标注等必须原样回传整个
                // message item，否则会丢内容。
                let plain = !message.content.is_empty()
                    && message.content.iter().all(|content| {
                        matches!(
                            content,
                            OutputContent::OutputText(output)
                                if output.annotations.is_empty() && output.extensions.is_empty()
                        )
                    });
                if plain {
                    let mut text = String::new();
                    for content in &message.content {
                        if let OutputContent::OutputText(output) = content {
                            text.push_str(&output.text);
                        }
                    }
                    Self::Message {
                        role: message.role,
                        content: InputMessageContent::Text(text),
                    }
                } else {
                    Self::Other(
                        serde_json::to_value(OutputItem::Message(message)).unwrap_or(Value::Null),
                    )
                }
            }
            OutputItem::FunctionCall(call) => Self::FunctionCall {
                call_id: call.call_id,
                name: call.name,
                arguments: call.arguments,
            },
            OutputItem::Reasoning(item) => Self::Reasoning {
                id: item.id,
                summary: Some(item.summary),
                encrypted_content: item.encrypted_content,
                extensions: item.extensions,
            },
            other => Self::Other(serde_json::to_value(other).unwrap_or(Value::Null)),
        }
    }
}

impl ResponsesResponse {
    /// 把本次响应转换为下一轮输入 item 列表。
    ///
    /// 适用于 `store: false` / ZDR 场景：官方不保存响应，多轮必须自行回传。
    pub fn into_input(self) -> Vec<InputItem> {
        self.output.into_iter().map(InputItem::from).collect()
    }
}

impl Serialize for InputItem {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Message { role, content } => {
                let mut map = Map::new();
                map.insert("type".to_owned(), Value::String("message".to_owned()));
                map.insert("role".to_owned(), Value::String(role.clone()));
                map.insert(
                    "content".to_owned(),
                    serde_json::to_value(content).map_err(serde::ser::Error::custom)?,
                );
                Value::Object(map).serialize(serializer)
            }
            Self::FunctionCall {
                call_id,
                name,
                arguments,
            } => {
                let mut map = Map::new();
                map.insert("type".to_owned(), Value::String("function_call".to_owned()));
                map.insert("call_id".to_owned(), Value::String(call_id.clone()));
                map.insert("name".to_owned(), Value::String(name.clone()));
                map.insert("arguments".to_owned(), Value::String(arguments.clone()));
                Value::Object(map).serialize(serializer)
            }
            Self::FunctionCallOutput { call_id, output } => {
                let mut map = Map::new();
                map.insert(
                    "type".to_owned(),
                    Value::String("function_call_output".to_owned()),
                );
                map.insert("call_id".to_owned(), Value::String(call_id.clone()));
                map.insert("output".to_owned(), Value::String(output.clone()));
                Value::Object(map).serialize(serializer)
            }
            Self::Reasoning {
                id,
                summary,
                encrypted_content,
                extensions,
            } => {
                let mut map = Map::new();
                map.insert("type".to_owned(), Value::String("reasoning".to_owned()));
                if let Some(id) = id {
                    map.insert("id".to_owned(), Value::String(id.clone()));
                }
                if let Some(summary) = summary {
                    map.insert(
                        "summary".to_owned(),
                        serde_json::to_value(summary).map_err(serde::ser::Error::custom)?,
                    );
                }
                if let Some(encrypted_content) = encrypted_content {
                    map.insert(
                        "encrypted_content".to_owned(),
                        Value::String(encrypted_content.clone()),
                    );
                }
                insert_extensions(&mut map, extensions);
                Value::Object(map).serialize(serializer)
            }
            Self::ItemReference { id } => {
                let mut map = Map::new();
                map.insert(
                    "type".to_owned(),
                    Value::String("item_reference".to_owned()),
                );
                map.insert("id".to_owned(), Value::String(id.clone()));
                Value::Object(map).serialize(serializer)
            }
            Self::Other(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for InputItem {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();

        match kind.as_str() {
            "message" => {
                remove_type(&mut value);
                let payload: MessagePayload =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::Message {
                    role: payload.role,
                    content: payload.content,
                })
            }
            "function_call" => {
                remove_type(&mut value);
                let payload: FunctionCallPayload =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::FunctionCall {
                    call_id: payload.call_id,
                    name: payload.name,
                    arguments: payload.arguments,
                })
            }
            "function_call_output" => {
                remove_type(&mut value);
                let payload: FunctionCallOutputPayload =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::FunctionCallOutput {
                    call_id: payload.call_id,
                    output: payload.output,
                })
            }
            "reasoning" => {
                remove_type(&mut value);
                let payload: ReasoningPayload =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::Reasoning {
                    id: payload.id,
                    summary: payload.summary,
                    encrypted_content: payload.encrypted_content,
                    extensions: payload.extensions,
                })
            }
            "item_reference" => {
                remove_type(&mut value);
                let payload: ItemReferencePayload =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::ItemReference { id: payload.id })
            }
            _ => Ok(Self::Other(value)),
        }
    }
}

#[derive(Deserialize)]
struct MessagePayload {
    role: String,
    content: InputMessageContent,
}

#[derive(Deserialize)]
struct FunctionCallPayload {
    call_id: String,
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct FunctionCallOutputPayload {
    call_id: String,
    output: String,
}

#[derive(Deserialize)]
struct ReasoningPayload {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    summary: Option<Vec<ReasoningSummary>>,
    #[serde(default)]
    encrypted_content: Option<String>,
    #[serde(flatten)]
    extensions: Extensions,
}

#[derive(Deserialize)]
struct ItemReferencePayload {
    id: String,
}

fn insert_optional(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        map.insert(key.to_owned(), Value::String(value.to_owned()));
    }
}

fn insert_extensions(map: &mut Map<String, Value>, extensions: &Extensions) {
    for (key, value) in extensions {
        map.insert(key.clone(), value.clone());
    }
}

fn remove_type(value: &mut Value) {
    if let Value::Object(map) = value {
        map.remove("type");
    }
}

/// 推理配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Reasoning {
    /// 推理投入等级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<ReasoningEffort>,
    /// 摘要展示方式（`auto` / `concise` / `detailed`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 文本输出配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TextConfig {
    /// 输出格式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<TextFormat>,
    /// 输出详细程度（`low` / `medium` / `high`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 文本输出格式。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TextFormat {
    /// 普通文本。
    Text,
    /// JSON 对象模式。
    JsonObject,
    /// JSON Schema 结构化输出。
    JsonSchema {
        /// 名称。
        name: String,
        /// 描述。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        /// JSON Schema。
        schema: Value,
        /// 是否严格。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        strict: Option<bool>,
    },
}

/// 上下文管理项（当前只有 `compaction`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextManagement {
    /// 类型（官方目前只有 `compaction`）。
    #[serde(rename = "type", default = "compaction_kind")]
    pub kind: String,
    /// 触发压缩的 token 阈值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact_threshold: Option<u32>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ContextManagement {
    /// 构造 compaction 项。
    pub fn compaction(compact_threshold: Option<u32>) -> Self {
        Self {
            kind: compaction_kind(),
            compact_threshold,
            extensions: Extensions::new(),
        }
    }
}

fn compaction_kind() -> String {
    "compaction".to_owned()
}

/// 可复用提示词模板。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prompt {
    /// 模板 ID。
    pub id: String,
    /// 模板版本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 模板变量（字符串或 input 片段，原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// Responses 提示词缓存选项。
///
/// 官方仅 `gpt-5.6+` 支持；airx 不做模型能力校验。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PromptCacheOptions {
    /// `implicit`（默认）或 `explicit`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// 每个缓存断点的最短生命周期（`30m` / `12h`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
    /// 仅预热缓存，不生成输出。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prewarm: Option<bool>,
    /// 用于诊断缓存的对比响应 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comparison_response_id: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// Responses 流式选项。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsesStreamOptions {
    /// 是否启用流式混淆（默认由服务端决定）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_obfuscation: Option<bool>,
}

/// Responses 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponsesRequest {
    /// 模型。
    pub model: ModelId,
    /// 输入。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<ResponsesInput>,
    /// 基于已有响应继续。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    /// 系统指令。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// 最大输出 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    /// 采样温度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// 核采样。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// 每个 token 位置返回的候选数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u32>,
    /// 工具。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ResponsesTool>>,
    /// 工具选择（Responses 扁平形状）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ResponsesToolChoice>,
    /// 是否允许并行工具调用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    /// 推理配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
    /// 文本输出配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextConfig>,
    /// 上下文管理（服务端压缩长对话）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_management: Option<Vec<ContextManagement>>,
    /// 可复用提示词模板。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<Prompt>,
    /// 终端用户标识（官方已标记为被 `safety_identifier` / `prompt_cache_key` 取代）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// 是否存储响应。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    /// 后台执行。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<bool>,
    /// 额外返回字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
    /// 截断策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncation: Option<String>,
    /// 会话（字符串 ID 或对象，原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<Value>,
    /// 元数据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    /// 服务等级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    /// Prompt 缓存键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    /// Prompt 缓存选项（gpt-5.6+）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_options: Option<PromptCacheOptions>,
    /// Prompt 缓存保留策略（已弃用；推荐 `prompt_cache_options.ttl`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_retention: Option<String>,
    /// 安全标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,
    /// 最大工具调用次数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<u32>,
    /// 流式选项（仅 `Responses::stream` 生效）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<ResponsesStreamOptions>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ResponsesRequest {
    /// 创建构建器。
    pub fn builder() -> ResponsesRequestBuilder {
        ResponsesRequestBuilder::default()
    }
}

/// [`ResponsesRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct ResponsesRequestBuilder {
    model: Option<ModelId>,
    input: Option<ResponsesInput>,
    previous_response_id: Option<String>,
    instructions: Option<String>,
    max_output_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    top_logprobs: Option<u32>,
    tools: Option<Vec<ResponsesTool>>,
    tool_choice: Option<ResponsesToolChoice>,
    parallel_tool_calls: Option<bool>,
    reasoning: Option<Reasoning>,
    text: Option<TextConfig>,
    context_management: Option<Vec<ContextManagement>>,
    prompt: Option<Prompt>,
    user: Option<String>,
    store: Option<bool>,
    background: Option<bool>,
    include: Option<Vec<String>>,
    truncation: Option<String>,
    conversation: Option<Value>,
    metadata: Option<Value>,
    service_tier: Option<String>,
    prompt_cache_key: Option<String>,
    prompt_cache_options: Option<PromptCacheOptions>,
    prompt_cache_retention: Option<String>,
    safety_identifier: Option<String>,
    max_tool_calls: Option<u32>,
    stream_options: Option<ResponsesStreamOptions>,
    extensions: Extensions,
}

impl ResponsesRequestBuilder {
    /// 设置模型（必填）。
    #[must_use]
    pub fn model(mut self, model: impl Into<ModelId>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// 设置纯文本输入。
    #[must_use]
    pub fn input(mut self, input: impl Into<String>) -> Self {
        self.input = Some(ResponsesInput::Text(input.into()));
        self
    }

    /// 设置 item 列表输入（适合 Agent 循环复用完整历史）。
    #[must_use]
    pub fn input_items(mut self, items: impl IntoIterator<Item = InputItem>) -> Self {
        self.input = Some(ResponsesInput::Items(items.into_iter().collect()));
        self
    }

    /// 基于已有响应继续。
    #[must_use]
    pub fn previous_response_id(mut self, id: impl Into<String>) -> Self {
        self.previous_response_id = Some(id.into());
        self
    }

    /// 设置系统指令。
    #[must_use]
    pub fn instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    /// 设置最大输出 token。
    #[must_use]
    pub fn max_output_tokens(mut self, tokens: u32) -> Self {
        self.max_output_tokens = Some(tokens);
        self
    }

    /// 设置 temperature。
    #[must_use]
    pub fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// 设置 top_p。
    #[must_use]
    pub fn top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    /// 设置每个 token 位置返回的候选数。
    #[must_use]
    pub fn top_logprobs(mut self, top_logprobs: u32) -> Self {
        self.top_logprobs = Some(top_logprobs);
        self
    }

    /// 设置工具（函数工具或服务端工具）。
    #[must_use]
    pub fn tools(mut self, tools: impl IntoIterator<Item = ResponsesTool>) -> Self {
        self.tools = Some(tools.into_iter().collect());
        self
    }

    /// 设置工具选择（也可传 Chat 的 [`crate::openai::chat::ToolChoice`]：
    /// 实现了 `Into<ResponsesToolChoice>`）。
    #[must_use]
    pub fn tool_choice(mut self, tool_choice: impl Into<ResponsesToolChoice>) -> Self {
        self.tool_choice = Some(tool_choice.into());
        self
    }

    /// 设置是否允许并行工具调用。
    #[must_use]
    pub fn parallel_tool_calls(mut self, allowed: bool) -> Self {
        self.parallel_tool_calls = Some(allowed);
        self
    }

    /// 设置推理投入等级。
    #[must_use]
    pub fn reasoning_effort(mut self, effort: ReasoningEffort) -> Self {
        self.reasoning.get_or_insert_with(Reasoning::default).effort = Some(effort);
        self
    }

    /// 设置 reasoning 摘要方式。
    #[must_use]
    pub fn reasoning_summary(mut self, summary: impl Into<String>) -> Self {
        self.reasoning
            .get_or_insert_with(Reasoning::default)
            .summary = Some(summary.into());
        self
    }

    /// 设置文本输出格式。
    #[must_use]
    pub fn text_format(mut self, format: TextFormat) -> Self {
        self.text.get_or_insert_with(TextConfig::default).format = Some(format);
        self
    }

    /// 设置文本详细程度。
    #[must_use]
    pub fn text_verbosity(mut self, verbosity: impl Into<String>) -> Self {
        self.text.get_or_insert_with(TextConfig::default).verbosity = Some(verbosity.into());
        self
    }

    /// 设置上下文管理（服务端压缩）。
    #[must_use]
    pub fn context_management(
        mut self,
        entries: impl IntoIterator<Item = ContextManagement>,
    ) -> Self {
        self.context_management = Some(entries.into_iter().collect());
        self
    }

    /// 设置可复用提示词模板。
    #[must_use]
    pub fn prompt(mut self, prompt: Prompt) -> Self {
        self.prompt = Some(prompt);
        self
    }

    /// 设置终端用户标识。
    #[must_use]
    pub fn user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// 是否存储响应。
    #[must_use]
    pub fn store(mut self, store: bool) -> Self {
        self.store = Some(store);
        self
    }

    /// 是否后台执行。
    #[must_use]
    pub fn background(mut self, background: bool) -> Self {
        self.background = Some(background);
        self
    }

    /// 追加额外返回字段。
    #[must_use]
    pub fn include(mut self, include: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.include = Some(include.into_iter().map(Into::into).collect());
        self
    }

    /// 设置截断策略。
    #[must_use]
    pub fn truncation(mut self, truncation: impl Into<String>) -> Self {
        self.truncation = Some(truncation.into());
        self
    }

    /// 设置会话（字符串 ID 或对象）。
    #[must_use]
    pub fn conversation(mut self, conversation: Value) -> Self {
        self.conversation = Some(conversation);
        self
    }

    /// 设置元数据。
    #[must_use]
    pub fn metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// 设置服务等级。
    #[must_use]
    pub fn service_tier(mut self, service_tier: impl Into<String>) -> Self {
        self.service_tier = Some(service_tier.into());
        self
    }

    /// 设置 Prompt 缓存键。
    #[must_use]
    pub fn prompt_cache_key(mut self, key: impl Into<String>) -> Self {
        self.prompt_cache_key = Some(key.into());
        self
    }

    /// 设置 Prompt 缓存选项（gpt-5.6+）。
    #[must_use]
    pub fn prompt_cache_options(mut self, options: PromptCacheOptions) -> Self {
        self.prompt_cache_options = Some(options);
        self
    }

    /// 设置 Prompt 缓存保留策略（已弃用）。
    #[must_use]
    pub fn prompt_cache_retention(mut self, retention: impl Into<String>) -> Self {
        self.prompt_cache_retention = Some(retention.into());
        self
    }

    /// 设置安全标识。
    #[must_use]
    pub fn safety_identifier(mut self, identifier: impl Into<String>) -> Self {
        self.safety_identifier = Some(identifier.into());
        self
    }

    /// 设置最大工具调用次数。
    #[must_use]
    pub fn max_tool_calls(mut self, max_tool_calls: u32) -> Self {
        self.max_tool_calls = Some(max_tool_calls);
        self
    }

    /// 设置流式选项（仅 `Responses::stream` 生效）。
    #[must_use]
    pub fn stream_options(mut self, options: ResponsesStreamOptions) -> Self {
        self.stream_options = Some(options);
        self
    }

    /// 添加协议扩展字段。
    #[must_use]
    pub fn extension(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
    }

    /// 构建请求。
    pub fn build(self) -> Result<ResponsesRequest, AirxError> {
        let model = self.model.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("ResponsesRequest.model 必填"))
        })?;

        Ok(ResponsesRequest {
            model,
            input: self.input,
            previous_response_id: self.previous_response_id,
            instructions: self.instructions,
            max_output_tokens: self.max_output_tokens,
            temperature: self.temperature,
            top_p: self.top_p,
            top_logprobs: self.top_logprobs,
            tools: self.tools,
            tool_choice: self.tool_choice,
            parallel_tool_calls: self.parallel_tool_calls,
            reasoning: self.reasoning,
            text: self.text,
            context_management: self.context_management,
            prompt: self.prompt,
            user: self.user,
            store: self.store,
            background: self.background,
            include: self.include,
            truncation: self.truncation,
            conversation: self.conversation,
            metadata: self.metadata,
            service_tier: self.service_tier,
            prompt_cache_key: self.prompt_cache_key,
            prompt_cache_options: self.prompt_cache_options,
            prompt_cache_retention: self.prompt_cache_retention,
            safety_identifier: self.safety_identifier,
            max_tool_calls: self.max_tool_calls,
            stream_options: self.stream_options,
            extensions: self.extensions,
        })
    }
}

#[cfg(test)]
mod alignment_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reasoning_and_text_format_serialize() {
        let request = ResponsesRequest::builder()
            .model("gpt-5")
            .input("hi")
            .reasoning_effort(ReasoningEffort::Medium)
            .text_format(TextFormat::JsonSchema {
                name: "poster".to_owned(),
                description: None,
                schema: json!({"type": "object"}),
                strict: Some(true),
            })
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["reasoning"]["effort"], "medium");
        assert_eq!(value["text"]["format"]["type"], "json_schema");
        assert_eq!(value["text"]["format"]["name"], "poster");
        assert_eq!(value["text"]["format"]["strict"], true);
    }

    #[test]
    fn input_item_roundtrips_reasoning_and_multimodal() {
        let items = vec![
            InputItem::Message {
                role: "user".to_owned(),
                content: InputMessageContent::Parts(vec![
                    InputContent::Text {
                        text: "describe".to_owned(),
                    },
                    InputContent::Image {
                        image_url: Some("https://example.com/cat.png".to_owned()),
                        file_id: None,
                        detail: Some("auto".to_owned()),
                        extensions: Extensions::new(),
                    },
                ]),
            },
            InputItem::Reasoning {
                id: Some("rs_1".to_owned()),
                summary: Some(vec![ReasoningSummary::text("thought")]),
                encrypted_content: Some("cipher".to_owned()),
                extensions: Extensions::new(),
            },
            InputItem::FunctionCall {
                call_id: "call_1".to_owned(),
                name: "lookup".to_owned(),
                arguments: "{\"q\":1}".to_owned(),
            },
            InputItem::FunctionCallOutput {
                call_id: "call_1".to_owned(),
                output: "ok".to_owned(),
            },
            InputItem::ItemReference {
                id: "item_1".to_owned(),
            },
        ];

        let value = serde_json::to_value(&items).expect("serialize");
        let back: Vec<InputItem> = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, items);
    }

    #[test]
    fn new_request_fields_serialize() {
        let request = ResponsesRequest::builder()
            .model("gpt-5")
            .previous_response_id("resp_1")
            .store(true)
            .include(["reasoning.encrypted_content"])
            .reasoning_summary("detailed")
            .text_verbosity("low")
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["previous_response_id"], "resp_1");
        assert_eq!(value["store"], true);
        assert_eq!(value["include"][0], "reasoning.encrypted_content");
        assert_eq!(value["reasoning"]["summary"], "detailed");
        assert_eq!(value["text"]["verbosity"], "low");
    }
}

#[cfg(test)]
mod replay_tests {
    use super::*;
    use serde_json::json;

    fn response() -> ResponsesResponse {
        serde_json::from_value(json!({
            "id": "resp_1",
            "output": [
                {
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "你好", "annotations": []}]
                },
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "lookup",
                    "arguments": "{\"q\":1}"
                },
                {
                    "type": "reasoning",
                    "id": "rs_1",
                    "summary": [],
                    "encrypted_content": "cipher"
                },
                {"type": "web_search_call", "id": "ws_1", "status": "completed"}
            ]
        }))
        .expect("response")
    }

    #[test]
    fn output_items_convert_to_input_items() {
        let items = response().into_input();
        assert_eq!(items.len(), 4);
        match &items[0] {
            InputItem::Message { role, content } => {
                assert_eq!(role, "assistant");
                assert_eq!(content.as_text(), Some("你好"));
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(
            items[1],
            InputItem::function_call("call_1", "lookup", "{\"q\":1}")
        );
        match &items[2] {
            InputItem::Reasoning {
                id,
                encrypted_content,
                ..
            } => {
                assert_eq!(id.as_deref(), Some("rs_1"));
                assert_eq!(encrypted_content.as_deref(), Some("cipher"));
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert!(matches!(items[3], InputItem::Other(_)));
    }

    #[test]
    fn replayed_items_keep_wire_shape() {
        let value = serde_json::to_value(response().into_input()).expect("serialize");
        assert_eq!(value[0]["type"], "message");
        assert_eq!(value[0]["role"], "assistant");
        assert_eq!(value[0]["content"], "你好");
        assert_eq!(value[1]["type"], "function_call");
        assert_eq!(value[1]["call_id"], "call_1");
        assert_eq!(value[2]["type"], "reasoning");
        assert_eq!(value[2]["encrypted_content"], "cipher");
        assert_eq!(value[3]["type"], "web_search_call");
    }

    #[test]
    fn input_item_constructors_serialize() {
        assert_eq!(
            serde_json::to_value(InputItem::user("hi")).expect("serialize"),
            json!({"type": "message", "role": "user", "content": "hi"})
        );
        assert_eq!(
            serde_json::to_value(InputItem::function_call_output("call_1", "42"))
                .expect("serialize"),
            json!({"type": "function_call_output", "call_id": "call_1", "output": "42"})
        );
    }

    #[test]
    fn new_p2_request_fields_serialize() {
        let request = ResponsesRequest::builder()
            .model("gpt-5.6")
            .context_management([ContextManagement::compaction(Some(120_000))])
            .top_logprobs(3)
            .user("user-1")
            .prompt(Prompt {
                id: "pmpt_1".to_owned(),
                version: Some("2".to_owned()),
                variables: Some(json!({"name": "Ada"})),
                extensions: Extensions::new(),
            })
            .prompt_cache_options(PromptCacheOptions {
                mode: Some("explicit".to_owned()),
                ttl: Some("12h".to_owned()),
                prewarm: None,
                comparison_response_id: None,
                extensions: Extensions::new(),
            })
            .prompt_cache_retention("24h")
            .stream_options(ResponsesStreamOptions {
                include_obfuscation: Some(false),
            })
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["context_management"][0]["type"], "compaction");
        assert_eq!(value["context_management"][0]["compact_threshold"], 120_000);
        assert_eq!(value["top_logprobs"], 3);
        assert_eq!(value["user"], "user-1");
        assert_eq!(value["prompt"]["id"], "pmpt_1");
        assert_eq!(value["prompt"]["version"], "2");
        assert_eq!(value["prompt"]["variables"]["name"], "Ada");
        assert_eq!(value["prompt_cache_options"]["mode"], "explicit");
        assert_eq!(value["prompt_cache_options"]["ttl"], "12h");
        assert_eq!(value["prompt_cache_retention"], "24h");
        assert_eq!(value["stream_options"]["include_obfuscation"], false);
    }

    #[test]
    fn responses_tool_choice_uses_flat_shape() {
        let request = ResponsesRequest::builder()
            .model("gpt-5")
            .tools([ResponsesTool::function(
                "lookup",
                json!({"type": "object"}),
                None,
            )])
            .tool_choice(ResponsesToolChoice::function("lookup"))
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["tools"][0]["type"], "function");
        assert_eq!(value["tools"][0]["name"], "lookup");
        assert!(value["tools"][0].get("function").is_none());
        assert_eq!(
            value["tool_choice"],
            json!({"type": "function", "name": "lookup"})
        );
    }
}

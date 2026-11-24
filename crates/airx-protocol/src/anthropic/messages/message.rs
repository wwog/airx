//! Anthropic 消息与内容块。
//!
//! 未知内容块保留为 `Other(Value)`，不因新增块类型而解码失败。

use airx_core::json::Extensions;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

/// 消息角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// 用户。
    User,
    /// 助手。
    Assistant,
    /// 系统（对话中途插入系统指令）。
    System,
}

impl Role {
    /// 系统角色。
    pub fn system() -> Self {
        Self::System
    }
}

/// 消息内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// 纯文本。
    Text(String),
    /// 内容块。
    Blocks(Vec<ContentBlock>),
}

/// 内容块。
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ContentBlock {
    /// 文本。
    Text(TextBlock),
    /// 图片。
    Image(ImageBlock),
    /// 文档（PDF / 纯文本 / URL / Files API 文件）。
    Document(DocumentBlock),
    /// 工具调用。
    ToolUse(ToolUseBlock),
    /// 工具结果。
    ToolResult(ToolResultBlock),
    /// thinking。
    Thinking(ThinkingBlock),
    /// 加密 thinking。
    RedactedThinking(RedactedThinkingBlock),
    /// 服务端工具调用（联网搜索 / 抓取 / 代码执行等，由 Anthropic 后台执行）。
    ServerToolUse(ServerToolUseBlock),
    /// 服务端联网搜索结果。
    WebSearchToolResult(WebSearchToolResultBlock),
    /// 未识别类型。
    Other(Value),
}

/// 缓存断点（prompt caching 的 `cache_control`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheControl {
    /// 固定为 `ephemeral`。
    #[serde(rename = "type")]
    pub kind: CacheKind,
    /// 缓存有效期。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<CacheTtl>,
}

impl CacheControl {
    /// 默认 5 分钟缓存断点。
    pub fn ephemeral() -> Self {
        Self::default()
    }

    /// 指定缓存有效期。
    #[must_use]
    pub fn with_ttl(mut self, ttl: CacheTtl) -> Self {
        self.ttl = Some(ttl);
        self
    }
}

/// 缓存断点类型。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheKind {
    /// `ephemeral`。
    #[default]
    Ephemeral,
}

/// 缓存有效期。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheTtl {
    /// 5 分钟（默认）。
    #[serde(rename = "5m")]
    FiveMinutes,
    /// 1 小时。
    #[serde(rename = "1h")]
    OneHour,
}

/// 引用开关（`citations`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitationsConfig {
    /// 是否开启。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// 文本块。
#[derive(Debug, Clone, PartialEq)]
pub struct TextBlock {
    /// 文本。
    pub text: String,
    /// 未识别的扩展字段。
    ///
    /// `cache_control` 与 `citations` 也存放在这里（官方字段，保持结构体字面量
    /// 构造不破坏），分别用 [`TextBlock::cached`] 与 [`TextBlock::citations`] 访问。
    pub extensions: Extensions,
}

impl Serialize for TextBlock {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = match serde_json::to_value(&self.extensions) {
            Ok(Value::Object(map)) => map,
            _ => Map::new(),
        };
        map.remove("type");
        map.insert("text".to_owned(), Value::String(self.text.clone()));
        map.insert("type".to_owned(), Value::String("text".to_owned()));
        Value::Object(map).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TextBlock {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Inner {
            text: String,
            #[serde(flatten)]
            extensions: Extensions,
        }

        let mut value = Value::deserialize(deserializer)?;
        if let Value::Object(map) = &mut value {
            map.remove("type");
        }
        let inner = serde_json::from_value::<Inner>(value).map_err(serde::de::Error::custom)?;
        Ok(Self {
            text: inner.text,
            extensions: inner.extensions,
        })
    }
}

impl TextBlock {
    /// 构造文本块。
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            extensions: Extensions::new(),
        }
    }

    /// 设置缓存断点（写入 `cache_control`）。
    #[must_use]
    pub fn cached(mut self, control: CacheControl) -> Self {
        if let Ok(value) = serde_json::to_value(control) {
            self.extensions.insert("cache_control".to_owned(), value);
        }
        self
    }

    /// 读取 `citations`（响应侧；不存在时返回空数组）。
    pub fn citations(&self) -> Vec<Value> {
        match self.extensions.get("citations") {
            Some(Value::Array(items)) => items.clone(),
            _ => Vec::new(),
        }
    }

    /// 追加一条 citation（流式聚合使用，非 agent 行为）。
    pub fn push_citation(&mut self, citation: Value) {
        let entry = self
            .extensions
            .entry("citations".to_owned())
            .or_insert_with(|| Value::Array(Vec::new()));
        if !entry.is_array() {
            *entry = Value::Array(Vec::new());
        }
        if let Value::Array(items) = entry {
            items.push(citation);
        }
    }
}

/// 图片块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageBlock {
    /// 图片来源（base64 / url）。
    pub source: Value,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 文档块（`type: "document"`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentBlock {
    /// 文档来源。
    pub source: DocumentSource,
    /// 文档标题。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// 文档上下文说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// 引用开关。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationsConfig>,
    /// 缓存断点。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl DocumentBlock {
    /// 用文档来源构造。
    pub fn new(source: DocumentSource) -> Self {
        Self {
            source,
            title: None,
            context: None,
            citations: None,
            cache_control: None,
            extensions: Extensions::new(),
        }
    }
}

/// 文档来源。
///
/// 官方 `source` 是 `base64 | text | url | file | content` 的联合，未知来源保留为
/// [`DocumentSource::Other`]，序列化时原样回写。
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DocumentSource {
    /// base64 编码的 PDF（`{"type":"base64","media_type":"application/pdf","data":...}`）。
    Base64Pdf {
        /// base64 数据。
        data: String,
        /// MIME 类型，默认 `application/pdf`。
        media_type: Option<String>,
    },
    /// 纯文本（`{"type":"text","media_type":"text/plain","data":...}`）。
    PlainText {
        /// 文本内容。
        data: String,
        /// MIME 类型，默认 `text/plain`。
        media_type: Option<String>,
    },
    /// URL（`{"type":"url","url":...}`）。
    Url {
        /// 文档 URL。
        url: String,
    },
    /// Files API 文件（`{"type":"file","file_id":...}`）。
    File {
        /// 文件 ID。
        file_id: String,
    },
    /// 内联内容块（`{"type":"content","content":...}`）。
    Content {
        /// 字符串或内容块数组。
        content: Value,
    },
    /// 未识别来源。
    Other(Value),
}

impl DocumentSource {
    /// base64 PDF。
    pub fn base64_pdf(data: impl Into<String>) -> Self {
        Self::Base64Pdf {
            data: data.into(),
            media_type: None,
        }
    }

    /// 纯文本。
    pub fn plain_text(data: impl Into<String>) -> Self {
        Self::PlainText {
            data: data.into(),
            media_type: None,
        }
    }

    /// URL。
    pub fn url(url: impl Into<String>) -> Self {
        Self::Url { url: url.into() }
    }

    /// Files API 文件。
    pub fn file(file_id: impl Into<String>) -> Self {
        Self::File {
            file_id: file_id.into(),
        }
    }
}

impl Serialize for DocumentSource {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = Map::new();
        match self {
            Self::Base64Pdf { data, media_type } => {
                map.insert("type".to_owned(), Value::String("base64".to_owned()));
                map.insert(
                    "media_type".to_owned(),
                    Value::String(
                        media_type
                            .clone()
                            .unwrap_or_else(|| "application/pdf".to_owned()),
                    ),
                );
                map.insert("data".to_owned(), Value::String(data.clone()));
            }
            Self::PlainText { data, media_type } => {
                map.insert("type".to_owned(), Value::String("text".to_owned()));
                map.insert(
                    "media_type".to_owned(),
                    Value::String(
                        media_type
                            .clone()
                            .unwrap_or_else(|| "text/plain".to_owned()),
                    ),
                );
                map.insert("data".to_owned(), Value::String(data.clone()));
            }
            Self::Url { url } => {
                map.insert("type".to_owned(), Value::String("url".to_owned()));
                map.insert("url".to_owned(), Value::String(url.clone()));
            }
            Self::File { file_id } => {
                map.insert("type".to_owned(), Value::String("file".to_owned()));
                map.insert("file_id".to_owned(), Value::String(file_id.clone()));
            }
            Self::Content { content } => {
                map.insert("type".to_owned(), Value::String("content".to_owned()));
                map.insert("content".to_owned(), content.clone());
            }
            Self::Other(value) => return value.serialize(serializer),
        }
        Value::Object(map).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DocumentSource {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();

        let text = |field: &str| -> Option<String> {
            value.get(field).and_then(Value::as_str).map(str::to_owned)
        };

        Ok(match kind {
            "base64" => Self::Base64Pdf {
                data: text("data").unwrap_or_default(),
                media_type: text("media_type"),
            },
            "text" => Self::PlainText {
                data: text("data").unwrap_or_default(),
                media_type: text("media_type"),
            },
            "url" => Self::Url {
                url: text("url").unwrap_or_default(),
            },
            "file" => Self::File {
                file_id: text("file_id").unwrap_or_default(),
            },
            "content" => Self::Content {
                content: value.get("content").cloned().unwrap_or(Value::Null),
            },
            _ => Self::Other(value),
        })
    }
}

/// 工具调用块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolUseBlock {
    /// 调用 ID。
    pub id: String,
    /// 工具名。
    pub name: String,
    /// 输入参数。
    #[serde(default)]
    pub input: Value,
    /// 调用来源（`direct` / 服务端工具代调）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<Value>,
    /// toolset 成员工具所属的 toolset 名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolset_name: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ToolUseBlock {
    /// 构造工具调用块。
    pub fn new(id: impl Into<String>, name: impl Into<String>, input: Value) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            input,
            caller: None,
            toolset_name: None,
            extensions: Extensions::new(),
        }
    }
}

/// 工具结果块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResultBlock {
    /// 对应的调用 ID。
    pub tool_use_id: String,
    /// 结果内容（字符串或内容块数组）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
    /// 是否错误。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    /// toolset 成员工具所属的 toolset 名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolset_name: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// thinking 块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThinkingBlock {
    /// 思考文本。
    pub thinking: String,
    /// 签名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 加密 thinking 块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedactedThinkingBlock {
    /// 加密数据。
    pub data: String,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 服务端工具调用块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerToolUseBlock {
    /// 调用 ID。
    pub id: String,
    /// 工具名（`web_search` / `web_fetch` / `code_execution` 等）。
    pub name: String,
    /// 输入参数。
    #[serde(default)]
    pub input: Value,
    /// 调用来源（直接调用 / 其他服务端工具）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 服务端联网搜索结果块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchToolResultBlock {
    /// 对应的服务端调用 ID。
    pub tool_use_id: String,
    /// 结果数组或错误对象（结构随官方演进，原样保留）。
    #[serde(default)]
    pub content: Value,
    /// 调用来源。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl Serialize for ContentBlock {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (kind, value) = match self {
            Self::Text(block) => ("text", serde_json::to_value(block)),
            Self::Image(block) => ("image", serde_json::to_value(block)),
            Self::Document(block) => ("document", serde_json::to_value(block)),
            Self::ToolUse(block) => ("tool_use", serde_json::to_value(block)),
            Self::ToolResult(block) => ("tool_result", serde_json::to_value(block)),
            Self::Thinking(block) => ("thinking", serde_json::to_value(block)),
            Self::RedactedThinking(block) => ("redacted_thinking", serde_json::to_value(block)),
            Self::ServerToolUse(block) => ("server_tool_use", serde_json::to_value(block)),
            Self::WebSearchToolResult(block) => {
                ("web_search_tool_result", serde_json::to_value(block))
            }
            Self::Other(value) => return value.serialize(serializer),
        };
        let mut value = value.map_err(serde::ser::Error::custom)?;
        if let Value::Object(map) = &mut value {
            map.insert("type".to_owned(), Value::String(kind.to_owned()));
        }
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ContentBlock {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();

        macro_rules! parse {
            ($ty:ty, $variant:ident) => {{
                if let Value::Object(map) = &mut value {
                    map.remove("type");
                }
                return serde_json::from_value::<$ty>(value)
                    .map(Self::$variant)
                    .map_err(serde::de::Error::custom);
            }};
        }

        match kind.as_str() {
            "text" => parse!(TextBlock, Text),
            "image" => parse!(ImageBlock, Image),
            "document" => parse!(DocumentBlock, Document),
            "tool_use" => parse!(ToolUseBlock, ToolUse),
            "tool_result" => parse!(ToolResultBlock, ToolResult),
            "thinking" => parse!(ThinkingBlock, Thinking),
            "redacted_thinking" => parse!(RedactedThinkingBlock, RedactedThinking),
            "server_tool_use" => parse!(ServerToolUseBlock, ServerToolUse),
            "web_search_tool_result" => parse!(WebSearchToolResultBlock, WebSearchToolResult),
            _ => Ok(Self::Other(value)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_blocks_are_preserved() {
        let blocks: Vec<ContentBlock> = serde_json::from_value(json!([
            {"type": "text", "text": "hi"},
            {"type": "future_block", "id": "future_1"},
            {"type": "thinking", "thinking": "..."}
        ]))
        .expect("blocks");

        assert!(matches!(blocks[0], ContentBlock::Text(_)));
        assert!(matches!(blocks[1], ContentBlock::Other(_)));
        assert!(matches!(blocks[2], ContentBlock::Thinking(_)));

        let value = serde_json::to_value(&blocks).expect("serialize");
        assert_eq!(value[1]["id"], "future_1");
        assert_eq!(value[1]["type"], "future_block");
    }

    #[test]
    fn text_message_serializes_as_plain_string() {
        let content = MessageContent::Text("hello".to_owned());
        assert_eq!(
            serde_json::to_value(content).expect("serialize"),
            json!("hello")
        );
    }

    #[test]
    fn system_role_round_trips() {
        let role: Role = serde_json::from_value(json!("system")).expect("role");
        assert_eq!(role, Role::System);
        assert_eq!(serde_json::to_value(role).expect("serialize"), "system");
    }

    #[test]
    fn document_blocks_round_trip_all_sources() {
        let blocks: Vec<ContentBlock> = serde_json::from_value(json!([
            {"type": "document", "source": {"type": "base64", "media_type": "application/pdf", "data": "AAA"}, "title": "invoice", "citations": {"enabled": true}},
            {"type": "document", "source": {"type": "text", "media_type": "text/plain", "data": "hello"}},
            {"type": "document", "source": {"type": "url", "url": "https://example.com/a.pdf"}},
            {"type": "document", "source": {"type": "file", "file_id": "file_1"}},
            {"type": "document", "source": {"type": "content", "content": [{"type": "text", "text": "x"}]}},
            {"type": "document", "source": {"type": "future_source", "blob": 1}}
        ]))
        .expect("documents");

        match &blocks[0] {
            ContentBlock::Document(block) => {
                assert_eq!(
                    block.source,
                    DocumentSource::Base64Pdf {
                        data: "AAA".to_owned(),
                        media_type: Some("application/pdf".to_owned())
                    }
                );
                assert_eq!(block.title.as_deref(), Some("invoice"));
                assert_eq!(
                    block.citations,
                    Some(CitationsConfig {
                        enabled: Some(true)
                    })
                );
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert!(matches!(
            &blocks[1],
            ContentBlock::Document(DocumentBlock {
                source: DocumentSource::PlainText { .. },
                ..
            })
        ));
        assert!(matches!(
            &blocks[2],
            ContentBlock::Document(DocumentBlock {
                source: DocumentSource::Url { .. },
                ..
            })
        ));
        assert!(matches!(
            &blocks[3],
            ContentBlock::Document(DocumentBlock {
                source: DocumentSource::File { .. },
                ..
            })
        ));
        assert!(matches!(
            &blocks[4],
            ContentBlock::Document(DocumentBlock {
                source: DocumentSource::Content { .. },
                ..
            })
        ));
        assert!(matches!(
            &blocks[5],
            ContentBlock::Document(DocumentBlock {
                source: DocumentSource::Other(_),
                ..
            })
        ));

        let value = serde_json::to_value(&blocks).expect("serialize");
        assert_eq!(value[0]["type"], "document");
        assert_eq!(value[0]["source"]["type"], "base64");
        assert_eq!(value[0]["source"]["media_type"], "application/pdf");
        assert_eq!(value[2]["source"]["type"], "url");
        assert_eq!(value[5]["source"]["blob"], 1);
    }

    #[test]
    fn document_source_helpers_default_media_types() {
        assert_eq!(
            serde_json::to_value(DocumentSource::base64_pdf("AAA")).expect("serialize"),
            json!({"type": "base64", "media_type": "application/pdf", "data": "AAA"})
        );
        assert_eq!(
            serde_json::to_value(DocumentSource::plain_text("hi")).expect("serialize"),
            json!({"type": "text", "media_type": "text/plain", "data": "hi"})
        );
    }

    #[test]
    fn text_block_cache_control_and_citations_use_extensions() {
        let mut block =
            TextBlock::new("hi").cached(CacheControl::ephemeral().with_ttl(CacheTtl::OneHour));
        block.push_citation(json!({"type": "char_location", "cited_text": "hi"}));

        let value = serde_json::to_value(&block).expect("serialize");
        assert_eq!(value["cache_control"]["type"], "ephemeral");
        assert_eq!(value["cache_control"]["ttl"], "1h");
        assert_eq!(value["citations"][0]["cited_text"], "hi");
        assert_eq!(block.citations().len(), 1);
    }

    #[test]
    fn tool_use_blocks_keep_caller_and_toolset() {
        let block: ContentBlock = serde_json::from_value(json!({
            "type": "tool_use", "id": "toolu_1", "name": "lookup", "input": {},
            "caller": {"type": "direct"}, "toolset_name": "crm"
        }))
        .expect("block");

        match &block {
            ContentBlock::ToolUse(block) => {
                assert_eq!(block.caller, Some(json!({"type": "direct"})));
                assert_eq!(block.toolset_name.as_deref(), Some("crm"));
            }
            other => panic!("unexpected: {other:?}"),
        }

        let value = serde_json::to_value(&block).expect("serialize");
        assert_eq!(value["caller"]["type"], "direct");
        assert_eq!(value["toolset_name"], "crm");
    }
}

#[cfg(test)]
mod server_tool_block_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn server_tool_use_and_web_search_result_are_typed() {
        let blocks: Vec<ContentBlock> = serde_json::from_value(json!([
            {"type": "server_tool_use", "id": "srv_1", "name": "web_search", "input": {"query": "rust"}},
            {"type": "web_search_tool_result", "tool_use_id": "srv_1", "content": [
                {"type": "web_search_result", "url": "https://example.com", "title": "Rust"}
            ]}
        ]))
        .expect("blocks");

        assert!(
            matches!(blocks[0], ContentBlock::ServerToolUse(_)),
            "block0 = {:?}",
            blocks[0]
        );
        assert!(matches!(blocks[1], ContentBlock::WebSearchToolResult(_)));

        let value = serde_json::to_value(&blocks).expect("serialize");
        assert_eq!(value[0]["name"], "web_search");
        assert_eq!(value[1]["content"][0]["type"], "web_search_result");
    }
}

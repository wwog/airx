//! Chat 消息类型。

use airx_core::json::Extensions;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use super::tool::ToolCall;

/// 用户消息内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserContent {
    /// 纯文本。
    Text(String),
    /// 多模态片段。
    Parts(Vec<ContentPart>),
}

impl UserContent {
    /// 文本内容（多模态时返回 None）。
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Parts(_) => None,
        }
    }
}

/// 多模态片段。
///
/// 覆盖官方 `ChatCompletionContentPart`：`text` / `image_url` / `input_audio` /
/// `file`；未识别类型进入 [`ContentPart::Other`]，保证新片段类型不丢字段。
#[derive(Debug, Clone, PartialEq)]
pub enum ContentPart {
    /// 文本。
    Text {
        /// 文本内容。
        text: String,
    },
    /// 图片 URL。
    ImageUrl {
        /// 图片地址。
        image_url: ImageUrl,
    },
    /// 音频输入（base64）。
    InputAudio {
        /// 音频数据。
        input_audio: InputAudio,
    },
    /// 文件输入（已上传 ID 或内联 base64）。
    File {
        /// 文件。
        file: FilePart,
    },
    /// 未识别片段。
    Other(Value),
}

impl ContentPart {
    /// 构造文本片段。
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// 构造图片片段。
    pub fn image_url(url: impl Into<String>) -> Self {
        Self::ImageUrl {
            image_url: ImageUrl {
                url: url.into(),
                detail: None,
                extensions: Extensions::new(),
            },
        }
    }

    /// 构造音频片段。
    pub fn input_audio(data: impl Into<String>, format: impl Into<String>) -> Self {
        Self::InputAudio {
            input_audio: InputAudio {
                data: data.into(),
                format: format.into(),
                extensions: Extensions::new(),
            },
        }
    }

    /// 构造已上传文件片段。
    pub fn file_id(file_id: impl Into<String>) -> Self {
        Self::File {
            file: FilePart {
                file_id: Some(file_id.into()),
                file_data: None,
                filename: None,
                extensions: Extensions::new(),
            },
        }
    }

    /// 构造内联文件片段。
    pub fn file_data(filename: impl Into<String>, data: impl Into<String>) -> Self {
        Self::File {
            file: FilePart {
                file_id: None,
                file_data: Some(data.into()),
                filename: Some(filename.into()),
                extensions: Extensions::new(),
            },
        }
    }
}

impl Serialize for ContentPart {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Text { text } => {
                Value::Object(named_map("text", [("text", Value::String(text.clone()))]))
                    .serialize(serializer)
            }
            Self::ImageUrl { image_url } => {
                let value = serde_json::to_value(image_url).map_err(serde::ser::Error::custom)?;
                Value::Object(named_map("image_url", [("image_url", value)])).serialize(serializer)
            }
            Self::InputAudio { input_audio } => {
                let value = serde_json::to_value(input_audio).map_err(serde::ser::Error::custom)?;
                Value::Object(named_map("input_audio", [("input_audio", value)]))
                    .serialize(serializer)
            }
            Self::File { file } => {
                let value = serde_json::to_value(file).map_err(serde::ser::Error::custom)?;
                Value::Object(named_map("file", [("file", value)])).serialize(serializer)
            }
            Self::Other(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ContentPart {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();

        match kind.as_str() {
            "text" => {
                remove_key(&mut value, "type");
                let payload: TextPayload =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::Text { text: payload.text })
            }
            "image_url" => {
                remove_key(&mut value, "type");
                let payload: ImageUrlPayload =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::ImageUrl {
                    image_url: payload.image_url,
                })
            }
            "input_audio" => {
                remove_key(&mut value, "type");
                let payload: InputAudioPayload =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::InputAudio {
                    input_audio: payload.input_audio,
                })
            }
            "file" => {
                remove_key(&mut value, "type");
                let payload: FilePayload =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::File { file: payload.file })
            }
            _ => Ok(Self::Other(value)),
        }
    }
}

fn named_map<const N: usize>(kind: &str, fields: [(&str, Value); N]) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("type".to_owned(), Value::String(kind.to_owned()));
    for (key, value) in fields {
        map.insert(key.to_owned(), value);
    }
    map
}

#[derive(Deserialize)]
struct TextPayload {
    text: String,
}

#[derive(Deserialize)]
struct ImageUrlPayload {
    image_url: ImageUrl,
}

#[derive(Deserialize)]
struct InputAudioPayload {
    input_audio: InputAudio,
}

#[derive(Deserialize)]
struct FilePayload {
    file: FilePart,
}

/// 图片地址。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageUrl {
    /// URL 或 data URI。
    pub url: String,
    /// 细节级别（`low` / `high` / `auto`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 音频输入。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputAudio {
    /// base64 编码的音频数据。
    pub data: String,
    /// 音频格式（`wav` / `mp3`）。
    pub format: String,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 文件输入。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilePart {
    /// 已上传文件 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    /// 内联文件数据（base64）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_data: Option<String>,
    /// 文件名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 请求消息。
///
/// 覆盖官方 `ChatCompletionMessageParam` 的 6 种角色
/// （`developer` / `system` / `user` / `assistant` / `tool` / 已弃用的 `function`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum ChatMessage {
    /// 系统指令。
    System {
        /// 文本内容。
        content: String,
        /// 可选的参与者名称。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// 开发者指令（o-series / gpt-5 起取代 system）。
    Developer {
        /// 文本内容。
        content: String,
        /// 可选的参与者名称。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// 用户输入。
    User {
        /// 文本或多模态内容。
        content: UserContent,
        /// 可选的参与者名称。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// 助手历史回复。
    Assistant {
        /// 文本内容（工具调用时可能为 null）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        /// 历史拒答内容。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        refusal: Option<String>,
        /// 历史工具调用。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<ToolCall>>,
        /// 可选的参与者名称。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// 工具执行结果。
    Tool {
        /// 对应的调用 ID。
        tool_call_id: String,
        /// 结果文本。
        content: String,
    },
    /// 已弃用的 function 结果（官方保留旧字段，airx 只做编码往返）。
    Function {
        /// 函数名。
        name: String,
        /// 结果文本。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<String>,
    },
}

impl ChatMessage {
    /// 构造 system 消息。
    pub fn system(content: impl Into<String>) -> Self {
        Self::System {
            content: content.into(),
            name: None,
        }
    }

    /// 构造 developer 消息。
    pub fn developer(content: impl Into<String>) -> Self {
        Self::Developer {
            content: content.into(),
            name: None,
        }
    }

    /// 构造 user 文本消息。
    pub fn user(content: impl Into<String>) -> Self {
        Self::User {
            content: UserContent::Text(content.into()),
            name: None,
        }
    }

    /// 构造 user 多模态消息。
    pub fn user_parts(parts: Vec<ContentPart>) -> Self {
        Self::User {
            content: UserContent::Parts(parts),
            name: None,
        }
    }

    /// 构造 assistant 文本消息。
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::Assistant {
            content: Some(content.into()),
            refusal: None,
            tool_calls: None,
            name: None,
        }
    }

    /// 构造 assistant 工具调用消息。
    pub fn assistant_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self::Assistant {
            content: None,
            refusal: None,
            tool_calls: Some(tool_calls),
            name: None,
        }
    }

    /// 构造工具结果消息。
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::Tool {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
        }
    }

    /// 设置参与者名称（`system` / `developer` / `user` / `assistant`；
    /// 其他角色保持不变）。
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> Self {
        let name = name.into();
        match &mut self {
            Self::System { name: slot, .. }
            | Self::Developer { name: slot, .. }
            | Self::User { name: slot, .. }
            | Self::Assistant { name: slot, .. } => *slot = Some(name),
            Self::Tool { .. } | Self::Function { .. } => {}
        }
        self
    }

    /// 文本内容。
    pub fn content(&self) -> Option<&str> {
        match self {
            Self::System { content, .. } => Some(content),
            Self::Developer { content, .. } => Some(content),
            Self::User { content, .. } => content.as_text(),
            Self::Assistant { content, .. } => content.as_deref(),
            Self::Tool { content, .. } => Some(content),
            Self::Function { content, .. } => content.as_deref(),
        }
    }

    /// 角色名。
    pub fn role(&self) -> &'static str {
        match self {
            Self::System { .. } => "system",
            Self::Developer { .. } => "developer",
            Self::User { .. } => "user",
            Self::Assistant { .. } => "assistant",
            Self::Tool { .. } => "tool",
            Self::Function { .. } => "function",
        }
    }
}

fn remove_key(value: &mut Value, key: &str) {
    if let Value::Object(map) = value {
        map.remove(key);
    }
}

/// 响应消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseMessage {
    /// 角色。
    pub role: String,
    /// 文本内容（工具调用时可能为 null）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// 拒答内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    /// 工具调用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// 引用标注（服务端联网工具返回，例如 DeepSeek / OpenAI web search）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Vec<serde_json::Value>>,
    /// 服务端工具调用记录（DeepSeek）。原样回传即可保持多轮上下文。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_tool_calls: Option<Vec<serde_json::Value>>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ResponseMessage {
    /// 文本内容（若存在）。
    pub fn text(&self) -> Option<&str> {
        self.content.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_message_keeps_plain_string_shape() {
        let message = ChatMessage::user("hello");
        let value = serde_json::to_value(&message).expect("serialize");
        assert_eq!(value, json!({"role": "user", "content": "hello"}));
        assert_eq!(message.content(), Some("hello"));
    }

    #[test]
    fn multimodal_message_uses_parts() {
        let message = ChatMessage::user_parts(vec![
            ContentPart::Text {
                text: "what is this?".to_owned(),
            },
            ContentPart::ImageUrl {
                image_url: ImageUrl {
                    url: "https://example.com/a.png".to_owned(),
                    detail: None,
                    extensions: Extensions::new(),
                },
            },
        ]);
        let value = serde_json::to_value(&message).expect("serialize");
        assert_eq!(value["content"][0]["type"], "text");
        assert_eq!(
            value["content"][1]["image_url"]["url"],
            "https://example.com/a.png"
        );
        assert_eq!(message.content(), None);
    }

    #[test]
    fn assistant_tool_calls_and_tool_result_roundtrip() {
        let history = vec![
            ChatMessage::assistant_tool_calls(vec![ToolCall::function("call_1", "lookup", "{}")]),
            ChatMessage::tool_result("call_1", "42"),
        ];

        let value = serde_json::to_value(&history).expect("serialize");
        assert_eq!(value[0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(value[1]["role"], "tool");
        assert_eq!(value[1]["tool_call_id"], "call_1");
    }

    #[test]
    fn response_message_keeps_unknown_fields() {
        let json = r#"{"role":"assistant","content":null,"audio":{"id":"a1"}}"#;
        let message: ResponseMessage = serde_json::from_str(json).expect("deserialize");
        assert_eq!(message.content, None);
        assert!(message.extensions.contains_key("audio"));
    }

    #[test]
    fn developer_message_roundtrips() {
        let message = ChatMessage::developer("be precise");
        assert_eq!(
            serde_json::to_value(&message).expect("serialize"),
            json!({"role": "developer", "content": "be precise"})
        );
        let back: ChatMessage =
            serde_json::from_value(json!({"role": "developer", "content": "be precise"}))
                .expect("deserialize");
        assert_eq!(back, message);
        assert_eq!(back.role(), "developer");
        assert_eq!(back.content(), Some("be precise"));
    }

    #[test]
    fn all_six_roles_roundtrip() {
        let messages = vec![
            ChatMessage::system("s"),
            ChatMessage::developer("d"),
            ChatMessage::user("u"),
            ChatMessage::assistant("a"),
            ChatMessage::tool_result("call_1", "t"),
            ChatMessage::Function {
                name: "fn".to_owned(),
                content: Some("f".to_owned()),
            },
        ];
        let value = serde_json::to_value(&messages).expect("serialize");
        let roles: Vec<&str> = value
            .as_array()
            .expect("array")
            .iter()
            .map(|item| item["role"].as_str().expect("role"))
            .collect();
        assert_eq!(
            roles,
            vec![
                "system",
                "developer",
                "user",
                "assistant",
                "tool",
                "function"
            ]
        );

        let back: Vec<ChatMessage> = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, messages);
    }

    #[test]
    fn name_is_optional_and_named_sets_it() {
        let message = ChatMessage::user("hi").named("alice");
        let value = serde_json::to_value(&message).expect("serialize");
        assert_eq!(value["name"], "alice");
        assert_eq!(message.role(), "user");

        let value = serde_json::to_value(ChatMessage::user("hi")).expect("serialize");
        assert!(value.get("name").is_none());

        // 角色不再支持 name 时保持不变。
        let tool = ChatMessage::tool_result("c", "x").named("ignored");
        assert_eq!(
            serde_json::to_value(&tool).expect("serialize"),
            json!({"role": "tool", "tool_call_id": "c", "content": "x"})
        );
    }

    #[test]
    fn assistant_refusal_and_tool_calls_are_typed() {
        let message = ChatMessage::Assistant {
            content: None,
            refusal: Some("no".to_owned()),
            tool_calls: Some(vec![ToolCall::function("call_1", "lookup", "{}")]),
            name: None,
        };
        let value = serde_json::to_value(&message).expect("serialize");
        assert_eq!(value["refusal"], "no");
        assert_eq!(value["tool_calls"][0]["function"]["name"], "lookup");
        let back: ChatMessage = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, message);
    }

    #[test]
    fn audio_and_file_parts_encode_as_nested_objects() {
        let audio = ContentPart::input_audio("UklGRg==", "wav");
        assert_eq!(
            serde_json::to_value(&audio).expect("serialize"),
            json!({"type": "input_audio", "input_audio": {"data": "UklGRg==", "format": "wav"}})
        );

        let file = ContentPart::file_id("file-1");
        assert_eq!(
            serde_json::to_value(&file).expect("serialize"),
            json!({"type": "file", "file": {"file_id": "file-1"}})
        );

        let file = ContentPart::file_data("a.pdf", "aGVsbG8=");
        assert_eq!(
            serde_json::to_value(&file).expect("serialize"),
            json!({"type": "file", "file": {"file_data": "aGVsbG8=", "filename": "a.pdf"}})
        );

        // 往返。
        let parsed: ContentPart = serde_json::from_value(json!({
            "type": "file",
            "file": {"file_id": "file-1", "filename": "a.pdf"}
        }))
        .expect("parse");
        match parsed {
            ContentPart::File { file } => {
                assert_eq!(file.file_id.as_deref(), Some("file-1"));
                assert_eq!(file.filename.as_deref(), Some("a.pdf"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn unknown_content_part_is_preserved() {
        let value = json!({"type": "future_part", "payload": 1});
        let part: ContentPart = serde_json::from_value(value.clone()).expect("parse");
        assert!(matches!(part, ContentPart::Other(_)));
        assert_eq!(serde_json::to_value(part).expect("serialize"), value);
    }

    #[test]
    fn text_parts_roundtrip_with_multimodal_user_content() {
        let message = ChatMessage::user_parts(vec![
            ContentPart::text("hi"),
            ContentPart::image_url("https://example.com/a.png"),
        ]);
        let value = serde_json::to_value(&message).expect("serialize");
        assert_eq!(value["content"][0], json!({"type": "text", "text": "hi"}));
        assert_eq!(
            value["content"][1],
            json!({"type": "image_url", "image_url": {"url": "https://example.com/a.png"}})
        );
        let back: ChatMessage = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, message);
    }
}

#[cfg(test)]
mod server_tool_result_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn annotations_and_server_tool_calls_are_typed() {
        let json = json!({
            "role": "assistant",
            "content": "today in rust",
            "annotations": [{
                "type": "url_citation",
                "url": "https://example.com/rust",
                "title": "Rust news"
            }],
            "server_tool_calls": [{
                "type": "web_search",
                "id": "search_1",
                "status": "completed"
            }]
        });

        let message: ResponseMessage = serde_json::from_value(json.clone()).expect("message");
        assert_eq!(message.annotations.as_ref().map(Vec::len), Some(1));
        assert_eq!(message.server_tool_calls.as_ref().map(Vec::len), Some(1));

        let value = serde_json::to_value(&message).expect("serialize");
        assert_eq!(value, json);
    }
}

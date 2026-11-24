//! Responses 工具声明与工具选择。
//!
//! 服务端工具（`web_search` 等）由 OpenAI 后台执行；
//! SDK 只发送声明并解析返回的 `web_search_call` / 引用标注，不发起本地网络请求。
//!
//! 注意：Responses 的函数工具与 `tool_choice` 是**扁平**结构
//! （`{"type":"function","name":...}`），与 Chat Completions 的
//! `{"type":"function","function":{...}}` 不同，因此不复用 Chat 类型。

use airx_core::json::Extensions;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::openai::chat::{ToolChoice, ToolChoiceMode};

/// Responses 工具。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponsesTool {
    /// 客户端函数工具（扁平形状）。
    Function(ResponsesFunctionTool),
    /// 服务端联网搜索。
    WebSearch(WebSearchTool),
    /// 其他服务端工具 / 未来新增工具（原样透传）。
    Other(Value),
}

impl ResponsesTool {
    /// 构造 function 工具。
    ///
    /// 编码为 `{"type":"function","name":...,"parameters":...}`。
    pub fn function(
        name: impl Into<String>,
        parameters: Value,
        description: Option<String>,
    ) -> Self {
        Self::Function(ResponsesFunctionTool::new(name, parameters, description))
    }

    /// 构造默认的联网搜索工具（`type = web_search`）。
    pub fn web_search() -> Self {
        Self::WebSearch(WebSearchTool::default())
    }

    /// 以原始 JSON 声明工具。
    pub fn raw(value: Value) -> Self {
        Self::Other(value)
    }
}

/// Responses 函数工具（扁平形状）。
///
/// ```json
/// {"type":"function","name":"lookup","parameters":{"type":"object"},"strict":true}
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponsesFunctionTool {
    /// 工具类型，`function`。
    #[serde(rename = "type", default = "function_kind")]
    pub kind: String,
    /// 函数名。
    pub name: String,
    /// 描述。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema（官方必填字段，允许 `null`）。
    #[serde(default)]
    pub parameters: Option<Value>,
    /// 是否启用严格模式（官方必填字段，允许 `null`）。
    #[serde(default)]
    pub strict: Option<bool>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ResponsesFunctionTool {
    /// 构造 function 工具。
    pub fn new(name: impl Into<String>, parameters: Value, description: Option<String>) -> Self {
        Self {
            kind: function_kind(),
            name: name.into(),
            description,
            parameters: Some(parameters),
            strict: None,
            extensions: Extensions::new(),
        }
    }

    /// 设置严格模式。
    #[must_use]
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = Some(strict);
        self
    }
}

/// Responses 工具选择。
///
/// 官方取值是 `none` / `auto` / `required` 字符串，或扁平对象：
/// - `{"type":"function","name":"..."}`
/// - `{"type":"allowed_tools","mode":"auto","tools":[...]}`
/// - `{"type":"custom","name":"..."}`
/// - `{"type":"web_search"}` 等内置工具
///
/// 其他结构（例如 `mcp`）走 [`ResponsesToolChoice::Other`] 原样透传。
#[derive(Debug, Clone, PartialEq)]
pub enum ResponsesToolChoice {
    /// 模式：`auto` / `none` / `required`。
    Mode(ToolChoiceMode),
    /// 强制指定函数。
    Function {
        /// 函数名。
        name: String,
    },
    /// 限定可用工具集合。
    AllowedTools {
        /// 模式：`auto` / `required`。
        mode: String,
        /// 允许的工具定义（原样保留）。
        tools: Vec<Value>,
    },
    /// 强制指定 custom 工具。
    Custom {
        /// 工具名。
        name: String,
    },
    /// 内置工具（`web_search` / `code_interpreter` 等）。
    Builtin(String),
    /// 未识别结构（原样透传）。
    Other(Value),
}

impl ResponsesToolChoice {
    /// `auto`。
    pub fn auto() -> Self {
        Self::Mode(ToolChoiceMode::Auto)
    }

    /// `none`。
    pub fn none() -> Self {
        Self::Mode(ToolChoiceMode::None)
    }

    /// `required`。
    pub fn required() -> Self {
        Self::Mode(ToolChoiceMode::Required)
    }

    /// 强制某个函数。
    pub fn function(name: impl Into<String>) -> Self {
        Self::Function { name: name.into() }
    }

    /// 限定可用工具集合。
    pub fn allowed_tools(mode: impl Into<String>, tools: Vec<Value>) -> Self {
        Self::AllowedTools {
            mode: mode.into(),
            tools,
        }
    }

    /// 强制某个 custom 工具。
    pub fn custom(name: impl Into<String>) -> Self {
        Self::Custom { name: name.into() }
    }

    /// 内置工具。
    pub fn builtin(kind: impl Into<String>) -> Self {
        Self::Builtin(kind.into())
    }
}

impl From<ToolChoice> for ResponsesToolChoice {
    fn from(choice: ToolChoice) -> Self {
        match choice {
            ToolChoice::Mode(mode) => Self::Mode(mode),
            ToolChoice::Function(function) => Self::Function {
                name: function.function.name,
            },
        }
    }
}

impl Serialize for ResponsesToolChoice {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Mode(mode) => mode.serialize(serializer),
            Self::Function { name } => named_choice("function", name).serialize(serializer),
            Self::Custom { name } => named_choice("custom", name).serialize(serializer),
            Self::AllowedTools { mode, tools } => {
                let mut map = Map::new();
                map.insert("type".to_owned(), Value::String("allowed_tools".to_owned()));
                map.insert("mode".to_owned(), Value::String(mode.clone()));
                map.insert(
                    "tools".to_owned(),
                    serde_json::to_value(tools).map_err(serde::ser::Error::custom)?,
                );
                Value::Object(map).serialize(serializer)
            }
            Self::Builtin(kind) => serde_json::json!({ "type": kind }).serialize(serializer),
            Self::Other(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ResponsesToolChoice {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::String(text) => match text.as_str() {
                "auto" => Ok(Self::Mode(ToolChoiceMode::Auto)),
                "none" => Ok(Self::Mode(ToolChoiceMode::None)),
                "required" => Ok(Self::Mode(ToolChoiceMode::Required)),
                other => Err(serde::de::Error::custom(format!(
                    "未知的 Responses tool_choice 模式: {other}"
                ))),
            },
            Value::Object(map) => {
                let kind = map
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                match kind.as_str() {
                    "function" | "custom" => {
                        let name = map
                            .get("name")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                serde::de::Error::custom("Responses tool_choice 缺少 name 字段")
                            })?
                            .to_owned();
                        if kind == "function" {
                            Ok(Self::Function { name })
                        } else {
                            Ok(Self::Custom { name })
                        }
                    }
                    "allowed_tools" => {
                        let mode = map
                            .get("mode")
                            .and_then(Value::as_str)
                            .unwrap_or("auto")
                            .to_owned();
                        let tools = map
                            .get("tools")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default();
                        Ok(Self::AllowedTools { mode, tools })
                    }
                    _ if map.len() == 1 => Ok(Self::Builtin(kind)),
                    _ => Ok(Self::Other(Value::Object(map))),
                }
            }
            other => Err(serde::de::Error::custom(format!(
                "Responses tool_choice 形状非法: {other}"
            ))),
        }
    }
}

fn named_choice(kind: &str, name: &str) -> Value {
    let mut map = Map::new();
    map.insert("type".to_owned(), Value::String(kind.to_owned()));
    map.insert("name".to_owned(), Value::String(name.to_owned()));
    Value::Object(map)
}

fn function_kind() -> String {
    "function".to_owned()
}

/// 联网搜索工具。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchTool {
    /// 工具类型。
    #[serde(rename = "type", default)]
    pub kind: WebSearchKind,
    /// 是否允许实时联网（false 表示仅用缓存）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_web_access: Option<bool>,
    /// 域名过滤。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<WebSearchFilters>,
    /// 用户位置（原样结构）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_location: Option<Value>,
    /// 上下文占用等级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<SearchContextSize>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self {
            kind: WebSearchKind::WebSearch,
            external_web_access: None,
            filters: None,
            user_location: None,
            search_context_size: None,
            extensions: Extensions::new(),
        }
    }
}

/// 搜索工具类型。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebSearchKind {
    /// 稳定别名。
    #[default]
    #[serde(rename = "web_search")]
    WebSearch,
    /// 版本化名称。
    #[serde(rename = "web_search_2025_08_26")]
    WebSearch2025_08_26,
}

/// 域名过滤。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WebSearchFilters {
    /// 只允许这些域名（含子域）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 搜索上下文等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchContextSize {
    /// 低。
    Low,
    /// 中。
    Medium,
    /// 高。
    High,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn web_search_tool_serializes_and_roundtrips() {
        let tool = ResponsesTool::web_search();
        let value = serde_json::to_value(&tool).expect("serialize");
        assert_eq!(value, json!({"type": "web_search"}));

        let parsed: ResponsesTool =
            serde_json::from_value(json!({"type": "web_search_2025_08_26"})).expect("parse");
        match parsed {
            ResponsesTool::WebSearch(tool) => {
                assert_eq!(tool.kind, WebSearchKind::WebSearch2025_08_26)
            }
            other => panic!("unexpected: {other:?}"),
        }

        // 未来服务端工具原样保留。
        let other: ResponsesTool =
            serde_json::from_value(json!({"type": "future_tool"})).expect("parse");
        assert!(matches!(other, ResponsesTool::Other(_)));
    }

    #[test]
    fn function_tool_uses_flat_responses_shape() {
        let tool = ResponsesTool::function("lookup", json!({"type": "object"}), None);
        let value = serde_json::to_value(&tool).expect("serialize");
        assert_eq!(
            value,
            json!({
                "type": "function",
                "name": "lookup",
                "parameters": {"type": "object"},
                "strict": null
            })
        );
        // 不得出现 Chat 的嵌套 function 对象。
        assert!(value.get("function").is_none());

        let back: ResponsesTool = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, tool);
    }

    #[test]
    fn function_tool_keeps_description_and_strict() {
        let tool = ResponsesTool::Function(
            ResponsesFunctionTool::new("lookup", json!({"type": "object"}), Some("d".to_owned()))
                .with_strict(true),
        );
        let value = serde_json::to_value(&tool).expect("serialize");
        assert_eq!(value["strict"], true);
        assert_eq!(value["description"], "d");
        assert_eq!(value["type"], "function");
    }

    #[test]
    fn tool_choice_modes_functions_and_builtins() {
        assert_eq!(
            serde_json::to_value(ResponsesToolChoice::auto()).expect("serialize"),
            json!("auto")
        );
        assert_eq!(
            serde_json::to_value(ResponsesToolChoice::required()).expect("serialize"),
            json!("required")
        );
        assert_eq!(
            serde_json::to_value(ResponsesToolChoice::function("lookup")).expect("serialize"),
            json!({"type": "function", "name": "lookup"})
        );
        assert_eq!(
            serde_json::to_value(ResponsesToolChoice::custom("shell")).expect("serialize"),
            json!({"type": "custom", "name": "shell"})
        );
        assert_eq!(
            serde_json::to_value(ResponsesToolChoice::builtin("web_search")).expect("serialize"),
            json!({"type": "web_search"})
        );
    }

    #[test]
    fn tool_choice_allowed_tools_roundtrips() {
        let choice = ResponsesToolChoice::allowed_tools(
            "required",
            vec![json!({"type": "function", "name": "get_weather"})],
        );
        let value = serde_json::to_value(&choice).expect("serialize");
        assert_eq!(
            value,
            json!({
                "type": "allowed_tools",
                "mode": "required",
                "tools": [{"type": "function", "name": "get_weather"}]
            })
        );
        let back: ResponsesToolChoice = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, choice);
    }

    #[test]
    fn tool_choice_unknown_object_is_preserved() {
        let value = json!({"type": "mcp", "server_label": "deepwiki", "name": "search"});
        let choice: ResponsesToolChoice = serde_json::from_value(value.clone()).expect("parse");
        assert!(matches!(choice, ResponsesToolChoice::Other(_)));
        assert_eq!(serde_json::to_value(choice).expect("serialize"), value);
    }

    #[test]
    fn tool_choice_converts_from_chat_choice() {
        assert_eq!(
            ResponsesToolChoice::from(ToolChoice::auto()),
            ResponsesToolChoice::auto()
        );
        assert_eq!(
            ResponsesToolChoice::from(ToolChoice::function("lookup")),
            ResponsesToolChoice::function("lookup")
        );
    }
}

//! Chat 工具、工具选择与结构化输出。

use airx_core::json::Extensions;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 工具声明。
///
/// - `Function`：客户端执行的自定义函数。
/// - `Other`：Provider 服务端工具（如 DeepSeek 的 `deepseek_web_search`）
///   或未来新增工具，原样透传。服务端工具由 Provider 后台执行，SDK 不发起网络请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Tool {
    /// 自定义函数工具。
    Function(FunctionTool),
    /// 服务端工具 / 未知工具。
    Other(Value),
}

/// 函数工具定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionTool {
    /// 工具类型，`function`。
    #[serde(rename = "type")]
    pub kind: String,
    /// 函数定义。
    pub function: FunctionDefinition,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl FunctionTool {
    /// 构造 function 工具。
    pub fn new(name: impl Into<String>, parameters: Value, description: Option<String>) -> Self {
        Self {
            kind: "function".to_owned(),
            function: FunctionDefinition {
                name: name.into(),
                description,
                parameters: Some(parameters),
                strict: None,
                extensions: Extensions::new(),
            },
            extensions: Extensions::new(),
        }
    }
}

impl Tool {
    /// 构造 function 工具。
    pub fn function(
        name: impl Into<String>,
        parameters: Value,
        description: Option<String>,
    ) -> Self {
        Self::Function(FunctionTool::new(name, parameters, description))
    }

    /// 声明一个服务端工具（例如 `deepseek_web_search`）。
    ///
    /// 只发送声明，实际联网与结果生成由 Provider 后台完成。
    pub fn server(kind: impl Into<String>) -> Self {
        Self::Other(serde_json::json!({ "type": kind.into() }))
    }

    /// 以原始 JSON 声明工具。
    pub fn raw(value: Value) -> Self {
        Self::Other(value)
    }
}

/// 函数定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// 函数名。
    pub name: String,
    /// 描述。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
    /// 是否启用严格模式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 工具调用（请求历史与响应共用）。
///
/// 官方 `ChatCompletionMessageToolCall = FunctionToolCall | CustomToolCall`：
/// - `function` 只在 `type = "function"` 时存在；
/// - `custom` 只在 `type = "custom"` 时存在。
///
/// 两者都可缺失（未知工具类型），此时只保留 `id` / `type` / `extensions`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// 调用 ID。
    pub id: String,
    /// 类型（`function` / `custom` / 其他）。
    #[serde(rename = "type", default = "function_kind")]
    pub kind: String,
    /// 函数调用内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionCall>,
    /// custom 工具调用内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom: Option<CustomToolCall>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ToolCall {
    /// 构造 function 调用。
    pub fn function(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind: function_kind(),
            function: Some(FunctionCall {
                name: name.into(),
                arguments: arguments.into(),
                extensions: Extensions::new(),
            }),
            custom: None,
            extensions: Extensions::new(),
        }
    }

    /// 构造 custom 调用（free-form 输入）。
    pub fn custom(
        id: impl Into<String>,
        name: impl Into<String>,
        input: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind: "custom".to_owned(),
            function: None,
            custom: Some(CustomToolCall {
                name: name.into(),
                input: input.into(),
                extensions: Extensions::new(),
            }),
            extensions: Extensions::new(),
        }
    }

    /// 函数名（function 或 custom）。
    pub fn name(&self) -> Option<&str> {
        self.function
            .as_ref()
            .map(|function| function.name.as_str())
            .or_else(|| self.custom.as_ref().map(|custom| custom.name.as_str()))
    }

    /// 调用参数（function 为 JSON 字符串；custom 为原始 input）。
    pub fn arguments(&self) -> Option<&str> {
        self.function
            .as_ref()
            .map(|function| function.arguments.as_str())
            .or_else(|| self.custom.as_ref().map(|custom| custom.input.as_str()))
    }
}

fn function_kind() -> String {
    "function".to_owned()
}

/// 函数调用内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    /// 函数名。
    pub name: String,
    /// 参数 JSON 字符串（流式期间可能不完整）。
    pub arguments: String,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// custom 工具调用内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomToolCall {
    /// 工具名。
    pub name: String,
    /// 模型生成的原始输入（free-form 字符串，不是 JSON 参数）。
    pub input: String,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 工具选择。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// 模式：`auto` / `none` / `required`。
    Mode(ToolChoiceMode),
    /// 强制指定函数。
    Function(ToolChoiceFunction),
}

impl ToolChoice {
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
        Self::Function(ToolChoiceFunction {
            kind: function_kind(),
            function: ToolChoiceFunctionName { name: name.into() },
        })
    }
}

/// 工具选择模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoiceMode {
    /// 自动选择。
    Auto,
    /// 不使用工具。
    None,
    /// 必须使用工具。
    Required,
}

/// 指定函数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolChoiceFunction {
    /// 类型，目前为 `function`。
    #[serde(rename = "type", default = "function_kind")]
    pub kind: String,
    /// 函数名。
    pub function: ToolChoiceFunctionName,
}

/// 指定函数名。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolChoiceFunctionName {
    /// 函数名。
    pub name: String,
}

/// 结构化输出格式。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    /// 普通文本。
    Text,
    /// JSON 对象模式。
    JsonObject,
    /// JSON Schema 模式。
    JsonSchema {
        /// Schema 规格。
        json_schema: JsonSchemaSpec,
    },
}

/// JSON Schema 规格。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonSchemaSpec {
    /// 名称。
    pub name: String,
    /// 描述。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Schema。
    pub schema: Value,
    /// 是否严格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_choice_serializes_modes_and_functions() {
        assert_eq!(
            serde_json::to_value(ToolChoice::auto()).expect("serialize"),
            json!("auto")
        );
        assert_eq!(
            serde_json::to_value(ToolChoice::function("lookup")).expect("serialize"),
            json!({"type": "function", "function": {"name": "lookup"}})
        );
    }

    #[test]
    fn response_format_is_typed_but_keeps_schema() {
        let format = ResponseFormat::JsonSchema {
            json_schema: JsonSchemaSpec {
                name: "answer".to_owned(),
                description: None,
                schema: json!({"type": "object"}),
                strict: Some(true),
                extensions: Extensions::new(),
            },
        };
        let value = serde_json::to_value(format).expect("serialize");
        assert_eq!(value["type"], "json_schema");
        assert_eq!(value["json_schema"]["strict"], true);
    }

    #[test]
    fn tool_call_roundtrips() {
        let call = ToolCall::function("call_1", "lookup", r#"{"q":"x"}"#);
        let value = serde_json::to_value(&call).expect("serialize");
        let back: ToolCall = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, call);
        assert_eq!(back.name(), Some("lookup"));
    }

    #[test]
    fn custom_tool_call_decodes_and_roundtrips() {
        let json = json!({
            "id": "call_custom",
            "type": "custom",
            "custom": {"name": "shell", "input": "ls -la"}
        });
        let call: ToolCall = serde_json::from_value(json.clone()).expect("deserialize");
        assert_eq!(call.kind, "custom");
        assert!(call.function.is_none());
        assert_eq!(call.custom.as_ref().map(|c| c.name.as_str()), Some("shell"));
        assert_eq!(call.arguments(), Some("ls -la"));
        assert_eq!(serde_json::to_value(&call).expect("serialize"), json);
    }

    #[test]
    fn function_tool_call_omits_custom_field() {
        let call = ToolCall::function("call_1", "lookup", "{}");
        let value = serde_json::to_value(&call).expect("serialize");
        assert_eq!(value["type"], "function");
        assert!(value.get("custom").is_none());
        assert_eq!(
            value,
            json!({"id": "call_1", "type": "function", "function": {"name": "lookup", "arguments": "{}"}})
        );
    }

    #[test]
    fn unknown_tool_call_kind_keeps_extensions() {
        let json = json!({
            "id": "call_1",
            "type": "future_tool",
            "future_payload": {"a": 1}
        });
        let call: ToolCall = serde_json::from_value(json.clone()).expect("deserialize");
        assert!(call.function.is_none());
        assert!(call.custom.is_none());
        assert_eq!(serde_json::to_value(&call).expect("serialize"), json);
    }
}

#[cfg(test)]
mod server_tool_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn server_tool_is_declared_verbatim() {
        let tool = Tool::server("deepseek_web_search");
        assert_eq!(
            serde_json::to_value(tool).expect("serialize"),
            json!({"type": "deepseek_web_search"})
        );
    }

    #[test]
    fn function_tool_still_serializes_normally() {
        let tool = Tool::function("lookup", json!({"type": "object"}), None);
        let value = serde_json::to_value(tool).expect("serialize");
        assert_eq!(value["type"], "function");
        assert_eq!(value["function"]["name"], "lookup");
    }
}

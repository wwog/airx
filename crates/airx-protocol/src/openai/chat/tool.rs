//! Chat 工具、工具选择与结构化输出。

use airx_core::json::Extensions;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 工具定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    /// 工具类型，目前为 `function`。
    #[serde(rename = "type")]
    pub kind: String,
    /// 函数定义。
    pub function: FunctionDefinition,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl Tool {
    /// 构造 function 工具。
    pub fn function(
        name: impl Into<String>,
        parameters: Value,
        description: Option<String>,
    ) -> Self {
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// 调用 ID。
    pub id: String,
    /// 类型，目前为 `function`。
    #[serde(rename = "type", default = "function_kind")]
    pub kind: String,
    /// 函数调用内容。
    pub function: FunctionCall,
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
            function: FunctionCall {
                name: name.into(),
                arguments: arguments.into(),
                extensions: Extensions::new(),
            },
            extensions: Extensions::new(),
        }
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
    }
}

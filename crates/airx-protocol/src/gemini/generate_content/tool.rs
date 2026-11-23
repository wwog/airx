//! Gemini 工具声明与工具选择。
//!
//! 客户端工具通过 `functionDeclarations` 声明；服务端工具（`googleSearch`、
//! `codeExecution`、`urlContext` 等）通过 `Tool::server` 原样写入。

use airx_core::json::Extensions;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 工具定义。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    /// 客户端函数声明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_declarations: Option<Vec<FunctionDeclaration>>,
    /// 服务端工具或未识别字段（`googleSearch` / `codeExecution` …）。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl Tool {
    /// 单个函数工具。
    pub fn function(declaration: FunctionDeclaration) -> Self {
        Self {
            function_declarations: Some(vec![declaration]),
            extensions: Extensions::new(),
        }
    }

    /// 多个函数工具。
    pub fn functions(declarations: impl IntoIterator<Item = FunctionDeclaration>) -> Self {
        Self {
            function_declarations: Some(declarations.into_iter().collect()),
            extensions: Extensions::new(),
        }
    }

    /// 服务端工具（如 `googleSearch` / `codeExecution` / `urlContext`）。
    pub fn server(kind: impl Into<String>, config: Value) -> Self {
        let mut extensions = Extensions::new();
        extensions.insert(kind.into(), config);
        Self {
            function_declarations: None,
            extensions,
        }
    }
}

/// 函数声明。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDeclaration {
    /// 工具名。
    pub name: String,
    /// 描述。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 参数 JSON Schema（OpenAPI 子集）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
    /// 输出 JSON Schema（部分模型支持）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl FunctionDeclaration {
    /// 使用工具名创建声明。
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            parameters: None,
            response: None,
            extensions: Extensions::new(),
        }
    }

    /// 设置描述。
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// 设置参数 JSON Schema。
    #[must_use]
    pub fn with_parameters(mut self, parameters: Value) -> Self {
        self.parameters = Some(parameters);
        self
    }
}

/// 工具选择配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfig {
    /// 函数调用配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_calling_config: Option<FunctionCallingConfig>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ToolConfig {
    /// 以函数调用配置构造。
    pub fn new(config: FunctionCallingConfig) -> Self {
        Self {
            function_calling_config: Some(config),
            extensions: Extensions::new(),
        }
    }
}

/// 函数调用配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCallingConfig {
    /// 调用模式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<FunctionCallingMode>,
    /// 允许调用的函数名（`mode = ANY` 时可用）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_function_names: Option<Vec<String>>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl FunctionCallingConfig {
    /// 指定调用模式。
    pub fn mode(mode: FunctionCallingMode) -> Self {
        Self {
            mode: Some(mode),
            allowed_function_names: None,
            extensions: Extensions::new(),
        }
    }

    /// 限定允许调用的函数。
    #[must_use]
    pub fn allow(mut self, names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.allowed_function_names = Some(names.into_iter().map(Into::into).collect());
        self
    }
}

/// 函数调用模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum FunctionCallingMode {
    /// 模型自行决定是否调用。
    Auto,
    /// 必须调用工具。
    Any,
    /// 不调用工具。
    None,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn function_declaration_serializes() {
        let tool = Tool::function(
            FunctionDeclaration::new("lookup")
                .with_description("Look up a record")
                .with_parameters(json!({"type": "object"})),
        );
        let value = serde_json::to_value(&tool).expect("serialize");
        assert_eq!(value["functionDeclarations"][0]["name"], "lookup");
        assert_eq!(
            value["functionDeclarations"][0]["description"],
            "Look up a record"
        );
        assert_eq!(
            value["functionDeclarations"][0]["parameters"]["type"],
            "object"
        );
    }

    #[test]
    fn server_tool_is_written_verbatim() {
        let tool = Tool::server("googleSearch", json!({}));
        assert_eq!(
            serde_json::to_value(&tool).expect("serialize"),
            json!({"googleSearch": {}})
        );
    }

    #[test]
    fn tool_config_modes_serialize_uppercase() {
        let config = ToolConfig::new(
            FunctionCallingConfig::mode(FunctionCallingMode::Any).allow(["lookup", "search"]),
        );
        let value = serde_json::to_value(&config).expect("serialize");
        assert_eq!(value["functionCallingConfig"]["mode"], "ANY");
        assert_eq!(
            value["functionCallingConfig"]["allowedFunctionNames"],
            json!(["lookup", "search"])
        );
    }

    #[test]
    fn invalid_mode_fails() {
        assert!(serde_json::from_value::<FunctionCallingMode>(json!("SOMETIMES")).is_err());
    }
}

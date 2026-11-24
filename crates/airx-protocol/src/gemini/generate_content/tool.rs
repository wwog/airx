//! Gemini 工具声明与工具选择。
//!
//! 客户端工具通过 `functionDeclarations` 声明；服务端工具（`googleSearch`、
//! `codeExecution`、`urlContext` 等）通过 `Tool::server` 或命名构造器原样写入。
//! airx 只声明与解析，不代理请求、不执行工具。

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

    /// Google 搜索接地（`{"googleSearch":{}}`）。
    pub fn google_search() -> Self {
        Self::server("googleSearch", serde_json::json!({}))
    }

    /// 代码执行（`{"codeExecution":{}}`）。
    pub fn code_execution() -> Self {
        Self::server("codeExecution", serde_json::json!({}))
    }

    /// URL 上下文（`{"urlContext":{}}`）。
    pub fn url_context() -> Self {
        Self::server("urlContext", serde_json::json!({}))
    }

    /// Google 地图（`{"googleMaps":{}}`）。
    pub fn google_maps() -> Self {
        Self::server("googleMaps", serde_json::json!({}))
    }

    /// 文件搜索（`{"fileSearch":{"fileSearchStoreNames":[...]}}`）。
    pub fn file_search(stores: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let names: Vec<String> = stores.into_iter().map(Into::into).collect();
        Self::server(
            "fileSearch",
            serde_json::json!({ "fileSearchStoreNames": names }),
        )
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
    /// 参数 Schema（标准 JSON Schema）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters_json_schema: Option<Value>,
    /// 输出 JSON Schema（OpenAPI 子集，部分模型支持）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<Value>,
    /// 输出 Schema（标准 JSON Schema）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_json_schema: Option<Value>,
    /// 调用行为（如 `BLOCKING` / `NON_BLOCKING`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behavior: Option<String>,
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
            parameters_json_schema: None,
            response: None,
            response_json_schema: None,
            behavior: None,
            extensions: Extensions::new(),
        }
    }

    /// 设置描述。
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// 设置参数 JSON Schema（OpenAPI 子集）。
    #[must_use]
    pub fn with_parameters(mut self, parameters: Value) -> Self {
        self.parameters = Some(parameters);
        self
    }

    /// 设置参数 Schema（标准 JSON Schema）。
    #[must_use]
    pub fn with_parameters_json_schema(mut self, schema: Value) -> Self {
        self.parameters_json_schema = Some(schema);
        self
    }

    /// 设置输出 JSON Schema（OpenAPI 子集）。
    #[must_use]
    pub fn with_response(mut self, response: Value) -> Self {
        self.response = Some(response);
        self
    }

    /// 设置输出 Schema（标准 JSON Schema）。
    #[must_use]
    pub fn with_response_json_schema(mut self, schema: Value) -> Self {
        self.response_json_schema = Some(schema);
        self
    }

    /// 设置调用行为（如 `NON_BLOCKING`）。
    #[must_use]
    pub fn with_behavior(mut self, behavior: impl Into<String>) -> Self {
        self.behavior = Some(behavior.into());
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
    /// 检索配置（结构随官方演进，原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval_config: Option<Value>,
    /// 是否把服务端工具调用（`toolCall` / `toolResponse`）也返回给客户端。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_server_side_tool_invocations: Option<bool>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ToolConfig {
    /// 以函数调用配置构造。
    pub fn new(config: FunctionCallingConfig) -> Self {
        Self {
            function_calling_config: Some(config),
            retrieval_config: None,
            include_server_side_tool_invocations: None,
            extensions: Extensions::new(),
        }
    }

    /// 是否返回服务端工具调用载荷。
    #[must_use]
    pub fn include_server_side_invocations(mut self, yes: bool) -> Self {
        self.include_server_side_tool_invocations = Some(yes);
        self
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

    /// 模型自行决定（`AUTO`）。
    pub fn auto() -> Self {
        Self::mode(FunctionCallingMode::Auto)
    }

    /// 必须调用函数（`ANY`）。
    pub fn any() -> Self {
        Self::mode(FunctionCallingMode::Any)
    }

    /// 不调用函数（`NONE`）。
    pub fn none() -> Self {
        Self::mode(FunctionCallingMode::None)
    }

    /// 仅允许 schema 校验通过的调用（`VALIDATED`）。
    pub fn validated() -> Self {
        Self::mode(FunctionCallingMode::Validated)
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
    /// 仅接受通过 schema 校验的调用。
    Validated,
    /// 未指定（官方默认值，等价于 AUTO）。
    #[serde(rename = "MODE_UNSPECIFIED")]
    ModeUnspecified,
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
    fn function_declaration_supports_std_json_schema_and_behavior() {
        let tool = Tool::function(
            FunctionDeclaration::new("lookup")
                .with_parameters_json_schema(
                    json!({"type": "object", "additionalProperties": false}),
                )
                .with_response_json_schema(json!({"type": "string"}))
                .with_behavior("NON_BLOCKING"),
        );
        let value = serde_json::to_value(&tool).expect("serialize");
        let declaration = &value["functionDeclarations"][0];

        assert_eq!(declaration["parametersJsonSchema"]["type"], "object");
        assert_eq!(declaration["responseJsonSchema"]["type"], "string");
        assert_eq!(declaration["behavior"], "NON_BLOCKING");
        assert!(declaration.get("parameters").is_none());
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
    fn server_tool_helpers() {
        assert_eq!(
            serde_json::to_value(Tool::code_execution()).expect("serialize"),
            json!({"codeExecution": {}})
        );
        assert_eq!(
            serde_json::to_value(Tool::url_context()).expect("serialize"),
            json!({"urlContext": {}})
        );
        assert_eq!(
            serde_json::to_value(Tool::file_search(["fileSearchStores/a"])).expect("serialize"),
            json!({"fileSearch": {"fileSearchStoreNames": ["fileSearchStores/a"]}})
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
    fn validated_mode_roundtrips() {
        let config = ToolConfig::new(FunctionCallingConfig::validated());
        let value = serde_json::to_value(&config).expect("serialize");
        assert_eq!(value["functionCallingConfig"]["mode"], "VALIDATED");

        let parsed: FunctionCallingMode = serde_json::from_value(json!("VALIDATED")).expect("mode");
        assert_eq!(parsed, FunctionCallingMode::Validated);

        let unspecified: FunctionCallingMode =
            serde_json::from_value(json!("MODE_UNSPECIFIED")).expect("mode");
        assert_eq!(unspecified, FunctionCallingMode::ModeUnspecified);
        assert_eq!(
            serde_json::to_value(FunctionCallingMode::ModeUnspecified).expect("serialize"),
            "MODE_UNSPECIFIED"
        );
    }

    #[test]
    fn include_server_side_invocations_serializes() {
        let config =
            ToolConfig::new(FunctionCallingConfig::auto()).include_server_side_invocations(true);
        let value = serde_json::to_value(&config).expect("serialize");
        assert_eq!(value["includeServerSideToolInvocations"], true);
    }

    #[test]
    fn invalid_mode_fails() {
        assert!(serde_json::from_value::<FunctionCallingMode>(json!("SOMETIMES")).is_err());
    }
}

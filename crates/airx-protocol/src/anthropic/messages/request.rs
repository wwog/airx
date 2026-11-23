//! Anthropic Messages 请求。

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::json::Extensions;
use airx_core::model::ModelId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::message::{ContentBlock, MessageContent, Role};

/// Messages 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessagesRequest {
    /// 模型。
    pub model: ModelId,
    /// 最大输出 token（Anthropic 必填）。
    pub max_tokens: u32,
    /// 消息列表。
    pub messages: Vec<Message>,
    /// 系统指令。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// 输出配置（effort / 结构化输出格式）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,
    /// 停止序列。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// 工具。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// 工具选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// thinking 配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    /// 是否流式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl MessagesRequest {
    /// 创建构建器。
    pub fn builder() -> MessagesRequestBuilder {
        MessagesRequestBuilder::default()
    }
}

/// 消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// 角色。
    pub role: Role,
    /// 内容。
    pub content: MessageContent,
}

impl Message {
    /// 用户文本消息。
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: MessageContent::Text(content.into()),
        }
    }

    /// 助手文本消息。
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: MessageContent::Text(content.into()),
        }
    }

    /// 块内容消息。
    pub fn blocks(role: Role, blocks: Vec<ContentBlock>) -> Self {
        Self {
            role,
            content: MessageContent::Blocks(blocks),
        }
    }
}

/// 工具声明。
///
/// - `Client`：自定义工具，由调用方执行。
/// - `WebSearch`：Anthropic 服务端联网搜索，由 Anthropic 后台执行。
/// - `Other`：其他服务端工具（web_fetch / code_execution / computer_use 等）
///   或未来新增工具，原样透传。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Tool {
    /// 自定义工具。
    Client(ClientTool),
    /// 服务端联网搜索。
    WebSearch(WebSearchTool),
    /// 其他服务端工具 / 未知工具。
    Other(Value),
}

/// 自定义工具。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientTool {
    /// 工具名。
    pub name: String,
    /// 描述。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 输入 JSON Schema（自定义工具必填）。
    pub input_schema: Value,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 服务端联网搜索工具。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchTool {
    /// 工具类型（版本化）。
    #[serde(rename = "type")]
    pub kind: WebSearchKind,
    /// 工具名，固定为 `web_search`。
    #[serde(default = "web_search_name")]
    pub name: String,
    /// 只允许这些域名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    /// 屏蔽这些域名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    /// 最大使用次数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u32>,
    /// 用户位置（原样结构）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_location: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self {
            kind: WebSearchKind::V20260318,
            name: web_search_name(),
            allowed_domains: None,
            blocked_domains: None,
            max_uses: None,
            user_location: None,
            extensions: Extensions::new(),
        }
    }
}

/// 搜索工具版本。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebSearchKind {
    /// `web_search_20250305`
    #[serde(rename = "web_search_20250305")]
    V20250305,
    /// `web_search_20260209`
    #[serde(rename = "web_search_20260209")]
    V20260209,
    /// `web_search_20260318`
    #[default]
    #[serde(rename = "web_search_20260318")]
    V20260318,
}

fn web_search_name() -> String {
    "web_search".to_owned()
}

impl Tool {
    /// 构造自定义工具。
    pub fn new(name: impl Into<String>, input_schema: Value) -> Self {
        Self::Client(ClientTool {
            name: name.into(),
            description: None,
            input_schema,
            extensions: Extensions::new(),
        })
    }

    /// 设置描述。
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        if let Self::Client(tool) = &mut self {
            tool.description = Some(description.into());
        }
        self
    }

    /// 构造服务端联网搜索工具。
    pub fn web_search() -> Self {
        Self::WebSearch(WebSearchTool::default())
    }

    /// 以原始 JSON 声明服务端工具（如 `web_fetch_20250910`）。
    pub fn raw(value: Value) -> Self {
        Self::Other(value)
    }
}

/// 工具选择。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolChoice {
    /// 自动。
    Auto,
    /// 必须使用工具。
    Any,
    /// 指定工具。
    Tool {
        /// 工具名。
        name: String,
    },
}

/// 输出配置。
///
/// 对应最新 Messages API 的 `output_config`：`effort` 控制推理投入，
/// `format` 声明结构化输出。`temperature` / `top_p` / `top_k` 已从接口移除。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig {
    /// 推理投入等级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<OutputEffort>,
    /// 结构化输出格式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<JsonOutputFormat>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 推理投入等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputEffort {
    /// 低。
    Low,
    /// 中。
    Medium,
    /// 高。
    High,
    /// 极高。
    XHigh,
    /// 最大。
    Max,
}

/// 结构化输出格式（JSON Schema）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "json_schema")]
pub struct JsonOutputFormat {
    /// JSON Schema。
    pub schema: Value,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl JsonOutputFormat {
    /// 构造 JSON Schema 输出格式。
    pub fn new(schema: Value) -> Self {
        Self {
            schema,
            extensions: Extensions::new(),
        }
    }
}

/// thinking 配置。
///
/// 最新 API 支持 `enabled`（带预算）/ `disabled` / `between_tools` / `adaptive`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ThinkingConfig {
    /// 固定预算的扩展思考。
    Enabled {
        /// 预算 token（≥ 1024 且小于 max_tokens）。
        budget_tokens: u32,
        /// 思考内容展示方式。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display: Option<ThinkingDisplay>,
    },
    /// 关闭思考。
    Disabled,
    /// 在工具调用之间思考。
    BetweenTools,
    /// 由模型自适应决定思考量。
    Adaptive {
        /// 思考内容展示方式。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display: Option<ThinkingDisplay>,
    },
}

/// 思考内容展示方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingDisplay {
    /// 正常返回思考内容。
    Summarized,
    /// 隐去思考内容，仅保留签名。
    Omitted,
}

impl ThinkingConfig {
    /// 固定预算的 thinking。
    pub fn enabled(budget_tokens: u32) -> Self {
        Self::Enabled {
            budget_tokens,
            display: None,
        }
    }

    /// 自适应 thinking。
    pub fn adaptive() -> Self {
        Self::Adaptive { display: None }
    }

    /// 关闭 thinking。
    pub fn disabled() -> Self {
        Self::Disabled
    }

    /// 工具间 thinking。
    pub fn between_tools() -> Self {
        Self::BetweenTools
    }
}

/// [`MessagesRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct MessagesRequestBuilder {
    model: Option<ModelId>,
    max_tokens: Option<u32>,
    messages: Vec<Message>,
    system: Option<String>,
    output_config: Option<OutputConfig>,
    stop_sequences: Option<Vec<String>>,
    tools: Option<Vec<Tool>>,
    tool_choice: Option<ToolChoice>,
    thinking: Option<ThinkingConfig>,
    stream: Option<bool>,
    extensions: Extensions,
}

impl MessagesRequestBuilder {
    /// 设置模型。
    #[must_use]
    pub fn model(mut self, model: impl Into<ModelId>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// 设置最大输出 token（必填）。
    #[must_use]
    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// 追加消息。
    #[must_use]
    pub fn message(mut self, message: Message) -> Self {
        self.messages.push(message);
        self
    }

    /// 追加用户文本消息。
    #[must_use]
    pub fn user(self, content: impl Into<String>) -> Self {
        self.message(Message::user(content))
    }

    /// 追加助手文本消息。
    #[must_use]
    pub fn assistant(self, content: impl Into<String>) -> Self {
        self.message(Message::assistant(content))
    }

    /// 设置系统指令。
    #[must_use]
    pub fn system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    /// 设置推理投入等级。
    #[must_use]
    pub fn output_effort(mut self, effort: OutputEffort) -> Self {
        self.output_config
            .get_or_insert_with(OutputConfig::default)
            .effort = Some(effort);
        self
    }

    /// 设置结构化输出格式。
    #[must_use]
    pub fn output_format(mut self, format: JsonOutputFormat) -> Self {
        self.output_config
            .get_or_insert_with(OutputConfig::default)
            .format = Some(format);
        self
    }

    /// 设置停止序列。
    #[must_use]
    pub fn stop_sequences(mut self, stop: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.stop_sequences = Some(stop.into_iter().map(Into::into).collect());
        self
    }

    /// 设置工具。
    #[must_use]
    pub fn tools(mut self, tools: impl IntoIterator<Item = Tool>) -> Self {
        self.tools = Some(tools.into_iter().collect());
        self
    }

    /// 设置工具选择。
    #[must_use]
    pub fn tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = Some(tool_choice);
        self
    }

    /// 设置 thinking。
    #[must_use]
    pub fn thinking(mut self, thinking: ThinkingConfig) -> Self {
        self.thinking = Some(thinking);
        self
    }

    /// 设置流式标记。
    #[must_use]
    pub fn stream(mut self, stream: bool) -> Self {
        self.stream = Some(stream);
        self
    }

    /// 添加扩展字段。
    #[must_use]
    pub fn extension(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
    }

    /// 构建请求。
    pub fn build(self) -> Result<MessagesRequest, AirxError> {
        let model = self.model.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("MessagesRequest.model 必填"))
        })?;
        let max_tokens = self.max_tokens.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("MessagesRequest.max_tokens 必填"))
        })?;

        if self.messages.is_empty() {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "MessagesRequest.messages 不能为空",
            )));
        }

        Ok(MessagesRequest {
            model,
            max_tokens,
            messages: self.messages,
            system: self.system,
            output_config: self.output_config,
            stop_sequences: self.stop_sequences,
            tools: self.tools,
            tool_choice: self.tool_choice,
            thinking: self.thinking,
            stream: self.stream,
            extensions: self.extensions,
        })
    }
}

#[cfg(test)]
mod alignment_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn output_config_and_thinking_match_latest_api() {
        let request = MessagesRequest::builder()
            .model("claude-opus-4-6")
            .max_tokens(1024)
            .user("hi")
            .output_effort(OutputEffort::High)
            .output_format(JsonOutputFormat::new(json!({"type": "object"})))
            .thinking(ThinkingConfig::adaptive())
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["output_config"]["effort"], "high");
        assert_eq!(value["output_config"]["format"]["type"], "json_schema");
        assert_eq!(value["thinking"]["type"], "adaptive");
        // 最新 Messages API 已移除采样参数。
        assert!(value.get("temperature").is_none());
        assert!(value.get("top_p").is_none());
        assert!(value.get("top_k").is_none());
    }

    #[test]
    fn thinking_variants_serialize() {
        assert_eq!(
            serde_json::to_value(ThinkingConfig::disabled()).expect("serialize")["type"],
            "disabled"
        );
        assert_eq!(
            serde_json::to_value(ThinkingConfig::between_tools()).expect("serialize")["type"],
            "between_tools"
        );

        let enabled = ThinkingConfig::Enabled {
            budget_tokens: 2048,
            display: Some(ThinkingDisplay::Omitted),
        };
        let value = serde_json::to_value(enabled).expect("serialize");
        assert_eq!(value["type"], "enabled");
        assert_eq!(value["budget_tokens"], 2048);
        assert_eq!(value["display"], "omitted");
    }
}

#[cfg(test)]
mod server_tool_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn web_search_tool_serializes() {
        let tool = Tool::web_search();
        assert_eq!(
            serde_json::to_value(tool).expect("serialize"),
            json!({"type": "web_search_20260318", "name": "web_search"})
        );
    }

    #[test]
    fn custom_tool_keeps_schema_and_description() {
        let tool =
            Tool::new("lookup", json!({"type": "object"})).with_description("Look up a record");
        let value = serde_json::to_value(tool).expect("serialize");
        assert_eq!(value["name"], "lookup");
        assert_eq!(value["description"], "Look up a record");
        assert_eq!(value["input_schema"]["type"], "object");
    }

    #[test]
    fn unknown_server_tool_is_preserved() {
        let tool = Tool::raw(json!({"type": "web_fetch_20250910", "name": "web_fetch"}));
        assert_eq!(
            serde_json::to_value(tool).expect("serialize"),
            json!({"type": "web_fetch_20250910", "name": "web_fetch"})
        );
    }
}

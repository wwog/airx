//! Anthropic Messages 请求。

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::json::Extensions;
use airx_core::model::ModelId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::anthropic::common::{RequestServiceTier, Speed};

use super::message::{CacheControl, ContentBlock, MessageContent, Role, TextBlock};

/// `system` 参数：单个字符串或多块（可挂缓存断点）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SystemPrompt {
    /// 单个字符串。
    Text(String),
    /// 文本块数组。
    Blocks(Vec<TextBlock>),
}

impl SystemPrompt {
    /// 追加一个文本块；字符串形式会先转为块数组。
    pub fn push_block(&mut self, block: TextBlock) {
        match self {
            Self::Text(text) => {
                let first = TextBlock::new(std::mem::take(text));
                *self = Self::Blocks(vec![first, block]);
            }
            Self::Blocks(blocks) => blocks.push(block),
        }
    }
}

impl From<String> for SystemPrompt {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for SystemPrompt {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<Vec<TextBlock>> for SystemPrompt {
    fn from(value: Vec<TextBlock>) -> Self {
        Self::Blocks(value)
    }
}

/// Messages 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessagesRequest {
    /// 模型。
    pub model: ModelId,
    /// 最大输出 token（Anthropic 必填）。
    pub max_tokens: u32,
    /// 消息列表。
    pub messages: Vec<Message>,
    /// 系统指令（字符串或多块）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,
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
    /// 顶层自动缓存断点（作用于最后一个可缓存块）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    /// 调用方用户标识（滥用检测）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// 请求的服务档位。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<RequestServiceTier>,
    /// 推理速度模式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<Speed>,
    /// 数据驻留 / 推理区域。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_geo: Option<String>,
    /// 缓存诊断（对比上一轮请求的 prompt 指纹）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<DiagnosticsParam>,
    /// 容器（代码执行 / skills 复用）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<ContainerRef>,
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

    /// 系统文本消息（对话中途插入系统指令）。
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
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

/// 元数据（`metadata`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    /// 外部用户标识（uuid / hash，不要放可识别信息）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl Metadata {
    /// 用 `user_id` 构造。
    pub fn new(user_id: impl Into<String>) -> Self {
        Self {
            user_id: Some(user_id.into()),
            extensions: Extensions::new(),
        }
    }
}

/// 请求级缓存诊断（`diagnostics`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsParam {
    /// 上一轮 `/v1/messages` 响应的 `id`；首轮可传 `None` 以仅开启诊断。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_message_id: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl DiagnosticsParam {
    /// 用上一轮消息 ID 构造。
    pub fn after(previous_message_id: impl Into<String>) -> Self {
        Self {
            previous_message_id: Some(previous_message_id.into()),
            extensions: Extensions::new(),
        }
    }
}

/// 容器引用：容器 ID（字符串）或容器参数对象。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContainerRef {
    /// 复用已有容器。
    Id(String),
    /// 容器参数（可选 id + skills）。
    Params(ContainerParam),
}

impl From<&str> for ContainerRef {
    fn from(value: &str) -> Self {
        Self::Id(value.to_owned())
    }
}

impl From<String> for ContainerRef {
    fn from(value: String) -> Self {
        Self::Id(value)
    }
}

/// 容器参数（`container`）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContainerParam {
    /// 容器 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 需要加载的 skills（结构随官方演进，原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 工具声明。
///
/// - `Client`：自定义工具，由调用方执行。
/// - `WebSearch`：Anthropic 服务端联网搜索，由 Anthropic 后台执行。
/// - `Other`：其他服务端工具（web_fetch / code_execution / computer_use 等）
///   或未来新增工具，原样透传。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
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
    /// 对象类型（`custom`；可省略）。
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// 严格模式：保证工具名与输入按 schema 校验。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    /// 工具级缓存断点。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    /// 输入示例（提升调用准确率）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<Value>>,
    /// 流式工具参数细粒度增量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eager_input_streaming: Option<bool>,
    /// 延迟加载（只在 tool search 返回 `tool_reference` 时载入）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    /// 允许的调用方（`direct` / `code_execution_*`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<String>>,
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
            kind: None,
            strict: None,
            cache_control: None,
            input_examples: None,
            eager_input_streaming: None,
            defer_loading: None,
            allowed_callers: None,
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

    /// 开启严格模式（`strict: true`，结构化输出的工具侧对偶）。
    #[must_use]
    pub fn strict(mut self, strict: bool) -> Self {
        if let Self::Client(tool) = &mut self {
            tool.strict = Some(strict);
        }
        self
    }

    /// 在工具声明处设置缓存断点。
    #[must_use]
    pub fn cached(mut self, control: CacheControl) -> Self {
        if let Self::Client(tool) = &mut self {
            tool.cache_control = Some(control);
        }
        self
    }

    /// 追加输入示例。
    #[must_use]
    pub fn input_examples(mut self, examples: impl IntoIterator<Item = Value>) -> Self {
        if let Self::Client(tool) = &mut self {
            tool.input_examples = Some(examples.into_iter().collect());
        }
        self
    }

    /// 开启 / 关闭工具参数细粒度流式增量。
    #[must_use]
    pub fn eager_input_streaming(mut self, eager: bool) -> Self {
        if let Self::Client(tool) = &mut self {
            tool.eager_input_streaming = Some(eager);
        }
        self
    }

    /// 延迟加载该工具（先由 tool search 命中再注入）。
    #[must_use]
    pub fn defer_loading(mut self, defer: bool) -> Self {
        if let Self::Client(tool) = &mut self {
            tool.defer_loading = Some(defer);
        }
        self
    }

    /// 限定允许的调用方（`direct` / `code_execution_20250825` 等）。
    #[must_use]
    pub fn allowed_callers(mut self, callers: impl IntoIterator<Item = impl Into<String>>) -> Self {
        if let Self::Client(tool) = &mut self {
            tool.allowed_callers = Some(callers.into_iter().map(Into::into).collect());
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
#[non_exhaustive]
pub enum ToolChoice {
    /// 自动。
    Auto {
        /// 为 `true` 时模型最多输出一个工具调用。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    /// 必须使用工具。
    Any {
        /// 为 `true` 时模型只输出一个工具调用。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    /// 指定工具。
    Tool {
        /// 工具名。
        name: String,
        /// 为 `true` 时模型只输出一个工具调用。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    /// 本次禁用工具。
    None,
}

impl Default for ToolChoice {
    fn default() -> Self {
        Self::Auto {
            disable_parallel_tool_use: None,
        }
    }
}

impl ToolChoice {
    /// 自动选择。
    pub fn auto() -> Self {
        Self::default()
    }

    /// 必须使用任意工具。
    pub fn any() -> Self {
        Self::Any {
            disable_parallel_tool_use: None,
        }
    }

    /// 指定工具。
    pub fn tool(name: impl Into<String>) -> Self {
        Self::Tool {
            name: name.into(),
            disable_parallel_tool_use: None,
        }
    }

    /// 禁用工具。
    pub fn none() -> Self {
        Self::None
    }

    /// 设置 `disable_parallel_tool_use`（`none` 无此字段，保持原样）。
    #[must_use]
    pub fn disable_parallel_tool_use(self, disable: bool) -> Self {
        match self {
            Self::Auto { .. } => Self::Auto {
                disable_parallel_tool_use: Some(disable),
            },
            Self::Any { .. } => Self::Any {
                disable_parallel_tool_use: Some(disable),
            },
            Self::Tool { name, .. } => Self::Tool {
                name,
                disable_parallel_tool_use: Some(disable),
            },
            Self::None => Self::None,
        }
    }
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

/// 由结构体字段或端点层写入的 `MessagesRequest` 字段名。
///
/// 这些键不允许再出现在 `extensions` 里：`#[serde(flatten)]` 会与结构体字段各写
/// 一次，产出重复 JSON 键，语义取决于解析器。`stream` 由流式端点编码时注入，
/// 同样不允许调用方手写。
const MODELED_FIELDS: &[&str] = &[
    "model",
    "max_tokens",
    "messages",
    "system",
    "output_config",
    "stop_sequences",
    "tools",
    "tool_choice",
    "thinking",
    "cache_control",
    "metadata",
    "service_tier",
    "speed",
    "inference_geo",
    "diagnostics",
    "container",
    "stream",
];

/// 拒绝与已建模字段重名的 `extensions` 键。
pub(crate) fn reject_modeled_extensions(
    extensions: &Extensions,
    label: &str,
) -> Result<(), AirxError> {
    if let Some(key) = MODELED_FIELDS
        .iter()
        .find(|key| extensions.contains_key(**key))
    {
        return Err(AirxError::Configuration(ConfigurationError::new(format!(
            "{label}.extensions 含已建模字段 `{key}`，请改用类型化入口"
        ))));
    }
    Ok(())
}

/// [`MessagesRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct MessagesRequestBuilder {
    model: Option<ModelId>,
    max_tokens: Option<u32>,
    messages: Vec<Message>,
    system: Option<SystemPrompt>,
    output_config: Option<OutputConfig>,
    stop_sequences: Option<Vec<String>>,
    tools: Option<Vec<Tool>>,
    tool_choice: Option<ToolChoice>,
    thinking: Option<ThinkingConfig>,
    cache_control: Option<CacheControl>,
    metadata: Option<Metadata>,
    service_tier: Option<RequestServiceTier>,
    speed: Option<Speed>,
    inference_geo: Option<String>,
    diagnostics: Option<DiagnosticsParam>,
    container: Option<ContainerRef>,
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

    /// 一次性设置完整消息列表（适合 Agent 循环复用历史）。
    #[must_use]
    pub fn messages(mut self, messages: impl IntoIterator<Item = Message>) -> Self {
        self.messages.extend(messages);
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

    /// 追加系统文本消息（对话中途插入系统指令）。
    #[must_use]
    pub fn system_message(self, content: impl Into<String>) -> Self {
        self.message(Message::system(content))
    }

    /// 设置系统指令（字符串）。
    #[must_use]
    pub fn system(mut self, system: impl Into<SystemPrompt>) -> Self {
        self.system = Some(system.into());
        self
    }

    /// 设置系统指令为文本块数组（可给某块挂 `cache_control`）。
    #[must_use]
    pub fn system_blocks(mut self, blocks: impl IntoIterator<Item = TextBlock>) -> Self {
        self.system = Some(SystemPrompt::Blocks(blocks.into_iter().collect()));
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

    /// 设置顶层自动缓存断点。
    #[must_use]
    pub fn cache_control(mut self, control: CacheControl) -> Self {
        self.cache_control = Some(control);
        self
    }

    /// 设置 `metadata.user_id`。
    #[must_use]
    pub fn user_id(mut self, user_id: impl Into<String>) -> Self {
        self.metadata = Some(Metadata::new(user_id));
        self
    }

    /// 设置完整 `metadata`。
    #[must_use]
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// 设置服务档位。
    #[must_use]
    pub fn service_tier(mut self, tier: RequestServiceTier) -> Self {
        self.service_tier = Some(tier);
        self
    }

    /// 设置推理速度模式。
    #[must_use]
    pub fn speed(mut self, speed: Speed) -> Self {
        self.speed = Some(speed);
        self
    }

    /// 设置推理区域（数据驻留）。
    #[must_use]
    pub fn inference_geo(mut self, geo: impl Into<String>) -> Self {
        self.inference_geo = Some(geo.into());
        self
    }

    /// 设置缓存诊断（传入上一轮响应 ID）。
    #[must_use]
    pub fn diagnostics(mut self, diagnostics: DiagnosticsParam) -> Self {
        self.diagnostics = Some(diagnostics);
        self
    }

    /// 设置容器引用。
    #[must_use]
    pub fn container(mut self, container: impl Into<ContainerRef>) -> Self {
        self.container = Some(container.into());
        self
    }

    /// 添加扩展字段。
    ///
    /// 只用于未建模字段；与已建模字段重名时 `build()` 直接报错。
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

        reject_modeled_extensions(&self.extensions, "MessagesRequest")?;

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
            cache_control: self.cache_control,
            metadata: self.metadata,
            service_tier: self.service_tier,
            speed: self.speed,
            inference_geo: self.inference_geo,
            diagnostics: self.diagnostics,
            container: self.container,
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
mod request_shape_tests {
    use super::super::message::CacheTtl;
    use super::*;
    use serde_json::json;

    #[test]
    fn system_accepts_string_and_blocks() {
        let plain = MessagesRequest::builder()
            .model("m")
            .max_tokens(8)
            .user("hi")
            .system("be brief")
            .build()
            .expect("request");
        assert_eq!(
            serde_json::to_value(&plain).expect("serialize")["system"],
            "be brief"
        );

        let blocks = MessagesRequest::builder()
            .model("m")
            .max_tokens(8)
            .user("hi")
            .system_blocks([
                TextBlock::new("be brief"),
                TextBlock::new("cached").cached(CacheControl::ephemeral()),
            ])
            .build()
            .expect("request");
        let value = serde_json::to_value(&blocks).expect("serialize");
        assert_eq!(value["system"][0]["type"], "text");
        assert_eq!(value["system"][1]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn system_prompt_round_trips() {
        let prompt: SystemPrompt =
            serde_json::from_value(json!([{"type": "text", "text": "a"}])).expect("prompt");
        assert!(matches!(prompt, SystemPrompt::Blocks(_)));
        let prompt: SystemPrompt = serde_json::from_value(json!("a")).expect("prompt");
        assert_eq!(prompt, SystemPrompt::Text("a".to_owned()));
    }

    #[test]
    fn tool_choice_variants_serialize() {
        assert_eq!(
            serde_json::to_value(ToolChoice::none()).expect("serialize"),
            json!({"type": "none"})
        );
        assert_eq!(
            serde_json::to_value(ToolChoice::auto().disable_parallel_tool_use(true))
                .expect("serialize"),
            json!({"type": "auto", "disable_parallel_tool_use": true})
        );
        assert_eq!(
            serde_json::to_value(ToolChoice::any().disable_parallel_tool_use(true))
                .expect("serialize"),
            json!({"type": "any", "disable_parallel_tool_use": true})
        );
        assert_eq!(
            serde_json::to_value(ToolChoice::tool("lookup").disable_parallel_tool_use(true))
                .expect("serialize"),
            json!({"type": "tool", "name": "lookup", "disable_parallel_tool_use": true})
        );
        assert_eq!(
            serde_json::to_value(ToolChoice::auto()).expect("serialize"),
            json!({"type": "auto"})
        );

        let parsed: ToolChoice = serde_json::from_value(json!({"type": "none"})).expect("none");
        assert_eq!(parsed, ToolChoice::None);
    }

    #[test]
    fn tool_flags_serialize() {
        let tool = Tool::new("lookup", json!({"type": "object"}))
            .strict(true)
            .cached(CacheControl::ephemeral().with_ttl(CacheTtl::OneHour))
            .input_examples([json!({"q": "sg"})])
            .eager_input_streaming(true)
            .defer_loading(false)
            .allowed_callers(["direct"]);

        let value = serde_json::to_value(tool).expect("serialize");
        assert_eq!(value["strict"], true);
        assert_eq!(value["cache_control"]["ttl"], "1h");
        assert_eq!(value["input_examples"][0]["q"], "sg");
        assert_eq!(value["eager_input_streaming"], true);
        assert_eq!(value["defer_loading"], false);
        assert_eq!(value["allowed_callers"][0], "direct");
        assert!(value.get("type").is_none());
    }

    #[test]
    fn request_level_fields_serialize() {
        let request = MessagesRequest::builder()
            .model("m")
            .max_tokens(16)
            .user("hi")
            .cache_control(CacheControl::ephemeral())
            .user_id("user_1")
            .service_tier(RequestServiceTier::Auto)
            .speed(Speed::Fast)
            .inference_geo("us")
            .diagnostics(DiagnosticsParam::after("msg_0"))
            .container("container_1")
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["cache_control"]["type"], "ephemeral");
        assert_eq!(value["metadata"]["user_id"], "user_1");
        assert_eq!(value["service_tier"], "auto");
        assert_eq!(value["speed"], "fast");
        assert_eq!(value["inference_geo"], "us");
        assert_eq!(value["diagnostics"]["previous_message_id"], "msg_0");
        assert_eq!(value["container"], "container_1");
    }

    #[test]
    fn extensions_cannot_shadow_modeled_fields() {
        let error = MessagesRequest::builder()
            .model("m")
            .max_tokens(16)
            .user("hi")
            .extension("system", json!("dup"))
            .build()
            .expect_err("duplicate");

        assert!(matches!(error, AirxError::Configuration(_)));
    }

    #[test]
    fn system_role_message_serializes() {
        let request = MessagesRequest::builder()
            .model("m")
            .max_tokens(16)
            .user("hi")
            .system_message("mid-conversation instruction")
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["messages"][1]["role"], "system");
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

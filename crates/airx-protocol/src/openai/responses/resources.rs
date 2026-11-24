//! Responses 生命周期与附加资源端点。
//!
//! - `GET/POST/DELETE /responses/{id}`：读取 / 取消 / 删除
//! - `GET /responses/{id}/input_items`：输入 item 分页
//! - `POST /responses/input_tokens`：输入 token 计数

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError, SerializationError};
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, header};
use airx_core::json::Extensions;
use airx_core::model::{ModelId, ProviderId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::endpoint::Endpoint;
use crate::openai::common::error::error_from_response;
use crate::openai::config::OpenAiConfig;

use super::request::{InputItem, PromptCacheOptions, ResponsesInput, ResponsesRequest, TextConfig};
use super::response::ResponsesResponse;
use super::tool::{ResponsesTool, ResponsesToolChoice};
use crate::openai::common::reasoning::ReasoningEffort;

const RESPONSES_PATH: &str = "/responses";

/// 删除响应结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteResponse {
    /// 响应 ID。
    pub id: String,
    /// 对象类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 是否删除。
    #[serde(default)]
    pub deleted: bool,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 输入 item 分页。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputItemList {
    /// 对象类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 当前页 item。
    #[serde(default)]
    pub data: Vec<InputItem>,
    /// 是否还有下一页。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
    /// 当前页第一个 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_id: Option<String>,
    /// 当前页最后一个 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl InputItemList {
    /// 下一页游标。
    pub fn next_cursor(&self) -> Option<&str> {
        if self.has_more == Some(true) {
            self.last_id.as_deref()
        } else {
            None
        }
    }
}

/// 输入 item 列表参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListInputItemsRequest {
    /// 响应 ID。
    pub response_id: String,
    /// 每页数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// 从该 item 之后开始。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// 排序（`asc` / `desc`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
}

impl ListInputItemsRequest {
    /// 创建请求。
    pub fn new(response_id: impl Into<String>) -> Self {
        Self {
            response_id: response_id.into(),
            limit: None,
            after: None,
            order: None,
        }
    }
}

/// 输入 token 计数请求。
///
/// 官方 13 个字段；airx 类型化其中与 token 计数直接相关的 11 个。
/// `model` 在官方是可选的（缺省由服务端决定），因此这里是 `Option<ModelId>`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputTokensCountRequest {
    /// 模型（可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,
    /// 输入。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<ResponsesInput>,
    /// 系统指令。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// 基于已有响应继续（计 token 时必须计入历史）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    /// 会话（字符串 ID 或对象，原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<Value>,
    /// 工具。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ResponsesTool>>,
    /// 工具选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ResponsesToolChoice>,
    /// 是否允许并行工具调用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    /// 推理配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Value>,
    /// 文本配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextConfig>,
    /// 截断策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncation: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl InputTokensCountRequest {
    /// 从 Responses 请求构造计数请求。
    ///
    /// 逐字段复制（含 `extensions`），避免“续接会话 / 会话 / 并行工具”等
    /// 字段被静默丢弃导致少算 token。
    pub fn from_request(request: &ResponsesRequest) -> Self {
        Self {
            model: Some(request.model.clone()),
            input: request.input.clone(),
            instructions: request.instructions.clone(),
            previous_response_id: request.previous_response_id.clone(),
            conversation: request.conversation.clone(),
            tools: request.tools.clone(),
            tool_choice: request.tool_choice.clone(),
            parallel_tool_calls: request.parallel_tool_calls,
            reasoning: request
                .reasoning
                .as_ref()
                .map(|reasoning| serde_json::to_value(reasoning).unwrap_or(Value::Null)),
            text: request.text.clone(),
            truncation: request.truncation.clone(),
            extensions: request.extensions.clone(),
        }
    }

    /// 创建构建器。
    pub fn builder() -> InputTokensCountRequestBuilder {
        InputTokensCountRequestBuilder::default()
    }
}

/// [`InputTokensCountRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct InputTokensCountRequestBuilder {
    model: Option<ModelId>,
    input: Option<ResponsesInput>,
    instructions: Option<String>,
    previous_response_id: Option<String>,
    conversation: Option<Value>,
    tools: Option<Vec<ResponsesTool>>,
    tool_choice: Option<ResponsesToolChoice>,
    parallel_tool_calls: Option<bool>,
    reasoning: Option<Value>,
    text: Option<TextConfig>,
    truncation: Option<String>,
    extensions: Extensions,
}

impl InputTokensCountRequestBuilder {
    /// 设置模型。
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

    /// 设置 item 列表输入。
    #[must_use]
    pub fn input_items(mut self, items: impl IntoIterator<Item = InputItem>) -> Self {
        self.input = Some(ResponsesInput::Items(items.into_iter().collect()));
        self
    }

    /// 设置系统指令。
    #[must_use]
    pub fn instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    /// 基于已有响应继续。
    #[must_use]
    pub fn previous_response_id(mut self, id: impl Into<String>) -> Self {
        self.previous_response_id = Some(id.into());
        self
    }

    /// 设置会话（字符串 ID 或对象）。
    #[must_use]
    pub fn conversation(mut self, conversation: Value) -> Self {
        self.conversation = Some(conversation);
        self
    }

    /// 设置工具。
    #[must_use]
    pub fn tools(mut self, tools: impl IntoIterator<Item = ResponsesTool>) -> Self {
        self.tools = Some(tools.into_iter().collect());
        self
    }

    /// 设置工具选择。
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
        self.reasoning = Some(serde_json::json!({ "effort": effort }));
        self
    }

    /// 设置文本配置。
    #[must_use]
    pub fn text(mut self, text: TextConfig) -> Self {
        self.text = Some(text);
        self
    }

    /// 设置截断策略。
    #[must_use]
    pub fn truncation(mut self, truncation: impl Into<String>) -> Self {
        self.truncation = Some(truncation.into());
        self
    }

    /// 添加扩展字段。
    #[must_use]
    pub fn extension(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
    }

    /// 构建请求（`model` 可选，由服务端决定）。
    pub fn build(self) -> Result<InputTokensCountRequest, AirxError> {
        Ok(InputTokensCountRequest {
            model: self.model,
            input: self.input,
            instructions: self.instructions,
            previous_response_id: self.previous_response_id,
            conversation: self.conversation,
            tools: self.tools,
            tool_choice: self.tool_choice,
            parallel_tool_calls: self.parallel_tool_calls,
            reasoning: self.reasoning,
            text: self.text,
            truncation: self.truncation,
            extensions: self.extensions,
        })
    }
}

/// 输入 token 计数响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputTokensCount {
    /// 对象类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 输入 token。
    #[serde(default)]
    pub input_tokens: u32,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 读取响应时的 query 选项。
///
/// 对应官方 `ResponseRetrieveParamsBase`。`stream` 仅编码到 query：
/// 本端点是 JSON 解码端点，`stream = true` 会得到 SSE 响应而无法解析；
/// 需要流式续读时请使用自定义端点或 raw execute。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetrieveResponseOptions {
    /// 额外返回字段（例如 `reasoning.encrypted_content`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,
    /// 是否启用流式混淆。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_obfuscation: Option<bool>,
    /// 从该事件序号之后开始（流式续读）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starting_after: Option<u64>,
    /// 是否流式返回。
    #[serde(default, skip_serializing_if = "is_false")]
    pub stream: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl RetrieveResponseOptions {
    /// 追加一个 `include` 字段。
    #[must_use]
    pub fn include(mut self, field: impl Into<String>) -> Self {
        self.include.push(field.into());
        self
    }

    /// 设置 `starting_after`。
    #[must_use]
    pub fn starting_after(mut self, sequence: u64) -> Self {
        self.starting_after = Some(sequence);
        self
    }

    /// 设置 `include_obfuscation`。
    #[must_use]
    pub fn include_obfuscation(mut self, enabled: bool) -> Self {
        self.include_obfuscation = Some(enabled);
        self
    }

    /// 设置 `stream`。
    #[must_use]
    pub fn stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }
}

/// `GET /responses/{id}` 的请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrieveResponseRequest {
    /// 响应 ID。
    pub id: String,
    /// query 选项。
    #[serde(default)]
    pub options: RetrieveResponseOptions,
}

impl RetrieveResponseRequest {
    /// 使用默认选项创建请求。
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            options: RetrieveResponseOptions::default(),
        }
    }

    /// 附加 query 选项。
    #[must_use]
    pub fn with_options(mut self, options: RetrieveResponseOptions) -> Self {
        self.options = options;
        self
    }
}

/// `GET /responses/{id}`。
#[derive(Debug, Clone)]
pub struct RetrieveResponseEndpoint {
    config: OpenAiConfig,
}

impl RetrieveResponseEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, request: &RetrieveResponseRequest) -> Result<airx_core::http::Uri, AirxError> {
        let mut query = Vec::new();
        if !request.options.include.is_empty() {
            let mut include = Vec::with_capacity(request.options.include.len());
            for field in &request.options.include {
                include.push(format!("include[]={}", query_value(field, "include")?));
            }
            query.push(include.join("&"));
        }
        if let Some(include_obfuscation) = request.options.include_obfuscation {
            query.push(format!("include_obfuscation={include_obfuscation}"));
        }
        if let Some(starting_after) = request.options.starting_after {
            query.push(format!("starting_after={starting_after}"));
        }
        if request.options.stream {
            query.push("stream=true".to_owned());
        }

        let path = format!(
            "{RESPONSES_PATH}/{}{}",
            response_id(&request.id)?,
            if query.is_empty() {
                String::new()
            } else {
                format!("?{}", query.join("&"))
            }
        );
        self.config.endpoint_uri(&path, "Responses retrieve")
    }
}

impl Endpoint for RetrieveResponseEndpoint {
    type Request = RetrieveResponseRequest;
    type Response = ResponsesResponse;

    fn encode(&self, request: &RetrieveResponseRequest) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::get(self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<ResponsesResponse, AirxError> {
        decode(response)
    }
}

/// `POST /responses/compact` 请求。
///
/// 服务端把长对话压缩为「用户消息 + 一个 compaction item」。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactRequest {
    /// 模型。
    pub model: ModelId,
    /// 输入。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<ResponsesInput>,
    /// 系统指令。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// 基于已有响应继续。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    /// Prompt 缓存键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    /// Prompt 缓存选项。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_options: Option<PromptCacheOptions>,
    /// Prompt 缓存保留策略（已弃用）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_retention: Option<String>,
    /// 服务等级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl CompactRequest {
    /// 创建构建器。
    pub fn builder() -> CompactRequestBuilder {
        CompactRequestBuilder::default()
    }
}

/// [`CompactRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct CompactRequestBuilder {
    model: Option<ModelId>,
    input: Option<ResponsesInput>,
    instructions: Option<String>,
    previous_response_id: Option<String>,
    prompt_cache_key: Option<String>,
    prompt_cache_options: Option<PromptCacheOptions>,
    prompt_cache_retention: Option<String>,
    service_tier: Option<String>,
    extensions: Extensions,
}

impl CompactRequestBuilder {
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

    /// 设置 item 列表输入。
    #[must_use]
    pub fn input_items(mut self, items: impl IntoIterator<Item = InputItem>) -> Self {
        self.input = Some(ResponsesInput::Items(items.into_iter().collect()));
        self
    }

    /// 设置系统指令。
    #[must_use]
    pub fn instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    /// 基于已有响应继续。
    #[must_use]
    pub fn previous_response_id(mut self, id: impl Into<String>) -> Self {
        self.previous_response_id = Some(id.into());
        self
    }

    /// 设置 Prompt 缓存键。
    #[must_use]
    pub fn prompt_cache_key(mut self, key: impl Into<String>) -> Self {
        self.prompt_cache_key = Some(key.into());
        self
    }

    /// 设置 Prompt 缓存选项。
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

    /// 设置服务等级。
    #[must_use]
    pub fn service_tier(mut self, service_tier: impl Into<String>) -> Self {
        self.service_tier = Some(service_tier.into());
        self
    }

    /// 添加扩展字段。
    #[must_use]
    pub fn extension(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
    }

    /// 构建请求。
    pub fn build(self) -> Result<CompactRequest, AirxError> {
        let model = self.model.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("CompactRequest.model 必填"))
        })?;
        Ok(CompactRequest {
            model,
            input: self.input,
            instructions: self.instructions,
            previous_response_id: self.previous_response_id,
            prompt_cache_key: self.prompt_cache_key,
            prompt_cache_options: self.prompt_cache_options,
            prompt_cache_retention: self.prompt_cache_retention,
            service_tier: self.service_tier,
            extensions: self.extensions,
        })
    }
}

/// `POST /responses/compact`。
#[derive(Debug, Clone)]
pub struct CompactResponseEndpoint {
    config: OpenAiConfig,
}

impl CompactResponseEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self) -> Result<airx_core::http::Uri, AirxError> {
        self.config
            .endpoint_uri(&format!("{RESPONSES_PATH}/compact"), "Responses compact")
    }
}

impl Endpoint for CompactResponseEndpoint {
    type Request = CompactRequest;
    type Response = ResponsesResponse;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        let body = serde_json::to_vec(request)
            .map_err(|error| AirxError::Serialization(SerializationError::from(error)))?;
        let mut http = HttpRequest::post(self.uri()?).with_body(body);
        http.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<ResponsesResponse, AirxError> {
        decode(response)
    }
}

/// `POST /responses/{id}/cancel`。
#[derive(Debug, Clone)]
pub struct CancelResponseEndpoint {
    config: OpenAiConfig,
}

impl CancelResponseEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        let path = format!("{RESPONSES_PATH}/{}/cancel", response_id(id)?);
        self.config.endpoint_uri(&path, "Responses cancel")
    }
}

impl Endpoint for CancelResponseEndpoint {
    type Request = String;
    type Response = ResponsesResponse;

    fn encode(&self, request: &String) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::post(self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<ResponsesResponse, AirxError> {
        decode(response)
    }
}

/// `DELETE /responses/{id}`。
#[derive(Debug, Clone)]
pub struct DeleteResponseEndpoint {
    config: OpenAiConfig,
}

impl DeleteResponseEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        let path = format!("{RESPONSES_PATH}/{}", response_id(id)?);
        self.config.endpoint_uri(&path, "Responses delete")
    }
}

impl Endpoint for DeleteResponseEndpoint {
    type Request = String;
    type Response = DeleteResponse;

    fn encode(&self, request: &String) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::new(airx_core::http::Method::DELETE, self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<DeleteResponse, AirxError> {
        decode(response)
    }
}

/// `GET /responses/{id}/input_items`。
#[derive(Debug, Clone)]
pub struct ListInputItemsEndpoint {
    config: OpenAiConfig,
}

impl ListInputItemsEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, request: &ListInputItemsRequest) -> Result<airx_core::http::Uri, AirxError> {
        let mut query = Vec::new();
        if let Some(limit) = request.limit {
            query.push(format!("limit={limit}"));
        }
        if let Some(after) = &request.after {
            query.push(format!("after={}", query_value(after, "after")?));
        }
        if let Some(order) = &request.order {
            query.push(format!("order={}", query_value(order, "order")?));
        }

        let path = format!(
            "{RESPONSES_PATH}/{}/input_items{}",
            response_id(&request.response_id)?,
            if query.is_empty() {
                String::new()
            } else {
                format!("?{}", query.join("&"))
            }
        );
        self.config.endpoint_uri(&path, "Responses input_items")
    }
}

impl Endpoint for ListInputItemsEndpoint {
    type Request = ListInputItemsRequest;
    type Response = InputItemList;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::get(self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<InputItemList, AirxError> {
        decode(response)
    }
}

/// `POST /responses/input_tokens`。
#[derive(Debug, Clone)]
pub struct CountInputTokensEndpoint {
    config: OpenAiConfig,
}

impl CountInputTokensEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self) -> Result<airx_core::http::Uri, AirxError> {
        self.config
            .endpoint_uri(&format!("{RESPONSES_PATH}/input_tokens"), "input_tokens")
    }
}

impl Endpoint for CountInputTokensEndpoint {
    type Request = InputTokensCountRequest;
    type Response = InputTokensCount;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        let body = serde_json::to_vec(request)
            .map_err(|error| AirxError::Serialization(SerializationError::from(error)))?;
        let mut http = HttpRequest::post(self.uri()?).with_body(body);
        http.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<InputTokensCount, AirxError> {
        decode(response)
    }
}

fn decode<T: for<'de> Deserialize<'de>>(response: HttpResponse) -> Result<T, AirxError> {
    if !response.status.is_success() {
        return Err(error_from_response(
            &ProviderId::openai(),
            response.status,
            &response.headers,
            &response.body,
        ));
    }
    serde_json::from_slice(&response.body)
        .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))
}

fn response_id(id: &str) -> Result<String, AirxError> {
    let text = id.trim();
    if text.is_empty()
        || !text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(AirxError::Configuration(ConfigurationError::new(
            "Responses id 非法",
        )));
    }
    Ok(text.to_owned())
}

fn query_value(value: &str, field: &str) -> Result<String, AirxError> {
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '/'))
    {
        return Err(AirxError::Configuration(ConfigurationError::new(format!(
            "Responses {field} 含非法字符"
        ))));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn input_item_list_decodes_cursor() {
        let list: InputItemList = serde_json::from_value(json!({
            "object": "list",
            "data": [{"type": "reasoning", "id": "rs_1", "summary": []}],
            "has_more": true,
            "last_id": "rs_1"
        }))
        .expect("list");
        assert_eq!(list.next_cursor(), Some("rs_1"));
        assert!(matches!(list.data[0], InputItem::Reasoning { .. }));
    }

    #[test]
    fn count_request_serializes_input() {
        let request = InputTokensCountRequest::builder()
            .model("gpt-5")
            .input("hi")
            .reasoning_effort(ReasoningEffort::Low)
            .build()
            .expect("request");
        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["model"], "gpt-5");
        assert_eq!(value["input"], "hi");
        assert_eq!(value["reasoning"]["effort"], "low");
    }

    #[test]
    fn count_request_model_is_optional() {
        let request = InputTokensCountRequest::builder()
            .input("hi")
            .build()
            .expect("request");
        let value = serde_json::to_value(&request).expect("serialize");
        assert!(value.get("model").is_none());
    }

    #[test]
    fn from_request_copies_all_modelled_fields() {
        let request = ResponsesRequest::builder()
            .model("gpt-5")
            .input("hi")
            .instructions("be brief")
            .previous_response_id("resp_prev")
            .conversation(json!({"id": "conv_1"}))
            .parallel_tool_calls(false)
            .truncation("auto")
            .tool_choice(ResponsesToolChoice::function("lookup"))
            .extension("custom_field", json!({"a": 1}))
            .build()
            .expect("request");

        let count = InputTokensCountRequest::from_request(&request);
        let value = serde_json::to_value(&count).expect("serialize");
        assert_eq!(value["model"], "gpt-5");
        assert_eq!(value["previous_response_id"], "resp_prev");
        assert_eq!(value["conversation"]["id"], "conv_1");
        assert_eq!(value["parallel_tool_calls"], false);
        assert_eq!(value["truncation"], "auto");
        assert_eq!(value["tool_choice"]["name"], "lookup");
        assert_eq!(value["custom_field"]["a"], 1);
    }

    #[test]
    fn retrieve_query_encodes_include_and_paging() {
        let endpoint = RetrieveResponseEndpoint::new(OpenAiConfig::new("sk-test"));
        let http = endpoint
            .encode(
                &RetrieveResponseRequest::new("resp_1").with_options(
                    RetrieveResponseOptions::default()
                        .include("reasoning.encrypted_content")
                        .include("web_search_call.results")
                        .starting_after(42)
                        .include_obfuscation(false),
                ),
            )
            .expect("encode");

        assert_eq!(http.method, airx_core::http::Method::GET);
        assert_eq!(
            http.uri.to_string(),
            "https://api.openai.com/v1/responses/resp_1?include[]=reasoning.encrypted_content&include[]=web_search_call.results&include_obfuscation=false&starting_after=42"
        );
    }

    #[test]
    fn retrieve_without_options_has_no_query() {
        let endpoint = RetrieveResponseEndpoint::new(OpenAiConfig::new("sk-test"));
        let http = endpoint
            .encode(&RetrieveResponseRequest::new("resp_1"))
            .expect("encode");
        assert_eq!(
            http.uri.to_string(),
            "https://api.openai.com/v1/responses/resp_1"
        );
    }

    #[test]
    fn compact_posts_to_compact_endpoint() {
        let endpoint = CompactResponseEndpoint::new(OpenAiConfig::new("sk-test"));
        let request = CompactRequest::builder()
            .model("gpt-5")
            .input("long conversation")
            .previous_response_id("resp_1")
            .build()
            .expect("request");
        let http = endpoint.encode(&request).expect("encode");

        assert_eq!(http.method, airx_core::http::Method::POST);
        assert_eq!(
            http.uri.to_string(),
            "https://api.openai.com/v1/responses/compact"
        );
        let body: Value = serde_json::from_slice(&http.body).expect("body");
        assert_eq!(body["model"], "gpt-5");
        assert_eq!(body["previous_response_id"], "resp_1");
    }
}

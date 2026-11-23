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

use super::request::{InputItem, ResponsesInput, ResponsesRequest, TextConfig};
use super::response::ResponsesResponse;
use super::tool::ResponsesTool;
use crate::openai::chat::ToolChoice;
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputTokensCountRequest {
    /// 模型。
    pub model: ModelId,
    /// 输入。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<ResponsesInput>,
    /// 系统指令。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// 工具。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ResponsesTool>>,
    /// 工具选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// 推理配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Value>,
    /// 文本配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextConfig>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl InputTokensCountRequest {
    /// 从 Responses 请求构造计数请求。
    pub fn from_request(request: &ResponsesRequest) -> Self {
        Self {
            model: request.model.clone(),
            input: request.input.clone(),
            instructions: request.instructions.clone(),
            tools: request.tools.clone(),
            tool_choice: request.tool_choice.clone(),
            reasoning: request
                .reasoning
                .as_ref()
                .map(|reasoning| serde_json::to_value(reasoning).unwrap_or(Value::Null)),
            text: request.text.clone(),
            extensions: Extensions::new(),
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
    tools: Option<Vec<ResponsesTool>>,
    tool_choice: Option<ToolChoice>,
    reasoning: Option<Value>,
    text: Option<TextConfig>,
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

    /// 设置工具。
    #[must_use]
    pub fn tools(mut self, tools: impl IntoIterator<Item = ResponsesTool>) -> Self {
        self.tools = Some(tools.into_iter().collect());
        self
    }

    /// 设置工具选择。
    #[must_use]
    pub fn tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = Some(tool_choice);
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

    /// 添加扩展字段。
    #[must_use]
    pub fn extension(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
    }

    /// 构建请求。
    pub fn build(self) -> Result<InputTokensCountRequest, AirxError> {
        let model = self.model.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new(
                "InputTokensCountRequest.model 必填",
            ))
        })?;
        Ok(InputTokensCountRequest {
            model,
            input: self.input,
            instructions: self.instructions,
            tools: self.tools,
            tool_choice: self.tool_choice,
            reasoning: self.reasoning,
            text: self.text,
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

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        let path = format!("{RESPONSES_PATH}/{}", response_id(id)?);
        self.config.endpoint_uri(&path, "Responses retrieve")
    }
}

impl Endpoint for RetrieveResponseEndpoint {
    type Request = String;
    type Response = ResponsesResponse;

    fn encode(&self, request: &String) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::get(self.uri(request)?);
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
}

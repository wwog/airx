//! Anthropic Messages `count_tokens`。
//!
//! `POST {messages_path}/count_tokens`，路径跟随配置的 Messages 路径。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError, SerializationError};
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, header};
use airx_core::json::Extensions;
use airx_core::model::{ModelId, ProviderId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::anthropic::common::error_from_response;
use crate::anthropic::config::AnthropicConfig;
use crate::endpoint::Endpoint;

use super::request::{Message, ThinkingConfig, Tool};

/// countTokens 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CountTokensRequest {
    /// 模型。
    pub model: ModelId,
    /// 消息列表。
    pub messages: Vec<Message>,
    /// 系统指令。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// 工具。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// thinking 配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl CountTokensRequest {
    /// 创建构建器。
    pub fn builder() -> CountTokensRequestBuilder {
        CountTokensRequestBuilder::default()
    }
}

/// [`CountTokensRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct CountTokensRequestBuilder {
    model: Option<ModelId>,
    messages: Vec<Message>,
    system: Option<String>,
    tools: Option<Vec<Tool>>,
    thinking: Option<ThinkingConfig>,
    extensions: Extensions,
}

impl CountTokensRequestBuilder {
    /// 设置模型。
    #[must_use]
    pub fn model(mut self, model: impl Into<ModelId>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// 追加消息。
    #[must_use]
    pub fn message(mut self, message: Message) -> Self {
        self.messages.push(message);
        self
    }

    /// 追加用户文本。
    #[must_use]
    pub fn user(self, text: impl Into<String>) -> Self {
        self.message(Message::user(text))
    }

    /// 追加助手文本。
    #[must_use]
    pub fn assistant(self, text: impl Into<String>) -> Self {
        self.message(Message::assistant(text))
    }

    /// 设置系统指令。
    #[must_use]
    pub fn system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    /// 设置工具。
    #[must_use]
    pub fn tools(mut self, tools: impl IntoIterator<Item = Tool>) -> Self {
        self.tools = Some(tools.into_iter().collect());
        self
    }

    /// 设置 thinking。
    #[must_use]
    pub fn thinking(mut self, thinking: ThinkingConfig) -> Self {
        self.thinking = Some(thinking);
        self
    }

    /// 添加扩展字段。
    #[must_use]
    pub fn extension(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
    }

    /// 构建请求。
    pub fn build(self) -> Result<CountTokensRequest, AirxError> {
        let model = self.model.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("CountTokensRequest.model 必填"))
        })?;
        if self.messages.is_empty() {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "CountTokensRequest.messages 不能为空",
            )));
        }

        Ok(CountTokensRequest {
            model,
            messages: self.messages,
            system: self.system,
            tools: self.tools,
            thinking: self.thinking,
            extensions: self.extensions,
        })
    }
}

/// countTokens 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CountTokensResponse {
    /// 输入 token 数。
    #[serde(default)]
    pub input_tokens: u32,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// countTokens 端点。
#[derive(Debug, Clone)]
pub struct CountTokensEndpoint {
    config: AnthropicConfig,
}

impl CountTokensEndpoint {
    /// 创建端点。
    pub fn new(config: AnthropicConfig) -> Self {
        Self { config }
    }

    /// 当前配置。
    pub fn config(&self) -> &AnthropicConfig {
        &self.config
    }

    fn uri(&self) -> Result<airx_core::http::Uri, AirxError> {
        self.config
            .resolve_path(&self.config.count_tokens_path(), "countTokens")
    }
}

impl Endpoint for CountTokensEndpoint {
    type Request = CountTokensRequest;
    type Response = CountTokensResponse;

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

    fn decode(&self, response: HttpResponse) -> Result<CountTokensResponse, AirxError> {
        if !response.status.is_success() {
            return Err(error_from_response(
                &ProviderId::anthropic(),
                response.status,
                &response.headers,
                &response.body,
            ));
        }

        serde_json::from_slice(&response.body)
            .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_serializes_expected_fields() {
        let request = CountTokensRequest::builder()
            .model("claude-sonnet-4-5")
            .system("be brief")
            .user("hi")
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["model"], "claude-sonnet-4-5");
        assert_eq!(value["system"], "be brief");
        assert_eq!(value["messages"][0]["content"], "hi");
    }

    #[test]
    fn empty_messages_is_rejected() {
        assert!(
            CountTokensRequest::builder()
                .model("claude-sonnet-4-5")
                .build()
                .is_err()
        );
    }

    #[test]
    fn response_decodes() {
        let response: CountTokensResponse =
            serde_json::from_value(json!({"input_tokens": 42})).expect("response");
        assert_eq!(response.input_tokens, 42);
    }
}

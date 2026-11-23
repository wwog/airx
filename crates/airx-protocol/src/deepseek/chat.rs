//! DeepSeek Chat 端点。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError, SerializationError};
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, Uri, header};
use airx_core::model::ProviderId;
use airx_core::stream::RawEvent;
use serde_json::Value;

use crate::endpoint::{Endpoint, StreamingEndpoint};
use crate::openai::chat::{ChatCompletionRequest, ChatCompletionResponse, ChatStreamEvent};
use crate::openai::common::error::{error_from_response, error_from_stream_value};

use super::compatibility::DeepSeekPolicy;
use super::config::DeepSeekConfig;

/// 端点路径。
pub const CHAT_PATH: &str = "/chat/completions";

/// DeepSeek Chat 端点。
#[derive(Debug, Clone)]
pub struct ChatEndpoint {
    config: DeepSeekConfig,
}

impl ChatEndpoint {
    /// 创建端点（兼容策略来自配置，默认 Strict）。
    pub fn new(config: DeepSeekConfig) -> Self {
        Self { config }
    }

    /// 当前配置。
    pub fn config(&self) -> &DeepSeekConfig {
        &self.config
    }

    /// 当前策略。
    pub fn policy(&self) -> DeepSeekPolicy {
        self.config.policy()
    }

    fn uri(&self) -> Result<Uri, AirxError> {
        let url = format!(
            "{}{}",
            self.config.base_url().trim_end_matches('/'),
            CHAT_PATH
        );
        url.parse().map_err(|error| {
            AirxError::Configuration(ConfigurationError::new(format!(
                "无法构造 DeepSeek URL: {error}"
            )))
        })
    }
}

impl Endpoint for ChatEndpoint {
    type Request = ChatCompletionRequest;
    type Response = ChatCompletionResponse;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        // 策略在编码阶段应用：Strict 报错，BestEffort 降级后继续。
        let (request, _warnings) = self.config.policy().prepare(request.clone())?;

        let body = serde_json::to_vec(&request)
            .map_err(|error| AirxError::Serialization(SerializationError::from(error)))?;

        let mut http = HttpRequest::post(self.uri()?).with_body(body);
        http.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let authorization = format!("Bearer {}", self.config.api_key);
        http.headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&authorization).map_err(|error| {
                AirxError::Configuration(ConfigurationError::new(format!(
                    "DeepSeek api_key 无法作为 Header: {error}"
                )))
            })?,
        );

        for (name, value) in self.config.default_headers() {
            http.headers.insert(name.clone(), value.clone());
        }

        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<ChatCompletionResponse, AirxError> {
        if !response.status.is_success() {
            return Err(error_from_response(
                &ProviderId::deepseek(),
                response.status,
                &response.headers,
                &response.body,
            ));
        }

        serde_json::from_slice(&response.body)
            .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))
    }
}

impl StreamingEndpoint for ChatEndpoint {
    type Event = ChatStreamEvent;

    fn decode_event(&self, event: RawEvent) -> Result<Option<Self::Event>, AirxError> {
        if event.data == "[DONE]" {
            return Ok(Some(ChatStreamEvent::Done));
        }

        let value: Value = serde_json::from_str(&event.data)
            .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))?;

        if value.get("error").is_some() {
            return Err(error_from_stream_value(&ProviderId::deepseek(), value));
        }

        let chunk = serde_json::from_value(value)
            .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))?;
        Ok(Some(ChatStreamEvent::Chunk(chunk)))
    }
}

//! Anthropic Messages 端点。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError, SerializationError};
use airx_core::http::{HeaderName, HeaderValue, HttpRequest, HttpResponse, Uri, header};
use airx_core::model::ProviderId;
use airx_core::stream::RawEvent;
use serde_json::Value;

use crate::anthropic::common::{error_from_event_value, error_from_response};
use crate::anthropic::config::AnthropicConfig;
use crate::endpoint::{Endpoint, StreamingEndpoint};

use super::event::MessagesEvent;
use super::request::MessagesRequest;
use super::response::MessagesResponse;

/// 端点路径。
pub const MESSAGES_PATH: &str = "/v1/messages";

/// Messages 端点。
#[derive(Debug, Clone)]
pub struct MessagesEndpoint {
    config: AnthropicConfig,
}

impl MessagesEndpoint {
    /// 创建端点。
    pub fn new(config: AnthropicConfig) -> Self {
        Self { config }
    }

    /// 当前配置。
    pub fn config(&self) -> &AnthropicConfig {
        &self.config
    }

    fn uri(&self) -> Result<Uri, AirxError> {
        let url = format!(
            "{}{}",
            self.config.base_url().trim_end_matches('/'),
            MESSAGES_PATH
        );
        url.parse().map_err(|error| {
            AirxError::Configuration(ConfigurationError::new(format!(
                "无法构造 Messages URL: {error}"
            )))
        })
    }
}

impl Endpoint for MessagesEndpoint {
    type Request = MessagesRequest;
    type Response = MessagesResponse;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        let body = serde_json::to_vec(request)
            .map_err(|error| AirxError::Serialization(SerializationError::from(error)))?;

        let mut http = HttpRequest::post(self.uri()?).with_body(body);
        http.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        http.headers.insert(
            HeaderName::from_static("x-api-key"),
            header_value(&self.config.api_key, "api_key")?,
        );
        http.headers.insert(
            HeaderName::from_static("anthropic-version"),
            header_value(self.config.api_version(), "api_version")?,
        );

        for (name, value) in self.config.default_headers() {
            http.headers.insert(name.clone(), value.clone());
        }

        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<MessagesResponse, AirxError> {
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

impl StreamingEndpoint for MessagesEndpoint {
    type Event = MessagesEvent;

    fn decode_event(&self, event: RawEvent) -> Result<Option<Self::Event>, AirxError> {
        if event.data.is_empty() {
            return Ok(None);
        }

        let value: Value = serde_json::from_str(&event.data)
            .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))?;

        match value.get("type").and_then(Value::as_str) {
            Some("ping") => Ok(None),
            Some("error") => Err(error_from_event_value(&ProviderId::anthropic(), value)),
            _ => {
                let parsed = serde_json::from_value(value).map_err(|error| {
                    AirxError::Deserialization(DeserializationError::from(error))
                })?;
                Ok(Some(parsed))
            }
        }
    }
}

fn header_value(value: &str, field: &str) -> Result<HeaderValue, AirxError> {
    HeaderValue::from_str(value).map_err(|error| {
        AirxError::Configuration(ConfigurationError::new(format!(
            "Anthropic {field} 无法作为 HTTP Header: {error}"
        )))
    })
}

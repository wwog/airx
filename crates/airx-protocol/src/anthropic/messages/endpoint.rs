//! Anthropic Messages 端点。

use airx_core::AirxError;
use airx_core::error::DeserializationError;
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, Uri, header};
use airx_core::model::ProviderId;
use airx_core::stream::RawEvent;
use serde_json::Value;

use crate::anthropic::common::{error_from_event_value, error_from_response};
use crate::anthropic::config::AnthropicConfig;
use crate::endpoint::{Endpoint, StreamingEndpoint, encode_json_body, encode_json_stream_body};

use super::event::MessagesEvent;
use super::request::MessagesRequest;
use super::response::MessagesResponse;

/// 默认端点路径（可通过 [`AnthropicConfig::with_messages_path`] 覆盖）。
pub const MESSAGES_PATH: &str = crate::anthropic::config::DEFAULT_MESSAGES_PATH;

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
        self.config.endpoint_uri()
    }

    fn http(&self, body: Vec<u8>) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::post(self.uri()?).with_body(body);
        http.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self.config.apply_headers(&mut http)?;

        Ok(http)
    }
}

impl Endpoint for MessagesEndpoint {
    type Request = MessagesRequest;
    type Response = MessagesResponse;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        self.http(encode_json_body(request)?)
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

    fn encode_stream(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        self.http(encode_json_stream_body(request)?)
    }

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

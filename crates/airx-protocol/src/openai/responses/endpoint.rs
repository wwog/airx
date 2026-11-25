//! Responses 端点：编码 / 解码。

use airx_core::AirxError;
use airx_core::error::{DeserializationError, ProviderError};
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, Uri, header};
use airx_core::model::ProviderId;
use airx_core::stream::RawEvent;
use serde_json::Value;

use crate::endpoint::{Endpoint, StreamingEndpoint, encode_json_body, encode_json_stream_body};
use crate::openai::common::error::error_from_response;
use crate::openai::config::OpenAiConfig;

use super::event::ResponsesEvent;
use super::request::ResponsesRequest;
use super::response::ResponsesResponse;

/// 默认端点路径（可通过 [`OpenAiConfig::with_responses_path`] 覆盖）。
pub const RESPONSES_PATH: &str = crate::openai::config::DEFAULT_RESPONSES_PATH;

/// Responses 端点。
#[derive(Debug, Clone)]
pub struct ResponsesEndpoint {
    config: OpenAiConfig,
}

impl ResponsesEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    /// 当前配置。
    pub fn config(&self) -> &OpenAiConfig {
        &self.config
    }

    fn uri(&self) -> Result<Uri, AirxError> {
        self.config
            .endpoint_uri(self.config.responses_path(), "Responses")
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

impl Endpoint for ResponsesEndpoint {
    type Request = ResponsesRequest;
    type Response = ResponsesResponse;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        self.http(encode_json_body(request)?)
    }

    fn decode(&self, response: HttpResponse) -> Result<ResponsesResponse, AirxError> {
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
}

impl StreamingEndpoint for ResponsesEndpoint {
    type Event = ResponsesEvent;

    fn encode_stream(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        self.http(encode_json_stream_body(request)?)
    }

    fn decode_event(&self, event: RawEvent) -> Result<Option<Self::Event>, AirxError> {
        if event.data.is_empty() || event.data == "[DONE]" {
            return Ok(None);
        }

        let value: Value = serde_json::from_str(&event.data)
            .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))?;

        if value.get("type").and_then(Value::as_str) == Some("error") {
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Responses 流返回错误");
            let mut error = ProviderError::new(ProviderId::openai(), message);
            error.code = value.get("code").and_then(Value::as_str).map(str::to_owned);
            error.raw_body = serde_json::value::to_raw_value(&value).ok();
            return Err(AirxError::Provider(Box::new(error)));
        }

        let parsed = serde_json::from_value(value)
            .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))?;
        Ok(Some(parsed))
    }
}

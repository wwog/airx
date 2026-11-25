//! Chat Completions 端点：编码 / 解码（纯函数）。

use airx_core::AirxError;
use airx_core::error::DeserializationError;
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, Uri, header};
use airx_core::model::ProviderId;
use airx_core::stream::RawEvent;
use serde_json::Value;

use crate::endpoint::{Endpoint, StreamingEndpoint, encode_json_body, encode_json_stream_body};
use crate::openai::common::error::{error_from_response, error_from_stream_value};
use crate::openai::config::OpenAiConfig;

use super::event::ChatStreamEvent;
use super::request::ChatCompletionRequest;
use super::response::ChatCompletionResponse;

/// 默认端点路径（可通过 [`OpenAiConfig::with_chat_path`] 覆盖）。
pub const CHAT_COMPLETIONS_PATH: &str = crate::openai::config::DEFAULT_CHAT_PATH;

/// Chat Completions 端点。
#[derive(Debug, Clone)]
pub struct ChatEndpoint {
    config: OpenAiConfig,
}

impl ChatEndpoint {
    /// 使用 Provider 配置创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    /// 当前配置。
    pub fn config(&self) -> &OpenAiConfig {
        &self.config
    }

    fn uri(&self) -> Result<Uri, AirxError> {
        self.config
            .endpoint_uri(self.config.chat_path(), "Chat Completions")
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

impl Endpoint for ChatEndpoint {
    type Request = ChatCompletionRequest;
    type Response = ChatCompletionResponse;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        self.http(encode_json_body(request)?)
    }

    fn decode(&self, response: HttpResponse) -> Result<ChatCompletionResponse, AirxError> {
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

impl StreamingEndpoint for ChatEndpoint {
    type Event = ChatStreamEvent;

    fn encode_stream(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        self.http(encode_json_stream_body(request)?)
    }

    fn decode_event(&self, event: RawEvent) -> Result<Option<Self::Event>, AirxError> {
        if event.data == "[DONE]" {
            return Ok(Some(ChatStreamEvent::Done));
        }

        let value: Value = serde_json::from_str(&event.data)
            .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))?;

        if value.get("error").is_some() {
            return Err(error_from_stream_value(&ProviderId::openai(), value));
        }

        let chunk = serde_json::from_value(value)
            .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))?;
        Ok(Some(ChatStreamEvent::Chunk(chunk)))
    }
}

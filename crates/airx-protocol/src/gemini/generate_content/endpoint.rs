//! Gemini `generateContent` / `streamGenerateContent` 端点。
//!
//! 模型是路径的一部分：`{base}/models/{model}:generateContent`。
//! 因此端点持有模型，请求体不包含模型字段。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError, SerializationError};
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, Uri, header};
use airx_core::model::{ModelId, ProviderId};
use airx_core::stream::RawEvent;
use serde_json::Value;

use crate::endpoint::{Endpoint, StreamingEndpoint};
use crate::gemini::common::{error_from_response, error_from_stream_value};
use crate::gemini::config::GeminiConfig;

use super::request::GenerateContentRequest;
use super::response::GenerateContentResponse;

/// 非流式路径后缀。
pub const GENERATE_CONTENT_SUFFIX: &str = ":generateContent";
/// 流式路径后缀（附带 `alt=sse`）。
pub const STREAM_GENERATE_CONTENT_SUFFIX: &str = ":streamGenerateContent?alt=sse";

/// 端点调用模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerateContentMode {
    /// 一次性返回完整响应。
    Generate,
    /// SSE 增量响应。
    Stream,
}

/// `generateContent` 端点。
#[derive(Debug, Clone)]
pub struct GenerateContentEndpoint {
    config: GeminiConfig,
    model: ModelId,
    mode: GenerateContentMode,
}

impl GenerateContentEndpoint {
    /// 非流式端点。
    pub fn new(config: GeminiConfig, model: impl Into<ModelId>) -> Self {
        Self {
            config,
            model: model.into(),
            mode: GenerateContentMode::Generate,
        }
    }

    /// 流式端点（`alt=sse`）。
    pub fn streaming(config: GeminiConfig, model: impl Into<ModelId>) -> Self {
        Self {
            config,
            model: model.into(),
            mode: GenerateContentMode::Stream,
        }
    }

    /// 当前配置。
    pub fn config(&self) -> &GeminiConfig {
        &self.config
    }

    /// 模型标识。
    pub fn model(&self) -> &ModelId {
        &self.model
    }

    /// 调用模式。
    pub fn mode(&self) -> GenerateContentMode {
        self.mode
    }

    fn uri(&self) -> Result<Uri, AirxError> {
        let model = model_path(&self.model)?;
        let suffix = match self.mode {
            GenerateContentMode::Generate => GENERATE_CONTENT_SUFFIX,
            GenerateContentMode::Stream => STREAM_GENERATE_CONTENT_SUFFIX,
        };
        let url = format!(
            "{}/{}{}",
            self.config.base_url().trim_end_matches('/'),
            model,
            suffix
        );
        url.parse().map_err(|error| {
            AirxError::Configuration(ConfigurationError::new(format!(
                "无法构造 Gemini generateContent URL: {error}"
            )))
        })
    }
}

/// 把模型标识规范化为路径中的资源名。
///
/// - `gemini-2.5-flash` → `models/gemini-2.5-flash`
/// - `models/gemini-2.5-flash` 原样保留
/// - `tunedModels/...` / `publishers/...` 原样保留
pub(crate) fn model_path(model: &ModelId) -> Result<String, AirxError> {
    let text = model.as_str().trim();
    if text.is_empty() {
        return Err(AirxError::Configuration(ConfigurationError::new(
            "Gemini model 不能为空",
        )));
    }

    let valid = text
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/'));
    let valid_segments = text
        .split('/')
        .all(|segment| !segment.is_empty() && segment != "." && segment != "..");
    if !valid || !valid_segments {
        return Err(AirxError::Configuration(ConfigurationError::new(format!(
            "Gemini model 含非法字符: {text}"
        ))));
    }

    if text.starts_with("models/")
        || text.starts_with("tunedModels/")
        || text.starts_with("publishers/")
    {
        Ok(text.to_owned())
    } else {
        Ok(format!("models/{text}"))
    }
}

impl Endpoint for GenerateContentEndpoint {
    type Request = GenerateContentRequest;
    type Response = GenerateContentResponse;

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

    fn decode(&self, response: HttpResponse) -> Result<GenerateContentResponse, AirxError> {
        if !response.status.is_success() {
            return Err(error_from_response(
                &ProviderId::gemini(),
                response.status,
                &response.headers,
                &response.body,
            ));
        }

        serde_json::from_slice(&response.body)
            .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))
    }
}

impl StreamingEndpoint for GenerateContentEndpoint {
    type Event = GenerateContentResponse;

    fn decode_event(&self, event: RawEvent) -> Result<Option<Self::Event>, AirxError> {
        if event.data.is_empty() {
            return Ok(None);
        }

        let value: Value = serde_json::from_str(&event.data)
            .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))?;

        if value.get("error").is_some() {
            return Err(error_from_stream_value(&ProviderId::gemini(), value));
        }

        serde_json::from_value(value)
            .map(Some)
            .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_path_normalizes_bare_names() {
        assert_eq!(
            model_path(&ModelId::new("gemini-2.5-flash")).expect("path"),
            "models/gemini-2.5-flash"
        );
        assert_eq!(
            model_path(&ModelId::new("models/gemini-2.5-pro")).expect("path"),
            "models/gemini-2.5-pro"
        );
        assert_eq!(
            model_path(&ModelId::new("tunedModels/my-model")).expect("path"),
            "tunedModels/my-model"
        );
        assert_eq!(
            model_path(&ModelId::new("publishers/google/models/gemini")).expect("path"),
            "publishers/google/models/gemini"
        );
    }

    #[test]
    fn invalid_models_are_rejected() {
        for model in [
            "",
            " ",
            "../x",
            "models//x",
            "gemini:generateContent",
            "中文",
        ] {
            assert!(
                model_path(&ModelId::new(model)).is_err(),
                "model should be rejected: {model:?}"
            );
        }
    }
}

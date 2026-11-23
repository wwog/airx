//! Gemini Embeddings API（`embedContent` / `batchEmbedContents`）。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError, SerializationError};
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, header};
use airx_core::json::Extensions;
use airx_core::model::{ModelId, ProviderId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::endpoint::Endpoint;
use crate::gemini::common::error_from_response;
use crate::gemini::config::GeminiConfig;
use crate::gemini::generate_content::model_path;

use super::generate_content::Content;

/// 单条嵌入请求体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedContentRequest {
    /// 内容。
    pub content: Content,
    /// 任务类型（如 `RETRIEVAL_QUERY`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
    /// 标题（文档检索场景）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// 输出维度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_dimensionality: Option<u32>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl EmbedContentRequest {
    /// 创建构建器。
    pub fn builder() -> EmbedContentRequestBuilder {
        EmbedContentRequestBuilder::default()
    }
}

/// [`EmbedContentRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct EmbedContentRequestBuilder {
    content: Option<Content>,
    task_type: Option<String>,
    title: Option<String>,
    output_dimensionality: Option<u32>,
    extensions: Extensions,
}

impl EmbedContentRequestBuilder {
    /// 设置内容。
    #[must_use]
    pub fn content(mut self, content: Content) -> Self {
        self.content = Some(content);
        self
    }

    /// 设置纯文本内容。
    #[must_use]
    pub fn text(self, text: impl Into<String>) -> Self {
        self.content(Content::user_text(text))
    }

    /// 设置任务类型。
    #[must_use]
    pub fn task_type(mut self, task_type: impl Into<String>) -> Self {
        self.task_type = Some(task_type.into());
        self
    }

    /// 设置标题。
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// 设置输出维度。
    #[must_use]
    pub fn output_dimensionality(mut self, dimensionality: u32) -> Self {
        self.output_dimensionality = Some(dimensionality);
        self
    }

    /// 添加扩展字段。
    #[must_use]
    pub fn extension(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
    }

    /// 构建请求。
    pub fn build(self) -> Result<EmbedContentRequest, AirxError> {
        let content = self.content.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("EmbedContentRequest.content 必填"))
        })?;
        Ok(EmbedContentRequest {
            content,
            task_type: self.task_type,
            title: self.title,
            output_dimensionality: self.output_dimensionality,
            extensions: self.extensions,
        })
    }
}

/// 嵌入向量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Embedding {
    /// 向量值。
    #[serde(default)]
    pub values: Vec<f32>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 单条嵌入响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbedContentResponse {
    /// 嵌入向量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Embedding>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 批量嵌入请求中的单条请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchEmbedItem {
    /// 模型资源名（批量接口要求显式写入）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 内容。
    pub content: Content,
    /// 任务类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
    /// 标题。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// 输出维度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_dimensionality: Option<u32>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl BatchEmbedItem {
    /// 从单条请求构造。
    pub fn from_request(request: EmbedContentRequest, model: &ModelId) -> Result<Self, AirxError> {
        Ok(Self {
            model: Some(model_path(model)?),
            content: request.content,
            task_type: request.task_type,
            title: request.title,
            output_dimensionality: request.output_dimensionality,
            extensions: request.extensions,
        })
    }
}

/// 批量嵌入请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchEmbedContentsRequest {
    /// 请求数组。
    pub requests: Vec<BatchEmbedItem>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 批量嵌入响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchEmbedContentsResponse {
    /// 嵌入数组。
    #[serde(default)]
    pub embeddings: Vec<Embedding>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `POST /models/{model}:embedContent`。
#[derive(Debug, Clone)]
pub struct EmbedContentEndpoint {
    config: GeminiConfig,
    model: ModelId,
}

impl EmbedContentEndpoint {
    /// 创建端点。
    pub fn new(config: GeminiConfig, model: impl Into<ModelId>) -> Self {
        Self {
            config,
            model: model.into(),
        }
    }

    fn uri(&self) -> Result<airx_core::http::Uri, AirxError> {
        self.config.resolve_path(
            &format!("{}:embedContent", model_path(&self.model)?),
            "embedContent",
        )
    }
}

impl Endpoint for EmbedContentEndpoint {
    type Request = EmbedContentRequest;
    type Response = EmbedContentResponse;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        post_json(self.uri()?, request, &self.config)
    }

    fn decode(&self, response: HttpResponse) -> Result<EmbedContentResponse, AirxError> {
        decode(response)
    }
}

/// `POST /models/{model}:batchEmbedContents`。
#[derive(Debug, Clone)]
pub struct BatchEmbedContentsEndpoint {
    config: GeminiConfig,
    model: ModelId,
}

impl BatchEmbedContentsEndpoint {
    /// 创建端点。
    pub fn new(config: GeminiConfig, model: impl Into<ModelId>) -> Self {
        Self {
            config,
            model: model.into(),
        }
    }

    fn uri(&self) -> Result<airx_core::http::Uri, AirxError> {
        self.config.resolve_path(
            &format!("{}:batchEmbedContents", model_path(&self.model)?),
            "batchEmbedContents",
        )
    }
}

impl Endpoint for BatchEmbedContentsEndpoint {
    type Request = BatchEmbedContentsRequest;
    type Response = BatchEmbedContentsResponse;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        post_json(self.uri()?, request, &self.config)
    }

    fn decode(&self, response: HttpResponse) -> Result<BatchEmbedContentsResponse, AirxError> {
        decode(response)
    }
}

fn post_json<T: Serialize>(
    uri: airx_core::http::Uri,
    request: &T,
    config: &GeminiConfig,
) -> Result<HttpRequest, AirxError> {
    let body = serde_json::to_vec(request)
        .map_err(|error| AirxError::Serialization(SerializationError::from(error)))?;
    let mut http = HttpRequest::post(uri).with_body(body);
    http.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    config.apply_headers(&mut http)?;
    Ok(http)
}

fn decode<T: for<'de> Deserialize<'de>>(response: HttpResponse) -> Result<T, AirxError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_serializes_camel_case() {
        let request = EmbedContentRequest::builder()
            .text("hello")
            .task_type("RETRIEVAL_QUERY")
            .output_dimensionality(256)
            .build()
            .expect("request");
        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["content"]["parts"][0]["text"], "hello");
        assert_eq!(value["taskType"], "RETRIEVAL_QUERY");
        assert_eq!(value["outputDimensionality"], 256);
    }

    #[test]
    fn response_decodes() {
        let response: EmbedContentResponse =
            serde_json::from_value(json!({"embedding": {"values": [0.1, 0.2]}})).expect("response");
        assert_eq!(
            response.embedding.expect("embedding").values,
            vec![0.1, 0.2]
        );
    }
}

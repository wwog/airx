//! OpenAI Embeddings API。

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

/// 端点路径。
pub const EMBEDDINGS_PATH: &str = "/embeddings";

/// 输入：单条文本或多条文本。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    /// 单条文本。
    Text(String),
    /// 多条文本。
    Texts(Vec<String>),
    /// 单条 token 序列。
    Tokens(Vec<u32>),
    /// 多条 token 序列。
    TokenBatches(Vec<Vec<u32>>),
}

impl From<&str> for EmbeddingInput {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for EmbeddingInput {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<Vec<String>> for EmbeddingInput {
    fn from(value: Vec<String>) -> Self {
        Self::Texts(value)
    }
}

/// Embeddings 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    /// 模型。
    pub model: ModelId,
    /// 输入。
    pub input: EmbeddingInput,
    /// 输出维度（部分模型支持）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<u32>,
    /// 编码格式（`float` / `base64`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<String>,
    /// 用户标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl EmbeddingRequest {
    /// 创建构建器。
    pub fn builder() -> EmbeddingRequestBuilder {
        EmbeddingRequestBuilder::default()
    }
}

/// [`EmbeddingRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct EmbeddingRequestBuilder {
    model: Option<ModelId>,
    input: Option<EmbeddingInput>,
    dimensions: Option<u32>,
    encoding_format: Option<String>,
    user: Option<String>,
    extensions: Extensions,
}

impl EmbeddingRequestBuilder {
    /// 设置模型。
    #[must_use]
    pub fn model(mut self, model: impl Into<ModelId>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// 设置输入。
    #[must_use]
    pub fn input(mut self, input: impl Into<EmbeddingInput>) -> Self {
        self.input = Some(input.into());
        self
    }

    /// 设置输出维度。
    #[must_use]
    pub fn dimensions(mut self, dimensions: u32) -> Self {
        self.dimensions = Some(dimensions);
        self
    }

    /// 设置编码格式。
    #[must_use]
    pub fn encoding_format(mut self, format: impl Into<String>) -> Self {
        self.encoding_format = Some(format.into());
        self
    }

    /// 设置用户标识。
    #[must_use]
    pub fn user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// 添加扩展字段。
    #[must_use]
    pub fn extension(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
    }

    /// 构建请求。
    pub fn build(self) -> Result<EmbeddingRequest, AirxError> {
        let model = self.model.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("EmbeddingRequest.model 必填"))
        })?;
        let input = self.input.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("EmbeddingRequest.input 必填"))
        })?;
        Ok(EmbeddingRequest {
            model,
            input,
            dimensions: self.dimensions,
            encoding_format: self.encoding_format,
            user: self.user,
            extensions: self.extensions,
        })
    }
}

/// Embeddings 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    /// 对象类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 向量结果。
    #[serde(default)]
    pub data: Vec<Embedding>,
    /// 实际模型。
    #[serde(default)]
    pub model: String,
    /// 用量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<EmbeddingUsage>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 单个向量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Embedding {
    /// 对象类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 向量。
    #[serde(default)]
    pub embedding: Vec<f32>,
    /// 序号。
    #[serde(default)]
    pub index: u32,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// Embeddings 用量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingUsage {
    /// 输入 token。
    #[serde(default)]
    pub prompt_tokens: u32,
    /// 总计。
    #[serde(default)]
    pub total_tokens: u32,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// Embeddings 端点。
#[derive(Debug, Clone)]
pub struct EmbeddingsEndpoint {
    config: OpenAiConfig,
}

impl EmbeddingsEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self) -> Result<airx_core::http::Uri, AirxError> {
        self.config.endpoint_uri(EMBEDDINGS_PATH, "embeddings")
    }
}

impl Endpoint for EmbeddingsEndpoint {
    type Request = EmbeddingRequest;
    type Response = EmbeddingResponse;

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

    fn decode(&self, response: HttpResponse) -> Result<EmbeddingResponse, AirxError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_serializes_text_and_model() {
        let request = EmbeddingRequest::builder()
            .model("text-embedding-3-small")
            .input("hello")
            .dimensions(256)
            .build()
            .expect("request");
        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["model"], "text-embedding-3-small");
        assert_eq!(value["input"], "hello");
        assert_eq!(value["dimensions"], 256);
    }

    #[test]
    fn response_decodes_vector() {
        let response: EmbeddingResponse = serde_json::from_value(json!({
            "object": "list",
            "data": [{"object": "embedding", "embedding": [0.1, 0.2], "index": 0}],
            "model": "text-embedding-3-small",
            "usage": {"prompt_tokens": 2, "total_tokens": 2}
        }))
        .expect("response");
        assert_eq!(response.data[0].embedding, vec![0.1, 0.2]);
        assert_eq!(response.usage.as_ref().map(|u| u.total_tokens), Some(2));
    }
}

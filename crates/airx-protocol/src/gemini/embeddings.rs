//! Gemini Embeddings API（`embedContent` / `batchEmbedContents`）。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError, SerializationError};
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, header};
use airx_core::json::Extensions;
use airx_core::model::{ModelId, ProviderId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::endpoint::Endpoint;
use crate::gemini::common::{UsageMetadata, error_from_response};
use crate::gemini::config::GeminiConfig;
use crate::gemini::generate_content::model_path;

use super::generate_content::Content;

/// 单条嵌入请求体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedContentRequest {
    /// 模型资源名（官方 body 标 required；端点编码时按路径自动补写）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
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
    /// 是否自动截断超长输入。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_truncate: Option<bool>,
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
    model: Option<String>,
    content: Option<Content>,
    task_type: Option<String>,
    title: Option<String>,
    output_dimensionality: Option<u32>,
    auto_truncate: Option<bool>,
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

    /// 是否自动截断。
    #[must_use]
    pub fn auto_truncate(mut self, auto_truncate: bool) -> Self {
        self.auto_truncate = Some(auto_truncate);
        self
    }

    /// 显式设置模型资源名（默认由端点按路径写入）。
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
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
            model: self.model,
            content,
            task_type: self.task_type,
            title: self.title,
            output_dimensionality: self.output_dimensionality,
            auto_truncate: self.auto_truncate,
            extensions: self.extensions,
        })
    }
}

/// 嵌入向量。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Embedding {
    /// 向量值（官方为 `number[]`，按 f64 保存以保留精度）。
    #[serde(default)]
    pub values: Vec<f64>,
    /// 向量形状（部分响应返回）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<Vec<i64>>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 单条嵌入响应。
///
/// 官方两份来源形状不同：REST 参考是单数 `embedding`，SDK 类型是数组
/// `embeddings[]`（另有 `truncated` / `usageMetadata`）。两种形状都接受。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedContentResponse {
    /// 单数形状的嵌入向量（REST 参考）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Embedding>,
    /// 数组形状的嵌入向量（SDK 类型）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embeddings: Vec<Embedding>,
    /// 输入是否被截断。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
    /// Token 用量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_metadata: Option<UsageMetadata>,
    /// 计费 / 统计元数据（原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl EmbedContentResponse {
    /// 所有嵌入向量（单数与数组形状都会返回）。
    pub fn embeddings(&self) -> impl Iterator<Item = &Embedding> {
        self.embedding.iter().chain(self.embeddings.iter())
    }

    /// 第一条嵌入向量。
    pub fn first_embedding(&self) -> Option<&Embedding> {
        self.embedding.as_ref().or_else(|| self.embeddings.first())
    }

    /// 第一条嵌入向量的值（两种响应形状都能取到）。
    pub fn values(&self) -> &[f64] {
        self.first_embedding()
            .map(|embedding| embedding.values.as_slice())
            .unwrap_or_default()
    }

    /// 嵌入向量条数。
    pub fn embedding_count(&self) -> usize {
        usize::from(self.embedding.is_some()) + self.embeddings.len()
    }

    /// 是否没有任何向量。
    pub fn is_empty(&self) -> bool {
        self.embedding_count() == 0
    }
}

/// 批量嵌入请求中的单条请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchEmbedItem {
    /// 模型资源名（批量接口要求每项显式写入）。
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
    /// 是否自动截断。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_truncate: Option<bool>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl BatchEmbedItem {
    /// 从单条请求构造；模型按批量接口要求写入每一项。
    pub fn from_request(request: EmbedContentRequest, model: &ModelId) -> Result<Self, AirxError> {
        Ok(Self {
            model: Some(model_path(model)?),
            content: request.content,
            task_type: request.task_type,
            title: request.title,
            output_dimensionality: request.output_dimensionality,
            auto_truncate: request.auto_truncate,
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
        let mut request = request.clone();
        if request.model.is_none() {
            request.model = Some(model_path(&self.model)?);
        }
        post_json(self.uri()?, &request, &self.config)
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
            .auto_truncate(true)
            .build()
            .expect("request");
        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["content"]["parts"][0]["text"], "hello");
        assert_eq!(value["taskType"], "RETRIEVAL_QUERY");
        assert_eq!(value["outputDimensionality"], 256);
        assert_eq!(value["autoTruncate"], true);
    }

    #[test]
    fn endpoint_writes_model_into_body() {
        let endpoint = EmbedContentEndpoint::new(GeminiConfig::new("k"), "gemini-embedding-001");
        let request = EmbedContentRequest::builder()
            .text("hello")
            .build()
            .expect("request");
        let http = endpoint.encode(&request).expect("encode");
        let body: Value = serde_json::from_slice(&http.body).expect("body");

        assert_eq!(body["model"], "models/gemini-embedding-001");
        assert_eq!(
            http.uri.path(),
            "/v1beta/models/gemini-embedding-001:embedContent"
        );
    }

    #[test]
    fn explicit_model_wins_over_path() {
        let endpoint = EmbedContentEndpoint::new(GeminiConfig::new("k"), "gemini-embedding-001");
        let request = EmbedContentRequest::builder()
            .text("hello")
            .model("models/custom")
            .build()
            .expect("request");
        let http = endpoint.encode(&request).expect("encode");
        let body: Value = serde_json::from_slice(&http.body).expect("body");
        assert_eq!(body["model"], "models/custom");
    }

    #[test]
    fn response_decodes_singular_shape() {
        let response: EmbedContentResponse =
            serde_json::from_value(json!({"embedding": {"values": [0.1, 0.2]}})).expect("response");
        assert_eq!(
            response.embedding.as_ref().expect("embedding").values,
            vec![0.1, 0.2]
        );
        assert_eq!(response.values(), &[0.1, 0.2]);
        assert_eq!(response.embedding_count(), 1);
    }

    #[test]
    fn response_decodes_array_shape() {
        let response: EmbedContentResponse = serde_json::from_value(json!({
            "embeddings": [{"values": [0.5, 0.25], "shape": [1, 2]}],
            "truncated": true,
            "usageMetadata": {"promptTokenCount": 3, "totalTokenCount": 3}
        }))
        .expect("response");

        assert!(response.embedding.is_none());
        assert_eq!(response.embeddings.len(), 1);
        assert_eq!(response.values(), &[0.5, 0.25]);
        assert_eq!(response.truncated, Some(true));
        assert_eq!(
            response
                .usage_metadata
                .as_ref()
                .map(|usage| usage.total_token_count),
            Some(3)
        );
        assert_eq!(response.embeddings().count(), 1);
        assert!(!response.is_empty());
    }

    #[test]
    fn batch_item_writes_model_per_item() {
        let request = EmbedContentRequest::builder()
            .text("hello")
            .build()
            .expect("request");
        let item = BatchEmbedItem::from_request(request, &ModelId::new("gemini-embedding-001"))
            .expect("item");
        let value = serde_json::to_value(&item).expect("serialize");
        assert_eq!(value["model"], "models/gemini-embedding-001");
    }
}

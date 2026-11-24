//! OpenAI Batches API。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError, SerializationError};
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, header};
use airx_core::json::Extensions;
use airx_core::model::ProviderId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::endpoint::Endpoint;
use crate::openai::common::error::error_from_response;
use crate::openai::config::OpenAiConfig;
use crate::openai::files::ExpiresAfter;

/// 端点路径。
pub const BATCHES_PATH: &str = "/batches";

/// 批处理对象。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchObject {
    /// 批处理 ID。
    pub id: String,
    /// 对象类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 目标端点。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// 输入文件 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_file_id: Option<String>,
    /// 完成窗口。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_window: Option<String>,
    /// 状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// 输出文件 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_file_id: Option<String>,
    /// 错误文件 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_file_id: Option<String>,
    /// 创建时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    /// 开始处理时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_progress_at: Option<u64>,
    /// 过期时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    /// 收尾时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finalizing_at: Option<u64>,
    /// 完成时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
    /// 失败时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_at: Option<u64>,
    /// 过期时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expired_at: Option<u64>,
    /// 取消中时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelling_at: Option<u64>,
    /// 取消时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_at: Option<u64>,
    /// 请求计数（结构随官方演进，原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_counts: Option<Value>,
    /// 处理该批次的模型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 批次 token 用量（2025-09-07 之后创建的批次才有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<BatchUsage>,
    /// 元数据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    /// 错误。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errors: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 批次 token 用量。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BatchUsage {
    /// 输入 token。
    #[serde(default)]
    pub input_tokens: u64,
    /// 输入 token 明细（原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens_details: Option<Value>,
    /// 输出 token。
    #[serde(default)]
    pub output_tokens: u64,
    /// 输出 token 明细（原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens_details: Option<Value>,
    /// 总计。
    #[serde(default)]
    pub total_tokens: u64,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 创建批处理请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateBatchRequest {
    /// 输入文件 ID。
    pub input_file_id: String,
    /// 目标端点。
    pub endpoint: String,
    /// 完成窗口（通常 `24h`）。
    pub completion_window: String,
    /// 元数据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    /// 输出 / 错误文件的过期策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_expires_after: Option<ExpiresAfter>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl CreateBatchRequest {
    /// 创建构建器。
    pub fn builder() -> CreateBatchRequestBuilder {
        CreateBatchRequestBuilder::default()
    }
}

/// [`CreateBatchRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct CreateBatchRequestBuilder {
    input_file_id: Option<String>,
    endpoint: Option<String>,
    completion_window: Option<String>,
    metadata: Option<Value>,
    output_expires_after: Option<ExpiresAfter>,
    extensions: Extensions,
}

impl CreateBatchRequestBuilder {
    /// 设置输入文件 ID。
    #[must_use]
    pub fn input_file_id(mut self, id: impl Into<String>) -> Self {
        self.input_file_id = Some(id.into());
        self
    }

    /// 设置目标端点。
    #[must_use]
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// 设置完成窗口。
    #[must_use]
    pub fn completion_window(mut self, window: impl Into<String>) -> Self {
        self.completion_window = Some(window.into());
        self
    }

    /// 设置元数据。
    #[must_use]
    pub fn metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// 设置输出 / 错误文件的过期策略。
    #[must_use]
    pub fn output_expires_after(mut self, expires_after: ExpiresAfter) -> Self {
        self.output_expires_after = Some(expires_after);
        self
    }

    /// 添加扩展字段。
    #[must_use]
    pub fn extension(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
    }

    /// 构建请求。
    pub fn build(self) -> Result<CreateBatchRequest, AirxError> {
        let input_file_id = self.input_file_id.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new(
                "CreateBatchRequest.input_file_id 必填",
            ))
        })?;
        let endpoint = self.endpoint.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("CreateBatchRequest.endpoint 必填"))
        })?;
        let completion_window = self.completion_window.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new(
                "CreateBatchRequest.completion_window 必填",
            ))
        })?;
        Ok(CreateBatchRequest {
            input_file_id,
            endpoint,
            completion_window,
            metadata: self.metadata,
            output_expires_after: self.output_expires_after,
            extensions: self.extensions,
        })
    }
}

/// 批处理列表。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchList {
    /// 对象类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 批处理数组。
    #[serde(default)]
    pub data: Vec<BatchObject>,
    /// 是否还有下一页。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
    /// 当前页第一个 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_id: Option<String>,
    /// 当前页最后一个 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl BatchList {
    /// 下一页游标。
    pub fn next_cursor(&self) -> Option<&str> {
        if self.has_more == Some(true) {
            self.last_id.as_deref()
        } else {
            None
        }
    }
}

/// 列表参数。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListBatchesRequest {
    /// 每页数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// 从该 ID 之后开始。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// `POST /batches`。
#[derive(Debug, Clone)]
pub struct CreateBatchEndpoint {
    config: OpenAiConfig,
}

impl CreateBatchEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self) -> Result<airx_core::http::Uri, AirxError> {
        self.config.endpoint_uri(BATCHES_PATH, "batches")
    }
}

impl Endpoint for CreateBatchEndpoint {
    type Request = CreateBatchRequest;
    type Response = BatchObject;

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

    fn decode(&self, response: HttpResponse) -> Result<BatchObject, AirxError> {
        decode(response)
    }
}

/// `GET /batches`。
#[derive(Debug, Clone)]
pub struct ListBatchesEndpoint {
    config: OpenAiConfig,
}

impl ListBatchesEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, request: &ListBatchesRequest) -> Result<airx_core::http::Uri, AirxError> {
        let mut query = Vec::new();
        if let Some(limit) = request.limit {
            query.push(format!("limit={limit}"));
        }
        if let Some(after) = &request.after {
            query.push(format!("after={}", batch_id(after, "after")?));
        }
        let path = if query.is_empty() {
            BATCHES_PATH.to_owned()
        } else {
            format!("{BATCHES_PATH}?{}", query.join("&"))
        };
        self.config.endpoint_uri(&path, "batches")
    }
}

impl Endpoint for ListBatchesEndpoint {
    type Request = ListBatchesRequest;
    type Response = BatchList;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::get(self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<BatchList, AirxError> {
        decode(response)
    }
}

/// `GET /batches/{id}` 与 `POST /batches/{id}/cancel`。
#[derive(Debug, Clone)]
pub struct BatchEndpoint {
    config: OpenAiConfig,
    cancel: bool,
}

impl BatchEndpoint {
    /// 获取批处理。
    pub fn retrieve(config: OpenAiConfig) -> Self {
        Self {
            config,
            cancel: false,
        }
    }

    /// 取消批处理。
    pub fn cancel(config: OpenAiConfig) -> Self {
        Self {
            config,
            cancel: true,
        }
    }

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        let path = if self.cancel {
            format!("{BATCHES_PATH}/{}/cancel", batch_id(id, "id")?)
        } else {
            format!("{BATCHES_PATH}/{}", batch_id(id, "id")?)
        };
        self.config.endpoint_uri(&path, "batches")
    }
}

impl Endpoint for BatchEndpoint {
    type Request = String;
    type Response = BatchObject;

    fn encode(&self, request: &String) -> Result<HttpRequest, AirxError> {
        let mut http = if self.cancel {
            HttpRequest::post(self.uri(request)?)
        } else {
            HttpRequest::get(self.uri(request)?)
        };
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<BatchObject, AirxError> {
        decode(response)
    }
}

fn decode<T: for<'de> Deserialize<'de>>(response: HttpResponse) -> Result<T, AirxError> {
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

fn batch_id(id: &str, field: &str) -> Result<String, AirxError> {
    let text = id.trim();
    if text.is_empty()
        || !text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(AirxError::Configuration(ConfigurationError::new(format!(
            "OpenAI batch {field} 非法"
        ))));
    }
    Ok(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn batch_object_decodes() {
        let batch: BatchObject = serde_json::from_value(json!({
            "id": "batch_1",
            "object": "batch",
            "endpoint": "/v1/chat/completions",
            "input_file_id": "file-1",
            "status": "completed",
            "model": "gpt-4o-mini",
            "request_counts": {"total": 1, "completed": 1, "failed": 0},
            "usage": {"input_tokens": 10, "output_tokens": 5, "total_tokens": 15}
        }))
        .expect("batch");
        assert_eq!(batch.id, "batch_1");
        assert_eq!(batch.status.as_deref(), Some("completed"));
        assert_eq!(batch.model.as_deref(), Some("gpt-4o-mini"));
        assert_eq!(
            batch.usage.as_ref().map(|usage| usage.total_tokens),
            Some(15)
        );
    }

    #[test]
    fn create_request_requires_fields() {
        assert!(CreateBatchRequest::builder().build().is_err());
        let request = CreateBatchRequest::builder()
            .input_file_id("file-1")
            .endpoint("/v1/chat/completions")
            .completion_window("24h")
            .build()
            .expect("request");
        assert_eq!(request.completion_window, "24h");
    }

    #[test]
    fn output_expires_after_serializes() {
        let request = CreateBatchRequest::builder()
            .input_file_id("file-1")
            .endpoint("/v1/chat/completions")
            .completion_window("24h")
            .output_expires_after(ExpiresAfter::after_seconds(3600))
            .build()
            .expect("request");
        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["output_expires_after"]["anchor"], "created_at");
        assert_eq!(value["output_expires_after"]["seconds"], 3600);
    }
}

//! Anthropic Messages Batches API。
//!
//! 路径跟随 `messages_path`：`{messages_path}/batches`。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError, SerializationError};
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, header};
use airx_core::json::Extensions;
use airx_core::model::ProviderId;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::anthropic::common::{AnthropicErrorResponse, error_from_response};
use crate::anthropic::config::AnthropicConfig;
use crate::anthropic::messages::{MessagesRequest, MessagesResponse};
use crate::endpoint::Endpoint;

/// 单个批处理请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchRequestItem {
    /// 调用方自定义 ID。
    pub custom_id: String,
    /// Messages 请求参数。
    pub params: MessagesRequest,
}

/// 创建批处理请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateBatchRequest {
    /// 请求数组。
    pub requests: Vec<BatchRequestItem>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl CreateBatchRequest {
    /// 构造请求。
    pub fn new(requests: impl IntoIterator<Item = BatchRequestItem>) -> Self {
        Self {
            requests: requests.into_iter().collect(),
            extensions: Extensions::new(),
        }
    }
}

/// 批处理对象。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageBatch {
    /// 批处理 ID。
    pub id: String,
    /// 对象类型。
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// 处理状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub processing_status: Option<String>,
    /// 请求计数（结构随官方演进，原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_counts: Option<Value>,
    /// 创建时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// 过期时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// 结束时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    /// 发起取消时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_initiated_at: Option<String>,
    /// 结果 URL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results_url: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 批处理列表。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchList {
    /// 当前页数据。
    #[serde(default)]
    pub data: Vec<MessageBatch>,
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

/// 列表参数。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListBatchesRequest {
    /// 每页数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// 从该 ID 之后开始。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_id: Option<String>,
    /// 从该 ID 之前开始。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_id: Option<String>,
}

/// 结果行（JSONL 的一行）。
///
/// 官方 `.jsonl` 每行是 `{custom_id, result}`，`result.type` 决定载荷形状。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchResultLine {
    /// 调用方自定义 ID（结果顺序不保证与请求一致）。
    pub custom_id: String,
    /// 处理结果。
    pub result: BatchResult,
}

/// 单条批处理结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum BatchResult {
    /// 成功（含完整 Messages 响应）。
    Succeeded {
        /// Messages 响应（boxed：避免批处理结果枚举整体过大）。
        message: Box<MessagesResponse>,
    },
    /// 失败（官方 `Shared.ErrorResponse` 信封：`{type:"error", error:{...}, request_id}`）。
    Errored {
        /// 错误信封。
        error: AnthropicErrorResponse,
    },
    /// 被取消。
    Canceled,
    /// 已过期。
    Expired,
}

/// 批处理删除结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeletedBatch {
    /// 批处理 ID。
    pub id: String,
    /// 对象类型（`message_batch_deleted`）。
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `POST {messages_path}/batches`。
#[derive(Debug, Clone)]
pub struct CreateBatchEndpoint {
    config: AnthropicConfig,
}

impl CreateBatchEndpoint {
    /// 创建端点。
    pub fn new(config: AnthropicConfig) -> Self {
        Self { config }
    }

    fn uri(&self) -> Result<airx_core::http::Uri, AirxError> {
        self.config
            .resolve_path(&batch_path(&self.config), "batches")
    }
}

impl Endpoint for CreateBatchEndpoint {
    type Request = CreateBatchRequest;
    type Response = MessageBatch;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        post_json(self.uri()?, request, &self.config)
    }

    fn decode(&self, response: HttpResponse) -> Result<MessageBatch, AirxError> {
        decode(response)
    }
}

/// `GET {messages_path}/batches`。
#[derive(Debug, Clone)]
pub struct ListBatchesEndpoint {
    config: AnthropicConfig,
}

impl ListBatchesEndpoint {
    /// 创建端点。
    pub fn new(config: AnthropicConfig) -> Self {
        Self { config }
    }

    fn uri(&self, request: &ListBatchesRequest) -> Result<airx_core::http::Uri, AirxError> {
        let mut query = Vec::new();
        if let Some(limit) = request.limit {
            query.push(format!("limit={limit}"));
        }
        if let Some(after_id) = &request.after_id {
            query.push(format!("after_id={}", query_value(after_id)?));
        }
        if let Some(before_id) = &request.before_id {
            query.push(format!("before_id={}", query_value(before_id)?));
        }
        let base = batch_path(&self.config);
        let path = if query.is_empty() {
            base
        } else {
            format!("{base}?{}", query.join("&"))
        };
        self.config.resolve_path(&path, "batches")
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

/// `GET {messages_path}/batches/{id}` 与 `POST .../cancel`、`GET .../results`。
#[derive(Debug, Clone)]
pub struct BatchEndpoint {
    config: AnthropicConfig,
    mode: BatchMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BatchMode {
    Retrieve,
    Cancel,
}

impl BatchEndpoint {
    /// 获取批处理。
    pub fn retrieve(config: AnthropicConfig) -> Self {
        Self {
            config,
            mode: BatchMode::Retrieve,
        }
    }

    /// 取消批处理。
    pub fn cancel(config: AnthropicConfig) -> Self {
        Self {
            config,
            mode: BatchMode::Cancel,
        }
    }

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        let base = format!("{}/{}", batch_path(&self.config), batch_id(id)?);
        let path = match self.mode {
            BatchMode::Retrieve => base,
            BatchMode::Cancel => format!("{base}/cancel"),
        };
        self.config.resolve_path(&path, "batches")
    }
}

impl Endpoint for BatchEndpoint {
    type Request = String;
    type Response = MessageBatch;

    fn encode(&self, request: &String) -> Result<HttpRequest, AirxError> {
        let mut http = match self.mode {
            BatchMode::Cancel => HttpRequest::post(self.uri(request)?),
            BatchMode::Retrieve => HttpRequest::get(self.uri(request)?),
        };
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<MessageBatch, AirxError> {
        decode(response)
    }
}

/// 结果端点（JSONL）。
#[derive(Debug, Clone)]
pub struct BatchResultsEndpoint {
    config: AnthropicConfig,
}

impl BatchResultsEndpoint {
    /// 创建端点。
    pub fn new(config: AnthropicConfig) -> Self {
        Self { config }
    }

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        self.config.resolve_path(
            &format!("{}/{}/results", batch_path(&self.config), batch_id(id)?),
            "batches",
        )
    }
}

impl Endpoint for BatchResultsEndpoint {
    type Request = String;
    type Response = Bytes;

    fn encode(&self, request: &String) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::get(self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<Bytes, AirxError> {
        if !response.status.is_success() {
            return Err(error_from_response(
                &ProviderId::anthropic(),
                response.status,
                &response.headers,
                &response.body,
            ));
        }
        Ok(response.body)
    }
}

/// `DELETE {messages_path}/batches/{id}`。
#[derive(Debug, Clone)]
pub struct DeleteBatchEndpoint {
    config: AnthropicConfig,
}

impl DeleteBatchEndpoint {
    /// 创建端点。
    pub fn new(config: AnthropicConfig) -> Self {
        Self { config }
    }

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        self.config.resolve_path(
            &format!("{}/{}", batch_path(&self.config), batch_id(id)?),
            "batches",
        )
    }
}

impl Endpoint for DeleteBatchEndpoint {
    type Request = String;
    type Response = DeletedBatch;

    fn encode(&self, request: &String) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::new(airx_core::http::Method::DELETE, self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<DeletedBatch, AirxError> {
        decode(response)
    }
}

/// 类型化结果端点（逐行解析 JSONL）。
#[derive(Debug, Clone)]
pub struct BatchResultsTypedEndpoint {
    config: AnthropicConfig,
}

impl BatchResultsTypedEndpoint {
    /// 创建端点。
    pub fn new(config: AnthropicConfig) -> Self {
        Self { config }
    }

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        self.config.resolve_path(
            &format!("{}/{}/results", batch_path(&self.config), batch_id(id)?),
            "batches",
        )
    }
}

impl Endpoint for BatchResultsTypedEndpoint {
    type Request = String;
    type Response = Vec<BatchResultLine>;

    fn encode(&self, request: &String) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::get(self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<Vec<BatchResultLine>, AirxError> {
        if !response.status.is_success() {
            return Err(error_from_response(
                &ProviderId::anthropic(),
                response.status,
                &response.headers,
                &response.body,
            ));
        }
        parse_result_lines(&response.body)
    }
}

/// 逐行解析批次结果 JSONL（空行忽略）。
pub fn parse_result_lines(body: &[u8]) -> Result<Vec<BatchResultLine>, AirxError> {
    let text = std::str::from_utf8(body).map_err(|error| {
        AirxError::Deserialization(DeserializationError::new(error.to_string()))
    })?;
    let mut lines = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        lines.push(
            serde_json::from_str::<BatchResultLine>(line)
                .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))?,
        );
    }
    Ok(lines)
}

fn batch_path(config: &AnthropicConfig) -> String {
    format!("{}/batches", config.messages_path().trim_end_matches('/'))
}

fn post_json<T: Serialize>(
    uri: airx_core::http::Uri,
    request: &T,
    config: &AnthropicConfig,
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
            &ProviderId::anthropic(),
            response.status,
            &response.headers,
            &response.body,
        ));
    }
    serde_json::from_slice(&response.body)
        .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))
}

fn batch_id(id: &str) -> Result<String, AirxError> {
    let text = id.trim();
    if text.is_empty()
        || !text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(AirxError::Configuration(ConfigurationError::new(
            "Anthropic batch id 非法",
        )));
    }
    Ok(text.to_owned())
}

fn query_value(value: &str) -> Result<String, AirxError> {
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(AirxError::Configuration(ConfigurationError::new(
            "Anthropic batches 游标含非法字符",
        )));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn batch_decodes() {
        let batch: MessageBatch = serde_json::from_value(json!({
            "id": "msgbatch_1",
            "type": "message_batch",
            "processing_status": "ended",
            "request_counts": {"processing": 0, "succeeded": 1, "errored": 0, "canceled": 0, "expired": 0}
        }))
        .expect("batch");
        assert_eq!(batch.id, "msgbatch_1");
    }

    #[test]
    fn delete_endpoint_uses_delete_method_and_path() {
        let endpoint = DeleteBatchEndpoint::new(AnthropicConfig::new("key"));
        let http = endpoint.encode(&"msgbatch_1".to_owned()).expect("encode");
        assert_eq!(http.method, airx_core::http::Method::DELETE);
        assert_eq!(http.uri.path(), "/v1/messages/batches/msgbatch_1");

        let deleted = endpoint
            .decode(HttpResponse::new(
                airx_core::http::StatusCode::OK,
                airx_core::http::HeaderMap::new(),
                br#"{"id":"msgbatch_1","type":"message_batch_deleted"}"#.as_slice(),
            ))
            .expect("decode");
        assert_eq!(deleted.id, "msgbatch_1");
        assert_eq!(deleted.kind.as_deref(), Some("message_batch_deleted"));
    }

    #[test]
    fn typed_results_parse_jsonl() {
        let body = concat!(
            r#"{"custom_id":"1","result":{"type":"succeeded","message":{"id":"msg_1","model":"claude","content":[{"type":"text","text":"ok"}],"usage":{"input_tokens":1,"output_tokens":1}}}}"#,
            "\n",
            r#"{"custom_id":"2","result":{"type":"errored","error":{"type":"error","error":{"type":"invalid_request_error","message":"bad","details":{"error_code":"request_too_large"}},"request_id":"req_1"}}}"#,
            "\n",
            r#"{"custom_id":"3","result":{"type":"canceled"}}"#,
            "\n",
            r#"{"custom_id":"4","result":{"type":"expired"}}"#,
            "\n"
        );

        let lines = parse_result_lines(body.as_bytes()).expect("lines");
        assert_eq!(lines.len(), 4);
        match &lines[0].result {
            BatchResult::Succeeded { message } => assert_eq!(message.text(), "ok"),
            other => panic!("unexpected: {other:?}"),
        }
        match &lines[1].result {
            BatchResult::Errored { error } => {
                assert_eq!(error.error.code().as_deref(), Some("request_too_large"));
                assert_eq!(error.request_id.as_deref(), Some("req_1"));
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(lines[2].result, BatchResult::Canceled);
        assert_eq!(lines[3].result, BatchResult::Expired);
    }
}

//! Chat Completions 已存储补全（stored completions）端点。
//!
//! 只有 `store: true` 的补全才会被保存；这 4 个端点用于读取 / 列出 / 更新元数据 /
//! 删除。列表接口的 `metadata` 过滤按官方形状编码为 `metadata[key]=value`。

use std::collections::BTreeMap;

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

use super::response::ChatCompletionResponse;

/// 已存储补全列表。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletionList {
    /// 对象类型（`list`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 补全数组。
    #[serde(default)]
    pub data: Vec<ChatCompletionResponse>,
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

impl ChatCompletionList {
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
pub struct ChatCompletionListRequest {
    /// 从该补全 ID 之后开始。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// 每页数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// 按模型过滤。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,
    /// 排序（`asc` / `desc`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
    /// 元数据过滤（编码为 `metadata[key]=value`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
}

impl ChatCompletionListRequest {
    /// 创建构建器。
    pub fn builder() -> ChatCompletionListRequestBuilder {
        ChatCompletionListRequestBuilder::default()
    }
}

/// [`ChatCompletionListRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct ChatCompletionListRequestBuilder {
    after: Option<String>,
    limit: Option<u32>,
    model: Option<ModelId>,
    order: Option<String>,
    metadata: Option<BTreeMap<String, String>>,
}

impl ChatCompletionListRequestBuilder {
    /// 从该补全 ID 之后开始。
    #[must_use]
    pub fn after(mut self, after: impl Into<String>) -> Self {
        self.after = Some(after.into());
        self
    }

    /// 每页数量。
    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// 按模型过滤。
    #[must_use]
    pub fn model(mut self, model: impl Into<ModelId>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// 排序（`asc` / `desc`）。
    #[must_use]
    pub fn order(mut self, order: impl Into<String>) -> Self {
        self.order = Some(order.into());
        self
    }

    /// 追加一个元数据过滤条件。
    #[must_use]
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata
            .get_or_insert_with(BTreeMap::new)
            .insert(key.into(), value.into());
        self
    }

    /// 构建。
    pub fn build(self) -> ChatCompletionListRequest {
        ChatCompletionListRequest {
            after: self.after,
            limit: self.limit,
            model: self.model,
            order: self.order,
            metadata: self.metadata,
        }
    }
}

/// 更新已存储补全的请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletionUpdateRequest {
    /// 补全 ID。
    pub id: String,
    /// 新元数据。
    pub metadata: Value,
}

impl ChatCompletionUpdateRequest {
    /// 创建请求。
    pub fn new(id: impl Into<String>, metadata: Value) -> Self {
        Self {
            id: id.into(),
            metadata,
        }
    }
}

/// 删除已存储补全的响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletionDeleteResponse {
    /// 补全 ID。
    pub id: String,
    /// 对象类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 是否删除。
    #[serde(default)]
    pub deleted: bool,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `GET /chat/completions/{id}`。
#[derive(Debug, Clone)]
pub struct RetrieveChatCompletionEndpoint {
    config: OpenAiConfig,
}

impl RetrieveChatCompletionEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        let path = format!("{}/{}", self.config.chat_path(), completion_id(id)?);
        self.config.endpoint_uri(&path, "Chat Completions retrieve")
    }
}

impl Endpoint for RetrieveChatCompletionEndpoint {
    type Request = String;
    type Response = ChatCompletionResponse;

    fn encode(&self, request: &String) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::get(self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<ChatCompletionResponse, AirxError> {
        decode(response)
    }
}

/// `GET /chat/completions`。
#[derive(Debug, Clone)]
pub struct ListChatCompletionsEndpoint {
    config: OpenAiConfig,
}

impl ListChatCompletionsEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, request: &ChatCompletionListRequest) -> Result<airx_core::http::Uri, AirxError> {
        let mut query = Vec::new();
        if let Some(after) = &request.after {
            query.push(format!("after={}", query_value(after, "after")?));
        }
        if let Some(limit) = request.limit {
            query.push(format!("limit={limit}"));
        }
        if let Some(model) = &request.model {
            query.push(format!("model={}", query_value(model.as_str(), "model")?));
        }
        if let Some(order) = &request.order {
            query.push(format!("order={}", query_value(order, "order")?));
        }
        if let Some(metadata) = &request.metadata {
            for (key, value) in metadata {
                query.push(format!(
                    "metadata[{}]={}",
                    encode_component(key)?,
                    encode_component(value)?
                ));
            }
        }

        let path = if query.is_empty() {
            self.config.chat_path().to_owned()
        } else {
            format!("{}?{}", self.config.chat_path(), query.join("&"))
        };
        self.config.endpoint_uri(&path, "Chat Completions list")
    }
}

impl Endpoint for ListChatCompletionsEndpoint {
    type Request = ChatCompletionListRequest;
    type Response = ChatCompletionList;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::get(self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<ChatCompletionList, AirxError> {
        decode(response)
    }
}

/// `POST /chat/completions/{id}`（更新 metadata）。
#[derive(Debug, Clone)]
pub struct UpdateChatCompletionEndpoint {
    config: OpenAiConfig,
}

impl UpdateChatCompletionEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        let path = format!("{}/{}", self.config.chat_path(), completion_id(id)?);
        self.config.endpoint_uri(&path, "Chat Completions update")
    }
}

impl Endpoint for UpdateChatCompletionEndpoint {
    type Request = ChatCompletionUpdateRequest;
    type Response = ChatCompletionResponse;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        let body = serde_json::json!({ "metadata": request.metadata });
        let body = serde_json::to_vec(&body)
            .map_err(|error| AirxError::Serialization(SerializationError::from(error)))?;
        let mut http = HttpRequest::post(self.uri(&request.id)?).with_body(body);
        http.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<ChatCompletionResponse, AirxError> {
        decode(response)
    }
}

/// `DELETE /chat/completions/{id}`。
#[derive(Debug, Clone)]
pub struct DeleteChatCompletionEndpoint {
    config: OpenAiConfig,
}

impl DeleteChatCompletionEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        let path = format!("{}/{}", self.config.chat_path(), completion_id(id)?);
        self.config.endpoint_uri(&path, "Chat Completions delete")
    }
}

impl Endpoint for DeleteChatCompletionEndpoint {
    type Request = String;
    type Response = ChatCompletionDeleteResponse;

    fn encode(&self, request: &String) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::new(airx_core::http::Method::DELETE, self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<ChatCompletionDeleteResponse, AirxError> {
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

fn completion_id(id: &str) -> Result<String, AirxError> {
    let text = id.trim();
    if text.is_empty()
        || !text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(AirxError::Configuration(ConfigurationError::new(
            "Chat completion id 非法",
        )));
    }
    Ok(text.to_owned())
}

fn query_value(value: &str, field: &str) -> Result<String, AirxError> {
    let text = value.trim();
    if text.is_empty()
        || !text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '/'))
    {
        return Err(AirxError::Configuration(ConfigurationError::new(format!(
            "Chat Completions list {field} 含非法字符"
        ))));
    }
    Ok(text.to_owned())
}

fn encode_component(value: &str) -> Result<String, AirxError> {
    if value.is_empty() {
        return Err(AirxError::Configuration(ConfigurationError::new(
            "Chat Completions metadata 过滤值不能为空",
        )));
    }
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config() -> OpenAiConfig {
        OpenAiConfig::new("sk-test")
    }

    #[test]
    fn retrieve_uses_chat_path_and_id() {
        let endpoint = RetrieveChatCompletionEndpoint::new(config());
        let http = endpoint.encode(&"chatcmpl-1".to_owned()).expect("encode");
        assert_eq!(http.method, airx_core::http::Method::GET);
        assert_eq!(
            http.uri.to_string(),
            "https://api.openai.com/v1/chat/completions/chatcmpl-1"
        );
    }

    #[test]
    fn list_encodes_all_query_params() {
        let endpoint = ListChatCompletionsEndpoint::new(config());
        let request = ChatCompletionListRequest::builder()
            .after("chatcmpl-1")
            .limit(5)
            .model("gpt-4o-mini")
            .order("desc")
            .metadata("trace", "a b")
            .build();
        let http = endpoint.encode(&request).expect("encode");
        assert_eq!(
            http.uri.query(),
            Some("after=chatcmpl-1&limit=5&model=gpt-4o-mini&order=desc&metadata[trace]=a%20b")
        );
        assert_eq!(http.uri.path(), "/v1/chat/completions");
    }

    #[test]
    fn list_without_params_has_no_query() {
        let endpoint = ListChatCompletionsEndpoint::new(config());
        let http = endpoint
            .encode(&ChatCompletionListRequest::default())
            .expect("encode");
        assert_eq!(http.uri.query(), None);
    }

    #[test]
    fn update_posts_metadata() {
        let endpoint = UpdateChatCompletionEndpoint::new(config());
        let http = endpoint
            .encode(&ChatCompletionUpdateRequest::new(
                "chatcmpl-1",
                json!({"trace": "t-1"}),
            ))
            .expect("encode");
        assert_eq!(http.method, airx_core::http::Method::POST);
        assert_eq!(http.uri.path(), "/v1/chat/completions/chatcmpl-1");
        let body: Value = serde_json::from_slice(&http.body).expect("body");
        assert_eq!(body["metadata"]["trace"], "t-1");
    }

    #[test]
    fn delete_uses_delete_method() {
        let endpoint = DeleteChatCompletionEndpoint::new(config());
        let http = endpoint.encode(&"chatcmpl-1".to_owned()).expect("encode");
        assert_eq!(http.method, airx_core::http::Method::DELETE);
        assert_eq!(http.uri.path(), "/v1/chat/completions/chatcmpl-1");
    }

    #[test]
    fn custom_chat_path_is_respected() {
        let config = config()
            .with_chat_path("v2/completions")
            .expect("chat path");
        let endpoint = RetrieveChatCompletionEndpoint::new(config);
        let http = endpoint.encode(&"c1".to_owned()).expect("encode");
        assert_eq!(
            http.uri.to_string(),
            "https://api.openai.com/v1/v2/completions/c1"
        );
    }

    #[test]
    fn invalid_ids_are_rejected() {
        assert!(completion_id("").is_err());
        assert!(completion_id("bad id").is_err());
    }

    #[test]
    fn list_decodes_cursor() {
        let list: ChatCompletionList = serde_json::from_value(json!({
            "object": "list",
            "data": [{"id": "c1", "model": "m", "choices": []}],
            "has_more": true,
            "last_id": "c1"
        }))
        .expect("list");
        assert_eq!(list.next_cursor(), Some("c1"));
    }
}

//! Anthropic Models API。
//!
//! `GET /v1/models`（cursor 分页）/ `GET /v1/models/{model_id}`。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError};
use airx_core::http::{HttpRequest, HttpResponse, Uri};
use airx_core::json::Extensions;
use airx_core::model::{ModelId, ProviderId};
use serde::{Deserialize, Serialize};

use crate::anthropic::common::error_from_response;
use crate::anthropic::config::AnthropicConfig;
use crate::endpoint::Endpoint;

/// 模型列表路径。
pub const MODELS_PATH: &str = "/v1/models";

/// 模型信息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model {
    /// 模型 ID。
    pub id: String,
    /// 对象类型，通常为 `model`。
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// 展示名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// 创建时间（RFC 3339）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 模型列表响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelList {
    /// 当前页数据。
    #[serde(default)]
    pub data: Vec<Model>,
    /// 是否还有下一页。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
    /// 当前页第一个 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_id: Option<String>,
    /// 当前页最后一个 ID（下一页的 `after_id`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ModelList {
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
pub struct ListModelsRequest {
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

impl ListModelsRequest {
    /// 创建构建器。
    pub fn builder() -> ListModelsRequestBuilder {
        ListModelsRequestBuilder::default()
    }
}

/// [`ListModelsRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct ListModelsRequestBuilder {
    limit: Option<u32>,
    after_id: Option<String>,
    before_id: Option<String>,
}

impl ListModelsRequestBuilder {
    /// 每页数量。
    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// 从该 ID 之后开始。
    #[must_use]
    pub fn after_id(mut self, after_id: impl Into<String>) -> Self {
        self.after_id = Some(after_id.into());
        self
    }

    /// 从该 ID 之前开始。
    #[must_use]
    pub fn before_id(mut self, before_id: impl Into<String>) -> Self {
        self.before_id = Some(before_id.into());
        self
    }

    /// 构建。
    pub fn build(self) -> ListModelsRequest {
        ListModelsRequest {
            limit: self.limit,
            after_id: self.after_id,
            before_id: self.before_id,
        }
    }
}

/// `GET /v1/models`。
#[derive(Debug, Clone)]
pub struct ListModelsEndpoint {
    config: AnthropicConfig,
}

impl ListModelsEndpoint {
    /// 创建端点。
    pub fn new(config: AnthropicConfig) -> Self {
        Self { config }
    }

    fn uri(&self, request: &ListModelsRequest) -> Result<Uri, AirxError> {
        let mut query = Vec::new();
        if let Some(limit) = request.limit {
            query.push(format!("limit={limit}"));
        }
        if let Some(after_id) = &request.after_id {
            query.push(format!("after_id={}", encode_query(after_id, "after_id")?));
        }
        if let Some(before_id) = &request.before_id {
            query.push(format!(
                "before_id={}",
                encode_query(before_id, "before_id")?
            ));
        }

        let path = if query.is_empty() {
            MODELS_PATH.to_owned()
        } else {
            format!("{MODELS_PATH}?{}", query.join("&"))
        };
        self.config.resolve_path(&path, "models")
    }
}

impl Endpoint for ListModelsEndpoint {
    type Request = ListModelsRequest;
    type Response = ModelList;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::get(self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<ModelList, AirxError> {
        decode(response)
    }
}

/// `GET /v1/models/{model_id}`。
#[derive(Debug, Clone)]
pub struct RetrieveModelEndpoint {
    config: AnthropicConfig,
}

impl RetrieveModelEndpoint {
    /// 创建端点。
    pub fn new(config: AnthropicConfig) -> Self {
        Self { config }
    }

    fn uri(&self, model: &ModelId) -> Result<Uri, AirxError> {
        let path = format!("{MODELS_PATH}/{}", model_path(model)?);
        self.config.resolve_path(&path, "model")
    }
}

impl Endpoint for RetrieveModelEndpoint {
    type Request = ModelId;
    type Response = Model;

    fn encode(&self, request: &ModelId) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::get(self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<Model, AirxError> {
        decode(response)
    }
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

fn model_path(model: &ModelId) -> Result<String, AirxError> {
    let text = model.as_str().trim();
    if text.is_empty() {
        return Err(AirxError::Configuration(ConfigurationError::new(
            "Anthropic model 不能为空",
        )));
    }
    if !text
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '/'))
    {
        return Err(AirxError::Configuration(ConfigurationError::new(format!(
            "Anthropic model 含非法字符: {text}"
        ))));
    }
    Ok(text.to_owned())
}

fn encode_query(value: &str, field: &str) -> Result<String, AirxError> {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '/'))
    {
        return Ok(value.to_owned());
    }
    Err(AirxError::Configuration(ConfigurationError::new(format!(
        "Anthropic {field} 含非法字符"
    ))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn model_list_decodes_cursor() {
        let list: ModelList = serde_json::from_value(json!({
            "data": [
                {"id": "claude-sonnet-4-5", "type": "model", "display_name": "Claude Sonnet 4.5", "created_at": "2025-09-29T00:00:00Z"}
            ],
            "has_more": true,
            "first_id": "claude-sonnet-4-5",
            "last_id": "claude-sonnet-4-5"
        }))
        .expect("list");

        assert_eq!(list.data[0].id, "claude-sonnet-4-5");
        assert_eq!(list.next_cursor(), Some("claude-sonnet-4-5"));
    }

    #[test]
    fn invalid_model_ids_are_rejected() {
        assert!(model_path(&ModelId::new("")).is_err());
        assert!(model_path(&ModelId::new("claude haiku")).is_err());
    }
}

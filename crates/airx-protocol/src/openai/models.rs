//! OpenAI Models API。
//!
//! `GET /models` / `GET /models/{model}`；不使用分页。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError};
use airx_core::http::{HttpRequest, HttpResponse, Uri};
use airx_core::json::Extensions;
use airx_core::model::{ModelId, ProviderId};
use serde::{Deserialize, Serialize};

use crate::endpoint::Endpoint;
use crate::openai::common::error::error_from_response;
use crate::openai::config::OpenAiConfig;

/// 模型列表路径。
pub const MODELS_PATH: &str = "/models";

/// 模型信息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model {
    /// 模型 ID。
    pub id: String,
    /// 对象类型，通常为 `model`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 创建时间（Unix 秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<u64>,
    /// 所有方。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned_by: Option<String>,
    /// 下线日期（官方以日期字符串返回，未公布时为 null）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shutdown_date: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 删除模型结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelDeleted {
    /// 模型 ID。
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

/// 模型列表响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelList {
    /// 对象类型，通常为 `list`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 模型数组。
    #[serde(default)]
    pub data: Vec<Model>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 空请求体（列表接口没有参数）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListModels;

/// `GET /models`。
#[derive(Debug, Clone)]
pub struct ListModelsEndpoint {
    config: OpenAiConfig,
}

impl ListModelsEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self) -> Result<Uri, AirxError> {
        self.config.endpoint_uri(MODELS_PATH, "models")
    }
}

impl Endpoint for ListModelsEndpoint {
    type Request = ListModels;
    type Response = ModelList;

    fn encode(&self, _request: &Self::Request) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::get(self.uri()?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<ModelList, AirxError> {
        decode(response)
    }
}

/// `GET /models/{model}`。
#[derive(Debug, Clone)]
pub struct RetrieveModelEndpoint {
    config: OpenAiConfig,
}

impl RetrieveModelEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, model: &ModelId) -> Result<Uri, AirxError> {
        let path = format!("{MODELS_PATH}/{}", model_path(model)?);
        self.config.endpoint_uri(&path, "model")
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

/// `DELETE /models/{model}`（仅 fine-tuned 模型可删，需要 Owner 权限）。
#[derive(Debug, Clone)]
pub struct DeleteModelEndpoint {
    config: OpenAiConfig,
}

impl DeleteModelEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, model: &ModelId) -> Result<Uri, AirxError> {
        let path = format!("{MODELS_PATH}/{}", model_path(model)?);
        self.config.endpoint_uri(&path, "model")
    }
}

impl Endpoint for DeleteModelEndpoint {
    type Request = ModelId;
    type Response = ModelDeleted;

    fn encode(&self, request: &ModelId) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::new(airx_core::http::Method::DELETE, self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<ModelDeleted, AirxError> {
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

fn model_path(model: &ModelId) -> Result<String, AirxError> {
    let text = model.as_str().trim();
    if text.is_empty() {
        return Err(AirxError::Configuration(ConfigurationError::new(
            "OpenAI model 不能为空",
        )));
    }
    if !text
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '/'))
    {
        return Err(AirxError::Configuration(ConfigurationError::new(format!(
            "OpenAI model 含非法字符: {text}"
        ))));
    }
    Ok(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn model_list_decodes_and_keeps_unknown_fields() {
        let list: ModelList = serde_json::from_value(json!({
            "object": "list",
            "data": [
                {"id": "gpt-4o-mini", "object": "model", "created": 1721172741, "owned_by": "system"},
                {"id": "text-embedding-3-small", "owned_by": "system", "futureField": true}
            ],
            "futureTopField": 1
        }))
        .expect("list");

        assert_eq!(list.data.len(), 2);
        assert_eq!(list.data[0].id, "gpt-4o-mini");
        assert!(list.data[1].extensions.contains_key("futureField"));
        assert!(list.extensions.contains_key("futureTopField"));
    }

    #[test]
    fn shutdown_date_and_delete_are_typed() {
        let model: Model = serde_json::from_value(json!({
            "id": "gpt-4o",
            "object": "model",
            "shutdown_date": "2026-12-01"
        }))
        .expect("model");
        assert_eq!(model.shutdown_date.as_deref(), Some("2026-12-01"));

        let deleted: ModelDeleted = serde_json::from_value(json!({
            "id": "ft:gpt-4o-mini:org::abc",
            "object": "model",
            "deleted": true
        }))
        .expect("deleted");
        assert!(deleted.deleted);
    }

    #[test]
    fn delete_endpoint_uses_delete_method() {
        let endpoint = DeleteModelEndpoint::new(OpenAiConfig::new("sk-test"));
        let http = endpoint
            .encode(&ModelId::new("ft:gpt-4o-mini"))
            .expect("encode");
        assert_eq!(http.method, airx_core::http::Method::DELETE);
        assert_eq!(http.uri.path(), "/v1/models/ft:gpt-4o-mini");
    }

    #[test]
    fn invalid_model_ids_are_rejected() {
        assert!(model_path(&ModelId::new("")).is_err());
        assert!(model_path(&ModelId::new("gpt 4o")).is_err());
        assert_eq!(
            model_path(&ModelId::new("gpt-4o-mini")).expect("path"),
            "gpt-4o-mini"
        );
    }
}

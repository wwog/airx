//! Gemini Models API。
//!
//! `GET /models`（pageToken 分页）/ `GET /models/{model}`。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError};
use airx_core::http::{HttpRequest, HttpResponse, Uri};
use airx_core::json::Extensions;
use airx_core::model::{ModelId, ProviderId};
use serde::{Deserialize, Serialize};

use crate::endpoint::Endpoint;
use crate::gemini::common::error_from_response;
use crate::gemini::config::GeminiConfig;

/// 模型列表路径。
pub const MODELS_PATH: &str = "models";

/// 模型信息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    /// 资源名（`models/...`）。
    pub name: String,
    /// 基础模型 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_model_id: Option<String>,
    /// 版本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 展示名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// 描述。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 输入 token 上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_token_limit: Option<u32>,
    /// 输出 token 上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_token_limit: Option<u32>,
    /// 支持的方法（如 `generateContent`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported_generation_methods: Option<Vec<String>>,
    /// 默认温度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// 默认 topP。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// 默认 topK。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 模型列表响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelList {
    /// 当前页模型。
    #[serde(default)]
    pub models: Vec<Model>,
    /// 下一页游标。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ModelList {
    /// 下一页游标。
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_page_token.as_deref()
    }
}

/// 列表参数。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListModelsRequest {
    /// 每页数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
    /// 翻页游标。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
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
    page_size: Option<u32>,
    page_token: Option<String>,
}

impl ListModelsRequestBuilder {
    /// 每页数量。
    #[must_use]
    pub fn page_size(mut self, page_size: u32) -> Self {
        self.page_size = Some(page_size);
        self
    }

    /// 翻页游标。
    #[must_use]
    pub fn page_token(mut self, page_token: impl Into<String>) -> Self {
        self.page_token = Some(page_token.into());
        self
    }

    /// 构建。
    pub fn build(self) -> ListModelsRequest {
        ListModelsRequest {
            page_size: self.page_size,
            page_token: self.page_token,
        }
    }
}

/// `GET /models`。
#[derive(Debug, Clone)]
pub struct ListModelsEndpoint {
    config: GeminiConfig,
}

impl ListModelsEndpoint {
    /// 创建端点。
    pub fn new(config: GeminiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, request: &ListModelsRequest) -> Result<Uri, AirxError> {
        let mut query = Vec::new();
        if let Some(page_size) = request.page_size {
            query.push(format!("pageSize={page_size}"));
        }
        if let Some(page_token) = &request.page_token {
            query.push(format!("pageToken={}", encode_query(page_token)?));
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

/// `GET /models/{model}`。
#[derive(Debug, Clone)]
pub struct GetModelEndpoint {
    config: GeminiConfig,
}

impl GetModelEndpoint {
    /// 创建端点。
    pub fn new(config: GeminiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, model: &ModelId) -> Result<Uri, AirxError> {
        let path = super::generate_content::model_path(model)?;
        self.config.resolve_path(&path, "model")
    }
}

impl Endpoint for GetModelEndpoint {
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
            &ProviderId::gemini(),
            response.status,
            &response.headers,
            &response.body,
        ));
    }

    serde_json::from_slice(&response.body)
        .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))
}

fn encode_query(value: &str) -> Result<String, AirxError> {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
    {
        return Ok(value.to_owned());
    }
    Err(AirxError::Configuration(ConfigurationError::new(
        "Gemini pageToken 含非法字符",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn model_list_decodes_and_keeps_unknown_fields() {
        let list: ModelList = serde_json::from_value(json!({
            "models": [
                {
                    "name": "models/gemini-2.5-flash",
                    "baseModelId": "gemini-2.5-flash",
                    "displayName": "Gemini 2.5 Flash",
                    "inputTokenLimit": 1048576,
                    "outputTokenLimit": 65536,
                    "supportedGenerationMethods": ["generateContent", "countTokens"],
                    "futureField": true
                }
            ],
            "nextPageToken": "page-2",
            "futureTopField": 1
        }))
        .expect("list");

        assert_eq!(list.models[0].name, "models/gemini-2.5-flash");
        assert_eq!(list.next_cursor(), Some("page-2"));
        assert!(list.models[0].extensions.contains_key("futureField"));
        assert!(list.extensions.contains_key("futureTopField"));
    }
}

//! Gemini Models API。
//!
//! `GET /models`（pageToken 分页）/ `GET /models/{model}` /
//! `POST /models/{model}:computeTokens`。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError, SerializationError};
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, Uri, header};
use airx_core::json::Extensions;
use airx_core::model::{ModelId, ProviderId};
use serde::{Deserialize, Serialize};

use crate::endpoint::Endpoint;
use crate::gemini::common::error_from_response;
use crate::gemini::config::GeminiConfig;

use super::generate_content::Content;
use super::generate_content::model_path;

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
    /// 最大温度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_temperature: Option<f32>,
    /// 默认 topP。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// 默认 topK。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// 是否为 thinking 模型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<bool>,
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
    /// 翻页游标（官方为不透明串，按百分号编码写入 query）。
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
            query.push(format!("pageToken={}", encode_query(page_token)));
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
        let path = model_path(model)?;
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

/// `computeTokens` 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputeTokensRequest {
    /// 对话内容。
    pub contents: Vec<Content>,
    /// 模型资源名（一般由路径决定，可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ComputeTokensRequest {
    /// 单轮文本请求。
    pub fn of_text(text: impl Into<String>) -> Self {
        Self {
            contents: vec![Content::user_text(text)],
            model: None,
            extensions: Extensions::new(),
        }
    }

    /// 指定内容的请求。
    pub fn of_contents(contents: impl IntoIterator<Item = Content>) -> Self {
        Self {
            contents: contents.into_iter().collect(),
            model: None,
            extensions: Extensions::new(),
        }
    }

    /// 校验：内容为空时报配置错误。
    pub fn validated(self) -> Result<Self, AirxError> {
        if self.contents.is_empty() {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "ComputeTokensRequest.contents 不能为空",
            )));
        }
        Ok(self)
    }
}

/// 单个分片的 token 信息（官方 `TokensInfo`）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokensInfo {
    /// 角色。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// token ID（字符串形式）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_ids: Option<Vec<String>>,
    /// token 文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Vec<String>>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `computeTokens` 响应。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputeTokensResponse {
    /// 按内容分片的 token 信息。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tokens_info: Vec<TokensInfo>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ComputeTokensResponse {
    /// token 总数（所有分片累加）。
    pub fn total_tokens(&self) -> usize {
        self.tokens_info
            .iter()
            .map(|info| {
                info.token_ids
                    .as_ref()
                    .map(Vec::len)
                    .or_else(|| info.tokens.as_ref().map(Vec::len))
                    .unwrap_or(0)
            })
            .sum()
    }
}

/// `POST /models/{model}:computeTokens`。
#[derive(Debug, Clone)]
pub struct ComputeTokensEndpoint {
    config: GeminiConfig,
    model: ModelId,
}

impl ComputeTokensEndpoint {
    /// 创建端点。
    pub fn new(config: GeminiConfig, model: impl Into<ModelId>) -> Self {
        Self {
            config,
            model: model.into(),
        }
    }

    fn uri(&self) -> Result<Uri, AirxError> {
        self.config.resolve_path(
            &format!("{}:computeTokens", model_path(&self.model)?),
            "computeTokens",
        )
    }
}

impl Endpoint for ComputeTokensEndpoint {
    type Request = ComputeTokensRequest;
    type Response = ComputeTokensResponse;

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

    fn decode(&self, response: HttpResponse) -> Result<ComputeTokensResponse, AirxError> {
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

/// 百分号编码 query 值（RFC 3986 unreserved 之外全部转义）。
///
/// 官方 `nextPageToken` 是不透明串，可能含 `+` / `/` / `=`，不能按白名单拒绝。
pub(crate) fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
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
                    "maxTemperature": 2.0,
                    "thinking": true,
                    "futureField": true
                }
            ],
            "nextPageToken": "page-2",
            "futureTopField": 1
        }))
        .expect("list");

        assert_eq!(list.models[0].name, "models/gemini-2.5-flash");
        assert_eq!(list.models[0].max_temperature, Some(2.0));
        assert_eq!(list.models[0].thinking, Some(true));
        assert_eq!(list.next_cursor(), Some("page-2"));
        assert!(list.models[0].extensions.contains_key("futureField"));
        assert!(list.extensions.contains_key("futureTopField"));
    }

    #[test]
    fn page_token_is_percent_encoded() {
        let endpoint = ListModelsEndpoint::new(GeminiConfig::new("k"));
        let request = ListModelsRequest::builder()
            .page_size(20)
            .page_token("a+b/c==")
            .build();
        let http = endpoint.encode(&request).expect("encode");

        assert_eq!(http.uri.path(), "/v1beta/models");
        assert_eq!(
            http.uri.query(),
            Some("pageSize=20&pageToken=a%2Bb%2Fc%3D%3D")
        );
    }

    #[test]
    fn page_token_with_unreserved_characters_is_untouched() {
        assert_eq!(encode_query("abc-XYZ_09.~"), "abc-XYZ_09.~");
    }

    #[test]
    fn compute_tokens_response_decodes() {
        let response: ComputeTokensResponse = serde_json::from_value(json!({
            "tokensInfo": [
                {"role": "user", "tokenIds": ["1", "2"], "tokens": ["he", "llo"]}
            ]
        }))
        .expect("response");

        assert_eq!(response.total_tokens(), 2);
        assert_eq!(
            response.tokens_info[0].tokens.as_deref(),
            Some(&["he".to_owned(), "llo".to_owned()][..])
        );
    }

    #[test]
    fn compute_tokens_endpoint_uses_model_path() {
        let endpoint = ComputeTokensEndpoint::new(GeminiConfig::new("k"), "gemini-2.5-flash");
        let http = endpoint
            .encode(&ComputeTokensRequest::of_text("hi"))
            .expect("encode");

        assert_eq!(
            http.uri.path(),
            "/v1beta/models/gemini-2.5-flash:computeTokens"
        );
        let body: serde_json::Value = serde_json::from_slice(&http.body).expect("body");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "hi");
        assert!(body.get("model").is_none());
    }
}

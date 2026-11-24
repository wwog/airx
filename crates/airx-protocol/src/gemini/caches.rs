//! Gemini Caches API（`cachedContents` CRUD）。
//!
//! 只做缓存的创建 / 读取 / 续期 / 删除 —— 不管理内容语义。生成请求通过
//! [`super::generate_content::GenerateContentRequest::cached_content`] 引用缓存，
//! 或用 [`CachedContent::name`] 直接回填。
//!
//! 端点：
//! - `POST   {base}/cachedContents`
//! - `GET    {base}/cachedContents?pageSize=&pageToken=`
//! - `GET    {base}/cachedContents/{id}`
//! - `PATCH  {base}/cachedContents/{id}?updateMask=...`
//! - `DELETE {base}/cachedContents/{id}`

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError, SerializationError};
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, Method, Uri, header};
use airx_core::json::Extensions;
use airx_core::model::ProviderId;
use serde::{Deserialize, Serialize};

use crate::endpoint::Endpoint;
use crate::gemini::common::error_from_response;
use crate::gemini::config::GeminiConfig;
use crate::gemini::generate_content::{Content, Tool, ToolConfig};
use crate::gemini::models::encode_query;

/// 缓存资源路径。
pub const CACHED_CONTENTS_PATH: &str = "cachedContents";

/// 缓存资源。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedContent {
    /// 资源名（`cachedContents/{id}`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 创建时使用的模型（`models/{model}`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 展示名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// 创建时间（RFC 3339）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create_time: Option<String>,
    /// 更新时间（RFC 3339）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_time: Option<String>,
    /// 过期时间（RFC 3339，始终由服务端返回）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expire_time: Option<String>,
    /// 缓存的内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contents: Option<Vec<Content>>,
    /// 系统指令。
    #[serde(
        rename = "systemInstruction",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub system_instruction: Option<Content>,
    /// 工具声明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// 工具选择。
    #[serde(
        rename = "toolConfig",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_config: Option<ToolConfig>,
    /// 缓存用量。
    #[serde(
        rename = "usageMetadata",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub usage_metadata: Option<CachedContentUsageMetadata>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl CachedContent {
    /// 直接把该缓存填进生成请求（`cachedContent`）。
    pub fn resource_name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

/// 缓存用量（官方 `CachedContentUsageMetadata`）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedContentUsageMetadata {
    /// 总 token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_token_count: Option<u32>,
    /// 文本条目数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_count: Option<u32>,
    /// 图片数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_count: Option<u32>,
    /// 音频时长（秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_duration_seconds: Option<f64>,
    /// 视频时长（秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_duration_seconds: Option<f64>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 缓存列表响应。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedContentList {
    /// 当前页缓存。
    #[serde(default)]
    pub cached_contents: Vec<CachedContent>,
    /// 下一页游标。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl CachedContentList {
    /// 下一页游标。
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_page_token.as_deref()
    }
}

/// 删除缓存的响应（官方返回空对象）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CachedContentDelete {
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 创建缓存请求。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCachedContentRequest {
    /// 模型资源名（`models/{model}`，必填）。
    pub model: String,
    /// 缓存的内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contents: Option<Vec<Content>>,
    /// 系统指令（目前仅纯文本）。
    #[serde(
        rename = "systemInstruction",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub system_instruction: Option<Content>,
    /// 工具声明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// 工具选择。
    #[serde(
        rename = "toolConfig",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_config: Option<ToolConfig>,
    /// 展示名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// 生存时间（如 `600s`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
    /// 过期时间（RFC 3339）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expire_time: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl CreateCachedContentRequest {
    /// 创建构建器。
    pub fn builder() -> CreateCachedContentRequestBuilder {
        CreateCachedContentRequestBuilder::default()
    }
}

/// [`CreateCachedContentRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct CreateCachedContentRequestBuilder {
    model: Option<String>,
    contents: Vec<Content>,
    system_instruction: Option<Content>,
    tools: Option<Vec<Tool>>,
    tool_config: Option<ToolConfig>,
    display_name: Option<String>,
    ttl: Option<String>,
    expire_time: Option<String>,
    extensions: Extensions,
}

impl CreateCachedContentRequestBuilder {
    /// 设置模型（必填）。
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// 追加一轮要缓存的内容。
    #[must_use]
    pub fn content(mut self, content: Content) -> Self {
        self.contents.push(content);
        self
    }

    /// 追加用户文本。
    #[must_use]
    pub fn user(self, text: impl Into<String>) -> Self {
        self.content(Content::user_text(text))
    }

    /// 设置系统指令。
    #[must_use]
    pub fn system(mut self, system: impl Into<String>) -> Self {
        self.system_instruction = Some(Content {
            role: None,
            parts: vec![crate::gemini::generate_content::Part::text(system)],
            extensions: Extensions::new(),
        });
        self
    }

    /// 设置工具。
    #[must_use]
    pub fn tools(mut self, tools: impl IntoIterator<Item = Tool>) -> Self {
        self.tools = Some(tools.into_iter().collect());
        self
    }

    /// 设置工具选择。
    #[must_use]
    pub fn tool_config(mut self, tool_config: ToolConfig) -> Self {
        self.tool_config = Some(tool_config);
        self
    }

    /// 设置展示名。
    #[must_use]
    pub fn display_name(mut self, display_name: impl Into<String>) -> Self {
        self.display_name = Some(display_name.into());
        self
    }

    /// 设置生存时间（如 `600s`）。
    #[must_use]
    pub fn ttl(mut self, ttl: impl Into<String>) -> Self {
        self.ttl = Some(ttl.into());
        self
    }

    /// 设置过期时间（RFC 3339）。
    #[must_use]
    pub fn expire_time(mut self, expire_time: impl Into<String>) -> Self {
        self.expire_time = Some(expire_time.into());
        self
    }

    /// 添加扩展字段。
    #[must_use]
    pub fn extension(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
    }

    /// 构建请求。
    pub fn build(self) -> Result<CreateCachedContentRequest, AirxError> {
        let model = self
            .model
            .filter(|model| !model.trim().is_empty())
            .ok_or_else(|| {
                AirxError::Configuration(ConfigurationError::new(
                    "CreateCachedContentRequest.model 必填",
                ))
            })?;

        Ok(CreateCachedContentRequest {
            model,
            contents: (!self.contents.is_empty()).then_some(self.contents),
            system_instruction: self.system_instruction,
            tools: self.tools,
            tool_config: self.tool_config,
            display_name: self.display_name,
            ttl: self.ttl,
            expire_time: self.expire_time,
            extensions: self.extensions,
        })
    }
}

/// 更新缓存请求（官方只允许续期，`displayName` 不可变）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCachedContentRequest {
    /// 新的生存时间（如 `3600s`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
    /// 新的过期时间（RFC 3339）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expire_time: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl UpdateCachedContentRequest {
    /// 以 TTL 续期。
    pub fn ttl(ttl: impl Into<String>) -> Self {
        Self {
            ttl: Some(ttl.into()),
            expire_time: None,
            extensions: Extensions::new(),
        }
    }

    /// 以绝对过期时间续期。
    pub fn expire_time(expire_time: impl Into<String>) -> Self {
        Self {
            ttl: None,
            expire_time: Some(expire_time.into()),
            extensions: Extensions::new(),
        }
    }

    /// 计算 `updateMask`（未显式指定时按已设置字段推导）。
    pub fn update_mask(&self) -> String {
        match (&self.ttl, &self.expire_time) {
            (Some(_), Some(_)) => "ttl,expireTime".to_owned(),
            (Some(_), None) => "ttl".to_owned(),
            (None, Some(_)) => "expireTime".to_owned(),
            (None, None) => String::new(),
        }
    }
}

/// 列表参数。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListCachedContentsRequest {
    /// 每页数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
    /// 翻页游标。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

impl ListCachedContentsRequest {
    /// 创建构建器。
    pub fn builder() -> ListCachedContentsRequestBuilder {
        ListCachedContentsRequestBuilder::default()
    }
}

/// [`ListCachedContentsRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct ListCachedContentsRequestBuilder {
    page_size: Option<u32>,
    page_token: Option<String>,
}

impl ListCachedContentsRequestBuilder {
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
    pub fn build(self) -> ListCachedContentsRequest {
        ListCachedContentsRequest {
            page_size: self.page_size,
            page_token: self.page_token,
        }
    }
}

/// 把缓存名规范化为资源路径（`abc` → `cachedContents/abc`）。
pub(crate) fn cached_content_path(name: &str) -> Result<String, AirxError> {
    let text = name.trim();
    if text.is_empty() {
        return Err(AirxError::Configuration(ConfigurationError::new(
            "Gemini cachedContent 名称不能为空",
        )));
    }

    let valid = text
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/'));
    let valid_segments = text
        .split('/')
        .all(|segment| !segment.is_empty() && segment != "." && segment != "..");
    if !valid || !valid_segments {
        return Err(AirxError::Configuration(ConfigurationError::new(format!(
            "Gemini cachedContent 名称含非法字符: {text}"
        ))));
    }

    if text.starts_with("cachedContents/") {
        Ok(text.to_owned())
    } else {
        Ok(format!("{CACHED_CONTENTS_PATH}/{text}"))
    }
}

/// `POST /cachedContents`。
#[derive(Debug, Clone)]
pub struct CreateCachedContentEndpoint {
    config: GeminiConfig,
}

impl CreateCachedContentEndpoint {
    /// 创建端点。
    pub fn new(config: GeminiConfig) -> Self {
        Self { config }
    }

    fn uri(&self) -> Result<Uri, AirxError> {
        self.config
            .resolve_path(CACHED_CONTENTS_PATH, "cachedContents")
    }
}

impl Endpoint for CreateCachedContentEndpoint {
    type Request = CreateCachedContentRequest;
    type Response = CachedContent;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        post_json(self.uri()?, request, &self.config)
    }

    fn decode(&self, response: HttpResponse) -> Result<CachedContent, AirxError> {
        decode(response)
    }
}

/// `GET /cachedContents`。
#[derive(Debug, Clone)]
pub struct ListCachedContentsEndpoint {
    config: GeminiConfig,
}

impl ListCachedContentsEndpoint {
    /// 创建端点。
    pub fn new(config: GeminiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, request: &ListCachedContentsRequest) -> Result<Uri, AirxError> {
        let mut query = Vec::new();
        if let Some(page_size) = request.page_size {
            query.push(format!("pageSize={page_size}"));
        }
        if let Some(page_token) = &request.page_token {
            query.push(format!("pageToken={}", encode_query(page_token)));
        }

        let path = if query.is_empty() {
            CACHED_CONTENTS_PATH.to_owned()
        } else {
            format!("{CACHED_CONTENTS_PATH}?{}", query.join("&"))
        };
        self.config.resolve_path(&path, "cachedContents")
    }
}

impl Endpoint for ListCachedContentsEndpoint {
    type Request = ListCachedContentsRequest;
    type Response = CachedContentList;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::get(self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<CachedContentList, AirxError> {
        decode(response)
    }
}

/// `GET /cachedContents/{id}`。
#[derive(Debug, Clone)]
pub struct GetCachedContentEndpoint {
    config: GeminiConfig,
}

impl GetCachedContentEndpoint {
    /// 创建端点。
    pub fn new(config: GeminiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, name: &str) -> Result<Uri, AirxError> {
        let path = cached_content_path(name)?;
        self.config.resolve_path(&path, "cachedContent")
    }
}

impl Endpoint for GetCachedContentEndpoint {
    type Request = String;
    type Response = CachedContent;

    fn encode(&self, request: &String) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::get(self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<CachedContent, AirxError> {
        decode(response)
    }
}

/// `PATCH /cachedContents/{id}`。
#[derive(Debug, Clone)]
pub struct UpdateCachedContentEndpoint {
    config: GeminiConfig,
    name: String,
}

impl UpdateCachedContentEndpoint {
    /// 创建端点（缓存名可带或不带 `cachedContents/` 前缀）。
    pub fn new(config: GeminiConfig, name: impl Into<String>) -> Self {
        Self {
            config,
            name: name.into(),
        }
    }

    fn uri(&self, request: &UpdateCachedContentRequest) -> Result<Uri, AirxError> {
        let path = cached_content_path(&self.name)?;
        let mask = request.update_mask();
        let path = if mask.is_empty() {
            path
        } else {
            format!("{path}?updateMask={}", encode_query(&mask))
        };
        self.config.resolve_path(&path, "cachedContent")
    }
}

impl Endpoint for UpdateCachedContentEndpoint {
    type Request = UpdateCachedContentRequest;
    type Response = CachedContent;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        let body = serde_json::to_vec(request)
            .map_err(|error| AirxError::Serialization(SerializationError::from(error)))?;
        let mut http = HttpRequest::new(Method::PATCH, self.uri(request)?).with_body(body);
        http.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<CachedContent, AirxError> {
        decode(response)
    }
}

/// `DELETE /cachedContents/{id}`。
#[derive(Debug, Clone)]
pub struct DeleteCachedContentEndpoint {
    config: GeminiConfig,
}

impl DeleteCachedContentEndpoint {
    /// 创建端点。
    pub fn new(config: GeminiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, name: &str) -> Result<Uri, AirxError> {
        let path = cached_content_path(name)?;
        self.config.resolve_path(&path, "cachedContent")
    }
}

impl Endpoint for DeleteCachedContentEndpoint {
    type Request = String;
    type Response = CachedContentDelete;

    fn encode(&self, request: &String) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::new(Method::DELETE, self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<CachedContentDelete, AirxError> {
        if !response.status.is_success() {
            return Err(error_from_response(
                &ProviderId::gemini(),
                response.status,
                &response.headers,
                &response.body,
            ));
        }

        let text = std::str::from_utf8(&response.body).unwrap_or("");
        if text.trim().is_empty() {
            return Ok(CachedContentDelete::default());
        }
        serde_json::from_str(text)
            .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))
    }
}

fn post_json<T: Serialize>(
    uri: Uri,
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

    fn config() -> GeminiConfig {
        GeminiConfig::new("test-key")
    }

    #[test]
    fn create_request_requires_model_and_serializes() {
        assert!(CreateCachedContentRequest::builder().build().is_err());

        let request = CreateCachedContentRequest::builder()
            .model("models/gemini-2.5-flash")
            .system("be brief")
            .user("long document")
            .display_name("doc-cache")
            .ttl("600s")
            .build()
            .expect("request");
        let value = serde_json::to_value(&request).expect("serialize");

        assert_eq!(value["model"], "models/gemini-2.5-flash");
        assert_eq!(value["contents"][0]["parts"][0]["text"], "long document");
        assert_eq!(value["systemInstruction"]["parts"][0]["text"], "be brief");
        assert_eq!(value["ttl"], "600s");
        assert_eq!(value["displayName"], "doc-cache");
    }

    #[test]
    fn create_endpoint_uses_cached_contents_path() {
        let endpoint = CreateCachedContentEndpoint::new(config());
        let request = CreateCachedContentRequest::builder()
            .model("models/gemini-2.5-flash")
            .user("hi")
            .build()
            .expect("request");
        let http = endpoint.encode(&request).expect("encode");

        assert_eq!(http.method, Method::POST);
        assert_eq!(http.uri.path(), "/v1beta/cachedContents");
        assert_eq!(http.headers["x-goog-api-key"], "test-key");
    }

    #[test]
    fn get_update_delete_paths_are_normalized() {
        let get = GetCachedContentEndpoint::new(config());
        let http = get.encode(&"abc".to_owned()).expect("encode");
        assert_eq!(http.uri.path(), "/v1beta/cachedContents/abc");

        let update = UpdateCachedContentEndpoint::new(config(), "cachedContents/abc");
        let request = UpdateCachedContentRequest::ttl("3600s");
        let http = update.encode(&request).expect("encode");
        assert_eq!(http.method, Method::PATCH);
        assert_eq!(http.uri.path(), "/v1beta/cachedContents/abc");
        assert_eq!(http.uri.query(), Some("updateMask=ttl"));

        let delete = DeleteCachedContentEndpoint::new(config());
        let http = delete.encode(&"abc".to_owned()).expect("encode");
        assert_eq!(http.method, Method::DELETE);
        assert_eq!(http.uri.path(), "/v1beta/cachedContents/abc");
    }

    #[test]
    fn update_mask_follows_fields() {
        assert_eq!(
            UpdateCachedContentRequest::expire_time("2030-01-01T00:00:00Z").update_mask(),
            "expireTime"
        );
        assert_eq!(UpdateCachedContentRequest::default().update_mask(), "");
    }

    #[test]
    fn list_encodes_page_token() {
        let endpoint = ListCachedContentsEndpoint::new(config());
        let request = ListCachedContentsRequest::builder()
            .page_size(10)
            .page_token("a+b==")
            .build();
        let http = endpoint.encode(&request).expect("encode");
        assert_eq!(http.uri.query(), Some("pageSize=10&pageToken=a%2Bb%3D%3D"));
    }

    #[test]
    fn cached_content_decodes() {
        let cached: CachedContent = serde_json::from_value(json!({
            "name": "cachedContents/abc",
            "model": "models/gemini-2.5-flash",
            "expireTime": "2026-01-01T00:00:00Z",
            "usageMetadata": {"totalTokenCount": 100, "textCount": 2},
            "futureField": true
        }))
        .expect("cached");

        assert_eq!(cached.resource_name(), Some("cachedContents/abc"));
        assert_eq!(
            cached
                .usage_metadata
                .as_ref()
                .and_then(|usage| usage.total_token_count),
            Some(100)
        );
        assert!(cached.extensions.contains_key("futureField"));
    }

    #[test]
    fn delete_tolerates_empty_body() {
        let endpoint = DeleteCachedContentEndpoint::new(config());
        let response = endpoint
            .decode(HttpResponse::new(
                airx_core::http::StatusCode::NO_CONTENT,
                airx_core::http::HeaderMap::new(),
                "",
            ))
            .expect("decode");
        assert!(response.extensions.is_empty());
    }

    #[test]
    fn invalid_names_are_rejected() {
        for name in ["", " ", "../x", "cachedContents//x", "中文"] {
            assert!(cached_content_path(name).is_err(), "{name:?}");
        }
    }
}

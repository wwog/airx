//! Anthropic Files API（multipart 上传、列表、下载、删除）。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError};
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, header};
use airx_core::json::Extensions;
use airx_core::model::ProviderId;
use airx_core::multipart::MultipartForm;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::anthropic::common::error_from_response;
use crate::anthropic::config::AnthropicConfig;
use crate::endpoint::Endpoint;

/// 文件路径。
pub const FILES_PATH: &str = "/v1/files";

/// 文件元数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileMetadata {
    /// 文件 ID。
    pub id: String,
    /// 对象类型（`file`）。
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// 文件名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// MIME 类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// 字节数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// 创建时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// 是否可下载。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloadable: Option<bool>,
    /// 过期时间（RFC 3339；不随 `expires_in_seconds` 上传时为 `None`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 文件列表。
///
/// 官方形状是 `{data, next_page}`：把 `next_page` 原样回填到
/// [`ListFilesRequest::page`] 即可取下一页。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileList {
    /// 文件数组。
    #[serde(default)]
    pub data: Vec<FileMetadata>,
    /// 下一页游标（`null` 表示没有更多）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl FileList {
    /// 下一页游标。
    pub fn next_page(&self) -> Option<&str> {
        self.next_page.as_deref()
    }

    /// 下一页游标（旧名，等价于 [`FileList::next_page`]）。
    #[deprecated(note = "官方 Files 分页字段是 next_page，请改用 next_page()")]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_page()
    }
}

/// 删除结果。
///
/// 官方 `DeletedFile` 只有 `{id, type}`；airx 早期自造的 `deleted: bool` 从未被
/// 官方返回，已移除。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileDeleteResponse {
    /// 文件 ID。
    pub id: String,
    /// 对象类型（`file_deleted`）。
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 官方命名（与 [`FileDeleteResponse`] 相同）。
pub type DeletedFile = FileDeleteResponse;

/// 上传请求。
#[derive(Debug, Clone, PartialEq)]
pub struct UploadFileRequest {
    /// 文件名。
    pub filename: String,
    /// MIME 类型。
    pub mime_type: String,
    /// 文件内容。
    pub data: Bytes,
    /// multipart boundary。
    pub boundary: String,
    /// 上传后多少秒过期（3600–7776000），`None` 表示不过期。
    pub expires_in_seconds: Option<u32>,
}

impl UploadFileRequest {
    /// 构造上传请求。
    pub fn new(
        filename: impl Into<String>,
        mime_type: impl Into<String>,
        data: impl Into<Bytes>,
        boundary: impl Into<String>,
    ) -> Self {
        Self {
            filename: filename.into(),
            mime_type: mime_type.into(),
            data: data.into(),
            boundary: boundary.into(),
            expires_in_seconds: None,
        }
    }

    /// 设置 `expires_in_seconds`（官方区间 3600–7776000 秒）。
    #[must_use]
    pub fn with_expires_in_seconds(mut self, seconds: u32) -> Self {
        self.expires_in_seconds = Some(seconds);
        self
    }
}

/// 列表参数。
///
/// 官方 `FileListParams`：`limit` / `page` / `ids`（`ids` 与 `page` / `limit`
/// 互斥）。旧的 `after_id` / `before_id` 在官方 Files 列表上已返回 400。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListFilesRequest {
    /// 每页数量（1–1000，默认 20）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// 上一页响应里的 `next_page` 游标。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// 只查询这些文件 ID（最多 100 个；与 `page` / `limit` 互斥）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<String>>,
}

impl ListFilesRequest {
    /// 创建构建器。
    pub fn builder() -> ListFilesRequestBuilder {
        ListFilesRequestBuilder::default()
    }

    /// 按 ID 集合查询。
    pub fn by_ids(ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            limit: None,
            page: None,
            ids: Some(ids.into_iter().map(Into::into).collect()),
        }
    }
}

/// [`ListFilesRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct ListFilesRequestBuilder {
    limit: Option<u32>,
    page: Option<String>,
    ids: Option<Vec<String>>,
}

impl ListFilesRequestBuilder {
    /// 每页数量。
    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// 下一页游标（来自上一页的 `next_page`）。
    #[must_use]
    pub fn page(mut self, page: impl Into<String>) -> Self {
        self.page = Some(page.into());
        self
    }

    /// 只查询这些文件 ID。
    #[must_use]
    pub fn ids(mut self, ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.ids = Some(ids.into_iter().map(Into::into).collect());
        self
    }

    /// 构建。
    pub fn build(self) -> ListFilesRequest {
        ListFilesRequest {
            limit: self.limit,
            page: self.page,
            ids: self.ids,
        }
    }
}

/// `POST /v1/files`。
#[derive(Debug, Clone)]
pub struct UploadFileEndpoint {
    config: AnthropicConfig,
}

impl UploadFileEndpoint {
    /// 创建端点。
    pub fn new(config: AnthropicConfig) -> Self {
        Self { config }
    }

    fn uri(&self) -> Result<airx_core::http::Uri, AirxError> {
        self.config.resolve_path(FILES_PATH, "files")
    }
}

impl Endpoint for UploadFileEndpoint {
    type Request = UploadFileRequest;
    type Response = FileMetadata;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        let mut form = MultipartForm::new(request.boundary.clone())?;
        if let Some(seconds) = request.expires_in_seconds {
            form = form.text("expires_in_seconds", seconds.to_string());
        }
        let form = form.file(
            "file",
            request.filename.clone(),
            request.mime_type.clone(),
            request.data.clone(),
        );
        let body = form.body()?;
        let mut http = HttpRequest::post(self.uri()?).with_body(body);
        http.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&form.content_type()).map_err(|error| {
                AirxError::Configuration(ConfigurationError::new(format!(
                    "multipart Content-Type 非法: {error}"
                )))
            })?,
        );
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<FileMetadata, AirxError> {
        decode(response)
    }
}

/// `GET /v1/files`。
#[derive(Debug, Clone)]
pub struct ListFilesEndpoint {
    config: AnthropicConfig,
}

impl ListFilesEndpoint {
    /// 创建端点。
    pub fn new(config: AnthropicConfig) -> Self {
        Self { config }
    }

    fn uri(&self, request: &ListFilesRequest) -> Result<airx_core::http::Uri, AirxError> {
        let mut query = Vec::new();
        if let Some(limit) = request.limit {
            query.push(format!("limit={limit}"));
        }
        if let Some(page) = &request.page {
            query.push(format!("page={}", query_value(page, "page")?));
        }
        if let Some(ids) = &request.ids {
            for id in ids {
                query.push(format!("ids[]={}", query_value(id, "ids")?));
            }
        }
        let path = if query.is_empty() {
            FILES_PATH.to_owned()
        } else {
            format!("{FILES_PATH}?{}", query.join("&"))
        };
        self.config.resolve_path(&path, "files")
    }
}

impl Endpoint for ListFilesEndpoint {
    type Request = ListFilesRequest;
    type Response = FileList;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::get(self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<FileList, AirxError> {
        decode(response)
    }
}

/// `GET /v1/files/{id}`。
#[derive(Debug, Clone)]
pub struct RetrieveFileEndpoint {
    config: AnthropicConfig,
}

impl RetrieveFileEndpoint {
    /// 创建端点。
    pub fn new(config: AnthropicConfig) -> Self {
        Self { config }
    }

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        self.config
            .resolve_path(&format!("{FILES_PATH}/{}", file_id(id)?), "files")
    }
}

impl Endpoint for RetrieveFileEndpoint {
    type Request = String;
    type Response = FileMetadata;

    fn encode(&self, request: &String) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::get(self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<FileMetadata, AirxError> {
        decode(response)
    }
}

/// `DELETE /v1/files/{id}`。
#[derive(Debug, Clone)]
pub struct DeleteFileEndpoint {
    config: AnthropicConfig,
}

impl DeleteFileEndpoint {
    /// 创建端点。
    pub fn new(config: AnthropicConfig) -> Self {
        Self { config }
    }

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        self.config
            .resolve_path(&format!("{FILES_PATH}/{}", file_id(id)?), "files")
    }
}

impl Endpoint for DeleteFileEndpoint {
    type Request = String;
    type Response = FileDeleteResponse;

    fn encode(&self, request: &String) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::new(airx_core::http::Method::DELETE, self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<FileDeleteResponse, AirxError> {
        decode(response)
    }
}

/// `GET /v1/files/{id}/content`。
#[derive(Debug, Clone)]
pub struct FileContentEndpoint {
    config: AnthropicConfig,
}

impl FileContentEndpoint {
    /// 创建端点。
    pub fn new(config: AnthropicConfig) -> Self {
        Self { config }
    }

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        self.config
            .resolve_path(&format!("{FILES_PATH}/{}/content", file_id(id)?), "files")
    }
}

impl Endpoint for FileContentEndpoint {
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

fn file_id(id: &str) -> Result<String, AirxError> {
    let text = id.trim();
    if text.is_empty()
        || !text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(AirxError::Configuration(ConfigurationError::new(
            "Anthropic file id 非法",
        )));
    }
    Ok(text.to_owned())
}

fn query_value(value: &str, field: &str) -> Result<String, AirxError> {
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(AirxError::Configuration(ConfigurationError::new(format!(
            "Anthropic files {field} 含非法字符"
        ))));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn endpoint() -> ListFilesEndpoint {
        ListFilesEndpoint::new(AnthropicConfig::new("sk-ant-test"))
    }

    #[test]
    fn file_metadata_decodes() {
        let file: FileMetadata = serde_json::from_value(json!({
            "id": "file_1",
            "type": "file",
            "filename": "a.pdf",
            "mime_type": "application/pdf",
            "expires_at": "2026-12-01T00:00:00Z"
        }))
        .expect("file");
        assert_eq!(file.id, "file_1");
        assert_eq!(file.mime_type.as_deref(), Some("application/pdf"));
        assert_eq!(file.expires_at.as_deref(), Some("2026-12-01T00:00:00Z"));
    }

    #[test]
    fn list_uses_page_cursor_and_ids() {
        let http = endpoint()
            .encode(&ListFilesRequest::builder().limit(10).page("page_1").build())
            .expect("encode");
        assert_eq!(http.uri.path(), "/v1/files");
        assert_eq!(http.uri.query(), Some("limit=10&page=page_1"));

        let http = endpoint()
            .encode(&ListFilesRequest::by_ids(["file_1", "file_2"]))
            .expect("encode");
        assert_eq!(http.uri.query(), Some("ids[]=file_1&ids[]=file_2"));
    }

    #[test]
    fn file_list_reads_next_page() {
        let list: FileList = serde_json::from_value(json!({
            "data": [{"id": "file_1"}],
            "next_page": "page_2"
        }))
        .expect("list");
        assert_eq!(list.next_page(), Some("page_2"));
        assert_eq!(list.data[0].id, "file_1");

        let list: FileList =
            serde_json::from_value(json!({"data": [], "next_page": null})).expect("list");
        assert_eq!(list.next_page(), None);
    }

    #[test]
    fn delete_response_has_no_deleted_flag() {
        let response: FileDeleteResponse =
            serde_json::from_value(json!({"id": "file_1", "type": "file_deleted"}))
                .expect("delete");
        assert_eq!(response.id, "file_1");
        assert_eq!(response.kind.as_deref(), Some("file_deleted"));

        let value = serde_json::to_value(&response).expect("serialize");
        assert_eq!(value, json!({"id": "file_1", "type": "file_deleted"}));
        assert!(value.get("deleted").is_none());

        let _ = DeletedFile {
            id: "file_1".to_owned(),
            kind: Some("file_deleted".to_owned()),
            extensions: Extensions::new(),
        };
    }

    #[test]
    fn upload_carries_expires_in_seconds() {
        let request = UploadFileRequest::new("a.pdf", "application/pdf", vec![1u8], "bnd")
            .with_expires_in_seconds(3600);
        let http = UploadFileEndpoint::new(AnthropicConfig::new("sk-ant-test"))
            .encode(&request)
            .expect("encode");
        let body = String::from_utf8_lossy(&http.body);
        assert!(body.contains("name=\"expires_in_seconds\""));
        assert!(body.contains("3600"));
        assert!(body.contains("filename=\"a.pdf\""));
    }
}

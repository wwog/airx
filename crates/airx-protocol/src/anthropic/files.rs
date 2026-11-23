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
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 文件列表。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileList {
    /// 文件数组。
    #[serde(default)]
    pub data: Vec<FileMetadata>,
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

impl FileList {
    /// 下一页游标。
    pub fn next_cursor(&self) -> Option<&str> {
        if self.has_more == Some(true) {
            self.last_id.as_deref()
        } else {
            None
        }
    }
}

/// 删除结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileDeleteResponse {
    /// 文件 ID。
    pub id: String,
    /// 对象类型。
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// 是否删除。
    #[serde(default)]
    pub deleted: bool,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

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
        }
    }
}

/// 列表参数。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListFilesRequest {
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

impl ListFilesRequest {
    /// 创建构建器。
    pub fn builder() -> ListFilesRequestBuilder {
        ListFilesRequestBuilder::default()
    }
}

/// [`ListFilesRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct ListFilesRequestBuilder {
    limit: Option<u32>,
    after_id: Option<String>,
    before_id: Option<String>,
}

impl ListFilesRequestBuilder {
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
    pub fn build(self) -> ListFilesRequest {
        ListFilesRequest {
            limit: self.limit,
            after_id: self.after_id,
            before_id: self.before_id,
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
        let form = MultipartForm::new(request.boundary.clone())?.file(
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
        if let Some(after_id) = &request.after_id {
            query.push(format!("after_id={}", query_value(after_id)?));
        }
        if let Some(before_id) = &request.before_id {
            query.push(format!("before_id={}", query_value(before_id)?));
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

fn query_value(value: &str) -> Result<String, AirxError> {
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(AirxError::Configuration(ConfigurationError::new(
            "Anthropic files 游标含非法字符",
        )));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn file_metadata_decodes() {
        let file: FileMetadata = serde_json::from_value(json!({
            "id": "file_1",
            "type": "file",
            "filename": "a.pdf",
            "mime_type": "application/pdf"
        }))
        .expect("file");
        assert_eq!(file.id, "file_1");
        assert_eq!(file.mime_type.as_deref(), Some("application/pdf"));
    }
}

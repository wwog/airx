//! OpenAI Files API（multipart 上传、列表、下载、删除）。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError};
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, header};
use airx_core::json::Extensions;
use airx_core::model::ProviderId;
use airx_core::multipart::MultipartForm;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::endpoint::Endpoint;
use crate::openai::common::error::error_from_response;
use crate::openai::config::OpenAiConfig;

/// 端点路径。
pub const FILES_PATH: &str = "/files";

/// 文件对象。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileObject {
    /// 文件 ID。
    pub id: String,
    /// 对象类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 字节数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// 创建时间（Unix 秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    /// 文件名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// 用途。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// 状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 文件列表。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileList {
    /// 对象类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 文件数组。
    #[serde(default)]
    pub data: Vec<FileObject>,
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

/// 删除文件结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileDeleteResponse {
    /// 文件 ID。
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

/// 上传请求。
#[derive(Debug, Clone, PartialEq)]
pub struct UploadFileRequest {
    /// 文件名。
    pub filename: String,
    /// 用途（如 `batch` / `assistants` / `vision`）。
    pub purpose: String,
    /// 文件内容。
    pub data: Bytes,
    /// multipart boundary。
    pub boundary: String,
}

impl UploadFileRequest {
    /// 构造上传请求。
    pub fn new(
        filename: impl Into<String>,
        purpose: impl Into<String>,
        data: impl Into<Bytes>,
        boundary: impl Into<String>,
    ) -> Self {
        Self {
            filename: filename.into(),
            purpose: purpose.into(),
            data: data.into(),
            boundary: boundary.into(),
        }
    }
}

/// 列表参数。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListFilesRequest {
    /// 按用途过滤。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// 每页数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// 从该 ID 之后开始。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
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
    purpose: Option<String>,
    limit: Option<u32>,
    after: Option<String>,
}

impl ListFilesRequestBuilder {
    /// 按用途过滤。
    #[must_use]
    pub fn purpose(mut self, purpose: impl Into<String>) -> Self {
        self.purpose = Some(purpose.into());
        self
    }

    /// 每页数量。
    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// 从该 ID 之后开始。
    #[must_use]
    pub fn after(mut self, after: impl Into<String>) -> Self {
        self.after = Some(after.into());
        self
    }

    /// 构建。
    pub fn build(self) -> ListFilesRequest {
        ListFilesRequest {
            purpose: self.purpose,
            limit: self.limit,
            after: self.after,
        }
    }
}

/// `POST /files`。
#[derive(Debug, Clone)]
pub struct UploadFileEndpoint {
    config: OpenAiConfig,
}

impl UploadFileEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self) -> Result<airx_core::http::Uri, AirxError> {
        self.config.endpoint_uri(FILES_PATH, "files")
    }
}

impl Endpoint for UploadFileEndpoint {
    type Request = UploadFileRequest;
    type Response = FileObject;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        let form = MultipartForm::new(request.boundary.clone())?
            .text("purpose", request.purpose.clone())
            .file(
                "file",
                request.filename.clone(),
                "application/octet-stream",
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

    fn decode(&self, response: HttpResponse) -> Result<FileObject, AirxError> {
        decode(response)
    }
}

/// `GET /files`。
#[derive(Debug, Clone)]
pub struct ListFilesEndpoint {
    config: OpenAiConfig,
}

impl ListFilesEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, request: &ListFilesRequest) -> Result<airx_core::http::Uri, AirxError> {
        let mut query = Vec::new();
        if let Some(purpose) = &request.purpose {
            query.push(format!("purpose={}", query_value(purpose, "purpose")?));
        }
        if let Some(limit) = request.limit {
            query.push(format!("limit={limit}"));
        }
        if let Some(after) = &request.after {
            query.push(format!("after={}", query_value(after, "after")?));
        }
        let path = if query.is_empty() {
            FILES_PATH.to_owned()
        } else {
            format!("{FILES_PATH}?{}", query.join("&"))
        };
        self.config.endpoint_uri(&path, "files")
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

/// `GET /files/{id}`。
#[derive(Debug, Clone)]
pub struct FileEndpoint {
    config: OpenAiConfig,
}

impl FileEndpoint {
    /// 获取文件元数据。
    pub fn retrieve(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        self.config
            .endpoint_uri(&format!("{FILES_PATH}/{}", file_id(id)?), "files")
    }
}

impl Endpoint for FileEndpoint {
    type Request = String;
    type Response = FileObject;

    fn encode(&self, request: &String) -> Result<HttpRequest, AirxError> {
        let mut http = HttpRequest::get(self.uri(request)?);
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<FileObject, AirxError> {
        decode(response)
    }
}

/// `DELETE /files/{id}` 专用端点（返回删除结果）。
#[derive(Debug, Clone)]
pub struct DeleteFileEndpoint {
    config: OpenAiConfig,
}

impl DeleteFileEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        self.config
            .endpoint_uri(&format!("{FILES_PATH}/{}", file_id(id)?), "files")
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

/// `GET /files/{id}/content`。
#[derive(Debug, Clone)]
pub struct FileContentEndpoint {
    config: OpenAiConfig,
}

impl FileContentEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self, id: &str) -> Result<airx_core::http::Uri, AirxError> {
        self.config
            .endpoint_uri(&format!("{FILES_PATH}/{}/content", file_id(id)?), "files")
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
                &ProviderId::openai(),
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
            &ProviderId::openai(),
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
            "OpenAI file id 非法",
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
            "OpenAI files {field} 含非法字符"
        ))));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn file_object_decodes() {
        let file: FileObject = serde_json::from_value(json!({
            "id": "file-1",
            "object": "file",
            "bytes": 10,
            "filename": "a.jsonl",
            "purpose": "batch"
        }))
        .expect("file");
        assert_eq!(file.id, "file-1");
        assert_eq!(file.purpose.as_deref(), Some("batch"));
    }

    #[test]
    fn invalid_ids_are_rejected() {
        assert!(file_id("").is_err());
        assert!(file_id("bad id").is_err());
    }
}

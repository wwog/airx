//! `multipart/form-data` 编码。
//!
//! 只负责把字段编码成 body；boundary 由调用方提供，便于测试与重放。

use bytes::{Bytes, BytesMut};

use crate::AirxError;
use crate::error::ConfigurationError;

/// 单个表单字段。
#[derive(Debug, Clone)]
pub struct MultipartPart {
    name: String,
    filename: Option<String>,
    content_type: Option<String>,
    data: Bytes,
}

impl MultipartPart {
    /// 构造文本字段。
    pub fn text(name: impl Into<String>, value: impl Into<Bytes>) -> Self {
        Self {
            name: name.into(),
            filename: None,
            content_type: None,
            data: value.into(),
        }
    }

    /// 构造文件字段。
    pub fn file(
        name: impl Into<String>,
        filename: impl Into<String>,
        content_type: impl Into<String>,
        data: impl Into<Bytes>,
    ) -> Self {
        Self {
            name: name.into(),
            filename: Some(filename.into()),
            content_type: Some(content_type.into()),
            data: data.into(),
        }
    }
}

/// 已排序的 `multipart/form-data` 表单。
#[derive(Debug, Clone)]
pub struct MultipartForm {
    boundary: String,
    parts: Vec<MultipartPart>,
}

impl MultipartForm {
    /// 使用调用方提供的 boundary 创建表单。
    pub fn new(boundary: impl Into<String>) -> Result<Self, AirxError> {
        let boundary = boundary.into();
        if boundary.is_empty()
            || boundary.len() > 70
            || boundary
                .chars()
                .any(|ch| ch.is_control() || ch == '"' || ch == '\\')
        {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "multipart boundary 非法",
            )));
        }

        Ok(Self {
            boundary,
            parts: Vec::new(),
        })
    }

    /// 追加文本字段。
    #[must_use]
    pub fn text(mut self, name: impl Into<String>, value: impl Into<Bytes>) -> Self {
        self.parts.push(MultipartPart::text(name, value));
        self
    }

    /// 追加文件字段。
    #[must_use]
    pub fn file(
        mut self,
        name: impl Into<String>,
        filename: impl Into<String>,
        content_type: impl Into<String>,
        data: impl Into<Bytes>,
    ) -> Self {
        self.parts
            .push(MultipartPart::file(name, filename, content_type, data));
        self
    }

    /// `Content-Type` Header 值。
    pub fn content_type(&self) -> String {
        format!("multipart/form-data; boundary={}", self.boundary)
    }

    /// 编码为完整 body。
    pub fn body(&self) -> Result<Bytes, AirxError> {
        let mut buffer = BytesMut::new();

        for part in &self.parts {
            validate_token(&part.name, "字段名")?;
            buffer.extend_from_slice(b"--");
            buffer.extend_from_slice(self.boundary.as_bytes());
            buffer.extend_from_slice(b"\r\n");

            buffer.extend_from_slice(b"Content-Disposition: form-data; name=\"");
            buffer.extend_from_slice(part.name.as_bytes());
            buffer.extend_from_slice(b"\"");

            if let Some(filename) = &part.filename {
                validate_token(filename, "文件名")?;
                buffer.extend_from_slice(b"; filename=\"");
                buffer.extend_from_slice(filename.as_bytes());
                buffer.extend_from_slice(b"\"");
            }
            buffer.extend_from_slice(b"\r\n");

            if let Some(content_type) = &part.content_type {
                validate_token(content_type, "Content-Type")?;
                buffer.extend_from_slice(b"Content-Type: ");
                buffer.extend_from_slice(content_type.as_bytes());
                buffer.extend_from_slice(b"\r\n");
            }

            buffer.extend_from_slice(b"\r\n");
            buffer.extend_from_slice(&part.data);
            buffer.extend_from_slice(b"\r\n");
        }

        buffer.extend_from_slice(b"--");
        buffer.extend_from_slice(self.boundary.as_bytes());
        buffer.extend_from_slice(b"--\r\n");
        Ok(buffer.freeze())
    }
}

fn validate_token(value: &str, field: &str) -> Result<(), AirxError> {
    if value.is_empty() || value.chars().any(|ch| ch.is_control() || ch == '"') {
        return Err(AirxError::Configuration(ConfigurationError::new(format!(
            "multipart {field} 非法"
        ))));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_text_and_file_parts() {
        let form = MultipartForm::new("boundary-1")
            .expect("form")
            .text("purpose", "assistants")
            .file("file", "hello.txt", "text/plain", "hello".as_bytes());
        let body = form.body().expect("body");
        let text = std::str::from_utf8(&body).expect("utf8");

        assert!(text.starts_with("--boundary-1\r\n"));
        assert!(text.contains("name=\"purpose\""));
        assert!(text.contains("filename=\"hello.txt\""));
        assert!(text.contains("Content-Type: text/plain"));
        assert!(text.ends_with("--boundary-1--\r\n"));
        assert_eq!(
            form.content_type(),
            "multipart/form-data; boundary=boundary-1"
        );
    }

    #[test]
    fn invalid_boundary_and_fields_are_rejected() {
        assert!(MultipartForm::new("").is_err());
        assert!(MultipartForm::new("bad\nboundary").is_err());

        let form = MultipartForm::new("boundary").expect("form");
        assert!(form.text("bad\nname", "x").body().is_err());
    }
}

//! 单次请求选项。
//!
//! 与客户端级配置分离：一次请求可以临时追加 Header、设置幂等键，
//! 而不修改客户端本身。

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::http::{HeaderMap, HeaderName, HeaderValue, HttpRequest};

/// 单次请求选项。
#[derive(Debug, Clone, Default)]
pub struct RequestOptions {
    headers: HeaderMap,
    idempotency_key: Option<String>,
}

impl RequestOptions {
    /// 创建空选项。
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一个 Header（按类型安全的方式构造）。
    #[must_use]
    pub fn header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.headers.insert(name, value);
        self
    }

    /// 以字符串追加 Header；非法名称 / 值立即报错。
    pub fn try_header(mut self, name: &str, value: &str) -> Result<Self, AirxError> {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
            AirxError::Configuration(ConfigurationError::new(format!(
                "请求 Header 名称非法: {error}"
            )))
        })?;
        let value = HeaderValue::from_str(value).map_err(|error| {
            AirxError::Configuration(ConfigurationError::new(format!(
                "请求 Header 值非法: {error}"
            )))
        })?;
        self.headers.insert(name, value);
        Ok(self)
    }

    /// 合并一组 Header。
    #[must_use]
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        for (name, value) in &headers {
            self.headers.insert(name.clone(), value.clone());
        }
        self
    }

    /// 设置 `Idempotency-Key`。
    #[must_use]
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }

    pub(crate) fn apply(&self, request: &mut HttpRequest) -> Result<(), AirxError> {
        for (name, value) in &self.headers {
            request.headers.insert(name.clone(), value.clone());
        }

        if let Some(key) = &self.idempotency_key {
            let value = HeaderValue::from_str(key).map_err(|error| {
                AirxError::Configuration(ConfigurationError::new(format!(
                    "Idempotency-Key 非法: {error}"
                )))
            })?;
            request
                .headers
                .insert(HeaderName::from_static("idempotency-key"), value);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airx_core::http::Uri;

    fn request() -> HttpRequest {
        HttpRequest::get("https://example.com/v1".parse::<Uri>().expect("uri"))
    }

    #[test]
    fn options_override_headers_and_add_idempotency_key() {
        let mut request = request();
        let options = RequestOptions::new()
            .header(
                HeaderName::from_static("x-trace-id"),
                HeaderValue::from_static("trace-1"),
            )
            .idempotency_key("idem-1");

        options.apply(&mut request).expect("apply");

        assert_eq!(request.headers["x-trace-id"], "trace-1");
        assert_eq!(request.headers["idempotency-key"], "idem-1");
    }

    #[test]
    fn invalid_idempotency_key_is_rejected() {
        let mut request = request();
        let error = RequestOptions::new()
            .idempotency_key("bad\nkey")
            .apply(&mut request)
            .expect_err("invalid");
        assert!(matches!(error, AirxError::Configuration(_)));
    }

    #[test]
    fn try_header_validates() {
        assert!(RequestOptions::new().try_header("x-ok", "value").is_ok());
        assert!(
            RequestOptions::new()
                .try_header("bad header", "value")
                .is_err()
        );
    }
}

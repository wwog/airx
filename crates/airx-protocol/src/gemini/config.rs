//! Gemini Provider 配置。
//!
//! Provider 配置（api_key / base_url / 默认 Header）与执行配置
//! （transport / timeout / retry）分离，后者属于 `airx` 与 `airx-transport`。

use std::fmt;

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::http::{HeaderMap, HeaderName, HeaderValue, HttpRequest, Uri};

/// 官方 API 基地址（已包含 API 版本）。
pub const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

/// Gemini 配置。
///
/// `Debug` 会隐藏 api_key，避免日志泄漏。
#[derive(Clone)]
pub struct GeminiConfig {
    pub(crate) api_key: String,
    pub(crate) base_url: String,
    pub(crate) default_headers: HeaderMap,
}

impl GeminiConfig {
    /// 使用官方地址创建配置。
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_owned(),
            default_headers: HeaderMap::new(),
        }
    }

    /// 覆盖基地址（代理 / 网关 / 兼容服务）。
    ///
    /// 必须包含 scheme 与 host；末尾 `/` 会被去掉。
    pub fn with_base_url(mut self, base_url: impl AsRef<str>) -> Result<Self, AirxError> {
        let text = base_url.as_ref().trim_end_matches('/');
        let uri: Uri = text.parse().map_err(|error| {
            AirxError::Configuration(ConfigurationError::new(format!(
                "Gemini base_url 非法: {error}"
            )))
        })?;

        if uri.scheme().is_none() || uri.authority().is_none() {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "Gemini base_url 必须是绝对地址",
            )));
        }

        self.base_url = text.to_owned();
        Ok(self)
    }

    /// 追加 Provider 级默认 Header（协议层写入，不会被执行层覆盖）。
    #[must_use]
    pub fn with_header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.default_headers.insert(name, value);
        self
    }

    /// 当前基地址。
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Provider 级默认 Header。
    pub fn default_headers(&self) -> &HeaderMap {
        &self.default_headers
    }

    /// 解析任意相对路径（或完整 URL）。
    pub(crate) fn resolve_path(&self, path: &str, label: &str) -> Result<Uri, AirxError> {
        crate::internal::url::endpoint_uri(&self.base_url, path, &format!("Gemini {label}"))
    }

    /// 写入认证与 Provider 级默认 Header。
    pub(crate) fn apply_headers(&self, http: &mut HttpRequest) -> Result<(), AirxError> {
        http.headers.insert(
            HeaderName::from_static("x-goog-api-key"),
            HeaderValue::from_str(&self.api_key).map_err(|error| {
                AirxError::Configuration(ConfigurationError::new(format!(
                    "Gemini api_key 无法作为 HTTP Header: {error}"
                )))
            })?,
        );

        for (name, value) in &self.default_headers {
            http.headers.insert(name.clone(), value.clone());
        }

        Ok(())
    }
}

impl fmt::Debug for GeminiConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GeminiConfig")
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .field("default_headers", &self.default_headers)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_base_url_is_official() {
        let config = GeminiConfig::new("test-key");
        assert_eq!(config.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn base_url_is_validated_and_trimmed() {
        let config = GeminiConfig::new("test-key")
            .with_base_url("https://gateway.example.com/gemini/")
            .expect("valid base url");
        assert_eq!(config.base_url(), "https://gateway.example.com/gemini");

        assert!(
            GeminiConfig::new("test-key")
                .with_base_url("/relative")
                .is_err()
        );
        assert!(GeminiConfig::new("test-key").with_base_url("").is_err());
    }

    #[test]
    fn debug_redacts_api_key() {
        let text = format!("{:?}", GeminiConfig::new("secret-value"));
        assert!(!text.contains("secret-value"));
        assert!(text.contains("<redacted>"));
    }
}

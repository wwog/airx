//! OpenAI Provider 配置。
//!
//! Provider 配置（api_key / base_url / 组织 / 版本 Header）与执行配置
//! （transport / timeout / retry）分离，后者属于 `airx` 与 `airx-transport`。

use std::fmt;

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::http::{HeaderMap, HeaderName, HeaderValue, Uri};

/// 官方 API 基地址（包含 `/v1`）。
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// OpenAI 配置。
///
/// `Debug` 会隐藏 api_key，避免日志泄漏。
#[derive(Clone)]
pub struct OpenAiConfig {
    pub(crate) api_key: String,
    pub(crate) base_url: String,
    pub(crate) organization: Option<String>,
    pub(crate) project: Option<String>,
    pub(crate) default_headers: HeaderMap,
}

impl OpenAiConfig {
    /// 使用官方地址创建配置。
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_owned(),
            organization: None,
            project: None,
            default_headers: HeaderMap::new(),
        }
    }

    /// 覆盖基地址（兼容代理、网关、OpenAI-Compatible 服务）。
    ///
    /// 必须包含 scheme 与 host；末尾 `/` 会被去掉。
    pub fn with_base_url(mut self, base_url: impl AsRef<str>) -> Result<Self, AirxError> {
        let text = base_url.as_ref().trim_end_matches('/');
        let uri: Uri = text.parse().map_err(|error| {
            AirxError::Configuration(ConfigurationError::new(format!(
                "OpenAI base_url 非法: {error}"
            )))
        })?;

        if uri.scheme().is_none() || uri.authority().is_none() {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "OpenAI base_url 必须是绝对地址",
            )));
        }

        self.base_url = text.to_owned();
        Ok(self)
    }

    /// 设置 `OpenAI-Organization` Header。
    #[must_use]
    pub fn with_organization(mut self, organization: impl Into<String>) -> Self {
        self.organization = Some(organization.into());
        self
    }

    /// 设置 `OpenAI-Project` Header。
    #[must_use]
    pub fn with_project(mut self, project: impl Into<String>) -> Self {
        self.project = Some(project.into());
        self
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

    /// 组织。
    pub fn organization(&self) -> Option<&str> {
        self.organization.as_deref()
    }

    /// 项目。
    pub fn project(&self) -> Option<&str> {
        self.project.as_deref()
    }

    /// Provider 级默认 Header。
    pub fn default_headers(&self) -> &HeaderMap {
        &self.default_headers
    }
}

impl fmt::Debug for OpenAiConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAiConfig")
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .field("organization", &self.organization)
            .field("project", &self.project)
            .field("default_headers", &self.default_headers)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_base_url_is_official() {
        let config = OpenAiConfig::new("sk-test");
        assert_eq!(config.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn base_url_is_validated_and_trimmed() {
        let config = OpenAiConfig::new("sk-test")
            .with_base_url("https://gateway.example.com/openai/")
            .expect("valid base url");
        assert_eq!(config.base_url(), "https://gateway.example.com/openai");

        assert!(
            OpenAiConfig::new("sk-test")
                .with_base_url("/relative")
                .is_err()
        );
        assert!(OpenAiConfig::new("sk-test").with_base_url("").is_err());
    }

    #[test]
    fn debug_redacts_api_key() {
        let text = format!("{:?}", OpenAiConfig::new("sk-secret-value"));
        assert!(!text.contains("sk-secret-value"));
        assert!(text.contains("<redacted>"));
    }
}

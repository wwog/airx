//! Anthropic Provider 配置。

use std::fmt;

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::http::{HeaderMap, HeaderName, HeaderValue, Uri};

/// 官方 API 基地址（不含 `/v1`）。
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
/// 默认 API 版本。
pub const DEFAULT_API_VERSION: &str = "2023-06-01";

/// Anthropic 配置。
#[derive(Clone)]
pub struct AnthropicConfig {
    pub(crate) api_key: String,
    pub(crate) base_url: String,
    pub(crate) api_version: String,
    pub(crate) default_headers: HeaderMap,
}

impl AnthropicConfig {
    /// 使用官方地址与默认版本创建配置。
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_owned(),
            api_version: DEFAULT_API_VERSION.to_owned(),
            default_headers: HeaderMap::new(),
        }
    }

    /// 覆盖基地址（代理 / 网关）。
    pub fn with_base_url(mut self, base_url: impl AsRef<str>) -> Result<Self, AirxError> {
        let text = base_url.as_ref().trim_end_matches('/');
        let uri: Uri = text.parse().map_err(|error| {
            AirxError::Configuration(ConfigurationError::new(format!(
                "Anthropic base_url 非法: {error}"
            )))
        })?;

        if uri.scheme().is_none() || uri.authority().is_none() {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "Anthropic base_url 必须是绝对地址",
            )));
        }

        self.base_url = text.to_owned();
        Ok(self)
    }

    /// 设置 API 版本。
    #[must_use]
    pub fn with_api_version(mut self, version: impl Into<String>) -> Self {
        self.api_version = version.into();
        self
    }

    /// 追加 Provider 级默认 Header（例如 beta Header）。
    #[must_use]
    pub fn with_header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.default_headers.insert(name, value);
        self
    }

    /// 基地址。
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// API 版本。
    pub fn api_version(&self) -> &str {
        &self.api_version
    }

    /// 默认 Header。
    pub fn default_headers(&self) -> &HeaderMap {
        &self.default_headers
    }
}

impl fmt::Debug for AnthropicConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnthropicConfig")
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .field("api_version", &self.api_version)
            .finish_non_exhaustive()
    }
}

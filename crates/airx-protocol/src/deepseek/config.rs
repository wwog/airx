//! DeepSeek 配置。

use std::fmt;

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::http::{HeaderMap, HeaderName, HeaderValue, Uri};

/// 官方基地址（不含 `/v1`）。
pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";

use super::compatibility::DeepSeekPolicy;

/// DeepSeek 配置。
#[derive(Clone)]
pub struct DeepSeekConfig {
    pub(crate) api_key: String,
    pub(crate) base_url: String,
    pub(crate) default_headers: HeaderMap,
    pub(crate) policy: DeepSeekPolicy,
}

impl DeepSeekConfig {
    /// 使用官方地址创建配置（默认 Strict 兼容策略）。
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_owned(),
            default_headers: HeaderMap::new(),
            policy: DeepSeekPolicy::strict(),
        }
    }

    /// 设置兼容策略。
    #[must_use]
    pub fn with_policy(mut self, policy: DeepSeekPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// 覆盖基地址。
    pub fn with_base_url(mut self, base_url: impl AsRef<str>) -> Result<Self, AirxError> {
        let text = base_url.as_ref().trim_end_matches('/');
        let uri: Uri = text.parse().map_err(|error| {
            AirxError::Configuration(ConfigurationError::new(format!(
                "DeepSeek base_url 非法: {error}"
            )))
        })?;

        if uri.scheme().is_none() || uri.authority().is_none() {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "DeepSeek base_url 必须是绝对地址",
            )));
        }

        self.base_url = text.to_owned();
        Ok(self)
    }

    /// 追加默认 Header。
    #[must_use]
    pub fn with_header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.default_headers.insert(name, value);
        self
    }

    /// 基地址。
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// 默认 Header。
    pub fn default_headers(&self) -> &HeaderMap {
        &self.default_headers
    }

    /// 兼容策略。
    pub fn policy(&self) -> DeepSeekPolicy {
        self.policy
    }
}

impl fmt::Debug for DeepSeekConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeepSeekConfig")
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

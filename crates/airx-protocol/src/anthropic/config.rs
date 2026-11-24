//! Anthropic Provider 配置。

use std::fmt;

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::http::{HeaderMap, HeaderName, HeaderValue, HttpRequest, Uri};

use crate::internal::url::{endpoint_path, endpoint_uri};

/// 官方 API 基地址（不含 `/v1`）。
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
/// 默认 Messages 路径。
pub const DEFAULT_MESSAGES_PATH: &str = "/v1/messages";
/// 默认 API 版本。
///
/// 官方当前版本为 `2026-10-21`；`output_config`、`thinking.display`、
/// `stop_reason=refusal` 等字段只在该版本面生效。兼容网关未必接受该值，
/// 接入网关时用 [`AnthropicConfig::with_api_version`] 显式固定。
pub const DEFAULT_API_VERSION: &str = "2026-10-21";

/// 认证方式。
///
/// 官方仍支持 `x-api-key`（`Authorization` 的 legacy fallback），
/// OAuth / WIF 短期 token 走 `Authorization: Bearer`。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthScheme {
    /// `x-api-key: <key>`（默认）。
    #[default]
    ApiKey,
    /// `Authorization: Bearer <token>`。
    Bearer,
}

/// Anthropic 配置。
#[derive(Clone)]
pub struct AnthropicConfig {
    pub(crate) credential: String,
    pub(crate) auth: AuthScheme,
    pub(crate) base_url: String,
    pub(crate) messages_path: String,
    pub(crate) api_version: String,
    pub(crate) default_headers: HeaderMap,
}

impl AnthropicConfig {
    /// 使用官方地址、默认路径与默认版本创建配置。
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            credential: api_key.into(),
            auth: AuthScheme::ApiKey,
            base_url: DEFAULT_BASE_URL.to_owned(),
            messages_path: DEFAULT_MESSAGES_PATH.to_owned(),
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

    /// 覆盖 Messages 路径。
    ///
    /// 相对路径会补前导 `/`；也可以直接传完整 URL（忽略 `base_url`）。
    pub fn with_messages_path(mut self, path: impl AsRef<str>) -> Result<Self, AirxError> {
        self.messages_path = endpoint_path(path.as_ref(), "Anthropic messages_path")?;
        Ok(self)
    }

    /// 设置 API 版本。
    #[must_use]
    pub fn with_api_version(mut self, version: impl Into<String>) -> Self {
        self.api_version = version.into();
        self
    }

    /// 切换认证方式。
    #[must_use]
    pub fn with_auth_scheme(mut self, scheme: AuthScheme) -> Self {
        self.auth = scheme;
        self
    }

    /// 使用 `Authorization: Bearer <token>`（OAuth / WIF 短期 token）。
    ///
    /// 该模式下不再发送 `x-api-key`。
    #[must_use]
    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.credential = token.into();
        self.auth = AuthScheme::Bearer;
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

    /// 当前 Messages 路径。
    pub fn messages_path(&self) -> &str {
        &self.messages_path
    }

    /// API 版本。
    pub fn api_version(&self) -> &str {
        &self.api_version
    }

    /// 当前认证方式。
    pub fn auth_scheme(&self) -> AuthScheme {
        self.auth
    }

    /// 默认 Header。
    pub fn default_headers(&self) -> &HeaderMap {
        &self.default_headers
    }

    /// 把模式路径解析为最终 URL。
    pub(crate) fn endpoint_uri(&self) -> Result<Uri, AirxError> {
        endpoint_uri(&self.base_url, &self.messages_path, "Anthropic Messages")
    }

    /// 解析任意相对路径（或完整 URL）。
    pub(crate) fn resolve_path(&self, path: &str, label: &str) -> Result<Uri, AirxError> {
        endpoint_uri(&self.base_url, path, &format!("Anthropic {label}"))
    }

    /// countTokens 路径：跟随 Messages 路径。
    pub fn count_tokens_path(&self) -> String {
        format!("{}/count_tokens", self.messages_path.trim_end_matches('/'))
    }

    /// 写入认证、版本与 Provider 级默认 Header。
    pub(crate) fn apply_headers(&self, http: &mut HttpRequest) -> Result<(), AirxError> {
        match self.auth {
            AuthScheme::ApiKey => {
                http.headers.insert(
                    HeaderName::from_static("x-api-key"),
                    header_value(&self.credential, "api_key")?,
                );
            }
            AuthScheme::Bearer => {
                let value = format!("Bearer {}", self.credential);
                http.headers.insert(
                    HeaderName::from_static("authorization"),
                    header_value(&value, "token")?,
                );
            }
        }
        http.headers.insert(
            HeaderName::from_static("anthropic-version"),
            header_value(&self.api_version, "api_version")?,
        );

        for (name, value) in &self.default_headers {
            http.headers.insert(name.clone(), value.clone());
        }

        Ok(())
    }
}

fn header_value(value: &str, field: &str) -> Result<HeaderValue, AirxError> {
    HeaderValue::from_str(value).map_err(|error| {
        AirxError::Configuration(ConfigurationError::new(format!(
            "Anthropic {field} 无法作为 HTTP Header: {error}"
        )))
    })
}

impl fmt::Debug for AnthropicConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnthropicConfig")
            .field("credential", &"<redacted>")
            .field("auth", &self.auth)
            .field("base_url", &self.base_url)
            .field("messages_path", &self.messages_path)
            .field("api_version", &self.api_version)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_base_url_and_path_are_official() {
        let config = AnthropicConfig::new("sk-ant-test");
        assert_eq!(config.base_url(), DEFAULT_BASE_URL);
        assert_eq!(config.messages_path(), DEFAULT_MESSAGES_PATH);
        assert_eq!(config.api_version(), DEFAULT_API_VERSION);
    }

    #[test]
    fn messages_path_can_be_relative_or_absolute() {
        let config = AnthropicConfig::new("sk-ant-test")
            .with_messages_path("v2/messages")
            .expect("path");
        assert_eq!(config.messages_path(), "/v2/messages");
        assert_eq!(
            config.endpoint_uri().expect("uri").to_string(),
            "https://api.anthropic.com/v2/messages"
        );

        let config = AnthropicConfig::new("sk-ant-test")
            .with_messages_path("https://gateway.example.com/anthropic/messages")
            .expect("path");
        assert_eq!(
            config.endpoint_uri().expect("uri").to_string(),
            "https://gateway.example.com/anthropic/messages"
        );
    }

    #[test]
    fn default_version_is_current_official() {
        assert_eq!(DEFAULT_API_VERSION, "2026-10-21");
        let config = AnthropicConfig::new("sk-ant-test");
        assert_eq!(config.api_version(), "2026-10-21");
        assert_eq!(config.auth_scheme(), AuthScheme::ApiKey);
    }

    #[test]
    fn bearer_auth_uses_authorization_header() {
        let config = AnthropicConfig::new("oauth-token").with_bearer_token("oauth-token");
        let mut http = HttpRequest::get("https://api.anthropic.com/v1/messages".parse().unwrap());
        config.apply_headers(&mut http).expect("headers");

        assert_eq!(http.headers["authorization"], "Bearer oauth-token");
        assert!(!http.headers.contains_key("x-api-key"));
        assert_eq!(http.headers["anthropic-version"], "2026-10-21");
    }

    #[test]
    fn api_key_auth_stays_default() {
        let config = AnthropicConfig::new("sk-ant-test");
        let mut http = HttpRequest::get("https://api.anthropic.com/v1/messages".parse().unwrap());
        config.apply_headers(&mut http).expect("headers");

        assert_eq!(http.headers["x-api-key"], "sk-ant-test");
        assert!(!http.headers.contains_key("authorization"));
    }

    #[test]
    fn invalid_messages_path_is_rejected() {
        assert!(
            AnthropicConfig::new("sk-ant-test")
                .with_messages_path("")
                .is_err()
        );
    }
}

//! OpenAI Provider 配置。
//!
//! Provider 配置（api_key / base_url / 端点路径 / 组织 / Header）与执行配置
//! （transport / timeout / retry）分离，后者属于 `airx` 与 `airx-transport`。
//!
//! 官方地址是默认值：只有接入三方兼容网关时才需要覆盖 `base_url`。
//! `chat` / `responses` 的路径同样有官方默认值，可独立覆盖为路径或完整 URL。

use std::fmt;

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::http::{HeaderMap, HeaderName, HeaderValue, Uri};

use crate::internal::url::{endpoint_path, endpoint_uri};

/// 官方 API 基地址（包含 `/v1`）。
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// Chat Completions 默认路径。
pub const DEFAULT_CHAT_PATH: &str = "/chat/completions";
/// Responses 默认路径。
pub const DEFAULT_RESPONSES_PATH: &str = "/responses";

/// OpenAI 配置。
///
/// `Debug` 会隐藏 api_key，避免日志泄漏。
#[derive(Clone)]
pub struct OpenAiConfig {
    pub(crate) api_key: String,
    pub(crate) base_url: String,
    pub(crate) chat_path: String,
    pub(crate) responses_path: String,
    pub(crate) organization: Option<String>,
    pub(crate) project: Option<String>,
    pub(crate) default_headers: HeaderMap,
}

impl OpenAiConfig {
    /// 使用官方地址与默认端点路径创建配置。
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_owned(),
            chat_path: DEFAULT_CHAT_PATH.to_owned(),
            responses_path: DEFAULT_RESPONSES_PATH.to_owned(),
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

    /// 覆盖 Chat Completions 路径。
    ///
    /// 相对路径会补前导 `/`；也可以直接传完整 URL（忽略 `base_url`）。
    pub fn with_chat_path(mut self, path: impl AsRef<str>) -> Result<Self, AirxError> {
        self.chat_path = endpoint_path(path.as_ref(), "OpenAI chat_path")?;
        Ok(self)
    }

    /// 覆盖 Responses 路径。
    ///
    /// 相对路径会补前导 `/`；也可以直接传完整 URL（忽略 `base_url`）。
    pub fn with_responses_path(mut self, path: impl AsRef<str>) -> Result<Self, AirxError> {
        self.responses_path = endpoint_path(path.as_ref(), "OpenAI responses_path")?;
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

    /// 当前 Chat Completions 路径。
    pub fn chat_path(&self) -> &str {
        &self.chat_path
    }

    /// 当前 Responses 路径。
    pub fn responses_path(&self) -> &str {
        &self.responses_path
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

    /// 把模式路径解析为最终 URL。
    pub(crate) fn endpoint_uri(&self, path: &str, label: &str) -> Result<Uri, AirxError> {
        endpoint_uri(&self.base_url, path, &format!("OpenAI {label}"))
    }
}

impl fmt::Debug for OpenAiConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAiConfig")
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .field("chat_path", &self.chat_path)
            .field("responses_path", &self.responses_path)
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
    fn default_base_url_and_paths_are_official() {
        let config = OpenAiConfig::new("sk-test");
        assert_eq!(config.base_url(), DEFAULT_BASE_URL);
        assert_eq!(config.chat_path(), DEFAULT_CHAT_PATH);
        assert_eq!(config.responses_path(), DEFAULT_RESPONSES_PATH);
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
    fn mode_paths_are_optional_and_overridable() {
        let config = OpenAiConfig::new("sk-test")
            .with_chat_path("v2/chat")
            .expect("chat path")
            .with_responses_path("https://gateway.example.com/respond")
            .expect("responses path");

        assert_eq!(config.chat_path(), "/v2/chat");
        assert_eq!(
            config.responses_path(),
            "https://gateway.example.com/respond"
        );

        let chat_uri = config
            .endpoint_uri(config.chat_path(), "chat")
            .expect("chat uri");
        assert_eq!(chat_uri.to_string(), "https://api.openai.com/v1/v2/chat");

        let responses_uri = config
            .endpoint_uri(config.responses_path(), "responses")
            .expect("responses uri");
        assert_eq!(
            responses_uri.to_string(),
            "https://gateway.example.com/respond"
        );
    }

    #[test]
    fn invalid_mode_paths_are_rejected() {
        for path in ["", " ", "//gateway.example.com/chat", "https://"] {
            assert!(
                OpenAiConfig::new("sk-test").with_chat_path(path).is_err(),
                "path should be rejected: {path:?}"
            );
        }
    }

    #[test]
    fn debug_redacts_api_key_and_shows_paths() {
        let text = format!("{:?}", OpenAiConfig::new("sk-secret-value"));
        assert!(!text.contains("sk-secret-value"));
        assert!(text.contains("<redacted>"));
        assert!(text.contains(DEFAULT_CHAT_PATH));
        assert!(text.contains(DEFAULT_RESPONSES_PATH));
    }
}

//! Client 构建器。
//!
//! 只负责配置：Provider 配置与执行配置在内部分开保存，构建时组合。

use std::sync::Arc;
use std::time::Duration;

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::http::HeaderMap;
use airx_transport::{RetryPolicy, Transport};

use crate::client::Client;
use crate::executor::Executor;

/// Client 构建器。
pub struct ClientBuilder {
    transport: Option<Arc<dyn Transport>>,
    timeout: Option<Duration>,
    retry_policy: RetryPolicy,
    default_headers: HeaderMap,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            transport: None,
            timeout: None,
            retry_policy: RetryPolicy::disabled(),
            default_headers: HeaderMap::new(),
        }
    }
}

impl ClientBuilder {
    /// 创建默认构建器（重试默认关闭）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 使用自定义 Transport。
    #[must_use]
    pub fn transport<T: Transport + 'static>(mut self, transport: T) -> Self {
        self.transport = Some(Arc::new(transport));
        self
    }

    /// 设置请求超时（仅在使用内置 native Transport 时生效）。
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// 设置重试策略（默认关闭）。
    #[must_use]
    pub fn retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    /// 追加执行级默认 Header（不覆盖协议层 Header）。
    #[must_use]
    pub fn default_headers(mut self, headers: HeaderMap) -> Self {
        for (name, value) in &headers {
            self.default_headers.append(name.clone(), value.clone());
        }
        self
    }

    /// 配置 OpenAI Provider。
    #[cfg(feature = "openai")]
    pub fn openai(self) -> OpenAiBuilder {
        OpenAiBuilder {
            client: self,
            api_key: None,
            base_url: None,
            organization: None,
            project: None,
        }
    }

    /// 配置 Anthropic Provider。
    #[cfg(feature = "anthropic")]
    pub fn anthropic(self) -> AnthropicBuilder {
        AnthropicBuilder {
            client: self,
            api_key: None,
            base_url: None,
            api_version: None,
        }
    }

    /// 构建不带 Provider 的 Client（供后续 Unified 入口使用）。
    pub fn build(self) -> Result<Client, AirxError> {
        self.build_inner()
    }

    #[cfg(feature = "openai")]
    pub(crate) fn build_with_openai(
        self,
        openai: Option<airx_protocol::openai::OpenAiConfig>,
    ) -> Result<Client, AirxError> {
        let mut client = self.build_inner()?;
        client.openai = openai;
        Ok(client)
    }

    #[cfg(feature = "anthropic")]
    pub(crate) fn build_with_anthropic(
        self,
        anthropic: Option<airx_protocol::anthropic::AnthropicConfig>,
    ) -> Result<Client, AirxError> {
        let mut client = self.build_inner()?;
        client.anthropic = anthropic;
        Ok(client)
    }

    fn build_inner(self) -> Result<Client, AirxError> {
        let transport = match self.transport {
            Some(transport) => transport,
            None => default_transport(self.timeout)?,
        };
        let transport = apply_retry(transport, self.retry_policy)?;

        Ok(Client {
            executor: Arc::new(Executor::new(transport, self.default_headers)),
            #[cfg(feature = "anthropic")]
            anthropic: None,
            #[cfg(feature = "openai")]
            openai: None,
        })
    }
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn default_transport(timeout: Option<Duration>) -> Result<Arc<dyn Transport>, AirxError> {
    let mut builder = airx_transport::ReqwestTransport::builder();
    if let Some(timeout) = timeout {
        builder = builder.timeout(timeout);
    }
    Ok(Arc::new(builder.build()?))
}

#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
fn default_transport(_timeout: Option<Duration>) -> Result<Arc<dyn Transport>, AirxError> {
    Ok(Arc::new(airx_transport::WasmTransport::new()))
}

#[cfg(not(any(
    all(feature = "native", not(target_arch = "wasm32")),
    all(target_arch = "wasm32", feature = "wasm")
)))]
fn default_transport(_timeout: Option<Duration>) -> Result<Arc<dyn Transport>, AirxError> {
    Err(AirxError::Configuration(ConfigurationError::new(
        "未提供 Transport，且当前未启用 native / wasm feature",
    )))
}

#[cfg(not(target_arch = "wasm32"))]
fn apply_retry(
    transport: Arc<dyn Transport>,
    policy: RetryPolicy,
) -> Result<Arc<dyn Transport>, AirxError> {
    if policy.is_enabled() {
        Ok(Arc::new(airx_transport::RetryTransport::new(
            transport, policy,
        )))
    } else {
        Ok(transport)
    }
}

#[cfg(target_arch = "wasm32")]
fn apply_retry(
    transport: Arc<dyn Transport>,
    policy: RetryPolicy,
) -> Result<Arc<dyn Transport>, AirxError> {
    if policy.is_enabled() {
        return Err(AirxError::Configuration(ConfigurationError::new(
            "WASM 目标暂不支持自动重试",
        )));
    }
    Ok(transport)
}

/// OpenAI Provider 构建器。
#[cfg(feature = "openai")]
pub struct OpenAiBuilder {
    client: ClientBuilder,
    api_key: Option<String>,
    base_url: Option<String>,
    organization: Option<String>,
    project: Option<String>,
}

#[cfg(feature = "openai")]
impl OpenAiBuilder {
    /// 设置 API Key（必填）。
    #[must_use]
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// 覆盖基地址（代理 / 网关 / OpenAI-Compatible 服务）。
    #[must_use]
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// 设置组织。
    #[must_use]
    pub fn organization(mut self, organization: impl Into<String>) -> Self {
        self.organization = Some(organization.into());
        self
    }

    /// 设置项目。
    #[must_use]
    pub fn project(mut self, project: impl Into<String>) -> Self {
        self.project = Some(project.into());
        self
    }

    /// 使用自定义 Transport。
    #[must_use]
    pub fn transport<T: Transport + 'static>(mut self, transport: T) -> Self {
        self.client = self.client.transport(transport);
        self
    }

    /// 设置请求超时。
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.client = self.client.timeout(timeout);
        self
    }

    /// 设置重试策略。
    #[must_use]
    pub fn retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.client = self.client.retry_policy(retry_policy);
        self
    }

    /// 构建 Client。
    pub fn build(self) -> Result<Client, AirxError> {
        let api_key = self.api_key.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("OpenAI api_key 必填"))
        })?;

        if api_key.trim().is_empty() {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "OpenAI api_key 不能为空",
            )));
        }

        let mut config = airx_protocol::openai::OpenAiConfig::new(api_key);
        if let Some(base_url) = self.base_url {
            config = config.with_base_url(base_url)?;
        }
        if let Some(organization) = self.organization {
            config = config.with_organization(organization);
        }
        if let Some(project) = self.project {
            config = config.with_project(project);
        }

        self.client.build_with_openai(Some(config))
    }
}

/// Anthropic Provider 构建器。
#[cfg(feature = "anthropic")]
pub struct AnthropicBuilder {
    client: ClientBuilder,
    api_key: Option<String>,
    base_url: Option<String>,
    api_version: Option<String>,
}

#[cfg(feature = "anthropic")]
impl AnthropicBuilder {
    /// 设置 API Key（必填）。
    #[must_use]
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// 覆盖基地址。
    #[must_use]
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// 设置 API 版本。
    #[must_use]
    pub fn api_version(mut self, version: impl Into<String>) -> Self {
        self.api_version = Some(version.into());
        self
    }

    /// 使用自定义 Transport。
    #[must_use]
    pub fn transport<T: Transport + 'static>(mut self, transport: T) -> Self {
        self.client = self.client.transport(transport);
        self
    }

    /// 设置请求超时。
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.client = self.client.timeout(timeout);
        self
    }

    /// 设置重试策略。
    #[must_use]
    pub fn retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.client = self.client.retry_policy(retry_policy);
        self
    }

    /// 构建 Client。
    pub fn build(self) -> Result<Client, AirxError> {
        let api_key = self.api_key.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("Anthropic api_key 必填"))
        })?;

        if api_key.trim().is_empty() {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "Anthropic api_key 不能为空",
            )));
        }

        let mut config = airx_protocol::anthropic::AnthropicConfig::new(api_key);
        if let Some(base_url) = self.base_url {
            config = config.with_base_url(base_url)?;
        }
        if let Some(version) = self.api_version {
            config = config.with_api_version(version);
        }

        self.client.build_with_anthropic(Some(config))
    }
}

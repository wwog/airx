//! Client 构建器。
//!
//! 只负责配置：Provider 配置与执行配置在内部分开保存，构建时组合。
//! 每个 Provider 分支的 `build()` 直接返回该协议的客户端，
//! 构建后即可复用，不需要再次选择 Provider。

use std::sync::Arc;
use std::time::Duration;

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::http::HeaderMap;
use airx_core::log::{LogConfig, LogHandle, Logger};
use airx_transport::{RetryPolicy, Transport};

use crate::executor::Executor;

/// 执行配置构建器；随后通过 `ClientBuilder::openai` 等进入具体 Provider。
pub struct ClientBuilder {
    transport: Option<Arc<dyn Transport>>,
    timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
    retry_policy: RetryPolicy,
    default_headers: HeaderMap,
    logger: Option<Arc<dyn Logger>>,
    log_config: LogConfig,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            transport: None,
            timeout: None,
            connect_timeout: None,
            retry_policy: RetryPolicy::default(),
            default_headers: HeaderMap::new(),
            logger: None,
            log_config: LogConfig::new(),
        }
    }
}

impl ClientBuilder {
    /// 创建默认构建器（native 默认 2 次重试、10 秒连接超时）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 使用自定义 Transport。
    #[must_use]
    pub fn transport<T: Transport + 'static>(mut self, transport: T) -> Self {
        self.transport = Some(Arc::new(transport));
        self
    }

    /// 设置整体请求超时（仅在使用内置 native Transport 时生效）。
    ///
    /// 这是总时长，长流会被截断；一般只需调整连接超时。
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// 设置连接超时（默认 10 秒，仅在使用内置 native Transport 时生效）。
    #[must_use]
    pub fn connect_timeout(mut self, connect_timeout: Duration) -> Self {
        self.connect_timeout = Some(connect_timeout);
        self
    }

    /// 设置重试策略（native 默认 [`RetryPolicy::standard`]，可显式禁用）。
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

    /// 注入日志实现（业务层自定义 sink）。
    ///
    /// 只设置 logger 不会产生任何输出：还需要通过 [`ClientBuilder::log_config`]
    /// 显式打开 scope，默认 [`LogConfig::new`] 关闭全部 scope。
    #[must_use]
    pub fn logger(mut self, logger: Arc<dyn Logger>) -> Self {
        self.logger = Some(logger);
        self
    }

    /// 设置日志过滤配置（按 scope 分级开启）。
    ///
    /// ```ignore
    /// let config = LogConfig::new()
    ///     .scope(Scope::NET, LogLevel::Info)
    ///     .scope(Scope::CALL, LogLevel::Info);
    /// let client = Client::builder().logger(logger).log_config(config);
    /// ```
    #[must_use]
    pub fn log_config(mut self, config: LogConfig) -> Self {
        self.log_config = config;
        self
    }

    /// 配置 OpenAI Provider。
    #[cfg(feature = "openai")]
    pub fn openai(self) -> OpenAiBuilder {
        OpenAiBuilder {
            client: self,
            api_key: None,
            base_url: None,
            chat_path: None,
            responses_path: None,
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
            messages_path: None,
            api_version: None,
        }
    }

    /// 配置 Gemini Provider。
    #[cfg(feature = "gemini")]
    pub fn gemini(self) -> GeminiBuilder {
        GeminiBuilder {
            client: self,
            api_key: None,
            base_url: None,
        }
    }

    /// 把执行配置固化为共享的 [`Executor`]。
    pub(crate) fn build_executor(self) -> Result<Arc<Executor>, AirxError> {
        let retry_policy = self.retry_policy;
        let log = match self.logger {
            Some(logger) => LogHandle::new(logger, self.log_config),
            None => LogHandle::disabled(),
        };
        let transport = match self.transport {
            Some(transport) => transport,
            None => default_transport(self.timeout, self.connect_timeout)?,
        };
        Ok(Arc::new(Executor::new(
            transport,
            self.default_headers,
            retry_policy,
            log,
        )))
    }
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn default_transport(
    timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
) -> Result<Arc<dyn Transport>, AirxError> {
    let mut builder = airx_transport::ReqwestTransport::builder();
    if let Some(timeout) = timeout {
        builder = builder.timeout(timeout);
    }
    if let Some(connect_timeout) = connect_timeout {
        builder = builder.connect_timeout(connect_timeout);
    }
    Ok(Arc::new(builder.build()?))
}

#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
fn default_transport(
    _timeout: Option<Duration>,
    _connect_timeout: Option<Duration>,
) -> Result<Arc<dyn Transport>, AirxError> {
    Ok(Arc::new(airx_transport::WasmTransport::new()))
}

#[cfg(not(any(
    all(feature = "native", not(target_arch = "wasm32")),
    all(target_arch = "wasm32", feature = "wasm")
)))]
fn default_transport(
    _timeout: Option<Duration>,
    _connect_timeout: Option<Duration>,
) -> Result<Arc<dyn Transport>, AirxError> {
    Err(AirxError::Configuration(ConfigurationError::new(
        "未提供 Transport，且当前未启用 native / wasm feature",
    )))
}

/// OpenAI Provider 构建器。
#[cfg(feature = "openai")]
pub struct OpenAiBuilder {
    client: ClientBuilder,
    api_key: Option<String>,
    base_url: Option<String>,
    chat_path: Option<String>,
    responses_path: Option<String>,
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

    /// 覆盖 Chat Completions 路径（默认 `/chat/completions`）。
    ///
    /// 兼容网关可传相对路径或完整 URL；`build()` 时校验。
    #[must_use]
    pub fn chat_path(mut self, path: impl Into<String>) -> Self {
        self.chat_path = Some(path.into());
        self
    }

    /// 覆盖 Responses 路径（默认 `/responses`）。
    #[must_use]
    pub fn responses_path(mut self, path: impl Into<String>) -> Self {
        self.responses_path = Some(path.into());
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

    /// 注入日志实现；需配合 [`OpenAiBuilder::log_config`] 打开 scope。
    #[must_use]
    pub fn logger(mut self, logger: Arc<dyn Logger>) -> Self {
        self.client = self.client.logger(logger);
        self
    }

    /// 设置日志过滤配置。
    #[must_use]
    pub fn log_config(mut self, config: LogConfig) -> Self {
        self.client = self.client.log_config(config);
        self
    }

    /// 构建可复用的 OpenAI 客户端。
    pub fn build(self) -> Result<crate::openai::OpenAi, AirxError> {
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
        if let Some(chat_path) = self.chat_path {
            config = config.with_chat_path(chat_path)?;
        }
        if let Some(responses_path) = self.responses_path {
            config = config.with_responses_path(responses_path)?;
        }
        if let Some(organization) = self.organization {
            config = config.with_organization(organization);
        }
        if let Some(project) = self.project {
            config = config.with_project(project);
        }

        Ok(crate::openai::OpenAi::new(
            self.client.build_executor()?,
            config,
        ))
    }
}

/// Anthropic Provider 构建器。
#[cfg(feature = "anthropic")]
pub struct AnthropicBuilder {
    client: ClientBuilder,
    api_key: Option<String>,
    base_url: Option<String>,
    messages_path: Option<String>,
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

    /// 覆盖 Messages 路径（默认 `/v1/messages`）。
    #[must_use]
    pub fn messages_path(mut self, path: impl Into<String>) -> Self {
        self.messages_path = Some(path.into());
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

    /// 注入日志实现；需配合 [`AnthropicBuilder::log_config`] 打开 scope。
    #[must_use]
    pub fn logger(mut self, logger: Arc<dyn Logger>) -> Self {
        self.client = self.client.logger(logger);
        self
    }

    /// 设置日志过滤配置。
    #[must_use]
    pub fn log_config(mut self, config: LogConfig) -> Self {
        self.client = self.client.log_config(config);
        self
    }

    /// 构建可复用的 Anthropic 客户端。
    pub fn build(self) -> Result<crate::anthropic::Anthropic, AirxError> {
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
        if let Some(messages_path) = self.messages_path {
            config = config.with_messages_path(messages_path)?;
        }
        if let Some(version) = self.api_version {
            config = config.with_api_version(version);
        }

        Ok(crate::anthropic::Anthropic::new(
            self.client.build_executor()?,
            config,
        ))
    }
}

/// Gemini Provider 构建器。
#[cfg(feature = "gemini")]
pub struct GeminiBuilder {
    client: ClientBuilder,
    api_key: Option<String>,
    base_url: Option<String>,
}

#[cfg(feature = "gemini")]
impl GeminiBuilder {
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

    /// 注入日志实现；需配合 [`GeminiBuilder::log_config`] 打开 scope。
    #[must_use]
    pub fn logger(mut self, logger: Arc<dyn Logger>) -> Self {
        self.client = self.client.logger(logger);
        self
    }

    /// 设置日志过滤配置。
    #[must_use]
    pub fn log_config(mut self, config: LogConfig) -> Self {
        self.client = self.client.log_config(config);
        self
    }

    /// 构建可复用的 Gemini 客户端。
    pub fn build(self) -> Result<crate::gemini::Gemini, AirxError> {
        let api_key = self.api_key.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("Gemini api_key 必填"))
        })?;

        if api_key.trim().is_empty() {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "Gemini api_key 不能为空",
            )));
        }

        let mut config = airx_protocol::gemini::GeminiConfig::new(api_key);
        if let Some(base_url) = self.base_url {
            config = config.with_base_url(base_url)?;
        }

        Ok(crate::gemini::Gemini::new(
            self.client.build_executor()?,
            config,
        ))
    }
}

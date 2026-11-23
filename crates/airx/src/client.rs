//! Client 门面。

use std::sync::Arc;

#[cfg(any(feature = "openai", feature = "anthropic"))]
use airx_core::AirxError;
#[cfg(any(feature = "openai", feature = "anthropic"))]
use airx_core::error::ConfigurationError;
use airx_core::http::HeaderMap;
#[cfg(feature = "anthropic")]
use airx_protocol::anthropic::AnthropicConfig;
#[cfg(feature = "openai")]
use airx_protocol::openai::OpenAiConfig;

#[cfg(feature = "anthropic")]
use crate::anthropic::Anthropic;
use crate::builder::ClientBuilder;
use crate::executor::Executor;
#[cfg(feature = "openai")]
use crate::openai::OpenAi;

/// 持有执行配置与各 Provider 配置的客户端。
pub struct Client {
    pub(crate) executor: Arc<Executor>,
    #[cfg(feature = "anthropic")]
    pub(crate) anthropic: Option<AnthropicConfig>,
    #[cfg(feature = "openai")]
    pub(crate) openai: Option<OpenAiConfig>,
}

impl Client {
    /// 创建构建器。
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// 进入 OpenAI Provider。
    ///
    /// 返回 `Result`：Client 可能未被配置为该 Provider。
    #[cfg(feature = "openai")]
    pub fn openai(&self) -> Result<OpenAi<'_>, AirxError> {
        let config = self.openai.as_ref().ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new(
                "该 Client 未配置 OpenAI Provider，请使用 Client::builder().openai()",
            ))
        })?;
        Ok(OpenAi::new(self, config))
    }

    /// 进入 Anthropic Provider。
    #[cfg(feature = "anthropic")]
    pub fn anthropic(&self) -> Result<Anthropic<'_>, AirxError> {
        let config = self.anthropic.as_ref().ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new(
                "该 Client 未配置 Anthropic Provider，请使用 Client::builder().anthropic()",
            ))
        })?;
        Ok(Anthropic::new(self, config))
    }

    /// 执行级默认 Header。
    pub fn default_headers(&self) -> &HeaderMap {
        self.executor.default_headers()
    }
}

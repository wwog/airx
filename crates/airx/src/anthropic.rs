//! Anthropic 客户端。
//!
//! 由 `Client::anthropic()` / `Client::builder().anthropic()` 构建，
//! 持有执行器与配置，可以复用发起任意多次请求。

use std::sync::Arc;

use airx_core::AirxError;
use airx_protocol::anthropic::AnthropicConfig;
use airx_protocol::anthropic::messages::{MessagesEndpoint, MessagesRequest, MessagesResponse};

use crate::api::ApiClient;
use crate::executor::Executor;
#[cfg(feature = "stream")]
use crate::stream::ProtocolStream;

/// 已定性的 Anthropic 客户端。
#[derive(Clone)]
pub struct Anthropic {
    executor: Arc<Executor>,
    config: AnthropicConfig,
}

impl Anthropic {
    pub(crate) fn new(executor: Arc<Executor>, config: AnthropicConfig) -> Self {
        Self { executor, config }
    }

    /// Provider 配置。
    pub fn config(&self) -> &AnthropicConfig {
        &self.config
    }

    /// Messages API（使用构建时的路径配置）。
    pub fn messages(&self) -> Messages {
        self.messages_with(&self.config)
    }

    /// Messages API，本次调用临时覆盖路径。
    ///
    /// 路径可以是相对路径（拼接到 `base_url`）或完整 URL。
    pub fn messages_at(&self, path: impl AsRef<str>) -> Result<Messages, AirxError> {
        let config = self.config.clone().with_messages_path(path)?;
        Ok(self.messages_with(&config))
    }

    fn messages_with(&self, config: &AnthropicConfig) -> Messages {
        Messages {
            api: ApiClient::new(
                MessagesEndpoint::new(config.clone()),
                Arc::clone(&self.executor),
            ),
        }
    }
}

/// Messages 调用入口。
#[derive(Clone)]
pub struct Messages {
    api: ApiClient<MessagesEndpoint>,
}

impl Messages {
    /// 创建一次非流式消息。
    pub async fn create(&self, request: MessagesRequest) -> Result<MessagesResponse, AirxError> {
        self.api.create(&request).await
    }

    /// 端点引用。
    pub fn endpoint(&self) -> &MessagesEndpoint {
        self.api.endpoint()
    }

    /// 创建一次流式消息。
    #[cfg(feature = "stream")]
    pub async fn stream(
        &self,
        mut request: MessagesRequest,
    ) -> Result<ProtocolStream<MessagesEndpoint>, AirxError> {
        request.stream = Some(true);
        self.api.stream(&request).await
    }
}

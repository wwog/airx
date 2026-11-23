//! Anthropic Provider 入口。

use std::sync::Arc;

use airx_core::AirxError;
use airx_protocol::anthropic::AnthropicConfig;
use airx_protocol::anthropic::messages::{MessagesEndpoint, MessagesRequest, MessagesResponse};

use crate::api::ApiClient;
use crate::client::Client;
#[cfg(feature = "stream")]
use crate::stream::ProtocolStream;

/// Anthropic Provider 视图。
pub struct Anthropic<'a> {
    client: &'a Client,
    config: &'a AnthropicConfig,
}

impl<'a> Anthropic<'a> {
    pub(crate) fn new(client: &'a Client, config: &'a AnthropicConfig) -> Self {
        Self { client, config }
    }

    /// Provider 配置。
    pub fn config(&self) -> &AnthropicConfig {
        self.config
    }

    /// Messages API。
    pub fn messages(&self) -> Messages {
        Messages {
            api: ApiClient::new(
                MessagesEndpoint::new(self.config.clone()),
                Arc::clone(&self.client.executor),
            ),
        }
    }
}

/// Messages 调用入口。
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

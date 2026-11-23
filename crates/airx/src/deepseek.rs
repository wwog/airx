//! DeepSeek Provider 入口。

use std::sync::Arc;

use airx_core::AirxError;
use airx_protocol::deepseek::DeepSeekConfig;
use airx_protocol::deepseek::chat::ChatEndpoint;
use airx_protocol::openai::chat::{ChatCompletionRequest, ChatCompletionResponse};

use crate::api::ApiClient;
use crate::client::Client;
#[cfg(feature = "stream")]
use crate::stream::ProtocolStream;

/// DeepSeek Provider 视图。
pub struct DeepSeek<'a> {
    client: &'a Client,
    config: &'a DeepSeekConfig,
}

impl<'a> DeepSeek<'a> {
    pub(crate) fn new(client: &'a Client, config: &'a DeepSeekConfig) -> Self {
        Self { client, config }
    }

    /// Provider 配置。
    pub fn config(&self) -> &DeepSeekConfig {
        self.config
    }

    /// Chat API。
    pub fn chat(&self) -> DeepSeekChat {
        DeepSeekChat {
            api: ApiClient::new(
                ChatEndpoint::new(self.config.clone()),
                Arc::clone(&self.client.executor),
            ),
        }
    }
}

/// DeepSeek Chat 调用入口。
pub struct DeepSeekChat {
    api: ApiClient<ChatEndpoint>,
}

impl DeepSeekChat {
    /// 创建一次非流式补全。
    pub async fn create(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, AirxError> {
        self.api.create(&request).await
    }

    /// 端点引用。
    pub fn endpoint(&self) -> &ChatEndpoint {
        self.api.endpoint()
    }

    /// 创建一次流式补全。
    #[cfg(feature = "stream")]
    pub async fn stream(
        &self,
        mut request: ChatCompletionRequest,
    ) -> Result<ProtocolStream<ChatEndpoint>, AirxError> {
        request.stream = Some(true);
        if request.stream_options.is_none() {
            request.stream_options = Some(airx_protocol::openai::chat::StreamOptions {
                include_usage: true,
            });
        }
        self.api.stream(&request).await
    }
}

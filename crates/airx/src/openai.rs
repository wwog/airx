//! OpenAI Provider 入口。

use std::sync::Arc;

use airx_core::AirxError;
use airx_protocol::openai::OpenAiConfig;
use airx_protocol::openai::chat::{
    ChatCompletionRequest, ChatCompletionResponse, ChatEndpoint, StreamOptions,
};
use airx_protocol::openai::responses::{ResponsesEndpoint, ResponsesRequest, ResponsesResponse};

use crate::api::ApiClient;
use crate::client::Client;
#[cfg(feature = "stream")]
use crate::stream::ProtocolStream;

/// OpenAI Provider 视图。
pub struct OpenAi<'a> {
    client: &'a Client,
    config: &'a OpenAiConfig,
}

impl<'a> OpenAi<'a> {
    pub(crate) fn new(client: &'a Client, config: &'a OpenAiConfig) -> Self {
        Self { client, config }
    }

    /// Provider 配置。
    pub fn config(&self) -> &OpenAiConfig {
        self.config
    }

    /// Chat Completions API。
    pub fn chat(&self) -> Chat {
        Chat {
            api: ApiClient::new(
                ChatEndpoint::new(self.config.clone()),
                Arc::clone(&self.client.executor),
            ),
        }
    }

    /// Responses API。
    pub fn responses(&self) -> Responses {
        Responses {
            api: ApiClient::new(
                ResponsesEndpoint::new(self.config.clone()),
                Arc::clone(&self.client.executor),
            ),
        }
    }
}

/// Chat Completions 调用入口。
pub struct Chat {
    api: ApiClient<ChatEndpoint>,
}

impl Chat {
    /// 创建一次非流式对话补全。
    pub async fn create(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, AirxError> {
        self.api.create(&request).await
    }

    /// 端点引用（可用于自定义编码 / 逃生通道）。
    pub fn endpoint(&self) -> &ChatEndpoint {
        self.api.endpoint()
    }

    /// 创建一次流式对话补全。
    ///
    /// 自动设置 `stream: true` 并请求最后一个 chunk 携带 usage。
    #[cfg(feature = "stream")]
    pub async fn stream(
        &self,
        mut request: ChatCompletionRequest,
    ) -> Result<ProtocolStream<ChatEndpoint>, AirxError> {
        request.stream = Some(true);
        if request.stream_options.is_none() {
            request.stream_options = Some(StreamOptions {
                include_usage: true,
            });
        }
        self.api.stream(&request).await
    }
}

/// Responses 调用入口。
pub struct Responses {
    api: ApiClient<ResponsesEndpoint>,
}

impl Responses {
    /// 创建一次非流式响应。
    pub async fn create(&self, request: ResponsesRequest) -> Result<ResponsesResponse, AirxError> {
        self.api.create(&request).await
    }

    /// 端点引用。
    pub fn endpoint(&self) -> &ResponsesEndpoint {
        self.api.endpoint()
    }

    /// 创建一次流式响应。
    #[cfg(feature = "stream")]
    pub async fn stream(
        &self,
        mut request: ResponsesRequest,
    ) -> Result<ProtocolStream<ResponsesEndpoint>, AirxError> {
        request.stream = Some(true);
        self.api.stream(&request).await
    }
}

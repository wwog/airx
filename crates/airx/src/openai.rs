//! OpenAI 客户端。
//!
//! 由 `Client::openai()` / `Client::builder().openai()` 构建，持有执行器与配置，
//! 可以复用发起任意多次请求。

use std::sync::Arc;

use airx_core::AirxError;
use airx_protocol::openai::OpenAiConfig;
use airx_protocol::openai::chat::{
    ChatCompletionRequest, ChatCompletionResponse, ChatEndpoint, StreamOptions,
};
use airx_protocol::openai::responses::{ResponsesEndpoint, ResponsesRequest, ResponsesResponse};

use crate::api::ApiClient;
use crate::executor::Executor;
#[cfg(feature = "stream")]
use crate::stream::ProtocolStream;

/// 已定性的 OpenAI 客户端。
#[derive(Clone)]
pub struct OpenAi {
    executor: Arc<Executor>,
    config: OpenAiConfig,
}

impl OpenAi {
    pub(crate) fn new(executor: Arc<Executor>, config: OpenAiConfig) -> Self {
        Self { executor, config }
    }

    /// Provider 配置。
    pub fn config(&self) -> &OpenAiConfig {
        &self.config
    }

    /// Chat Completions API（使用构建时的路径配置）。
    pub fn chat(&self) -> Chat {
        self.chat_with(&self.config)
    }

    /// Chat Completions API，本次调用临时覆盖路径。
    ///
    /// 路径可以是相对路径（拼接到 `base_url`）或完整 URL。
    pub fn chat_at(&self, path: impl AsRef<str>) -> Result<Chat, AirxError> {
        let config = self.config.clone().with_chat_path(path)?;
        Ok(self.chat_with(&config))
    }

    /// Responses API（使用构建时的路径配置）。
    pub fn responses(&self) -> Responses {
        self.responses_with(&self.config)
    }

    /// Responses API，本次调用临时覆盖路径。
    pub fn responses_at(&self, path: impl AsRef<str>) -> Result<Responses, AirxError> {
        let config = self.config.clone().with_responses_path(path)?;
        Ok(self.responses_with(&config))
    }

    fn chat_with(&self, config: &OpenAiConfig) -> Chat {
        Chat {
            api: ApiClient::new(
                ChatEndpoint::new(config.clone()),
                Arc::clone(&self.executor),
            ),
        }
    }

    fn responses_with(&self, config: &OpenAiConfig) -> Responses {
        Responses {
            api: ApiClient::new(
                ResponsesEndpoint::new(config.clone()),
                Arc::clone(&self.executor),
            ),
        }
    }
}

/// Chat Completions 调用入口。
#[derive(Clone)]
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
#[derive(Clone)]
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

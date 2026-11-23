//! Anthropic 客户端。
//!
//! 由 `Client::anthropic()` / `Client::builder().anthropic()` 构建，
//! 持有执行器与配置，可以复用发起任意多次请求。

use std::sync::Arc;

use airx_core::AirxError;
use airx_core::model::ModelId;
use airx_protocol::anthropic::AnthropicConfig;
use airx_protocol::anthropic::messages::{
    CountTokensEndpoint, CountTokensRequest, CountTokensResponse, MessagesEndpoint,
    MessagesRequest, MessagesResponse,
};
use airx_protocol::anthropic::models::{
    ListModelsEndpoint, ListModelsRequest, Model, ModelList, RetrieveModelEndpoint,
};
use serde::de::DeserializeOwned;

use crate::api::ApiClient;
use crate::executor::Executor;
use crate::options::RequestOptions;
use crate::response::{WithMeta, parse_json};
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

    /// Models API。
    pub fn models(&self) -> AnthropicModels {
        AnthropicModels {
            executor: Arc::clone(&self.executor),
            config: self.config.clone(),
            options: RequestOptions::default(),
        }
    }

    fn messages_with(&self, config: &AnthropicConfig) -> Messages {
        Messages {
            api: ApiClient::new(
                MessagesEndpoint::new(config.clone()),
                Arc::clone(&self.executor),
            ),
            options: RequestOptions::default(),
        }
    }
}

/// Messages 调用入口。
#[derive(Clone)]
pub struct Messages {
    api: ApiClient<MessagesEndpoint>,
    options: RequestOptions,
}

impl Messages {
    /// 设置本次调用的请求选项（Header / 幂等键）。
    #[must_use]
    pub fn with_options(mut self, options: RequestOptions) -> Self {
        self.options = options;
        self
    }

    /// 创建一次非流式消息。
    pub async fn create(&self, request: MessagesRequest) -> Result<MessagesResponse, AirxError> {
        self.api.create_with(&request, &self.options).await
    }

    /// 创建一次非流式消息，同时返回 HTTP 元数据。
    pub async fn create_with_meta(
        &self,
        request: MessagesRequest,
    ) -> Result<WithMeta<MessagesResponse>, AirxError> {
        self.api.create_with_meta(&request, &self.options).await
    }

    /// 创建并解析结构化输出。
    pub async fn parse_as<T: DeserializeOwned>(
        &self,
        request: MessagesRequest,
    ) -> Result<T, AirxError> {
        let response = self.create(request).await?;
        parse_json(&response.text())
    }

    /// 统计输入 token。
    pub async fn count_tokens(
        &self,
        request: CountTokensRequest,
    ) -> Result<CountTokensResponse, AirxError> {
        let api = ApiClient::new(
            CountTokensEndpoint::new(self.api.endpoint().config().clone()),
            self.api.executor(),
        );
        api.create_with(&request, &self.options).await
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
        self.api.stream_with(&request, &self.options).await
    }
}

/// Anthropic Models 调用入口。
#[derive(Clone)]
pub struct AnthropicModels {
    executor: Arc<Executor>,
    config: AnthropicConfig,
    options: RequestOptions,
}

impl AnthropicModels {
    /// 设置本次调用的请求选项。
    #[must_use]
    pub fn with_options(mut self, options: RequestOptions) -> Self {
        self.options = options;
        self
    }

    /// 获取一页模型。
    pub async fn list(&self, request: ListModelsRequest) -> Result<ModelList, AirxError> {
        let api = ApiClient::new(
            ListModelsEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        api.create_with(&request, &self.options).await
    }

    /// 获取单个模型。
    pub async fn retrieve(&self, model: impl Into<ModelId>) -> Result<Model, AirxError> {
        let api = ApiClient::new(
            RetrieveModelEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        let model = model.into();
        api.create_with(&model, &self.options).await
    }

    /// 自动翻页获取全部模型。
    pub async fn list_all(&self, request: ListModelsRequest) -> Result<Vec<Model>, AirxError> {
        let mut all = Vec::new();
        let mut after_id = request.after_id.clone();

        loop {
            let page = self
                .list(ListModelsRequest {
                    limit: request.limit,
                    after_id: after_id.clone(),
                    before_id: request.before_id.clone(),
                })
                .await?;

            let next = page.next_cursor().map(str::to_owned);
            all.extend(page.data);
            match next {
                Some(cursor) if Some(&cursor) != after_id.as_ref() => after_id = Some(cursor),
                _ => break,
            }
        }

        Ok(all)
    }
}

//! Gemini 客户端。
//!
//! 由 `Client::gemini()` / `Client::builder().gemini()` 构建，持有执行器与配置，
//! 可以复用；模型仍按 Gemini 的路径约定在每次调用时指定。

use std::sync::Arc;

use airx_core::AirxError;
use airx_core::model::ModelId;
use airx_protocol::gemini::GeminiConfig;
use airx_protocol::gemini::generate_content::{
    CountTokensEndpoint, CountTokensRequest, CountTokensResponse, GenerateContentEndpoint,
    GenerateContentRequest, GenerateContentResponse,
};
use airx_protocol::gemini::models::{
    GetModelEndpoint, ListModelsEndpoint, ListModelsRequest, Model, ModelList,
};
use serde::de::DeserializeOwned;

use crate::api::ApiClient;
use crate::executor::Executor;
use crate::options::RequestOptions;
use crate::response::{WithMeta, parse_json};
#[cfg(feature = "stream")]
use crate::stream::ProtocolStream;

/// 已定性的 Gemini 客户端。
#[derive(Clone)]
pub struct Gemini {
    executor: Arc<Executor>,
    config: GeminiConfig,
}

impl Gemini {
    pub(crate) fn new(executor: Arc<Executor>, config: GeminiConfig) -> Self {
        Self { executor, config }
    }

    /// Provider 配置。
    pub fn config(&self) -> &GeminiConfig {
        &self.config
    }

    /// 指定模型，进入 `generateContent` 调用。
    pub fn generate_content(&self, model: impl Into<ModelId>) -> GenerateContent {
        GenerateContent {
            executor: Arc::clone(&self.executor),
            config: self.config.clone(),
            model: model.into(),
            options: RequestOptions::default(),
        }
    }

    /// Models API。
    pub fn models(&self) -> GeminiModels {
        GeminiModels {
            executor: Arc::clone(&self.executor),
            config: self.config.clone(),
            options: RequestOptions::default(),
        }
    }
}

/// `generateContent` 调用入口。
///
/// Gemini 把模型放在路径里，因此模型在进入 API 时指定。
#[derive(Clone)]
pub struct GenerateContent {
    executor: Arc<Executor>,
    config: GeminiConfig,
    model: ModelId,
    options: RequestOptions,
}

impl GenerateContent {
    /// 模型标识。
    pub fn model(&self) -> &ModelId {
        &self.model
    }

    /// 设置本次调用的请求选项（Header / 幂等键）。
    #[must_use]
    pub fn with_options(mut self, options: RequestOptions) -> Self {
        self.options = options;
        self
    }

    /// 非流式端点引用（可用于自定义编码 / 逃生通道）。
    pub fn endpoint(&self) -> GenerateContentEndpoint {
        GenerateContentEndpoint::new(self.config.clone(), self.model.clone())
    }

    /// 流式端点引用。
    #[cfg(feature = "stream")]
    pub fn stream_endpoint(&self) -> GenerateContentEndpoint {
        GenerateContentEndpoint::streaming(self.config.clone(), self.model.clone())
    }

    /// 创建一次非流式生成。
    pub async fn create(
        &self,
        request: GenerateContentRequest,
    ) -> Result<GenerateContentResponse, AirxError> {
        let api = ApiClient::new(self.endpoint(), Arc::clone(&self.executor));
        api.create_with(&request, &self.options).await
    }

    /// 创建一次非流式生成，同时返回 HTTP 元数据。
    pub async fn create_with_meta(
        &self,
        request: GenerateContentRequest,
    ) -> Result<WithMeta<GenerateContentResponse>, AirxError> {
        let api = ApiClient::new(self.endpoint(), Arc::clone(&self.executor));
        api.create_with_meta(&request, &self.options).await
    }

    /// 创建并解析结构化输出。
    pub async fn parse_as<T: DeserializeOwned>(
        &self,
        request: GenerateContentRequest,
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
            CountTokensEndpoint::new(self.config.clone(), self.model.clone()),
            Arc::clone(&self.executor),
        );
        api.create_with(&request, &self.options).await
    }

    /// 创建一次流式生成。
    #[cfg(feature = "stream")]
    pub async fn stream(
        &self,
        request: GenerateContentRequest,
    ) -> Result<ProtocolStream<GenerateContentEndpoint>, AirxError> {
        let api = ApiClient::new(self.stream_endpoint(), Arc::clone(&self.executor));
        api.stream_with(&request, &self.options).await
    }
}

/// Gemini Models 调用入口。
#[derive(Clone)]
pub struct GeminiModels {
    executor: Arc<Executor>,
    config: GeminiConfig,
    options: RequestOptions,
}

impl GeminiModels {
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
    pub async fn get(&self, model: impl Into<ModelId>) -> Result<Model, AirxError> {
        let api = ApiClient::new(
            GetModelEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        let model = model.into();
        api.create_with(&model, &self.options).await
    }

    /// 自动翻页获取全部模型。
    pub async fn list_all(&self, request: ListModelsRequest) -> Result<Vec<Model>, AirxError> {
        let mut all = Vec::new();
        let mut page_token = request.page_token.clone();

        loop {
            let page = self
                .list(ListModelsRequest {
                    page_size: request.page_size,
                    page_token: page_token.clone(),
                })
                .await?;

            let next = page.next_cursor().map(str::to_owned);
            all.extend(page.models);
            match next {
                Some(cursor) if Some(&cursor) != page_token.as_ref() => page_token = Some(cursor),
                _ => break,
            }
        }

        Ok(all)
    }
}

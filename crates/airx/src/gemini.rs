//! Gemini 客户端。
//!
//! 由 `Client::gemini()` / `Client::builder().gemini()` 构建，持有执行器与配置，
//! 可以复用；模型仍按 Gemini 的路径约定在每次调用时指定。

use std::sync::Arc;

use airx_core::AirxError;
use airx_core::http::{HttpRequest, HttpResponse};
use airx_core::model::ModelId;
use airx_protocol::gemini::GeminiConfig;
use airx_protocol::gemini::caches::{
    CachedContent, CachedContentDelete, CachedContentList, CreateCachedContentEndpoint,
    CreateCachedContentRequest, DeleteCachedContentEndpoint, GetCachedContentEndpoint,
    ListCachedContentsEndpoint, ListCachedContentsRequest, UpdateCachedContentEndpoint,
    UpdateCachedContentRequest,
};
use airx_protocol::gemini::embeddings::{
    BatchEmbedContentsEndpoint, BatchEmbedContentsRequest, BatchEmbedContentsResponse,
    BatchEmbedItem, EmbedContentEndpoint, EmbedContentRequest, EmbedContentResponse,
};
use airx_protocol::gemini::generate_content::{
    CountTokensEndpoint, CountTokensRequest, CountTokensResponse, GenerateContentEndpoint,
    GenerateContentRequest, GenerateContentResponse,
};
use airx_protocol::gemini::models::{
    ComputeTokensEndpoint, ComputeTokensRequest, ComputeTokensResponse, GetModelEndpoint,
    ListModelsEndpoint, ListModelsRequest, Model, ModelList,
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

    /// 直接执行预编码请求，复用客户端的认证、Header、超时与重试。
    pub async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, AirxError> {
        self.executor.execute(request).await
    }

    /// 与 [`Gemini::execute`] 相同，并应用单次请求选项。
    pub async fn execute_with(
        &self,
        request: HttpRequest,
        options: &RequestOptions,
    ) -> Result<HttpResponse, AirxError> {
        self.executor.execute_with(request, options).await
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

    /// Caches API（`cachedContents` 的创建 / 读取 / 续期 / 删除）。
    pub fn caches(&self) -> GeminiCaches {
        GeminiCaches {
            executor: Arc::clone(&self.executor),
            config: self.config.clone(),
            options: RequestOptions::default(),
        }
    }

    /// Embeddings API（模型在路径中）。
    pub fn embed_content(&self, model: impl Into<ModelId>) -> GeminiEmbeddings {
        GeminiEmbeddings {
            executor: Arc::clone(&self.executor),
            config: self.config.clone(),
            model: model.into(),
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

    /// 获取单个模型（与其它 Provider 的 `retrieve` 命名对齐）。
    pub async fn retrieve(&self, model: impl Into<ModelId>) -> Result<Model, AirxError> {
        self.get(model).await
    }

    /// 统计文本里的 token（`computeTokens`，返回 token ID 与文本）。
    pub async fn compute_tokens(
        &self,
        model: impl Into<ModelId>,
        request: ComputeTokensRequest,
    ) -> Result<ComputeTokensResponse, AirxError> {
        let api = ApiClient::new(
            ComputeTokensEndpoint::new(self.config.clone(), model.into()),
            Arc::clone(&self.executor),
        );
        api.create_with(&request, &self.options).await
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

/// Gemini Embeddings 调用入口。
#[derive(Clone)]
pub struct GeminiEmbeddings {
    executor: Arc<Executor>,
    config: GeminiConfig,
    model: ModelId,
    options: RequestOptions,
}

impl GeminiEmbeddings {
    /// 设置本次调用的请求选项。
    #[must_use]
    pub fn with_options(mut self, options: RequestOptions) -> Self {
        self.options = options;
        self
    }

    /// 单条嵌入。
    pub async fn create(
        &self,
        request: EmbedContentRequest,
    ) -> Result<EmbedContentResponse, AirxError> {
        let api = ApiClient::new(
            EmbedContentEndpoint::new(self.config.clone(), self.model.clone()),
            Arc::clone(&self.executor),
        );
        api.create_with(&request, &self.options).await
    }

    /// 批量嵌入；每条请求自动带上当前模型。
    pub async fn batch(
        &self,
        requests: impl IntoIterator<Item = EmbedContentRequest>,
    ) -> Result<BatchEmbedContentsResponse, AirxError> {
        let mut items = Vec::new();
        for request in requests {
            items.push(BatchEmbedItem::from_request(request, &self.model)?);
        }
        let request = BatchEmbedContentsRequest {
            requests: items,
            extensions: airx_core::json::Extensions::new(),
        };
        let api = ApiClient::new(
            BatchEmbedContentsEndpoint::new(self.config.clone(), self.model.clone()),
            Arc::clone(&self.executor),
        );
        api.create_with(&request, &self.options).await
    }
}

/// Gemini Caches 调用入口（`cachedContents` CRUD）。
#[derive(Clone)]
pub struct GeminiCaches {
    executor: Arc<Executor>,
    config: GeminiConfig,
    options: RequestOptions,
}

impl GeminiCaches {
    /// 设置本次调用的请求选项。
    #[must_use]
    pub fn with_options(mut self, options: RequestOptions) -> Self {
        self.options = options;
        self
    }

    /// 创建缓存。
    pub async fn create(
        &self,
        request: CreateCachedContentRequest,
    ) -> Result<CachedContent, AirxError> {
        let api = ApiClient::new(
            CreateCachedContentEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        api.create_with(&request, &self.options).await
    }

    /// 读取缓存（名称可带或不带 `cachedContents/` 前缀）。
    pub async fn get(&self, name: impl Into<String>) -> Result<CachedContent, AirxError> {
        let api = ApiClient::new(
            GetCachedContentEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        api.create_with(&name.into(), &self.options).await
    }

    /// 列出一页缓存。
    pub async fn list(
        &self,
        request: ListCachedContentsRequest,
    ) -> Result<CachedContentList, AirxError> {
        let api = ApiClient::new(
            ListCachedContentsEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        api.create_with(&request, &self.options).await
    }

    /// 续期缓存（只允许更新过期时间）。
    pub async fn update(
        &self,
        name: impl Into<String>,
        request: UpdateCachedContentRequest,
    ) -> Result<CachedContent, AirxError> {
        let api = ApiClient::new(
            UpdateCachedContentEndpoint::new(self.config.clone(), name),
            Arc::clone(&self.executor),
        );
        api.create_with(&request, &self.options).await
    }

    /// 删除缓存。
    pub async fn delete(&self, name: impl Into<String>) -> Result<CachedContentDelete, AirxError> {
        let api = ApiClient::new(
            DeleteCachedContentEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        api.create_with(&name.into(), &self.options).await
    }

    /// 自动翻页获取全部缓存。
    pub async fn list_all(
        &self,
        request: ListCachedContentsRequest,
    ) -> Result<Vec<CachedContent>, AirxError> {
        let mut all = Vec::new();
        let mut page_token = request.page_token.clone();

        loop {
            let page = self
                .list(ListCachedContentsRequest {
                    page_size: request.page_size,
                    page_token: page_token.clone(),
                })
                .await?;

            let next = page.next_cursor().map(str::to_owned);
            all.extend(page.cached_contents);
            match next {
                Some(cursor) if Some(&cursor) != page_token.as_ref() => page_token = Some(cursor),
                _ => break,
            }
        }

        Ok(all)
    }
}

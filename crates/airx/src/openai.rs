//! OpenAI 客户端。
//!
//! 由 `Client::openai()` / `Client::builder().openai()` 构建，持有执行器与配置，
//! 可以复用发起任意多次请求。

use std::sync::Arc;

use airx_core::AirxError;
use airx_core::http::{HttpRequest, HttpResponse};
use airx_core::model::ModelId;
use airx_protocol::openai::OpenAiConfig;
use airx_protocol::openai::batches::{
    BatchEndpoint, BatchList, BatchObject, CreateBatchEndpoint, CreateBatchRequest,
    ListBatchesEndpoint, ListBatchesRequest,
};
use airx_protocol::openai::chat::{
    ChatCompletionRequest, ChatCompletionResponse, ChatEndpoint, StreamOptions,
};
use airx_protocol::openai::embeddings::{EmbeddingRequest, EmbeddingResponse, EmbeddingsEndpoint};
use airx_protocol::openai::files::{
    DeleteFileEndpoint, FileContentEndpoint, FileDeleteResponse, FileEndpoint, FileList,
    FileObject, ListFilesEndpoint, ListFilesRequest, UploadFileEndpoint, UploadFileRequest,
};
use airx_protocol::openai::models::{
    ListModels, ListModelsEndpoint, Model, ModelList, RetrieveModelEndpoint,
};
use airx_protocol::openai::responses::{
    CancelResponseEndpoint, CountInputTokensEndpoint, DeleteResponse, DeleteResponseEndpoint,
    InputItemList, InputTokensCount, InputTokensCountRequest, ListInputItemsEndpoint,
    ListInputItemsRequest, ResponsesEndpoint, ResponsesRequest, ResponsesResponse,
    RetrieveResponseEndpoint,
};
use bytes::Bytes;
use serde::de::DeserializeOwned;

use crate::api::ApiClient;
use crate::executor::Executor;
use crate::options::RequestOptions;
use crate::response::{WithMeta, parse_json};
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

    /// 直接执行预编码请求，复用客户端的认证、Header、超时与重试。
    pub async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, AirxError> {
        self.executor.execute(request).await
    }

    /// 与 [`OpenAi::execute`] 相同，并应用单次请求选项。
    pub async fn execute_with(
        &self,
        request: HttpRequest,
        options: &RequestOptions,
    ) -> Result<HttpResponse, AirxError> {
        self.executor.execute_with(request, options).await
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

    /// Models API。
    pub fn models(&self) -> OpenAiModels {
        OpenAiModels {
            executor: Arc::clone(&self.executor),
            config: self.config.clone(),
            options: RequestOptions::default(),
        }
    }

    /// Embeddings API。
    pub fn embeddings(&self) -> OpenAiEmbeddings {
        OpenAiEmbeddings {
            executor: Arc::clone(&self.executor),
            config: self.config.clone(),
            options: RequestOptions::default(),
        }
    }

    /// Files API。
    pub fn files(&self) -> OpenAiFiles {
        OpenAiFiles {
            executor: Arc::clone(&self.executor),
            config: self.config.clone(),
            options: RequestOptions::default(),
        }
    }

    /// Batches API。
    pub fn batches(&self) -> OpenAiBatches {
        OpenAiBatches {
            executor: Arc::clone(&self.executor),
            config: self.config.clone(),
            options: RequestOptions::default(),
        }
    }

    fn chat_with(&self, config: &OpenAiConfig) -> Chat {
        Chat {
            api: ApiClient::new(
                ChatEndpoint::new(config.clone()),
                Arc::clone(&self.executor),
            ),
            options: RequestOptions::default(),
        }
    }

    fn responses_with(&self, config: &OpenAiConfig) -> Responses {
        Responses {
            api: ApiClient::new(
                ResponsesEndpoint::new(config.clone()),
                Arc::clone(&self.executor),
            ),
            options: RequestOptions::default(),
        }
    }
}

/// Chat Completions 调用入口。
#[derive(Clone)]
pub struct Chat {
    api: ApiClient<ChatEndpoint>,
    options: RequestOptions,
}

impl Chat {
    /// 设置本次调用的请求选项（Header / 幂等键）。
    #[must_use]
    pub fn with_options(mut self, options: RequestOptions) -> Self {
        self.options = options;
        self
    }

    /// 创建一次非流式对话补全。
    pub async fn create(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, AirxError> {
        self.api.create_with(&request, &self.options).await
    }

    /// 创建一次非流式对话补全，同时返回 HTTP 元数据。
    pub async fn create_with_meta(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<WithMeta<ChatCompletionResponse>, AirxError> {
        self.api.create_with_meta(&request, &self.options).await
    }

    /// 创建并解析结构化输出。
    ///
    /// 调用方需自行设置 `response_format`；本方法只负责把文本解析为 `T`。
    pub async fn parse_as<T: DeserializeOwned>(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<T, AirxError> {
        let response = self.create(request).await?;
        parse_json(response.first_content().unwrap_or_default())
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
        self.api.stream_with(&request, &self.options).await
    }
}

/// Responses 调用入口。
#[derive(Clone)]
pub struct Responses {
    api: ApiClient<ResponsesEndpoint>,
    options: RequestOptions,
}

impl Responses {
    /// 设置本次调用的请求选项（Header / 幂等键）。
    #[must_use]
    pub fn with_options(mut self, options: RequestOptions) -> Self {
        self.options = options;
        self
    }

    /// 创建一次非流式响应。
    pub async fn create(&self, request: ResponsesRequest) -> Result<ResponsesResponse, AirxError> {
        self.api.create_with(&request, &self.options).await
    }

    /// 创建一次非流式响应，同时返回 HTTP 元数据。
    pub async fn create_with_meta(
        &self,
        request: ResponsesRequest,
    ) -> Result<WithMeta<ResponsesResponse>, AirxError> {
        self.api.create_with_meta(&request, &self.options).await
    }

    /// 创建并解析结构化输出（读取 `output_text`）。
    pub async fn parse_as<T: DeserializeOwned>(
        &self,
        request: ResponsesRequest,
    ) -> Result<T, AirxError> {
        let response = self.create(request).await?;
        parse_json(&response.output_text())
    }

    /// 读取已有响应。
    pub async fn retrieve(&self, id: impl Into<String>) -> Result<ResponsesResponse, AirxError> {
        let api = ApiClient::new(
            RetrieveResponseEndpoint::new(self.config()),
            self.api.executor(),
        );
        let id = id.into();
        api.create_with(&id, &self.options).await
    }

    /// 取消后台响应。
    pub async fn cancel(&self, id: impl Into<String>) -> Result<ResponsesResponse, AirxError> {
        let api = ApiClient::new(
            CancelResponseEndpoint::new(self.config()),
            self.api.executor(),
        );
        let id = id.into();
        api.create_with(&id, &self.options).await
    }

    /// 删除已存储的响应。
    pub async fn delete(&self, id: impl Into<String>) -> Result<DeleteResponse, AirxError> {
        let api = ApiClient::new(
            DeleteResponseEndpoint::new(self.config()),
            self.api.executor(),
        );
        let id = id.into();
        api.create_with(&id, &self.options).await
    }

    /// 列出响应的输入 item。
    pub async fn input_items(
        &self,
        request: ListInputItemsRequest,
    ) -> Result<InputItemList, AirxError> {
        let api = ApiClient::new(
            ListInputItemsEndpoint::new(self.config()),
            self.api.executor(),
        );
        api.create_with(&request, &self.options).await
    }

    /// 估算输入 token。
    pub async fn input_tokens_count(
        &self,
        request: InputTokensCountRequest,
    ) -> Result<InputTokensCount, AirxError> {
        let api = ApiClient::new(
            CountInputTokensEndpoint::new(self.config()),
            self.api.executor(),
        );
        api.create_with(&request, &self.options).await
    }

    fn config(&self) -> OpenAiConfig {
        self.api.endpoint().config().clone()
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
        self.api.stream_with(&request, &self.options).await
    }
}

/// OpenAI Models 调用入口。
#[derive(Clone)]
pub struct OpenAiModels {
    executor: Arc<Executor>,
    config: OpenAiConfig,
    options: RequestOptions,
}

impl OpenAiModels {
    /// 设置本次调用的请求选项。
    #[must_use]
    pub fn with_options(mut self, options: RequestOptions) -> Self {
        self.options = options;
        self
    }

    /// 列出全部模型（该接口不分页）。
    pub async fn list(&self) -> Result<ModelList, AirxError> {
        let api = ApiClient::new(
            ListModelsEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        api.create_with(&ListModels, &self.options).await
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
}

/// OpenAI Embeddings 调用入口。
#[derive(Clone)]
pub struct OpenAiEmbeddings {
    executor: Arc<Executor>,
    config: OpenAiConfig,
    options: RequestOptions,
}

impl OpenAiEmbeddings {
    /// 设置本次调用的请求选项。
    #[must_use]
    pub fn with_options(mut self, options: RequestOptions) -> Self {
        self.options = options;
        self
    }

    /// 创建向量。
    pub async fn create(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, AirxError> {
        let api = ApiClient::new(
            EmbeddingsEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        api.create_with(&request, &self.options).await
    }
}

/// OpenAI Files 调用入口。
#[derive(Clone)]
pub struct OpenAiFiles {
    executor: Arc<Executor>,
    config: OpenAiConfig,
    options: RequestOptions,
}

impl OpenAiFiles {
    /// 设置本次调用的请求选项。
    #[must_use]
    pub fn with_options(mut self, options: RequestOptions) -> Self {
        self.options = options;
        self
    }

    /// 上传文件（multipart）。
    pub async fn upload(
        &self,
        filename: impl Into<String>,
        purpose: impl Into<String>,
        data: impl Into<Bytes>,
    ) -> Result<FileObject, AirxError> {
        let request = UploadFileRequest::new(filename, purpose, data, crate::multipart::boundary());
        let api = ApiClient::new(
            UploadFileEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        api.create_with(&request, &self.options).await
    }

    /// 列出文件。
    pub async fn list(&self, request: ListFilesRequest) -> Result<FileList, AirxError> {
        let api = ApiClient::new(
            ListFilesEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        api.create_with(&request, &self.options).await
    }

    /// 获取文件元数据。
    pub async fn retrieve(&self, id: impl Into<String>) -> Result<FileObject, AirxError> {
        let api = ApiClient::new(
            FileEndpoint::retrieve(self.config.clone()),
            Arc::clone(&self.executor),
        );
        let id = id.into();
        api.create_with(&id, &self.options).await
    }

    /// 删除文件。
    pub async fn delete(&self, id: impl Into<String>) -> Result<FileDeleteResponse, AirxError> {
        let api = ApiClient::new(
            DeleteFileEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        let id = id.into();
        api.create_with(&id, &self.options).await
    }

    /// 下载文件内容。
    pub async fn content(&self, id: impl Into<String>) -> Result<Bytes, AirxError> {
        let api = ApiClient::new(
            FileContentEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        let id = id.into();
        api.create_with(&id, &self.options).await
    }
}

/// OpenAI Batches 调用入口。
#[derive(Clone)]
pub struct OpenAiBatches {
    executor: Arc<Executor>,
    config: OpenAiConfig,
    options: RequestOptions,
}

impl OpenAiBatches {
    /// 设置本次调用的请求选项。
    #[must_use]
    pub fn with_options(mut self, options: RequestOptions) -> Self {
        self.options = options;
        self
    }

    /// 创建批处理。
    pub async fn create(&self, request: CreateBatchRequest) -> Result<BatchObject, AirxError> {
        let api = ApiClient::new(
            CreateBatchEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        api.create_with(&request, &self.options).await
    }

    /// 获取批处理。
    pub async fn retrieve(&self, id: impl Into<String>) -> Result<BatchObject, AirxError> {
        let api = ApiClient::new(
            BatchEndpoint::retrieve(self.config.clone()),
            Arc::clone(&self.executor),
        );
        let id = id.into();
        api.create_with(&id, &self.options).await
    }

    /// 列出批处理。
    pub async fn list(&self, request: ListBatchesRequest) -> Result<BatchList, AirxError> {
        let api = ApiClient::new(
            ListBatchesEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        api.create_with(&request, &self.options).await
    }

    /// 取消批处理。
    pub async fn cancel(&self, id: impl Into<String>) -> Result<BatchObject, AirxError> {
        let api = ApiClient::new(
            BatchEndpoint::cancel(self.config.clone()),
            Arc::clone(&self.executor),
        );
        let id = id.into();
        api.create_with(&id, &self.options).await
    }
}

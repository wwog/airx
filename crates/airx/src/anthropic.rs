//! Anthropic 客户端。
//!
//! 由 `Client::anthropic()` / `Client::builder().anthropic()` 构建，
//! 持有执行器与配置，可以复用发起任意多次请求。

use std::sync::Arc;

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::http::{HttpRequest, HttpResponse};
use airx_core::model::ModelId;
use airx_protocol::anthropic::AnthropicConfig;
use airx_protocol::anthropic::batches::{
    BatchEndpoint, BatchList, BatchResultLine, BatchResultsEndpoint, BatchResultsTypedEndpoint,
    CreateBatchEndpoint, CreateBatchRequest, DeleteBatchEndpoint, DeletedBatch,
    ListBatchesEndpoint, ListBatchesRequest, MessageBatch,
};
use airx_protocol::anthropic::files::{
    DeleteFileEndpoint, FileContentEndpoint, FileDeleteResponse, FileList, FileMetadata,
    ListFilesEndpoint, ListFilesRequest, RetrieveFileEndpoint, UploadFileEndpoint,
    UploadFileRequest,
};
use airx_protocol::anthropic::messages::{
    CountTokensEndpoint, CountTokensRequest, CountTokensResponse, MessagesEndpoint,
    MessagesRequest, MessagesResponse,
};
use airx_protocol::anthropic::models::{
    ListModelsEndpoint, ListModelsRequest, Model, ModelList, RetrieveModelEndpoint,
};
use bytes::Bytes;
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

    /// 直接执行预编码请求，复用客户端的认证、Header、超时与重试。
    pub async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, AirxError> {
        self.executor.execute(request).await
    }

    /// 与 [`Anthropic::execute`] 相同，并应用单次请求选项。
    pub async fn execute_with(
        &self,
        request: HttpRequest,
        options: &RequestOptions,
    ) -> Result<HttpResponse, AirxError> {
        self.executor.execute_with(request, options).await
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

    /// Files API。
    pub fn files(&self) -> AnthropicFiles {
        AnthropicFiles {
            executor: Arc::clone(&self.executor),
            config: self.config.clone(),
            options: RequestOptions::default(),
        }
    }

    /// Messages Batches API。
    pub fn batches(&self) -> AnthropicBatches {
        AnthropicBatches {
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
    ///
    /// 请求里 `stream == true` 时直接返回 `Configuration` 错误：SSE 响应体交给 JSON
    /// 解码只会得到难以定位的 `Deserialization`，应改用 [`Messages::stream`]。
    pub async fn create(&self, request: MessagesRequest) -> Result<MessagesResponse, AirxError> {
        reject_streaming(&request)?;
        self.api.create_with(&request, &self.options).await
    }

    /// 创建一次非流式消息，同时返回 HTTP 元数据。
    pub async fn create_with_meta(
        &self,
        request: MessagesRequest,
    ) -> Result<WithMeta<MessagesResponse>, AirxError> {
        reject_streaming(&request)?;
        self.api.create_with_meta(&request, &self.options).await
    }

    /// 创建并解析结构化输出。
    ///
    /// 与官方 `messages.parse` 不同：**不注入也不校验 schema**，只做
    /// `serde_json::from_str(response.text())`。schema 仍要通过
    /// `.output_format(JsonOutputFormat::new(schema))` 交给模型。
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

/// 非流式入口拒绝 `stream = true`。
fn reject_streaming(request: &MessagesRequest) -> Result<(), AirxError> {
    if request.stream == Some(true) {
        return Err(AirxError::Configuration(ConfigurationError::new(
            "MessagesRequest.stream=true 需要走 messages().stream()；create() 只解码普通 JSON 响应",
        )));
    }
    Ok(())
}

#[cfg(feature = "stream")]
impl ProtocolStream<MessagesEndpoint> {
    /// 消费整个流；流内错误 / 截断时保留已累积的部分响应。
    ///
    /// 返回 `(部分响应, 错误)`：`error` 为 `None` 仅表示流以 `message_stop`
    /// 正常结束。官方明确流内 `error` 不遵循 HTTP 语义，业务常需要
    /// 「部分结果 + 错误」。
    pub async fn final_response_partial(mut self) -> (MessagesResponse, Option<AirxError>) {
        use airx_core::error::StreamError;
        use airx_core::stream::StreamStatus;

        let mut accumulator = airx_protocol::anthropic::messages::MessagesStreamAccumulator::new();
        let mut failure = None;
        loop {
            match self.next().await {
                Ok(Some(event)) => {
                    if let Err(error) = accumulator.apply(event) {
                        failure = Some(error);
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        if failure.is_none() {
            failure = match accumulator.status() {
                StreamStatus::Completed => None,
                StreamStatus::Failed => Some(AirxError::Stream(StreamError::failed(
                    accumulator
                        .failure_message()
                        .unwrap_or("Anthropic 流内错误")
                        .to_owned(),
                ))),
                _ => Some(AirxError::Stream(StreamError::incomplete(
                    "未收到 message_stop，流已提前结束",
                ))),
            };
        }
        (accumulator.finish_partial(), failure)
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
                    lifecycle: request.lifecycle.clone(),
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

/// Anthropic Files 调用入口。
#[derive(Clone)]
pub struct AnthropicFiles {
    executor: Arc<Executor>,
    config: AnthropicConfig,
    options: RequestOptions,
}

impl AnthropicFiles {
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
        mime_type: impl Into<String>,
        data: impl Into<Bytes>,
    ) -> Result<FileMetadata, AirxError> {
        self.upload_request(UploadFileRequest::new(
            filename,
            mime_type,
            data,
            crate::multipart::boundary(),
        ))
        .await
    }

    /// 上传文件并让它在 `expires_in_seconds` 后过期（官方区间 3600–7776000 秒）。
    pub async fn upload_with_expiry(
        &self,
        filename: impl Into<String>,
        mime_type: impl Into<String>,
        data: impl Into<Bytes>,
        expires_in_seconds: u32,
    ) -> Result<FileMetadata, AirxError> {
        let request =
            UploadFileRequest::new(filename, mime_type, data, crate::multipart::boundary())
                .with_expires_in_seconds(expires_in_seconds);
        self.upload_request(request).await
    }

    /// 用完整 [`UploadFileRequest`] 上传（自定义 boundary / 过期时间）。
    pub async fn upload_request(
        &self,
        request: UploadFileRequest,
    ) -> Result<FileMetadata, AirxError> {
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
    pub async fn retrieve(&self, id: impl Into<String>) -> Result<FileMetadata, AirxError> {
        let api = ApiClient::new(
            RetrieveFileEndpoint::new(self.config.clone()),
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

/// Anthropic Messages Batches 调用入口。
#[derive(Clone)]
pub struct AnthropicBatches {
    executor: Arc<Executor>,
    config: AnthropicConfig,
    options: RequestOptions,
}

impl AnthropicBatches {
    /// 设置本次调用的请求选项。
    #[must_use]
    pub fn with_options(mut self, options: RequestOptions) -> Self {
        self.options = options;
        self
    }

    /// 创建批处理。
    pub async fn create(&self, request: CreateBatchRequest) -> Result<MessageBatch, AirxError> {
        let api = ApiClient::new(
            CreateBatchEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        api.create_with(&request, &self.options).await
    }

    /// 列出批处理。
    pub async fn list(&self, request: ListBatchesRequest) -> Result<BatchList, AirxError> {
        let api = ApiClient::new(
            ListBatchesEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        api.create_with(&request, &self.options).await
    }

    /// 获取批处理。
    pub async fn retrieve(&self, id: impl Into<String>) -> Result<MessageBatch, AirxError> {
        let api = ApiClient::new(
            BatchEndpoint::retrieve(self.config.clone()),
            Arc::clone(&self.executor),
        );
        let id = id.into();
        api.create_with(&id, &self.options).await
    }

    /// 取消批处理。
    pub async fn cancel(&self, id: impl Into<String>) -> Result<MessageBatch, AirxError> {
        let api = ApiClient::new(
            BatchEndpoint::cancel(self.config.clone()),
            Arc::clone(&self.executor),
        );
        let id = id.into();
        api.create_with(&id, &self.options).await
    }

    /// 获取结果 JSONL。
    pub async fn results(&self, id: impl Into<String>) -> Result<Bytes, AirxError> {
        let api = ApiClient::new(
            BatchResultsEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        let id = id.into();
        api.create_with(&id, &self.options).await
    }

    /// 获取结果并逐行解析为 [`BatchResultLine`]。
    ///
    /// 原始 JSONL 仍可用 [`AnthropicBatches::results`] 取得。
    pub async fn results_typed(
        &self,
        id: impl Into<String>,
    ) -> Result<Vec<BatchResultLine>, AirxError> {
        let api = ApiClient::new(
            BatchResultsTypedEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        let id = id.into();
        api.create_with(&id, &self.options).await
    }

    /// 删除批处理。
    pub async fn delete(&self, id: impl Into<String>) -> Result<DeletedBatch, AirxError> {
        let api = ApiClient::new(
            DeleteBatchEndpoint::new(self.config.clone()),
            Arc::clone(&self.executor),
        );
        let id = id.into();
        api.create_with(&id, &self.options).await
    }
}

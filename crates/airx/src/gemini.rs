//! Gemini 客户端。
//!
//! 由 `Client::gemini()` / `Client::builder().gemini()` 构建，持有执行器与配置，
//! 可以复用；模型仍按 Gemini 的路径约定在每次调用时指定。

use std::sync::Arc;

use airx_core::AirxError;
use airx_core::model::ModelId;
use airx_protocol::gemini::GeminiConfig;
use airx_protocol::gemini::generate_content::{
    GenerateContentEndpoint, GenerateContentRequest, GenerateContentResponse,
};

use crate::api::ApiClient;
use crate::executor::Executor;
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
}

impl GenerateContent {
    /// 模型标识。
    pub fn model(&self) -> &ModelId {
        &self.model
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
        api.create(&request).await
    }

    /// 创建一次流式生成。
    #[cfg(feature = "stream")]
    pub async fn stream(
        &self,
        request: GenerateContentRequest,
    ) -> Result<ProtocolStream<GenerateContentEndpoint>, AirxError> {
        let api = ApiClient::new(self.stream_endpoint(), Arc::clone(&self.executor));
        api.stream(&request).await
    }
}

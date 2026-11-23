//! 统一 API 编排。
//!
//! Unified 只做三件事：选择 Provider、纯函数转换、把警告带回响应。
//! 真正的请求仍走各 Provider 的 native endpoint 与同一套 Transport。

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::model::{ModelCapabilities, ModelId, ProviderId};
use airx_unified::{CompatibilityMode, GenerateRequest, GenerateResponse};

use crate::client::Client;

impl Client {
    /// 统一生成入口（Strict：不可表达即报错）。
    pub async fn generate(&self, request: GenerateRequest) -> Result<GenerateResponse, AirxError> {
        self.generate_with(request, CompatibilityMode::Strict).await
    }

    /// 统一生成入口（BestEffort：降级并返回 Warning）。
    pub async fn generate_best_effort(
        &self,
        request: GenerateRequest,
    ) -> Result<GenerateResponse, AirxError> {
        self.generate_with(request, CompatibilityMode::BestEffort)
            .await
    }

    /// 查询模型能力（未知模型返回 `Unknown`）。
    pub fn capabilities(&self, provider: &ProviderId, model: &ModelId) -> ModelCapabilities {
        airx_unified::capabilities(provider, model)
    }

    async fn generate_with(
        &self,
        request: GenerateRequest,
        mode: CompatibilityMode,
    ) -> Result<GenerateResponse, AirxError> {
        // 未启用任何 Provider feature 时 mode 不会被使用。
        let _ = mode;
        let provider = resolve_provider(&request);

        // DeepSeek 不是独立协议：Unified 走 OpenAI-Compatible Chat，
        // 通过 base_url 指向对应部署。Anthropic-Compatible 部署请用 Native API。
        if provider == ProviderId::openai() || provider == ProviderId::deepseek() {
            #[cfg(feature = "openai")]
            {
                let mut response = self.generate_openai(&request, mode).await?;
                response.provider = provider;
                return Ok(response);
            }
            #[cfg(not(feature = "openai"))]
            return Err(feature_disabled("openai"));
        }

        if provider == ProviderId::anthropic() {
            #[cfg(feature = "anthropic")]
            return self.generate_anthropic(&request, mode).await;
            #[cfg(not(feature = "anthropic"))]
            return Err(feature_disabled("anthropic"));
        }

        Err(AirxError::Configuration(ConfigurationError::new(format!(
            "未知 Provider: {provider}；请在请求中显式指定"
        ))))
    }

    #[cfg(feature = "openai")]
    async fn generate_openai(
        &self,
        request: &GenerateRequest,
        mode: CompatibilityMode,
    ) -> Result<GenerateResponse, AirxError> {
        use airx_protocol::openai::chat::ChatEndpoint;

        let config = self.openai.as_ref().ok_or_else(|| missing("OpenAI"))?;
        let adapted = airx_unified::adapter::openai::to_openai(request, mode)?;
        let api =
            crate::api::ApiClient::new(ChatEndpoint::new(config.clone()), self.executor.clone());
        let response = api.create(&adapted.native).await?;

        let mut unified = airx_unified::adapter::openai::from_openai(response);
        unified.warnings = adapted.warnings;
        Ok(unified)
    }

    #[cfg(feature = "anthropic")]
    async fn generate_anthropic(
        &self,
        request: &GenerateRequest,
        mode: CompatibilityMode,
    ) -> Result<GenerateResponse, AirxError> {
        use airx_protocol::anthropic::messages::MessagesEndpoint;

        let config = self
            .anthropic
            .as_ref()
            .ok_or_else(|| missing("Anthropic"))?;
        let adapted = airx_unified::adapter::anthropic::to_anthropic(request, mode)?;
        let api = crate::api::ApiClient::new(
            MessagesEndpoint::new(config.clone()),
            self.executor.clone(),
        );
        let response = api.create(&adapted.native).await?;

        let mut unified = airx_unified::adapter::anthropic::from_anthropic(response);
        unified.warnings = adapted.warnings;
        Ok(unified)
    }
}

fn resolve_provider(request: &GenerateRequest) -> ProviderId {
    if let Some(provider) = &request.provider {
        return provider.clone();
    }

    let model = request.model.as_str().to_ascii_lowercase();
    if model.contains("claude") {
        ProviderId::anthropic()
    } else {
        // DeepSeek / OpenAI-Compatible 部署都使用 OpenAI Chat 协议。
        ProviderId::openai()
    }
}

#[cfg(any(feature = "openai", feature = "anthropic"))]
fn missing(provider: &str) -> AirxError {
    AirxError::Configuration(ConfigurationError::new(format!(
        "该 Client 未配置 {provider} Provider"
    )))
}

// 仅在部分 Provider feature 组合下使用（例如未启用 openai 时）。
#[allow(dead_code)]
fn feature_disabled(feature: &str) -> AirxError {
    AirxError::Configuration(ConfigurationError::new(format!(
        "Provider `{feature}` 的 feature 未启用"
    )))
}

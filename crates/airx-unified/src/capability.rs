//! 能力判定。
//!
//! 类型在 `airx-core`，判定在这里：同一 Provider 的不同模型可以不同，
//! 未知模型一律返回 `Unknown`，不假定能力可用。

use airx_core::model::{CapabilitySupport, ModelCapabilities, ModelId, ProviderId};

/// 根据 Provider 与模型返回能力。
pub fn capabilities(provider: &ProviderId, model: &ModelId) -> ModelCapabilities {
    let name = model.as_str().to_ascii_lowercase();

    if provider == &ProviderId::openai()
        && (name.contains("gpt-4o") || name.contains("gpt-4.1") || name.contains("o3"))
    {
        return ModelCapabilities {
            streaming: CapabilitySupport::Supported,
            tools: CapabilitySupport::Supported,
            vision: CapabilitySupport::Supported,
            audio: CapabilitySupport::ModelDependent,
            structured_output: CapabilitySupport::Supported,
            reasoning: CapabilitySupport::ModelDependent,
            prompt_caching: CapabilitySupport::Supported,
        };
    }

    if provider == &ProviderId::anthropic() && name.contains("claude") {
        return ModelCapabilities {
            streaming: CapabilitySupport::Supported,
            tools: CapabilitySupport::Supported,
            vision: CapabilitySupport::Supported,
            audio: CapabilitySupport::Unsupported,
            structured_output: CapabilitySupport::ModelDependent,
            reasoning: CapabilitySupport::ModelDependent,
            prompt_caching: CapabilitySupport::Supported,
        };
    }

    if provider == &ProviderId::deepseek() && name.contains("reasoner") {
        return ModelCapabilities {
            streaming: CapabilitySupport::Supported,
            tools: CapabilitySupport::Unsupported,
            vision: CapabilitySupport::Unsupported,
            audio: CapabilitySupport::Unsupported,
            structured_output: CapabilitySupport::Unsupported,
            reasoning: CapabilitySupport::Supported,
            prompt_caching: CapabilitySupport::Unsupported,
        };
    }

    if provider == &ProviderId::deepseek() && name.contains("chat") {
        return ModelCapabilities {
            streaming: CapabilitySupport::Supported,
            tools: CapabilitySupport::Supported,
            vision: CapabilitySupport::Unsupported,
            audio: CapabilitySupport::Unsupported,
            structured_output: CapabilitySupport::ModelDependent,
            reasoning: CapabilitySupport::Unsupported,
            prompt_caching: CapabilitySupport::Unsupported,
        };
    }

    ModelCapabilities::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_models_have_capabilities() {
        let caps = capabilities(&ProviderId::openai(), &ModelId::new("gpt-4o-mini"));
        assert!(caps.tools.is_supported());
        assert!(caps.streaming.is_supported());

        let caps = capabilities(&ProviderId::deepseek(), &ModelId::new("deepseek-reasoner"));
        assert!(caps.reasoning.is_supported());
        assert_eq!(caps.tools, CapabilitySupport::Unsupported);
    }

    #[test]
    fn unknown_model_is_unknown() {
        let caps = capabilities(&ProviderId::openai(), &ModelId::new("mystery"));
        assert_eq!(caps.streaming, CapabilitySupport::Unknown);
    }
}

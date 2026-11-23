//! DeepSeek 兼容策略。
//!
//! 目前覆盖 reasoner 模型不支持的采样参数：
//! `Strict`（默认）直接报错；`BestEffort` 移除参数并继续。
//!
//! 兼容规则只存在于本模块，不污染 OpenAI 协议类型。

use airx_core::AirxError;
use airx_core::error::CompatibilityError;

use crate::openai::chat::ChatCompletionRequest;

/// 兼容模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompatibilityMode {
    /// 不可表达即报错。
    #[default]
    Strict,
    /// 允许降级（移除不受支持的参数）。
    BestEffort,
}

/// DeepSeek 兼容策略。
#[derive(Debug, Clone, Copy, Default)]
pub struct DeepSeekPolicy {
    /// 兼容模式。
    pub mode: CompatibilityMode,
}

impl DeepSeekPolicy {
    /// Strict 策略（默认）。
    pub fn strict() -> Self {
        Self {
            mode: CompatibilityMode::Strict,
        }
    }

    /// BestEffort 策略。
    pub fn best_effort() -> Self {
        Self {
            mode: CompatibilityMode::BestEffort,
        }
    }

    /// 应用策略，返回可发送的请求；`warnings` 记录被移除的字段。
    pub fn prepare(
        &self,
        mut request: ChatCompletionRequest,
    ) -> Result<(ChatCompletionRequest, Vec<String>), AirxError> {
        let mut warnings = Vec::new();
        let is_reasoner = request.model.as_str().contains("reasoner");

        if is_reasoner {
            for (name, present) in [
                ("temperature", request.temperature.is_some()),
                ("top_p", request.top_p.is_some()),
            ] {
                if !present {
                    continue;
                }
                match self.mode {
                    CompatibilityMode::Strict => {
                        return Err(AirxError::Compatibility(CompatibilityError::new(format!(
                            "DeepSeek reasoner 模型不支持 {name}；请移除该参数或使用 BestEffort"
                        ))));
                    }
                    CompatibilityMode::BestEffort => {
                        if name == "temperature" {
                            request.temperature = None;
                        } else {
                            request.top_p = None;
                        }
                        warnings.push(format!("已移除 DeepSeek reasoner 不支持的 {name}"));
                    }
                }
            }
        }

        Ok((request, warnings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::chat::ChatCompletionRequest;

    fn request(model: &str) -> ChatCompletionRequest {
        ChatCompletionRequest::builder()
            .model(model)
            .user("hi")
            .temperature(0.5)
            .build()
            .expect("request")
    }

    #[test]
    fn strict_rejects_unsupported_sampling() {
        let error = DeepSeekPolicy::strict()
            .prepare(request("deepseek-reasoner"))
            .expect_err("strict");
        assert!(matches!(error, AirxError::Compatibility(_)));
    }

    #[test]
    fn best_effort_removes_parameter_with_warning() {
        let (request, warnings) = DeepSeekPolicy::best_effort()
            .prepare(request("deepseek-reasoner"))
            .expect("prepared");
        assert_eq!(request.temperature, None);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("temperature"));
    }

    #[test]
    fn chat_model_is_untouched() {
        let (request, warnings) = DeepSeekPolicy::strict()
            .prepare(request("deepseek-chat"))
            .expect("prepared");
        assert_eq!(request.temperature, Some(0.5));
        assert!(warnings.is_empty());
    }
}

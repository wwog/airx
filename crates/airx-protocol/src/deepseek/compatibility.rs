//! DeepSeek 兼容策略。
//!
//! 依据 DeepSeek Chat Completions 文档（2026-11）：
//!
//! - thinking 模式下 `temperature` 无效；
//! - thinking 模式下 `tool_choice` 不接受 `required` 与指定工具；
//! - `response_format` 只支持 `text` / `json_object`，不支持 `json_schema`；
//! - `thinking` / `reasoning_effort` 都缺省时 thinking 默认开启。
//!
//! `Strict`（默认）直接报错；`BestEffort` 降级并返回警告。
//! 兼容规则只存在于本模块，不污染 OpenAI 协议类型。

use airx_core::AirxError;
use airx_core::error::CompatibilityError;
use serde_json::Value;

use crate::openai::chat::{ChatCompletionRequest, ResponseFormat, ToolChoice, ToolChoiceMode};
use crate::openai::common::reasoning::ReasoningEffort;

/// 兼容模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompatibilityMode {
    /// 不可表达即报错。
    #[default]
    Strict,
    /// 允许降级。
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

    /// 应用策略，返回可发送的请求；`warnings` 记录被移除/降级的字段。
    pub fn prepare(
        &self,
        mut request: ChatCompletionRequest,
    ) -> Result<(ChatCompletionRequest, Vec<String>), AirxError> {
        let mut warnings = Vec::new();
        let thinking = thinking_enabled(&request);

        if thinking && request.temperature.is_some() {
            match self.mode {
                CompatibilityMode::Strict => {
                    return Err(self.error(
                        "DeepSeek thinking 模式下 temperature 无效；请移除该参数、关闭 thinking，或使用 BestEffort",
                    ));
                }
                CompatibilityMode::BestEffort => {
                    request.temperature = None;
                    warnings.push("已移除 DeepSeek thinking 模式下无效的 temperature".to_owned());
                }
            }
        }

        if thinking
            && matches!(
                request.tool_choice,
                Some(ToolChoice::Mode(ToolChoiceMode::Required)) | Some(ToolChoice::Function(_))
            )
        {
            match self.mode {
                CompatibilityMode::Strict => {
                    return Err(self.error(
                        "DeepSeek thinking 模式不支持 tool_choice=required 或指定工具；请改用 auto 或关闭 thinking",
                    ));
                }
                CompatibilityMode::BestEffort => {
                    request.tool_choice = Some(ToolChoice::auto());
                    warnings.push(
                        "DeepSeek thinking 模式不支持强制定向工具，已降级为 tool_choice=auto"
                            .to_owned(),
                    );
                }
            }
        }

        if matches!(
            request.response_format,
            Some(ResponseFormat::JsonSchema { .. })
        ) {
            match self.mode {
                CompatibilityMode::Strict => {
                    return Err(self.error(
                        "DeepSeek 只支持 response_format=text|json_object，不支持 json_schema；如需结构化输出请使用 json_object 并在提示词中约束",
                    ));
                }
                CompatibilityMode::BestEffort => {
                    request.response_format = Some(ResponseFormat::JsonObject);
                    warnings.push(
                        "DeepSeek 不支持 json_schema，已降级为 json_object（需在提示词中要求输出 JSON）"
                            .to_owned(),
                    );
                }
            }
        }

        Ok((request, warnings))
    }

    fn error(&self, message: &str) -> AirxError {
        AirxError::Compatibility(CompatibilityError::new(message))
    }
}

/// thinking 是否开启。
fn thinking_enabled(request: &ChatCompletionRequest) -> bool {
    if let Some(kind) = request
        .extensions
        .get("thinking")
        .and_then(|thinking| thinking.get("type"))
        .and_then(Value::as_str)
    {
        return kind != "disabled";
    }

    match request.reasoning_effort {
        Some(ReasoningEffort::None) => false,
        Some(_) => true,
        // 文档：thinking 与 reasoning_effort 都缺省时默认开启。
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::chat::{ChatCompletionRequest, JsonSchemaSpec, Tool};
    use serde_json::json;

    fn request() -> ChatCompletionRequest {
        ChatCompletionRequest::builder()
            .model("deepseek-flash")
            .user("hi")
            .temperature(0.5)
            .build()
            .expect("request")
    }

    #[test]
    fn strict_rejects_temperature_in_thinking_mode() {
        let error = DeepSeekPolicy::strict()
            .prepare(request())
            .expect_err("strict");
        assert!(matches!(error, AirxError::Compatibility(_)));
    }

    #[test]
    fn best_effort_removes_temperature_with_warning() {
        let (request, warnings) = DeepSeekPolicy::best_effort()
            .prepare(request())
            .expect("prepared");
        assert_eq!(request.temperature, None);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("temperature"));
    }

    #[test]
    fn non_thinking_mode_keeps_temperature() {
        let request = ChatCompletionRequest::builder()
            .model("deepseek-flash")
            .user("hi")
            .temperature(0.5)
            .reasoning_effort(ReasoningEffort::None)
            .build()
            .expect("request");

        let (request, warnings) = DeepSeekPolicy::strict().prepare(request).expect("prepared");
        assert_eq!(request.temperature, Some(0.5));
        assert!(warnings.is_empty());
    }

    #[test]
    fn json_schema_is_rejected_or_downgraded() {
        let schema = ResponseFormat::JsonSchema {
            json_schema: JsonSchemaSpec {
                name: "answer".to_owned(),
                description: None,
                schema: json!({"type": "object"}),
                strict: Some(true),
                extensions: Default::default(),
            },
        };
        let request = ChatCompletionRequest::builder()
            .model("deepseek-flash")
            .user("hi")
            .response_format(schema)
            .build()
            .expect("request");

        assert!(DeepSeekPolicy::strict().prepare(request.clone()).is_err());

        let (request, warnings) = DeepSeekPolicy::best_effort()
            .prepare(request)
            .expect("prepared");
        assert!(matches!(
            request.response_format,
            Some(ResponseFormat::JsonObject)
        ));
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("json_schema"))
        );
    }

    #[test]
    fn required_tool_choice_is_downgraded_in_thinking_mode() {
        let request = ChatCompletionRequest::builder()
            .model("deepseek-flash")
            .user("hi")
            .tools([Tool::function("lookup", json!({"type": "object"}), None)])
            .tool_choice(ToolChoice::required())
            .build()
            .expect("request");

        assert!(DeepSeekPolicy::strict().prepare(request.clone()).is_err());

        let (request, warnings) = DeepSeekPolicy::best_effort()
            .prepare(request)
            .expect("prepared");
        assert!(matches!(
            request.tool_choice,
            Some(ToolChoice::Mode(ToolChoiceMode::Auto))
        ));
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("tool_choice"))
        );
    }
}

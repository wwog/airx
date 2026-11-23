//! Chat Completions 请求。

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::json::Extensions;
use airx_core::model::ModelId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::reasoning::{ReasoningEffort, Verbosity};

use super::message::ChatMessage;
use super::tool::{ResponseFormat, Tool, ToolChoice};

/// Chat Completions 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    /// 模型。
    pub model: ModelId,
    /// 消息列表。
    pub messages: Vec<ChatMessage>,
    /// 采样温度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// 核采样。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// 最大输出 token（已弃用；o-series 不兼容，推荐 `max_completion_tokens`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// 最大输出 token（含推理 token，官方推荐字段）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    /// 推理投入等级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    /// 输出详尽程度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<Verbosity>,
    /// 停止序列。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// 终端用户标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// 工具列表。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// 工具选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// 是否允许并行工具调用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    /// 结构化输出格式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    /// 是否流式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// 流式选项。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ChatCompletionRequest {
    /// 创建构建器。
    pub fn builder() -> ChatCompletionRequestBuilder {
        ChatCompletionRequestBuilder::default()
    }
}

/// 流式选项。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamOptions {
    /// 最后一个 chunk 是否携带 usage。
    #[serde(default)]
    pub include_usage: bool,
}

/// [`ChatCompletionRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct ChatCompletionRequestBuilder {
    model: Option<ModelId>,
    messages: Vec<ChatMessage>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_tokens: Option<u32>,
    max_completion_tokens: Option<u32>,
    reasoning_effort: Option<ReasoningEffort>,
    verbosity: Option<Verbosity>,
    stop: Option<Vec<String>>,
    user: Option<String>,
    tools: Option<Vec<Tool>>,
    tool_choice: Option<ToolChoice>,
    parallel_tool_calls: Option<bool>,
    response_format: Option<ResponseFormat>,
    stream: Option<bool>,
    stream_options: Option<StreamOptions>,
    extensions: Extensions,
}

impl ChatCompletionRequestBuilder {
    /// 设置模型（必填）。
    #[must_use]
    pub fn model(mut self, model: impl Into<ModelId>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// 追加一条消息。
    #[must_use]
    pub fn message(mut self, message: ChatMessage) -> Self {
        self.messages.push(message);
        self
    }

    /// 追加 system 消息。
    #[must_use]
    pub fn system(self, content: impl Into<String>) -> Self {
        self.message(ChatMessage::system(content))
    }

    /// 追加 user 消息。
    #[must_use]
    pub fn user(self, content: impl Into<String>) -> Self {
        self.message(ChatMessage::user(content))
    }

    /// 追加 assistant 消息。
    #[must_use]
    pub fn assistant(self, content: impl Into<String>) -> Self {
        self.message(ChatMessage::assistant(content))
    }

    /// 设置 temperature。
    #[must_use]
    pub fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// 设置 top_p。
    #[must_use]
    pub fn top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    /// 设置最大输出 token。
    #[must_use]
    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// 设置 `max_completion_tokens`（推荐；包含推理 token）。
    #[must_use]
    pub fn max_completion_tokens(mut self, max_completion_tokens: u32) -> Self {
        self.max_completion_tokens = Some(max_completion_tokens);
        self
    }

    /// 设置推理投入等级。
    #[must_use]
    pub fn reasoning_effort(mut self, reasoning_effort: ReasoningEffort) -> Self {
        self.reasoning_effort = Some(reasoning_effort);
        self
    }

    /// 设置输出详尽程度。
    #[must_use]
    pub fn verbosity(mut self, verbosity: Verbosity) -> Self {
        self.verbosity = Some(verbosity);
        self
    }

    /// 设置停止序列。
    #[must_use]
    pub fn stop(mut self, stop: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.stop = Some(stop.into_iter().map(Into::into).collect());
        self
    }

    /// 设置终端用户标识。
    #[must_use]
    pub fn user_id(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// 设置工具列表。
    #[must_use]
    pub fn tools(mut self, tools: impl IntoIterator<Item = Tool>) -> Self {
        self.tools = Some(tools.into_iter().collect());
        self
    }

    /// 设置工具选择。
    #[must_use]
    pub fn tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = Some(tool_choice);
        self
    }

    /// 设置是否允许并行工具调用。
    #[must_use]
    pub fn parallel_tool_calls(mut self, allowed: bool) -> Self {
        self.parallel_tool_calls = Some(allowed);
        self
    }

    /// 设置结构化输出格式。
    #[must_use]
    pub fn response_format(mut self, format: ResponseFormat) -> Self {
        self.response_format = Some(format);
        self
    }

    /// 设置流式标记。
    #[must_use]
    pub fn stream(mut self, stream: bool) -> Self {
        self.stream = Some(stream);
        self
    }

    /// 设置流式选项。
    #[must_use]
    pub fn stream_options(mut self, options: StreamOptions) -> Self {
        self.stream_options = Some(options);
        self
    }

    /// 添加一个协议扩展字段。
    #[must_use]
    pub fn extension(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
    }

    /// 构建请求。
    pub fn build(self) -> Result<ChatCompletionRequest, AirxError> {
        let model = self.model.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("ChatCompletionRequest.model 必填"))
        })?;

        if self.messages.is_empty() {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "ChatCompletionRequest.messages 不能为空",
            )));
        }

        Ok(ChatCompletionRequest {
            model,
            messages: self.messages,
            temperature: self.temperature,
            top_p: self.top_p,
            max_tokens: self.max_tokens,
            max_completion_tokens: self.max_completion_tokens,
            reasoning_effort: self.reasoning_effort,
            verbosity: self.verbosity,
            stop: self.stop,
            user: self.user,
            tools: self.tools,
            tool_choice: self.tool_choice,
            parallel_tool_calls: self.parallel_tool_calls,
            response_format: self.response_format,
            stream: self.stream,
            stream_options: self.stream_options,
            extensions: self.extensions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::chat::tool::JsonSchemaSpec;
    use serde_json::json;

    #[test]
    fn builder_requires_model_and_messages() {
        assert!(ChatCompletionRequest::builder().build().is_err());
        assert!(
            ChatCompletionRequest::builder()
                .model("gpt-4o-mini")
                .build()
                .is_err()
        );
    }

    #[test]
    fn optional_fields_are_omitted_when_none() {
        let request = ChatCompletionRequest::builder()
            .model("gpt-4o-mini")
            .user("hi")
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(
            value,
            json!({
                "model": "gpt-4o-mini",
                "messages": [{"role": "user", "content": "hi"}],
            })
        );
    }

    #[test]
    fn tools_and_stream_options_serialize() {
        let request = ChatCompletionRequest::builder()
            .model("gpt-4o-mini")
            .user("hi")
            .tools([Tool::function("lookup", json!({"type": "object"}), None)])
            .tool_choice(ToolChoice::auto())
            .stream(true)
            .stream_options(StreamOptions {
                include_usage: true,
            })
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["tools"][0]["type"], "function");
        assert_eq!(value["tool_choice"], "auto");
        assert_eq!(value["stream"], true);
        assert_eq!(value["stream_options"]["include_usage"], true);
    }

    #[test]
    fn response_format_json_schema_is_typed() {
        let request = ChatCompletionRequest::builder()
            .model("m")
            .user("hi")
            .response_format(ResponseFormat::JsonSchema {
                json_schema: JsonSchemaSpec {
                    name: "answer".to_owned(),
                    description: None,
                    schema: json!({"type": "object"}),
                    strict: Some(true),
                    extensions: Extensions::new(),
                },
            })
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["response_format"]["type"], "json_schema");
    }

    #[test]
    fn extensions_are_flattened() {
        let request = ChatCompletionRequest::builder()
            .model("gpt-4o-mini")
            .user("hi")
            .extension("logit_bias", json!({"50256": -100}))
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["logit_bias"]["50256"], -100);
    }
}

#[cfg(test)]
mod modern_params_tests {
    use super::*;
    use crate::openai::common::reasoning::{ReasoningEffort, Verbosity};

    #[test]
    fn max_completion_tokens_and_output_controls_serialize() {
        let request = ChatCompletionRequest::builder()
            .model("o3")
            .user("hi")
            .max_completion_tokens(128)
            .reasoning_effort(ReasoningEffort::High)
            .verbosity(Verbosity::Low)
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["max_completion_tokens"], 128);
        assert_eq!(value["reasoning_effort"], "high");
        assert_eq!(value["verbosity"], "low");
        assert!(value.get("max_tokens").is_none());
    }
}

#[cfg(test)]
mod json_mode_tests {
    use super::*;
    use crate::openai::chat::tool::ResponseFormat;

    #[test]
    fn json_object_mode_serializes_for_deepseek_compatible_deployments() {
        let request = ChatCompletionRequest::builder()
            .model("deepseek-flash")
            .system("你只输出 json，示例：{\"question\":\"q\",\"answer\":\"a\"}")
            .user("以 json 返回")
            .response_format(ResponseFormat::JsonObject)
            .max_tokens(512)
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["response_format"]["type"], "json_object");
        assert_eq!(value["max_tokens"], 512);
    }
}

//! OpenAI Responses 请求与输入模型。

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::json::Extensions;
use airx_core::model::ModelId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::tool::ResponsesTool;
use crate::openai::chat::ToolChoice;
use crate::openai::common::reasoning::ReasoningEffort;

/// Responses 输入：纯文本或 item 列表。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponsesInput {
    /// 纯文本输入。
    Text(String),
    /// 结构化输入项。
    Items(Vec<InputItem>),
}

/// 输入项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputItem {
    /// 消息。
    Message {
        /// 角色。
        role: String,
        /// 文本内容。
        content: String,
    },
    /// 函数调用结果。
    FunctionCallOutput {
        /// 对应的调用 ID。
        call_id: String,
        /// 结果文本。
        output: String,
    },
}

/// 推理配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Reasoning {
    /// 推理投入等级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<ReasoningEffort>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 文本输出配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TextConfig {
    /// 输出格式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<TextFormat>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 文本输出格式。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TextFormat {
    /// 普通文本。
    Text,
    /// JSON 对象模式。
    JsonObject,
    /// JSON Schema 结构化输出。
    JsonSchema {
        /// 名称。
        name: String,
        /// 描述。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        /// JSON Schema。
        schema: Value,
        /// 是否严格。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        strict: Option<bool>,
    },
}

/// Responses 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponsesRequest {
    /// 模型。
    pub model: ModelId,
    /// 输入。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<ResponsesInput>,
    /// 系统指令。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// 最大输出 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    /// 采样温度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// 核采样。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// 工具。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ResponsesTool>>,
    /// 工具选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// 是否允许并行工具调用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    /// 推理配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
    /// 文本输出配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextConfig>,
    /// 是否流式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ResponsesRequest {
    /// 创建构建器。
    pub fn builder() -> ResponsesRequestBuilder {
        ResponsesRequestBuilder::default()
    }
}

/// [`ResponsesRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct ResponsesRequestBuilder {
    model: Option<ModelId>,
    input: Option<ResponsesInput>,
    instructions: Option<String>,
    max_output_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    tools: Option<Vec<ResponsesTool>>,
    tool_choice: Option<ToolChoice>,
    parallel_tool_calls: Option<bool>,
    reasoning: Option<Reasoning>,
    text: Option<TextConfig>,
    stream: Option<bool>,
    extensions: Extensions,
}

impl ResponsesRequestBuilder {
    /// 设置模型（必填）。
    #[must_use]
    pub fn model(mut self, model: impl Into<ModelId>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// 设置纯文本输入。
    #[must_use]
    pub fn input(mut self, input: impl Into<String>) -> Self {
        self.input = Some(ResponsesInput::Text(input.into()));
        self
    }

    /// 设置 item 列表输入。
    #[must_use]
    pub fn input_items(mut self, items: Vec<InputItem>) -> Self {
        self.input = Some(ResponsesInput::Items(items));
        self
    }

    /// 设置系统指令。
    #[must_use]
    pub fn instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    /// 设置最大输出 token。
    #[must_use]
    pub fn max_output_tokens(mut self, tokens: u32) -> Self {
        self.max_output_tokens = Some(tokens);
        self
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

    /// 设置工具（函数工具或服务端联网工具）。
    #[must_use]
    pub fn tools(mut self, tools: impl IntoIterator<Item = ResponsesTool>) -> Self {
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

    /// 设置推理投入等级。
    #[must_use]
    pub fn reasoning_effort(mut self, effort: ReasoningEffort) -> Self {
        self.reasoning.get_or_insert_with(Reasoning::default).effort = Some(effort);
        self
    }

    /// 设置文本输出格式。
    #[must_use]
    pub fn text_format(mut self, format: TextFormat) -> Self {
        self.text.get_or_insert_with(TextConfig::default).format = Some(format);
        self
    }

    /// 设置流式标记。
    #[must_use]
    pub fn stream(mut self, stream: bool) -> Self {
        self.stream = Some(stream);
        self
    }

    /// 添加协议扩展字段。
    #[must_use]
    pub fn extension(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
    }

    /// 构建请求。
    pub fn build(self) -> Result<ResponsesRequest, AirxError> {
        let model = self.model.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("ResponsesRequest.model 必填"))
        })?;

        Ok(ResponsesRequest {
            model,
            input: self.input,
            instructions: self.instructions,
            max_output_tokens: self.max_output_tokens,
            temperature: self.temperature,
            top_p: self.top_p,
            tools: self.tools,
            tool_choice: self.tool_choice,
            parallel_tool_calls: self.parallel_tool_calls,
            reasoning: self.reasoning,
            text: self.text,
            stream: self.stream,
            extensions: self.extensions,
        })
    }
}

#[cfg(test)]
mod alignment_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reasoning_and_text_format_serialize() {
        let request = ResponsesRequest::builder()
            .model("gpt-5")
            .input("hi")
            .reasoning_effort(ReasoningEffort::Medium)
            .text_format(TextFormat::JsonSchema {
                name: "poster".to_owned(),
                description: None,
                schema: json!({"type": "object"}),
                strict: Some(true),
            })
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["reasoning"]["effort"], "medium");
        assert_eq!(value["text"]["format"]["type"], "json_schema");
        assert_eq!(value["text"]["format"]["name"], "poster");
        assert_eq!(value["text"]["format"]["strict"], true);
    }
}

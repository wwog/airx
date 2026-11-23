//! 统一请求模型。

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::model::{ModelId, ProviderId};
use serde_json::Value;

use crate::content::Message;

/// 统一工具定义。
#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    /// 工具名。
    pub name: String,
    /// 描述。
    pub description: Option<String>,
    /// 参数 JSON Schema。
    pub parameters: Value,
}

impl Tool {
    /// 构造工具。
    pub fn new(name: impl Into<String>, parameters: Value) -> Self {
        Self {
            name: name.into(),
            description: None,
            parameters,
        }
    }
}

/// 生成选项。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GenerationOptions {
    /// 温度。
    pub temperature: Option<f32>,
    /// 核采样。
    pub top_p: Option<f32>,
    /// 最大输出 token。
    pub max_tokens: Option<u32>,
    /// 停止序列。
    pub stop: Vec<String>,
}

/// Provider 专属扩展（按命名空间隔离）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProviderExtensions {
    /// OpenAI 原生字段。
    pub openai: Option<Value>,
    /// Anthropic 原生字段。
    pub anthropic: Option<Value>,
    /// DeepSeek 原生字段。
    pub deepseek: Option<Value>,
}

/// 统一生成请求。
#[derive(Debug, Clone, PartialEq)]
pub struct GenerateRequest {
    /// 模型。
    pub model: ModelId,
    /// 指定 Provider（可选，未指定时由上层推断）。
    pub provider: Option<ProviderId>,
    /// 消息。
    pub messages: Vec<Message>,
    /// 系统指令。
    pub instructions: Option<String>,
    /// 工具。
    pub tools: Vec<Tool>,
    /// 生成选项。
    pub options: GenerationOptions,
    /// Provider 扩展。
    pub extensions: ProviderExtensions,
}

impl GenerateRequest {
    /// 创建构建器。
    pub fn builder() -> GenerateRequestBuilder {
        GenerateRequestBuilder::default()
    }
}

/// [`GenerateRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct GenerateRequestBuilder {
    model: Option<ModelId>,
    provider: Option<ProviderId>,
    messages: Vec<Message>,
    instructions: Option<String>,
    tools: Vec<Tool>,
    options: GenerationOptions,
    extensions: ProviderExtensions,
}

impl GenerateRequestBuilder {
    /// 设置模型。
    #[must_use]
    pub fn model(mut self, model: impl Into<ModelId>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// 指定 Provider。
    #[must_use]
    pub fn provider(mut self, provider: ProviderId) -> Self {
        self.provider = Some(provider);
        self
    }

    /// 追加消息。
    #[must_use]
    pub fn message(mut self, message: Message) -> Self {
        self.messages.push(message);
        self
    }

    /// 追加用户文本消息。
    #[must_use]
    pub fn user(self, text: impl Into<String>) -> Self {
        self.message(Message::user(text))
    }

    /// 追加助手文本消息。
    #[must_use]
    pub fn assistant(self, text: impl Into<String>) -> Self {
        self.message(Message::assistant(text))
    }

    /// 设置系统指令。
    #[must_use]
    pub fn instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    /// 追加工具。
    #[must_use]
    pub fn tool(mut self, tool: Tool) -> Self {
        self.tools.push(tool);
        self
    }

    /// 设置温度。
    #[must_use]
    pub fn temperature(mut self, temperature: f32) -> Self {
        self.options.temperature = Some(temperature);
        self
    }

    /// 设置最大输出 token。
    #[must_use]
    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.options.max_tokens = Some(max_tokens);
        self
    }

    /// 设置停止序列。
    #[must_use]
    pub fn stop(mut self, stop: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.options.stop = stop.into_iter().map(Into::into).collect();
        self
    }

    /// 设置 OpenAI 扩展。
    #[must_use]
    pub fn openai_extension(mut self, value: Value) -> Self {
        self.extensions.openai = Some(value);
        self
    }

    /// 设置 Anthropic 扩展。
    #[must_use]
    pub fn anthropic_extension(mut self, value: Value) -> Self {
        self.extensions.anthropic = Some(value);
        self
    }

    /// 设置 DeepSeek 扩展。
    #[must_use]
    pub fn deepseek_extension(mut self, value: Value) -> Self {
        self.extensions.deepseek = Some(value);
        self
    }

    /// 构建请求。
    pub fn build(self) -> Result<GenerateRequest, AirxError> {
        let model = self.model.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("GenerateRequest.model 必填"))
        })?;

        if self.messages.is_empty() {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "GenerateRequest.messages 不能为空",
            )));
        }

        Ok(GenerateRequest {
            model,
            provider: self.provider,
            messages: self.messages,
            instructions: self.instructions,
            tools: self.tools,
            options: self.options,
            extensions: self.extensions,
        })
    }
}

//! Gemini `generateContent` 请求。
//!
//! 模型不在请求体中：Gemini 的路径是 `/models/{model}:generateContent`，
//! 因此模型由 [`super::endpoint::GenerateContentEndpoint`] 持有，
//! 请求结构只对应官方 JSON body。

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::json::Extensions;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::content::{Content, Part};
use super::generation::{GenerationConfig, SafetySetting, ThinkingConfig};
use super::tool::{Tool, ToolConfig};

/// `generateContent` 请求体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerateContentRequest {
    /// 对话内容。
    pub contents: Vec<Content>,
    /// 系统指令。
    #[serde(
        rename = "systemInstruction",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub system_instruction: Option<Content>,
    /// 工具。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// 工具选择。
    #[serde(
        rename = "toolConfig",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_config: Option<ToolConfig>,
    /// 生成配置。
    #[serde(
        rename = "generationConfig",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub generation_config: Option<GenerationConfig>,
    /// 安全设置。
    #[serde(
        rename = "safetySettings",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub safety_settings: Option<Vec<SafetySetting>>,
    /// 缓存的上下文资源名。
    #[serde(
        rename = "cachedContent",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub cached_content: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl GenerateContentRequest {
    /// 创建构建器。
    pub fn builder() -> GenerateContentRequestBuilder {
        GenerateContentRequestBuilder::default()
    }
}

/// [`GenerateContentRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct GenerateContentRequestBuilder {
    contents: Vec<Content>,
    system_instruction: Option<Content>,
    tools: Option<Vec<Tool>>,
    tool_config: Option<ToolConfig>,
    generation_config: Option<GenerationConfig>,
    safety_settings: Vec<SafetySetting>,
    cached_content: Option<String>,
    extensions: Extensions,
}

impl GenerateContentRequestBuilder {
    /// 追加一轮内容。
    #[must_use]
    pub fn content(mut self, content: Content) -> Self {
        self.contents.push(content);
        self
    }

    /// 追加用户文本。
    #[must_use]
    pub fn user(self, text: impl Into<String>) -> Self {
        self.content(Content::user_text(text))
    }

    /// 追加模型文本（多轮回传）。
    #[must_use]
    pub fn model(self, text: impl Into<String>) -> Self {
        self.content(Content::model_text(text))
    }

    /// 追加自定义片段到当前轮内容；没有内容时新建用户轮。
    #[must_use]
    pub fn part(mut self, part: Part) -> Self {
        match self.contents.last_mut() {
            Some(content) => content.parts.push(part),
            None => self
                .contents
                .push(Content::parts(super::content::Role::User, vec![part])),
        }
        self
    }

    /// 设置系统指令（`systemInstruction` 不带角色）。
    #[must_use]
    pub fn system(mut self, system: impl Into<String>) -> Self {
        self.system_instruction = Some(Content {
            role: None,
            parts: vec![Part::text(system)],
            extensions: Extensions::new(),
        });
        self
    }

    /// 设置工具。
    #[must_use]
    pub fn tools(mut self, tools: impl IntoIterator<Item = Tool>) -> Self {
        self.tools = Some(tools.into_iter().collect());
        self
    }

    /// 追加工具。
    #[must_use]
    pub fn tool(mut self, tool: Tool) -> Self {
        self.tools.get_or_insert_with(Vec::new).push(tool);
        self
    }

    /// 设置工具选择。
    #[must_use]
    pub fn tool_config(mut self, tool_config: ToolConfig) -> Self {
        self.tool_config = Some(tool_config);
        self
    }

    /// 设置生成配置。
    #[must_use]
    pub fn generation_config(mut self, generation_config: GenerationConfig) -> Self {
        self.generation_config = Some(generation_config);
        self
    }

    /// 设置温度（写入 `generationConfig.temperature`）。
    #[must_use]
    pub fn temperature(mut self, temperature: f32) -> Self {
        self.generation_config
            .get_or_insert_with(GenerationConfig::default)
            .temperature = Some(temperature);
        self
    }

    /// 设置最大输出 token。
    #[must_use]
    pub fn max_output_tokens(mut self, max_output_tokens: u32) -> Self {
        self.generation_config
            .get_or_insert_with(GenerationConfig::default)
            .max_output_tokens = Some(max_output_tokens);
        self
    }

    /// 设置 thinking 配置。
    #[must_use]
    pub fn thinking(mut self, thinking: ThinkingConfig) -> Self {
        self.generation_config
            .get_or_insert_with(GenerationConfig::default)
            .thinking_config = Some(thinking);
        self
    }

    /// 设置结构化输出 Schema。
    #[must_use]
    pub fn json_schema(mut self, schema: Value) -> Self {
        let config = self
            .generation_config
            .get_or_insert_with(GenerationConfig::default);
        config.response_mime_type = Some("application/json".to_owned());
        config.response_schema = Some(schema);
        self
    }

    /// 追加安全设置。
    #[must_use]
    pub fn safety_setting(mut self, setting: SafetySetting) -> Self {
        self.safety_settings.push(setting);
        self
    }

    /// 设置缓存上下文。
    #[must_use]
    pub fn cached_content(mut self, cached_content: impl Into<String>) -> Self {
        self.cached_content = Some(cached_content.into());
        self
    }

    /// 添加扩展字段。
    #[must_use]
    pub fn extension(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
    }

    /// 构建请求。
    pub fn build(self) -> Result<GenerateContentRequest, AirxError> {
        if self.contents.is_empty() {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "GenerateContentRequest.contents 不能为空",
            )));
        }

        Ok(GenerateContentRequest {
            contents: self.contents,
            system_instruction: self.system_instruction,
            tools: self.tools,
            tool_config: self.tool_config,
            generation_config: self.generation_config,
            safety_settings: (!self.safety_settings.is_empty()).then_some(self.safety_settings),
            cached_content: self.cached_content,
            extensions: self.extensions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builder_produces_official_shape() {
        let request = GenerateContentRequest::builder()
            .system("be concise")
            .user("hi")
            .model("hello")
            .max_output_tokens(64)
            .thinking(ThinkingConfig::disabled())
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["contents"][0]["role"], "user");
        assert_eq!(value["contents"][0]["parts"][0]["text"], "hi");
        assert_eq!(value["contents"][1]["role"], "model");
        assert_eq!(value["systemInstruction"]["parts"][0]["text"], "be concise");
        assert_eq!(value["generationConfig"]["maxOutputTokens"], 64);
        assert_eq!(
            value["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            0
        );
    }

    #[test]
    fn empty_contents_is_rejected() {
        let error = GenerateContentRequest::builder()
            .build()
            .expect_err("empty");
        assert!(matches!(error, AirxError::Configuration(_)));
    }

    #[test]
    fn unknown_fields_are_preserved() {
        let request: GenerateContentRequest = serde_json::from_value(json!({
            "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
            "futureField": true
        }))
        .expect("request");

        assert!(request.extensions.contains_key("futureField"));
        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["futureField"], true);
    }
}

//! Gemini `countTokens`。
//!
//! `POST {base}/{model}:countTokens`。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError, SerializationError};
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, header};
use airx_core::json::Extensions;
use airx_core::model::{ModelId, ProviderId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::endpoint::Endpoint;
use crate::gemini::common::error_from_response;
use crate::gemini::config::GeminiConfig;

use super::content::{Content, Part};
use super::endpoint::model_path;
use super::generation::GenerationConfig;
use super::request::GenerateContentRequest;
use super::tool::Tool;

/// countTokens 请求。
///
/// 官方支持两种互斥形态：`contents`（+/`systemInstruction`/`tools`/
/// `generationConfig`），或整体 [`CountTokensRequest::generate_content_request`]。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CountTokensRequest {
    /// 模型资源名（官方在路径中要求；body 中一般不写）。
    ///
    /// 只有嵌套的 `generateContentRequest` 形态需要/会自动补写。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 对话内容。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
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
    /// 生成配置（仅与 token 计数相关的字段会被服务端使用）。
    #[serde(
        rename = "generationConfig",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub generation_config: Option<GenerationConfig>,
    /// 整体形态：直接嵌入一个 `generateContentRequest`。
    #[serde(
        rename = "generateContentRequest",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub generate_content_request: Option<Box<GenerateContentRequest>>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl CountTokensRequest {
    /// 创建构建器。
    pub fn builder() -> CountTokensRequestBuilder {
        CountTokensRequestBuilder::default()
    }

    /// 用完整的 `generateContentRequest` 构造（官方 `generateContentRequest` 形态）。
    pub fn of(request: &GenerateContentRequest) -> Self {
        Self {
            model: request.model.clone(),
            contents: Vec::new(),
            system_instruction: None,
            tools: None,
            generation_config: None,
            generate_content_request: Some(Box::new(request.clone())),
            extensions: Extensions::new(),
        }
    }
}

/// [`CountTokensRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct CountTokensRequestBuilder {
    model: Option<String>,
    contents: Vec<Content>,
    system_instruction: Option<Content>,
    tools: Option<Vec<Tool>>,
    generation_config: Option<GenerationConfig>,
    extensions: Extensions,
}

impl CountTokensRequestBuilder {
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

    /// 追加片段到当前轮。
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

    /// 设置系统指令。
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

    /// 设置生成配置。
    #[must_use]
    pub fn generation_config(mut self, generation_config: GenerationConfig) -> Self {
        self.generation_config = Some(generation_config);
        self
    }

    /// 显式设置模型资源名（一般由端点按路径补写）。
    #[must_use]
    pub fn model_id(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// 添加扩展字段。
    #[must_use]
    pub fn extension(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
    }

    /// 构建请求。
    pub fn build(self) -> Result<CountTokensRequest, AirxError> {
        if self.contents.is_empty() {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "CountTokensRequest.contents 不能为空",
            )));
        }

        Ok(CountTokensRequest {
            model: self.model,
            contents: self.contents,
            system_instruction: self.system_instruction,
            tools: self.tools,
            generation_config: self.generation_config,
            generate_content_request: None,
            extensions: self.extensions,
        })
    }
}

/// countTokens 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountTokensResponse {
    /// 总 token 数。
    #[serde(default)]
    pub total_tokens: u32,
    /// 命中缓存的内容 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_content_token_count: Option<u32>,
    /// 按模态拆分的输入 token（原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// countTokens 端点。
#[derive(Debug, Clone)]
pub struct CountTokensEndpoint {
    config: GeminiConfig,
    model: ModelId,
}

impl CountTokensEndpoint {
    /// 创建端点。
    pub fn new(config: GeminiConfig, model: impl Into<ModelId>) -> Self {
        Self {
            config,
            model: model.into(),
        }
    }

    fn uri(&self) -> Result<airx_core::http::Uri, AirxError> {
        let path = format!("{}:countTokens", model_path(&self.model)?);
        self.config.resolve_path(&path, "countTokens")
    }
}

impl Endpoint for CountTokensEndpoint {
    type Request = CountTokensRequest;
    type Response = CountTokensResponse;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        let mut request = request.clone();
        // 嵌套 `generateContentRequest` 形态要求 body 内带 `model`；
        // 简单形态的 model 只在路径里（REST 把顶层 model 记为路径参数）。
        if let Some(nested) = request.generate_content_request.as_mut() {
            if nested.model.is_none() {
                nested.model = Some(model_path(&self.model)?);
            }
        }
        let body = serde_json::to_vec(&request)
            .map_err(|error| AirxError::Serialization(SerializationError::from(error)))?;

        let mut http = HttpRequest::post(self.uri()?).with_body(body);
        http.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<CountTokensResponse, AirxError> {
        if !response.status.is_success() {
            return Err(error_from_response(
                &ProviderId::gemini(),
                response.status,
                &response.headers,
                &response.body,
            ));
        }

        serde_json::from_slice(&response.body)
            .map_err(|error| AirxError::Deserialization(DeserializationError::from(error)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_serializes_official_shape() {
        let request = CountTokensRequest::builder()
            .system("be brief")
            .user("hi")
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["contents"][0]["parts"][0]["text"], "hi");
        assert_eq!(value["systemInstruction"]["parts"][0]["text"], "be brief");
    }

    #[test]
    fn empty_contents_is_rejected() {
        assert!(CountTokensRequest::builder().build().is_err());
    }

    #[test]
    fn response_decodes() {
        let response: CountTokensResponse =
            serde_json::from_value(json!({"totalTokens": 42})).expect("response");
        assert_eq!(response.total_tokens, 42);
    }

    #[test]
    fn of_wraps_generate_content_request() {
        let request = GenerateContentRequest::builder()
            .system("be brief")
            .user("hi")
            .build()
            .expect("generate request");
        let tokens = CountTokensRequest::of(&request);

        let value = serde_json::to_value(&tokens).expect("serialize");
        assert_eq!(
            value["generateContentRequest"]["contents"][0]["parts"][0]["text"],
            "hi"
        );
        assert!(value.get("contents").is_none(), "contents 被忽略");
    }

    #[test]
    fn endpoint_fills_nested_model() {
        let request = GenerateContentRequest::builder()
            .user("hi")
            .build()
            .expect("generate request");
        let endpoint = CountTokensEndpoint::new(GeminiConfig::new("k"), "gemini-2.5-flash");
        let http = endpoint
            .encode(&CountTokensRequest::of(&request))
            .expect("encode");
        let body: Value = serde_json::from_slice(&http.body).expect("body");

        assert_eq!(
            body["generateContentRequest"]["model"],
            "models/gemini-2.5-flash"
        );
        assert_eq!(
            http.uri.path(),
            "/v1beta/models/gemini-2.5-flash:countTokens"
        );
    }
}

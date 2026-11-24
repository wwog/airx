//! OpenAI Moderations API。
//!
//! `POST /moderations`：对文本 / 图片输入做内容分类。不涉及 Agent 运行时，
//! 只做请求编码与响应解码。

use std::collections::BTreeMap;

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError, SerializationError};
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, header};
use airx_core::json::Extensions;
use airx_core::model::ProviderId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::endpoint::Endpoint;
use crate::openai::common::error::error_from_response;
use crate::openai::config::OpenAiConfig;

/// 端点路径。
pub const MODERATIONS_PATH: &str = "/moderations";

/// 审核输入。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModerationInput {
    /// 单条文本。
    Text(String),
    /// 多条文本。
    Texts(Vec<String>),
    /// 多模态输入项。
    Items(Vec<ModerationInputItem>),
}

impl From<&str> for ModerationInput {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for ModerationInput {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<Vec<String>> for ModerationInput {
    fn from(value: Vec<String>) -> Self {
        Self::Texts(value)
    }
}

/// 多模态审核输入项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModerationInputItem {
    /// 文本。
    Text {
        /// 待分类文本。
        text: String,
    },
    /// 图片（URL 或 data URI）。
    ImageUrl {
        /// 图片。
        image_url: ModerationImageUrl,
    },
}

impl ModerationInputItem {
    /// 构造文本项。
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// 构造图片项。
    pub fn image_url(url: impl Into<String>) -> Self {
        Self::ImageUrl {
            image_url: ModerationImageUrl { url: url.into() },
        }
    }
}

/// 图片地址。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModerationImageUrl {
    /// 图片 URL 或 base64 data URI。
    pub url: String,
}

/// 审核请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModerationRequest {
    /// 输入。
    pub input: ModerationInput,
    /// 审核模型（默认由服务端决定）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ModerationRequest {
    /// 创建构建器。
    pub fn builder() -> ModerationRequestBuilder {
        ModerationRequestBuilder::default()
    }
}

/// [`ModerationRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct ModerationRequestBuilder {
    input: Option<ModerationInput>,
    model: Option<String>,
    extensions: Extensions,
}

impl ModerationRequestBuilder {
    /// 设置输入。
    #[must_use]
    pub fn input(mut self, input: impl Into<ModerationInput>) -> Self {
        self.input = Some(input.into());
        self
    }

    /// 设置单条文本输入。
    #[must_use]
    pub fn text(self, text: impl Into<String>) -> Self {
        self.input(ModerationInput::Text(text.into()))
    }

    /// 设置多条文本输入。
    #[must_use]
    pub fn texts(mut self, texts: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.input = Some(ModerationInput::Texts(
            texts.into_iter().map(Into::into).collect(),
        ));
        self
    }

    /// 设置多模态输入项。
    #[must_use]
    pub fn items(mut self, items: impl IntoIterator<Item = ModerationInputItem>) -> Self {
        self.input = Some(ModerationInput::Items(items.into_iter().collect()));
        self
    }

    /// 设置审核模型。
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
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
    pub fn build(self) -> Result<ModerationRequest, AirxError> {
        let input = self.input.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("ModerationRequest.input 必填"))
        })?;
        Ok(ModerationRequest {
            input,
            model: self.model,
            extensions: self.extensions,
        })
    }
}

/// 审核响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModerationResponse {
    /// 请求 ID。
    pub id: String,
    /// 实际使用的模型。
    #[serde(default)]
    pub model: String,
    /// 每条输入的结果。
    #[serde(default)]
    pub results: Vec<ModerationResult>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 单条输入的审核结果。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModerationResult {
    /// 是否被标记。
    #[serde(default)]
    pub flagged: bool,
    /// 各分类是否命中。
    #[serde(default)]
    pub categories: BTreeMap<String, bool>,
    /// 各分类得分。
    #[serde(default)]
    pub category_scores: BTreeMap<String, f64>,
    /// 各分类适用的输入类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_applied_input_types: Option<BTreeMap<String, Vec<String>>>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// `POST /moderations`。
#[derive(Debug, Clone)]
pub struct ModerationsEndpoint {
    config: OpenAiConfig,
}

impl ModerationsEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self) -> Result<airx_core::http::Uri, AirxError> {
        self.config.endpoint_uri(MODERATIONS_PATH, "moderations")
    }
}

impl Endpoint for ModerationsEndpoint {
    type Request = ModerationRequest;
    type Response = ModerationResponse;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError> {
        let body = serde_json::to_vec(request)
            .map_err(|error| AirxError::Serialization(SerializationError::from(error)))?;
        let mut http = HttpRequest::post(self.uri()?).with_body(body);
        http.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self.config.apply_headers(&mut http)?;
        Ok(http)
    }

    fn decode(&self, response: HttpResponse) -> Result<ModerationResponse, AirxError> {
        if !response.status.is_success() {
            return Err(error_from_response(
                &ProviderId::openai(),
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
    fn request_requires_input() {
        assert!(ModerationRequest::builder().build().is_err());
    }

    #[test]
    fn text_input_serializes_as_string() {
        let request = ModerationRequest::builder()
            .text("hello")
            .model("omni-moderation-latest")
            .build()
            .expect("request");
        assert_eq!(
            serde_json::to_value(&request).expect("serialize"),
            json!({"input": "hello", "model": "omni-moderation-latest"})
        );
    }

    #[test]
    fn multimodal_input_serializes_as_items() {
        let request = ModerationRequest::builder()
            .items([
                ModerationInputItem::text("hello"),
                ModerationInputItem::image_url("https://example.com/a.png"),
            ])
            .build()
            .expect("request");
        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["input"][0]["type"], "text");
        assert_eq!(value["input"][1]["type"], "image_url");
        assert_eq!(
            value["input"][1]["image_url"]["url"],
            "https://example.com/a.png"
        );
    }

    #[test]
    fn response_decodes_categories_and_scores() {
        let response: ModerationResponse = serde_json::from_value(json!({
            "id": "modr-1",
            "model": "omni-moderation-latest",
            "results": [{
                "flagged": true,
                "categories": {"violence": true, "hate": false},
                "category_scores": {"violence": 0.9, "hate": 0.01},
                "category_applied_input_types": {"violence": ["text"]}
            }]
        }))
        .expect("response");
        assert_eq!(response.id, "modr-1");
        assert!(response.results[0].flagged);
        assert!(response.results[0].categories["violence"]);
        assert_eq!(response.results[0].category_scores["violence"], 0.9);
        assert_eq!(
            response.results[0]
                .category_applied_input_types
                .as_ref()
                .expect("types")["violence"],
            vec!["text".to_owned()]
        );
    }

    #[test]
    fn endpoint_uses_moderations_path() {
        let endpoint = ModerationsEndpoint::new(OpenAiConfig::new("sk-test"));
        let http = endpoint
            .encode(
                &ModerationRequest::builder()
                    .text("hello")
                    .build()
                    .expect("request"),
            )
            .expect("encode");
        assert_eq!(http.method, airx_core::http::Method::POST);
        assert_eq!(
            http.uri.to_string(),
            "https://api.openai.com/v1/moderations"
        );
    }
}

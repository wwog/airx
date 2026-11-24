//! OpenAI Embeddings API。

use airx_core::AirxError;
use airx_core::error::{ConfigurationError, DeserializationError, SerializationError};
use airx_core::http::{HeaderValue, HttpRequest, HttpResponse, header};
use airx_core::json::Extensions;
use airx_core::model::{ModelId, ProviderId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::endpoint::Endpoint;
use crate::openai::common::error::error_from_response;
use crate::openai::config::OpenAiConfig;

/// 端点路径。
pub const EMBEDDINGS_PATH: &str = "/embeddings";

/// 输入：单条文本或多条文本。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    /// 单条文本。
    Text(String),
    /// 多条文本。
    Texts(Vec<String>),
    /// 单条 token 序列。
    Tokens(Vec<u32>),
    /// 多条 token 序列。
    TokenBatches(Vec<Vec<u32>>),
}

impl From<&str> for EmbeddingInput {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for EmbeddingInput {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<Vec<String>> for EmbeddingInput {
    fn from(value: Vec<String>) -> Self {
        Self::Texts(value)
    }
}

/// Embeddings 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    /// 模型。
    pub model: ModelId,
    /// 输入。
    pub input: EmbeddingInput,
    /// 输出维度（部分模型支持）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<u32>,
    /// 编码格式（`float` / `base64`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<String>,
    /// 用户标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl EmbeddingRequest {
    /// 创建构建器。
    pub fn builder() -> EmbeddingRequestBuilder {
        EmbeddingRequestBuilder::default()
    }
}

/// [`EmbeddingRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct EmbeddingRequestBuilder {
    model: Option<ModelId>,
    input: Option<EmbeddingInput>,
    dimensions: Option<u32>,
    encoding_format: Option<String>,
    user: Option<String>,
    extensions: Extensions,
}

impl EmbeddingRequestBuilder {
    /// 设置模型。
    #[must_use]
    pub fn model(mut self, model: impl Into<ModelId>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// 设置输入。
    #[must_use]
    pub fn input(mut self, input: impl Into<EmbeddingInput>) -> Self {
        self.input = Some(input.into());
        self
    }

    /// 设置输出维度。
    #[must_use]
    pub fn dimensions(mut self, dimensions: u32) -> Self {
        self.dimensions = Some(dimensions);
        self
    }

    /// 设置编码格式。
    #[must_use]
    pub fn encoding_format(mut self, format: impl Into<String>) -> Self {
        self.encoding_format = Some(format.into());
        self
    }

    /// 设置用户标识。
    #[must_use]
    pub fn user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// 添加扩展字段。
    #[must_use]
    pub fn extension(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
    }

    /// 构建请求。
    pub fn build(self) -> Result<EmbeddingRequest, AirxError> {
        let model = self.model.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("EmbeddingRequest.model 必填"))
        })?;
        let input = self.input.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("EmbeddingRequest.input 必填"))
        })?;
        Ok(EmbeddingRequest {
            model,
            input,
            dimensions: self.dimensions,
            encoding_format: self.encoding_format,
            user: self.user,
            extensions: self.extensions,
        })
    }
}

/// Embeddings 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    /// 对象类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 向量结果。
    #[serde(default)]
    pub data: Vec<Embedding>,
    /// 实际模型。
    #[serde(default)]
    pub model: String,
    /// 用量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<EmbeddingUsage>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 单个向量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Embedding {
    /// 对象类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 向量（`float` 数组或 base64 编码的 float32 序列）。
    #[serde(default)]
    pub embedding: EmbeddingVector,
    /// 序号。
    #[serde(default)]
    pub index: u32,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 向量编码：浮点数组或 base64。
///
/// 官方 `encoding_format` 默认 `float`；选择 `base64` 时响应是
/// base64 字符串（小端 float32 序列），必须解码后才能使用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbeddingVector {
    /// `encoding_format = "float"`。
    Float(Vec<f32>),
    /// `encoding_format = "base64"`。
    Base64(String),
}

impl Default for EmbeddingVector {
    fn default() -> Self {
        Self::Float(Vec::new())
    }
}

impl EmbeddingVector {
    /// 是否为浮点数组。
    pub fn is_float(&self) -> bool {
        matches!(self, Self::Float(_))
    }

    /// 直接取浮点数组（base64 时返回 None）。
    pub fn as_float(&self) -> Option<&[f32]> {
        match self {
            Self::Float(values) => Some(values),
            Self::Base64(_) => None,
        }
    }

    /// 解码为浮点数组。
    ///
    /// base64 按小端 float32 解码；非法 base64 或长度不是 4 的倍数时返回
    /// `AirxError::Deserialization`，便于调用方区分“服务端给了坏数据”。
    pub fn to_f32(&self) -> Result<Vec<f32>, AirxError> {
        match self {
            Self::Float(values) => Ok(values.clone()),
            Self::Base64(encoded) => {
                let bytes = decode_base64(encoded).map_err(|message| {
                    AirxError::Deserialization(DeserializationError::new(format!(
                        "Embedding base64 解码失败: {message}"
                    )))
                })?;
                if bytes.len() % 4 != 0 {
                    return Err(AirxError::Deserialization(DeserializationError::new(
                        "Embedding base64 长度不是 4 的倍数",
                    )));
                }
                Ok(bytes
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect())
            }
        }
    }
}

/// 兼容旧代码：`assert_eq!(embedding, vec![...])` 只对浮点数组成立。
impl PartialEq<Vec<f32>> for EmbeddingVector {
    fn eq(&self, other: &Vec<f32>) -> bool {
        matches!(self, Self::Float(values) if values == other)
    }
}

/// 解码标准 base64（容忍空白字符与缺失填充）。
pub(crate) fn decode_base64(input: &str) -> Result<Vec<u8>, &'static str> {
    let mut output = Vec::with_capacity(input.len() / 4 * 3);
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    let mut padding = 0usize;

    for byte in input.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            padding += 1;
            continue;
        }
        if padding > 0 {
            return Err("填充字符之后仍有数据");
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("包含非法 base64 字符"),
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }
    }

    if bits >= 6 {
        return Err("base64 长度非法");
    }
    if padding > 2 {
        return Err("base64 填充非法");
    }
    Ok(output)
}

/// Embeddings 用量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingUsage {
    /// 输入 token。
    #[serde(default)]
    pub prompt_tokens: u32,
    /// 总计。
    #[serde(default)]
    pub total_tokens: u32,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// Embeddings 端点。
#[derive(Debug, Clone)]
pub struct EmbeddingsEndpoint {
    config: OpenAiConfig,
}

impl EmbeddingsEndpoint {
    /// 创建端点。
    pub fn new(config: OpenAiConfig) -> Self {
        Self { config }
    }

    fn uri(&self) -> Result<airx_core::http::Uri, AirxError> {
        self.config.endpoint_uri(EMBEDDINGS_PATH, "embeddings")
    }
}

impl Endpoint for EmbeddingsEndpoint {
    type Request = EmbeddingRequest;
    type Response = EmbeddingResponse;

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

    fn decode(&self, response: HttpResponse) -> Result<EmbeddingResponse, AirxError> {
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
    fn request_serializes_text_and_model() {
        let request = EmbeddingRequest::builder()
            .model("text-embedding-3-small")
            .input("hello")
            .dimensions(256)
            .build()
            .expect("request");
        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["model"], "text-embedding-3-small");
        assert_eq!(value["input"], "hello");
        assert_eq!(value["dimensions"], 256);
    }

    #[test]
    fn response_decodes_vector() {
        let response: EmbeddingResponse = serde_json::from_value(json!({
            "object": "list",
            "data": [{"object": "embedding", "embedding": [0.1, 0.2], "index": 0}],
            "model": "text-embedding-3-small",
            "usage": {"prompt_tokens": 2, "total_tokens": 2}
        }))
        .expect("response");
        assert_eq!(response.data[0].embedding, vec![0.1, 0.2]);
        assert!(response.data[0].embedding.is_float());
        assert_eq!(
            response.data[0].embedding.to_f32().expect("decode"),
            vec![0.1, 0.2]
        );
        assert_eq!(response.usage.as_ref().map(|u| u.total_tokens), Some(2));
    }

    #[test]
    fn response_decodes_base64_vector() {
        // 两个小端 float32：[0.1, 0.25]
        let values = [0.1f32, 0.25f32];
        let mut bytes = Vec::new();
        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        let encoded = encode_for_test(&bytes);

        let response: EmbeddingResponse = serde_json::from_value(json!({
            "object": "list",
            "data": [{"object": "embedding", "embedding": encoded, "index": 0}],
            "model": "text-embedding-3-small"
        }))
        .expect("response");

        let vector = &response.data[0].embedding;
        assert!(matches!(vector, EmbeddingVector::Base64(_)));
        assert!(vector.as_float().is_none());
        assert_eq!(vector.to_f32().expect("decode"), values.to_vec());
        // 往返不丢编码形式。
        let roundtrip = serde_json::to_value(&response).expect("serialize");
        assert_eq!(roundtrip["data"][0]["embedding"], encoded);
    }

    #[test]
    fn bad_base64_is_a_deserialization_error() {
        let vector = EmbeddingVector::Base64("!!!not-base64!!!".to_owned());
        match vector.to_f32() {
            Err(AirxError::Deserialization(_)) => {}
            other => panic!("unexpected: {other:?}"),
        }

        let vector = EmbeddingVector::Base64("QUJD".to_owned()); // 3 bytes
        match vector.to_f32() {
            Err(AirxError::Deserialization(_)) => {}
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn base64_decoder_handles_padding_and_whitespace() {
        assert_eq!(decode_base64("TWFu").expect("decode"), b"Man".to_vec());
        assert_eq!(decode_base64("TWE=").expect("decode"), b"Ma".to_vec());
        assert_eq!(decode_base64("TQ==").expect("decode"), b"M".to_vec());
        assert_eq!(decode_base64("TW Fu\n").expect("decode"), b"Man".to_vec());
        assert!(decode_base64("T").is_err());
        assert!(decode_base64("TQ===").is_err());
        assert!(decode_base64("TQ==A").is_err());
    }

    /// 测试用最小 base64 编码器（避免为测试引入依赖）。
    fn encode_for_test(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b0 = u32::from(chunk[0]);
            let b1 = chunk.get(1).copied().map(u32::from).unwrap_or(0);
            let b2 = chunk.get(2).copied().map(u32::from).unwrap_or(0);
            let triple = (b0 << 16) | (b1 << 8) | b2;
            out.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
            out.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
            if chunk.len() > 1 {
                out.push(ALPHABET[((triple >> 6) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(ALPHABET[(triple & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
    }
}

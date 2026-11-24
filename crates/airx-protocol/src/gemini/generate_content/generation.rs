//! Gemini `generationConfig` 与安全设置。

use airx_core::json::Extensions;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 生成配置。
///
/// 字段名与官方 `generationConfig` 的 camelCase 名称一一对应；
/// 未建模字段通过 [`GenerationConfig::extensions`] 透传。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    /// 采样温度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// 核采样。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// top-k 采样。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// 候选数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_count: Option<u32>,
    /// 最大输出 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    /// 停止序列。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// 输出 MIME 类型（`text/plain` / `application/json`）。
    ///
    /// SDK 3.6.1 已把 `responseMimeType` / `responseSchema` 标为 deprecated，
    /// 推荐 `responseFormat`；REST 仍按现行字段接受，airx 暂保留这对字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_mime_type: Option<String>,
    /// 结构化输出 JSON Schema（OpenAPI 子集）。
    ///
    /// SDK 3.6.1 标为 deprecated（推荐 `responseFormat`）；新代码优先
    /// [`GenerationConfig::response_json_schema`]（标准 JSON Schema）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<Value>,
    /// 结构化输出 JSON Schema（标准 JSON Schema）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_json_schema: Option<Value>,
    /// 存在惩罚。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// 频率惩罚。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// 随机种子。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    /// 输出模态（如 `["TEXT"]` / `["TEXT", "IMAGE"]`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_modalities: Option<Vec<String>>,
    /// thinking 配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_config: Option<ThinkingConfig>,
    /// 媒体分辨率。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_resolution: Option<String>,
    /// 语音配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speech_config: Option<SpeechConfig>,
    /// 图片配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_config: Option<ImageConfig>,
    /// 是否导出 logprobs。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_logprobs: Option<bool>,
    /// 每步返回的 top logprobs 数量（`0..=20`，需同时设 `response_logprobs`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<u32>,
    /// 翻译配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation_config: Option<TranslationConfig>,
    /// 语音转写配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_transcription_config: Option<AudioTranscriptionConfig>,
    /// 是否开启情绪自适应回复。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_affective_dialog: Option<bool>,
    /// 是否开启增强的公民回答。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_enhanced_civic_answers: Option<bool>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl GenerationConfig {
    /// 设置温度。
    #[must_use]
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// 设置最大输出 token。
    #[must_use]
    pub fn with_max_output_tokens(mut self, max_output_tokens: u32) -> Self {
        self.max_output_tokens = Some(max_output_tokens);
        self
    }

    /// 设置停止序列。
    #[must_use]
    pub fn with_stop_sequences(
        mut self,
        stop: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.stop_sequences = Some(stop.into_iter().map(Into::into).collect());
        self
    }

    /// 要求 JSON 输出并指定 OpenAPI 子集 Schema（`responseSchema`）。
    ///
    /// SDK 3.6.1 已把 `responseSchema` 标为 deprecated（推荐 `responseFormat`）；
    /// 需要标准 JSON Schema 时用 [`GenerationConfig::with_json_schema_std`]。
    #[must_use]
    pub fn with_json_schema(mut self, schema: Value) -> Self {
        self.response_mime_type = Some("application/json".to_owned());
        self.response_schema = Some(schema);
        self
    }

    /// 要求 JSON 输出并指定标准 JSON Schema（`responseJsonSchema`）。
    #[must_use]
    pub fn with_json_schema_std(mut self, schema: Value) -> Self {
        self.response_mime_type = Some("application/json".to_owned());
        self.response_json_schema = Some(schema);
        self
    }

    /// 设置 thinking 配置。
    #[must_use]
    pub fn with_thinking(mut self, thinking: ThinkingConfig) -> Self {
        self.thinking_config = Some(thinking);
        self
    }
}

/// thinking 配置。
///
/// Gemini 2.5 使用 `thinkingBudget`（token 预算，`-1` 表示动态，`0` 表示关闭）；
/// Gemini 3 使用 `thinkingLevel`。两者都保留，避免因模型代际切换而丢字段。
///
/// 注意：Gemini 4 同时发送 `thinkingBudget` 与 `thinkingLevel` 会返回 400；
/// 构造器会自动清空另一个字段，但直接写结构体字面量时需自行保证互斥。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingConfig {
    /// 思考预算（-1 动态 / 0 关闭 / 正数为上限）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<i32>,
    /// 是否返回思考内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_thoughts: Option<bool>,
    /// 思考等级（`minimal` / `low` / `medium` / `high` / `xhigh`，大小写以官方示例为准）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ThinkingConfig {
    /// 指定预算，并要求返回思考内容。
    pub fn budget(tokens: i32) -> Self {
        Self {
            thinking_budget: Some(tokens),
            include_thoughts: Some(true),
            thinking_level: None,
            extensions: Extensions::new(),
        }
    }

    /// 动态思考预算（`-1`）。
    pub fn dynamic() -> Self {
        Self::budget(-1)
    }

    /// 关闭思考（`0`）。
    pub fn disabled() -> Self {
        Self {
            thinking_budget: Some(0),
            include_thoughts: None,
            thinking_level: None,
            extensions: Extensions::new(),
        }
    }

    /// 指定思考等级（Gemini 3）。
    pub fn level(level: impl Into<String>) -> Self {
        Self {
            thinking_budget: None,
            include_thoughts: Some(true),
            thinking_level: Some(level.into()),
            extensions: Extensions::new(),
        }
    }
}

/// 语音配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechConfig {
    /// 单说话人语音。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_config: Option<VoiceConfig>,
    /// 语言代码（如 `en-US`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_code: Option<String>,
    /// 多说话人配置（结构随官方演进，原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_speaker_voice_config: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl SpeechConfig {
    /// 预置音色（`{"prebuiltVoiceConfig":{"voiceName": ...}}`）。
    pub fn prebuilt(voice_name: impl Into<String>) -> Self {
        Self {
            voice_config: Some(VoiceConfig {
                prebuilt_voice_config: Some(PrebuiltVoiceConfig {
                    voice_name: Some(voice_name.into()),
                    extensions: Extensions::new(),
                }),
                voice: None,
                extensions: Extensions::new(),
            }),
            language_code: None,
            multi_speaker_voice_config: None,
            extensions: Extensions::new(),
        }
    }
}

/// 单说话人语音配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceConfig {
    /// 预置音色。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prebuilt_voice_config: Option<PrebuiltVoiceConfig>,
    /// 语音 ID（存储语音 / 克隆语音）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 预置音色。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrebuiltVoiceConfig {
    /// 音色名（如 `Aoede`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_name: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 图片生成配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageConfig {
    /// 宽高比（如 `1:1` / `16:9`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<String>,
    /// 图片尺寸（如 `1K` / `2K`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_size: Option<String>,
    /// 人物生成策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person_generation: Option<String>,
    /// 输出 MIME 类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_mime_type: Option<String>,
    /// 输出压缩质量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_compression_quality: Option<u32>,
    /// 更多输出选项（原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_output_options: Option<Value>,
    /// 显著人物策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prominent_people: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 翻译配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationConfig {
    /// 目标语言（BCP-47，如 `en` / `es`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_language_code: Option<String>,
    /// 目标语言是否也生成音频。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub echo_target_language: Option<bool>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 语音转写配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTranscriptionConfig {
    /// 语言提示（BCP-47）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_codes: Option<Vec<String>>,
    /// 自动语言检测（结构随官方演进，原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_auto: Option<Value>,
    /// 语言提示结构（原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_hints: Option<Value>,
    /// 自定义词表。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_vocabulary: Option<Vec<String>>,
    /// 适配短语。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptation_phrases: Option<Vec<String>>,
    /// 是否输出词级时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub word_timestamp: Option<bool>,
    /// 是否区分说话人。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diarization: Option<bool>,
    /// 转写模式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// 源文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_text: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 服务等级（请求顶层 `serviceTier`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceTier {
    /// 未指定（等同 standard）。
    Unspecified,
    /// 标准。
    Standard,
    /// 弹性（更便宜、更慢）。
    Flex,
    /// 优先。
    Priority,
}

/// 安全设置。
///
/// 官方 `category` / `threshold` 均为 optional；`method` 为 Vertex 专属，不建模。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetySetting {
    /// 安全类别（如 `HARM_CATEGORY_HARASSMENT`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// 拦截阈值（如 `BLOCK_MEDIUM_AND_ABOVE`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl SafetySetting {
    /// 构造安全设置。
    pub fn new(category: impl Into<String>, threshold: impl Into<String>) -> Self {
        Self {
            category: Some(category.into()),
            threshold: Some(threshold.into()),
            extensions: Extensions::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn generation_config_uses_official_field_names() {
        let config = GenerationConfig::default()
            .with_temperature(0.2)
            .with_max_output_tokens(512)
            .with_stop_sequences(["END"])
            .with_json_schema(json!({"type": "object"}))
            .with_thinking(ThinkingConfig::dynamic());

        let value = serde_json::to_value(&config).expect("serialize");
        let temperature = value["temperature"].as_f64().expect("temperature");
        assert!((temperature - 0.2).abs() < 1e-6);
        assert_eq!(value["maxOutputTokens"], 512);
        assert_eq!(value["stopSequences"], json!(["END"]));
        assert_eq!(value["responseMimeType"], "application/json");
        assert_eq!(value["responseSchema"]["type"], "object");
        assert_eq!(value["thinkingConfig"]["thinkingBudget"], -1);
        assert_eq!(value["thinkingConfig"]["includeThoughts"], true);
    }

    #[test]
    fn json_schema_std_writes_response_json_schema() {
        let config = GenerationConfig::default()
            .with_json_schema_std(json!({"type": "object", "additionalProperties": false}));
        let value = serde_json::to_value(&config).expect("serialize");

        assert_eq!(value["responseMimeType"], "application/json");
        assert_eq!(value["responseJsonSchema"]["type"], "object");
        assert!(value.get("responseSchema").is_none());
    }

    #[test]
    fn unknown_generation_fields_are_preserved() {
        let config: GenerationConfig = serde_json::from_value(json!({
            "temperature": 0.5,
            "futureKnob": {"enabled": true}
        }))
        .expect("config");

        assert_eq!(config.temperature, Some(0.5));
        assert!(config.extensions.contains_key("futureKnob"));

        let value = serde_json::to_value(&config).expect("serialize");
        assert_eq!(value["futureKnob"]["enabled"], true);
    }

    #[test]
    fn thinking_level_is_supported_for_gemini3() {
        let value = serde_json::to_value(ThinkingConfig::level("high")).expect("serialize");
        assert_eq!(value["thinkingLevel"], "high");
        assert_eq!(value["includeThoughts"], true);
    }

    #[test]
    fn extended_generation_config_fields_roundtrip() {
        let value = json!({
            "responseLogprobs": true,
            "logprobs": 5,
            "translationConfig": {"targetLanguageCode": "es", "echoTargetLanguage": true},
            "audioTranscriptionConfig": {
                "languageCodes": ["en-US"],
                "wordTimestamp": true,
                "diarization": false
            },
            "enableAffectiveDialog": true,
            "enableEnhancedCivicAnswers": false,
            "speechConfig": {
                "voiceConfig": {"prebuiltVoiceConfig": {"voiceName": "Aoede"}},
                "languageCode": "en-US"
            },
            "imageConfig": {"aspectRatio": "16:9", "imageSize": "2K"}
        });

        let config: GenerationConfig = serde_json::from_value(value.clone()).expect("config");
        assert_eq!(config.logprobs, Some(5));
        assert_eq!(config.response_logprobs, Some(true));
        assert_eq!(
            config
                .speech_config
                .as_ref()
                .and_then(|speech| speech.voice_config.as_ref())
                .and_then(|voice| voice.prebuilt_voice_config.as_ref())
                .and_then(|prebuilt| prebuilt.voice_name.as_deref()),
            Some("Aoede")
        );
        assert_eq!(
            config
                .image_config
                .as_ref()
                .and_then(|image| image.aspect_ratio.as_deref()),
            Some("16:9")
        );

        assert_eq!(serde_json::to_value(&config).expect("serialize"), value);
    }

    #[test]
    fn safety_setting_serializes() {
        let value = serde_json::to_value(SafetySetting::new(
            "HARM_CATEGORY_HARASSMENT",
            "BLOCK_MEDIUM_AND_ABOVE",
        ))
        .expect("serialize");
        assert_eq!(value["category"], "HARM_CATEGORY_HARASSMENT");
        assert_eq!(value["threshold"], "BLOCK_MEDIUM_AND_ABOVE");
    }

    #[test]
    fn safety_setting_fields_are_optional() {
        let setting: SafetySetting =
            serde_json::from_value(json!({"category": "HARM_CATEGORY_HARASSMENT"}))
                .expect("setting");
        assert_eq!(setting.threshold, None);

        let empty: SafetySetting = serde_json::from_value(json!({})).expect("empty");
        assert_eq!(empty.category, None);
    }

    #[test]
    fn service_tier_is_lowercase() {
        let value = serde_json::to_value(ServiceTier::Flex).expect("serialize");
        assert_eq!(value, "flex");
        let parsed: ServiceTier = serde_json::from_value(json!("priority")).expect("tier");
        assert_eq!(parsed, ServiceTier::Priority);
    }
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_mime_type: Option<String>,
    /// 结构化输出 JSON Schema（OpenAPI 子集）。
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
    /// 语音配置（原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speech_config: Option<Value>,
    /// 图片配置（原样保留）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_config: Option<Value>,
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

    /// 要求 JSON 输出并指定 Schema。
    #[must_use]
    pub fn with_json_schema(mut self, schema: Value) -> Self {
        self.response_mime_type = Some("application/json".to_owned());
        self.response_schema = Some(schema);
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingConfig {
    /// 思考预算（-1 动态 / 0 关闭 / 正数为上限）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<i32>,
    /// 是否返回思考内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_thoughts: Option<bool>,
    /// 思考等级（如 `low` / `high`）。
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

/// 安全设置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetySetting {
    /// 安全类别（如 `HARM_CATEGORY_HARASSMENT`）。
    pub category: String,
    /// 拦截阈值（如 `BLOCK_MEDIUM_AND_ABOVE`）。
    pub threshold: String,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl SafetySetting {
    /// 构造安全设置。
    pub fn new(category: impl Into<String>, threshold: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            threshold: threshold.into(),
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
    fn safety_setting_serializes() {
        let value = serde_json::to_value(SafetySetting::new(
            "HARM_CATEGORY_HARASSMENT",
            "BLOCK_MEDIUM_AND_ABOVE",
        ))
        .expect("serialize");
        assert_eq!(value["category"], "HARM_CATEGORY_HARASSMENT");
        assert_eq!(value["threshold"], "BLOCK_MEDIUM_AND_ABOVE");
    }
}

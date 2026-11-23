//! 原始 JSON 片段。
//!
//! [`serde_json::value::RawValue`] 保留原始字节，可避免“解析成 `Value` 再序列化”
//! 造成的数字格式、空白与字段顺序变化。用于签名、代理转发与未知字段透传。

use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_json::value::RawValue;

use crate::error::{AirxError, DeserializationError};

/// 原始 JSON 片段。
pub type RawJson = Box<RawValue>;

/// 解析一段 JSON 文本并保留原始字节。
///
/// 只校验语法，不做结构映射；这样 Provider 新增字段时仍可透传。
pub fn parse_raw(json: &str) -> Result<RawJson, AirxError> {
    RawValue::from_string(json.to_owned())
        .map_err(|err| AirxError::Deserialization(DeserializationError::from(err)))
}

/// 把 `Value` 转成原始 JSON 片段。
///
/// 注意：源已经是 `Value`，此处的“原始”仅指转换后的字节，
/// 无法恢复 `Value` 构建前的空白与字段顺序。
pub fn raw_from_value(value: &Value) -> Result<RawJson, AirxError> {
    serde_json::value::to_raw_value(value)
        .map_err(|err| AirxError::Deserialization(DeserializationError::from(err)))
}

/// 单个未知字段，保留字段名与原始值。
///
/// 与 [`super::value::Extensions`] 的分工：
/// - 需要稳定顺序、结构化访问 → `Extensions`
/// - 需要原样转发、字节保真 → `RawExtension`
#[derive(Debug, Clone)]
pub struct RawExtension {
    /// 字段名。
    pub key: String,
    /// 原始字段值。
    pub value: RawJson,
}

impl RawExtension {
    /// 创建原始扩展字段。
    pub fn new(key: impl Into<String>, value: RawJson) -> Self {
        Self {
            key: key.into(),
            value,
        }
    }

    /// 把原始值反序列化为目标类型。
    pub fn deserialize_value<T: DeserializeOwned>(&self) -> Result<T, AirxError> {
        serde_json::from_str(self.value.get())
            .map_err(|err| AirxError::Deserialization(DeserializationError::from(err)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn raw_json_preserves_number_formatting() {
        // Value 会把 1.0 记为浮点；原始片段保留字面量。
        let raw = parse_raw(r#"{"value":1.0}"#).expect("parse raw");
        assert_eq!(raw.get(), r#"{"value":1.0}"#);
    }

    #[test]
    fn raw_json_rejects_invalid_syntax() {
        assert!(parse_raw("{").is_err());
    }

    #[test]
    fn raw_from_value_roundtrips_semantics() {
        let value = json!({"a": 1, "b": [true, null]});
        let raw = raw_from_value(&value).expect("raw");
        let back: Value = serde_json::from_str(raw.get()).expect("value");
        assert_eq!(back, value);
    }

    #[test]
    fn raw_extension_deserializes_on_demand() {
        let ext = RawExtension::new(
            "thinking",
            parse_raw(r#"{"type":"thinking"}"#).expect("raw"),
        );
        let value: Value = ext.deserialize_value().expect("deserialize");
        assert_eq!(value["type"], "thinking");
    }
}

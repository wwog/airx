//! JSON 输出模式。

use serde::Serialize;

use crate::error::{AirxError, ConfigurationError, SerializationError};

/// JSON 输出模式。
///
/// 不能用单一模式覆盖所有场景：请求要紧凑，日志要好读，
/// 转发要保真，签名 / 缓存 Key 要确定性。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum JsonMode {
    /// 紧凑输出，用于 API 请求。
    #[default]
    Compact,
    /// 带缩进，用于日志与调试。
    Pretty,
    /// 尽量保留原始数据。
    ///
    /// 对 `serde_json::Value` 输入等价于 [`JsonMode::Compact`]：
    /// 要保留原始字节必须使用 [`super::raw::RawJson`]。
    Preserve,
    /// 确定性规范化，用于测试快照、缓存 Key 与签名。
    ///
    /// 尚未实现（计划 M9 接入 RFC 8785 JCS），当前返回
    /// [`AirxError::Configuration`]，避免给出不可靠的“规范化”结果。
    Canonical,
}

/// 按指定模式序列化。
pub fn to_string<T>(value: &T, mode: JsonMode) -> Result<String, AirxError>
where
    T: Serialize + ?Sized,
{
    match mode {
        JsonMode::Compact | JsonMode::Preserve => serde_json::to_string(value)
            .map_err(|err| AirxError::Serialization(SerializationError::from(err))),
        JsonMode::Pretty => serde_json::to_string_pretty(value)
            .map_err(|err| AirxError::Serialization(SerializationError::from(err))),
        JsonMode::Canonical => Err(AirxError::Configuration(ConfigurationError::new(
            "Canonical JSON（RFC 8785 JCS）尚未实现，计划于 M9 接入",
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compact_has_no_whitespace() {
        let value = json!({"a": 1, "b": [1, 2]});
        let text = to_string(&value, JsonMode::Compact).expect("serialize");
        assert_eq!(text, r#"{"a":1,"b":[1,2]}"#);
    }

    #[test]
    fn pretty_is_multiline() {
        let value = json!({"a": 1});
        let text = to_string(&value, JsonMode::Pretty).expect("serialize");
        assert!(text.contains('\n'));
    }

    #[test]
    fn preserve_matches_compact_for_values() {
        let value = json!({"a": 1, "b": 2});
        let compact = to_string(&value, JsonMode::Compact).expect("compact");
        let preserve = to_string(&value, JsonMode::Preserve).expect("preserve");
        assert_eq!(compact, preserve);
    }

    #[test]
    fn canonical_is_explicitly_unsupported() {
        let err = to_string(&json!({"a": 1}), JsonMode::Canonical).expect_err("not supported");
        assert!(matches!(err, AirxError::Configuration(_)));
    }
}

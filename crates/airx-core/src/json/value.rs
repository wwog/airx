//! 动态 JSON 值与未知字段容器。

pub use serde_json::{Map, Value};

/// 协议扩展字段容器。
///
/// 使用 `BTreeMap`：提供确定性顺序，便于测试与快照对比。
/// 但**不要**把字段顺序当作协议语义——JSON 对象本身无序；
/// 需要字节级保真时使用 [`super::raw::RawExtension`]。
pub type Extensions = std::collections::BTreeMap<String, Value>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    struct Envelope {
        model: String,
        #[serde(flatten)]
        extensions: Extensions,
    }

    #[test]
    fn extensions_keep_unknown_fields_in_both_directions() {
        let json = r#"{"model":"m","temperature":0.2,"custom":{"nested":true}}"#;
        let envelope: Envelope = serde_json::from_str(json).expect("deserialize");

        assert_eq!(envelope.model, "m");
        assert_eq!(envelope.extensions.len(), 2);
        assert_eq!(envelope.extensions["temperature"], 0.2);
        assert_eq!(envelope.extensions["custom"]["nested"], true);

        let text = serde_json::to_string(&envelope).expect("serialize");
        let value: Value = serde_json::from_str(&text).expect("parse");
        assert_eq!(value["temperature"], 0.2);
        assert_eq!(value["custom"]["nested"], true);
    }

    #[test]
    fn extensions_use_deterministic_order() {
        let mut extensions = Extensions::new();
        extensions.insert("z".to_owned(), Value::from(1));
        extensions.insert("a".to_owned(), Value::from(2));

        let text = serde_json::to_string(&extensions).expect("serialize");
        assert_eq!(text, r#"{"a":2,"z":1}"#);
    }
}

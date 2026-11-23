//! 增量 JSON 累积。
//!
//! 工具调用参数会以 delta 片段到达，且**不能假设每个片段都是完整 JSON**：
//!
//! ```text
//! {"city":"Singa   +   pore","unit":"celsius"}
//! ```
//!
//! 这里只做**结构完整性判断**（括号 / 字符串是否闭合），
//! 完整后才执行一次反序列化；不对每个 delta 做完整解析。
//!
//! 结构判断是保守的：它不校验 `,` / `:` 的合法性，
//! 因此 `Complete` 只表示“可以尝试解析”，语法错误仍由 serde 报告。

use serde::de::DeserializeOwned;

use crate::error::{AirxError, DeserializationError, StreamError};

/// JSON 片段的结构状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JsonState {
    /// 还没有任何非空白内容。
    #[default]
    Empty,
    /// 已开始但尚未闭合。
    Incomplete,
    /// 结构已闭合，可尝试解析。
    Complete,
    /// 结构非法（多余闭合符、尾随内容、非法起始字节）。
    Malformed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootKind {
    Object,
    Array,
    String,
    Number,
    Literal,
}

/// 增量 JSON 累积器。
#[derive(Debug, Default)]
pub struct JsonAccumulator {
    buf: String,
    pos: usize,
    state: JsonState,
    depth: usize,
    in_string: bool,
    escaped: bool,
    root: Option<RootKind>,
    literal: Option<&'static str>,
    literal_matched: usize,
}

impl JsonAccumulator {
    /// 创建累积器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一个片段。
    ///
    /// 片段必须是合法 UTF-8（来自 JSON 字符串字段，天然满足）。
    pub fn push_str(&mut self, fragment: &str) {
        self.buf.push_str(fragment);
        self.scan();
    }

    /// 当前累积的原始文本。
    pub fn as_str(&self) -> &str {
        &self.buf
    }

    /// 已累积的字节数。
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// 是否尚未累积任何内容。
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// 当前结构状态。
    pub fn state(&self) -> JsonState {
        self.state
    }

    /// 结构是否已闭合。
    pub fn is_complete(&self) -> bool {
        self.state == JsonState::Complete
    }

    /// 结束累积：为顶层数值补上“已完成”判定。
    ///
    /// 对象 / 数组 / 字符串不会因为流结束而变成完整，
    /// 未闭合仍保持 [`JsonState::Incomplete`]。
    pub fn finish(&mut self) -> JsonState {
        if self.state == JsonState::Incomplete && matches!(self.root, Some(RootKind::Number)) {
            self.state = if serde_json::from_str::<serde_json::Number>(self.buf.trim()).is_ok() {
                JsonState::Complete
            } else {
                JsonState::Malformed
            };
        }
        self.state
    }

    /// 反序列化为目标类型。
    pub fn parse<T: DeserializeOwned>(&self) -> Result<T, AirxError> {
        match self.state {
            JsonState::Complete => serde_json::from_str(&self.buf)
                .map_err(|err| AirxError::Deserialization(DeserializationError::from(err))),
            JsonState::Malformed => Err(AirxError::Deserialization(DeserializationError::new(
                "JSON 片段包含多余闭合符或尾随内容",
            ))),
            JsonState::Empty | JsonState::Incomplete => Err(AirxError::Stream(
                StreamError::incomplete("JSON 片段尚未闭合"),
            )),
        }
    }

    /// 反序列化为动态 [`serde_json::Value`]。
    pub fn parse_value(&self) -> Result<serde_json::Value, AirxError> {
        self.parse()
    }

    /// 清空状态，复用累积器。
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// 取出累积文本。
    pub fn into_string(self) -> String {
        self.buf
    }

    fn scan(&mut self) {
        while self.pos < self.buf.len() {
            let byte = self.buf.as_bytes()[self.pos];
            match self.state {
                JsonState::Malformed => return,
                JsonState::Empty => {
                    if byte.is_ascii_whitespace() {
                        self.pos += 1;
                        continue;
                    }
                    self.begin(byte);
                }
                JsonState::Complete => {
                    if byte.is_ascii_whitespace() {
                        self.pos += 1;
                        continue;
                    }
                    self.state = JsonState::Malformed;
                    return;
                }
                JsonState::Incomplete => self.advance(byte),
            }
        }
    }

    fn begin(&mut self, byte: u8) {
        match byte {
            b'{' => {
                self.root = Some(RootKind::Object);
                self.depth = 1;
                self.state = JsonState::Incomplete;
                self.pos += 1;
            }
            b'[' => {
                self.root = Some(RootKind::Array);
                self.depth = 1;
                self.state = JsonState::Incomplete;
                self.pos += 1;
            }
            b'"' => {
                self.root = Some(RootKind::String);
                self.in_string = true;
                self.escaped = false;
                self.state = JsonState::Incomplete;
                self.pos += 1;
            }
            b'-' | b'0'..=b'9' => {
                self.root = Some(RootKind::Number);
                self.state = JsonState::Incomplete;
                self.pos += 1;
            }
            b't' => self.begin_literal("true"),
            b'f' => self.begin_literal("false"),
            b'n' => self.begin_literal("null"),
            _ => self.state = JsonState::Malformed,
        }
    }

    fn begin_literal(&mut self, target: &'static str) {
        self.root = Some(RootKind::Literal);
        self.literal = Some(target);
        self.literal_matched = 1;
        self.state = JsonState::Incomplete;
        self.pos += 1;
    }

    fn advance(&mut self, byte: u8) {
        if self.in_string {
            if self.escaped {
                self.escaped = false;
                self.pos += 1;
                return;
            }
            match byte {
                b'\\' => {
                    self.escaped = true;
                    self.pos += 1;
                }
                b'"' => {
                    self.in_string = false;
                    self.pos += 1;
                    if matches!(self.root, Some(RootKind::String)) {
                        self.state = JsonState::Complete;
                    }
                }
                _ => self.pos += 1,
            }
            return;
        }

        match self.root {
            Some(RootKind::Literal) => {
                self.scan_literal();
                return;
            }
            Some(RootKind::Number) => {
                self.scan_number(byte);
                return;
            }
            _ => {}
        }

        match byte {
            b'"' => {
                self.in_string = true;
                self.escaped = false;
                self.pos += 1;
            }
            b'{' | b'[' => {
                self.depth += 1;
                self.pos += 1;
            }
            b'}' | b']' => {
                if self.depth == 0 {
                    self.state = JsonState::Malformed;
                    return;
                }
                self.depth -= 1;
                self.pos += 1;
                if self.depth == 0 {
                    self.state = JsonState::Complete;
                }
            }
            _ => self.pos += 1,
        }
    }

    fn scan_literal(&mut self) {
        let Some(target) = self.literal else {
            self.state = JsonState::Malformed;
            return;
        };
        let target = target.as_bytes();

        while self.pos < self.buf.len() && self.literal_matched < target.len() {
            if self.buf.as_bytes()[self.pos] != target[self.literal_matched] {
                self.state = JsonState::Malformed;
                return;
            }
            self.literal_matched += 1;
            self.pos += 1;
        }

        if self.literal_matched == target.len() {
            self.state = JsonState::Complete;
        }
    }

    fn scan_number(&mut self, byte: u8) {
        if byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E') {
            self.pos += 1;
        } else if byte.is_ascii_whitespace() {
            self.pos += 1;
            self.state = JsonState::Complete;
        } else {
            self.state = JsonState::Malformed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_completes_only_after_final_brace() {
        let text = r#"{"city":"Singapore","unit":"celsius"}"#;
        for (index, _) in text.char_indices() {
            let mut acc = JsonAccumulator::new();
            acc.push_str(&text[..index]);
            assert_ne!(
                acc.state(),
                JsonState::Complete,
                "prefix {} should not be complete: {:?}",
                index,
                &text[..index]
            );
        }

        let mut acc = JsonAccumulator::new();
        acc.push_str(text);
        assert_eq!(acc.state(), JsonState::Complete);
        assert_eq!(
            acc.parse_value().expect("parse"),
            json!({
                "city": "Singapore",
                "unit": "celsius",
            })
        );
    }

    #[test]
    fn one_byte_at_a_time_reaches_complete() {
        let text = r#"{"a":[1,2,{"b":true}]}"#;
        let mut acc = JsonAccumulator::new();
        for byte in text.bytes() {
            acc.push_str(std::str::from_utf8(&[byte]).expect("ascii"));
        }
        assert!(acc.is_complete());
        let value: serde_json::Value = acc.parse().expect("parse");
        assert_eq!(value["a"][2]["b"], true);
    }

    #[test]
    fn braces_inside_strings_do_not_count() {
        let mut acc = JsonAccumulator::new();
        acc.push_str(r#"{"text":"a } [ \" b"}"#);
        assert!(acc.is_complete());
        let value: serde_json::Value = acc.parse().expect("parse");
        assert_eq!(value["text"], r#"a } [ " b"#);
    }

    #[test]
    fn trailing_content_is_malformed() {
        let mut acc = JsonAccumulator::new();
        acc.push_str(r#"{"a":1}}"#);
        assert_eq!(acc.state(), JsonState::Malformed);
        assert!(acc.parse::<serde_json::Value>().is_err());
    }

    #[test]
    fn empty_and_whitespace_are_empty() {
        let mut acc = JsonAccumulator::new();
        assert_eq!(acc.state(), JsonState::Empty);
        acc.push_str("  \n\t");
        assert_eq!(acc.state(), JsonState::Empty);
    }

    #[test]
    fn literals_and_numbers_are_supported() {
        let mut acc = JsonAccumulator::new();
        acc.push_str("tru");
        assert_eq!(acc.state(), JsonState::Incomplete);
        acc.push_str("e");
        assert_eq!(acc.state(), JsonState::Complete);

        let mut acc = JsonAccumulator::new();
        acc.push_str("12.5");
        assert_eq!(acc.state(), JsonState::Incomplete);
        assert_eq!(acc.finish(), JsonState::Complete);
        assert_eq!(acc.parse::<f64>().expect("number"), 12.5);
    }

    #[test]
    fn broken_literal_is_malformed() {
        let mut acc = JsonAccumulator::new();
        acc.push_str("trux");
        assert_eq!(acc.state(), JsonState::Malformed);
    }

    #[test]
    fn string_root_completes_at_closing_quote() {
        let mut acc = JsonAccumulator::new();
        acc.push_str(r#""hello"#);
        assert_eq!(acc.state(), JsonState::Incomplete);
        acc.push_str("\"");
        assert_eq!(acc.state(), JsonState::Complete);
        assert_eq!(acc.parse::<String>().expect("string"), "hello");
    }

    #[test]
    fn incomplete_parse_is_a_stream_error() {
        let mut acc = JsonAccumulator::new();
        acc.push_str(r#"{"a":"#);
        let err = acc.parse::<serde_json::Value>().expect_err("incomplete");
        assert!(matches!(err, AirxError::Stream(_)));
    }

    #[test]
    fn clear_resets_state() {
        let mut acc = JsonAccumulator::new();
        acc.push_str(r#"{"a":1}"#);
        assert!(acc.is_complete());
        acc.clear();
        assert_eq!(acc.state(), JsonState::Empty);
        assert!(acc.is_empty());
    }
}

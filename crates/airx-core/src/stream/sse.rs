//! SSE 字段语法。
//!
//! 只实现语法层：`event` / `data` / `id` / `retry` / 注释。
//! 事件语义（例如 OpenAI 的 `[DONE]`、Anthropic 的事件名集合）由协议层处理。

/// 一个已完成分发的 SSE 事件。
///
/// `data` 是多行 `data:` 以 `\n` 连接后的结果（不含末尾换行）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawEvent {
    /// `event:` 字段。
    pub event: Option<String>,
    /// 合并后的 `data:` 内容。
    pub data: String,
    /// `id:` 字段（仅当前事件显式声明时存在）。
    pub id: Option<String>,
    /// `retry:` 字段（毫秒）。
    pub retry: Option<u64>,
}

/// 一行 SSE 的解析结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SseField<'a> {
    /// 以 `:` 开头的注释行。
    Comment(&'a str),
    /// 字段行；没有 `:` 时 `value` 为空字符串。
    Field {
        /// 字段名。
        name: &'a str,
        /// 字段值（已去掉一个可选的前导空格）。
        value: &'a str,
    },
}

/// 解析一行 SSE（不含行结束符）。
///
/// 空行的语义是“分发事件”，由 [`super::decoder::SseDecoder`] 处理，
/// 不会进入本函数。
pub fn parse_field(line: &str) -> SseField<'_> {
    if let Some(comment) = line.strip_prefix(':') {
        return SseField::Comment(comment);
    }

    match line.find(':') {
        Some(index) => {
            let name = &line[..index];
            let mut value = &line[index + 1..];
            if let Some(stripped) = value.strip_prefix(' ') {
                value = stripped;
            }
            SseField::Field { name, value }
        }
        None => SseField::Field {
            name: line,
            value: "",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_field() {
        assert_eq!(
            parse_field("event: message_start"),
            SseField::Field {
                name: "event",
                value: "message_start",
            }
        );
    }

    #[test]
    fn json_colon_is_not_a_separator() {
        assert_eq!(
            parse_field(r#"data: {"type":"ping"}"#),
            SseField::Field {
                name: "data",
                value: r#"{"type":"ping"}"#,
            }
        );
    }

    #[test]
    fn only_one_leading_space_is_stripped() {
        assert_eq!(
            parse_field("data:  two"),
            SseField::Field {
                name: "data",
                value: " two",
            }
        );
    }

    #[test]
    fn field_without_colon_has_empty_value() {
        assert_eq!(
            parse_field("data"),
            SseField::Field {
                name: "data",
                value: "",
            }
        );
    }

    #[test]
    fn comment_is_recognized() {
        assert_eq!(
            parse_field(": keep-alive"),
            SseField::Comment(" keep-alive")
        );
    }
}

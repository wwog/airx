//! SSE 事件装配：字节 → [`RawEvent`]。
//!
//! 负责跨 chunk 的状态：事件类型、多行 data、id、retry、注释。
//! 分发规则遵循 SSE 规范：只有以空行结束且 `data` 非空的事件才会产出。

use super::buffer::LineBuffer;
use super::sse::{RawEvent, SseField, parse_field};
use crate::error::StreamError;

/// 增量 SSE 解码器。
#[derive(Debug, Default)]
pub struct SseDecoder {
    lines: LineBuffer,
    event: Option<String>,
    data: String,
    data_lines: usize,
    id: Option<String>,
    retry: Option<u64>,
    last_event_id: Option<String>,
}

impl SseDecoder {
    /// 创建解码器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 写入一段字节。
    pub fn push(&mut self, chunk: &[u8]) {
        self.lines.push(chunk);
    }

    /// 取出下一个完整事件。
    ///
    /// 返回 `Ok(None)` 表示需要更多输入。
    pub fn next_event(&mut self) -> Result<Option<RawEvent>, StreamError> {
        loop {
            let Some(line) = self.lines.next_line()? else {
                return Ok(None);
            };

            if line.is_empty() {
                if self.data.is_empty() {
                    // 规范：data 缓冲区为空时不分发，但仍重置事件字段。
                    self.reset_event();
                    continue;
                }
                return Ok(Some(self.take_event()));
            }

            self.apply_line(&line);
        }
    }

    /// 流结束时冲刷。
    ///
    /// - 若剩余内容构成一个完整的空行，则返回最后一个待分发事件；
    /// - 若存在未以空行结束的事件，返回 [`crate::error::StreamErrorKind::Incomplete`]。
    ///
    /// 网络断开不得被当作成功完成，因此这里不返回 `Ok(Some)` 来“补全”残缺事件。
    pub fn finish(&mut self) -> Result<Option<RawEvent>, StreamError> {
        if let Some(line) = self.lines.finish()? {
            if line.is_empty() {
                if self.data.is_empty() {
                    self.reset_event();
                } else {
                    return Ok(Some(self.take_event()));
                }
            } else {
                self.apply_line(&line);
            }
        }

        if self.data_lines > 0 {
            return Err(StreamError::incomplete("SSE 流在事件结束空行之前终止"));
        }

        Ok(None)
    }

    /// 最近一次显式声明的 `id:`。
    pub fn last_event_id(&self) -> Option<&str> {
        self.last_event_id.as_deref()
    }

    fn apply_line(&mut self, line: &str) {
        match parse_field(line) {
            SseField::Comment(_) => {}
            SseField::Field { name, value } => match name {
                "event" => self.event = Some(value.to_owned()),
                "data" => {
                    if self.data_lines > 0 {
                        self.data.push('\n');
                    }
                    self.data.push_str(value);
                    self.data_lines += 1;
                }
                "id" => {
                    if !value.contains('\0') {
                        self.id = Some(value.to_owned());
                    }
                }
                "retry" if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
                    if let Ok(millis) = value.parse::<u64>() {
                        self.retry = Some(millis);
                    }
                }
                _ => {}
            },
        }
    }

    fn take_event(&mut self) -> RawEvent {
        let event = RawEvent {
            event: self.event.take(),
            data: std::mem::take(&mut self.data),
            id: self.id.take(),
            retry: self.retry.take(),
        };
        self.data_lines = 0;
        if let Some(id) = &event.id {
            self.last_event_id = Some(id.clone());
        }
        event
    }

    fn reset_event(&mut self) {
        self.event = None;
        self.data.clear();
        self.data_lines = 0;
        self.id = None;
        self.retry = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::StreamErrorKind;

    fn decode_all(input: &[u8], chunk_size: usize) -> Vec<RawEvent> {
        let mut decoder = SseDecoder::new();
        let mut events = Vec::new();
        for chunk in input.chunks(chunk_size.max(1)) {
            decoder.push(chunk);
            while let Some(event) = decoder.next_event().expect("event") {
                events.push(event);
            }
        }
        if let Some(event) = decoder.finish().expect("finish") {
            events.push(event);
        }
        events
    }

    #[test]
    fn decodes_openai_style_stream_at_every_chunk_size() {
        let input = concat!(
            "data: {\"delta\":\"你\"}\n\n",
            "data: {\"delta\":\"好\"}\n\n",
            "data: [DONE]\n\n",
        )
        .as_bytes();

        for chunk_size in 1..=input.len() {
            let events = decode_all(input, chunk_size);
            assert_eq!(events.len(), 3, "chunk_size={chunk_size}");
            assert_eq!(events[0].data, r#"{"delta":"你"}"#);
            assert_eq!(events[1].data, r#"{"delta":"好"}"#);
            assert_eq!(events[2].data, "[DONE]");
        }
    }

    #[test]
    fn decodes_anthropic_style_named_events() {
        let input = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\"}\n",
            "\n",
            ": ping\n",
            "\n",
            "event: content_block_delta\n",
            "data: {\"delta\":{\"text\":\"Hi\"}}\n",
            "\n",
        );

        let events = decode_all(input.as_bytes(), 3);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event.as_deref(), Some("message_start"));
        assert_eq!(events[0].data, r#"{"type":"message_start"}"#);
        assert_eq!(events[1].event.as_deref(), Some("content_block_delta"));
    }

    #[test]
    fn multi_line_data_is_joined_with_newline() {
        let input = b"data: first\ndata: second\n\n";
        let events = decode_all(input, 2);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "first\nsecond");
    }

    #[test]
    fn event_without_data_is_not_dispatched() {
        let input = b"event: ping\n\n";
        assert!(decode_all(input, 1).is_empty());
    }

    #[test]
    fn id_and_retry_are_captured() {
        let input = b"id: 42\nretry: 1500\ndata: x\n\n";
        let events = decode_all(input, 1);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id.as_deref(), Some("42"));
        assert_eq!(events[0].retry, Some(1500));
    }

    #[test]
    fn invalid_retry_is_ignored() {
        let input = b"retry: 12x\ndata: x\n\n";
        let events = decode_all(input, 1);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].retry, None);
    }

    #[test]
    fn last_event_id_is_tracked_across_events() {
        let input = b"id: 1\ndata: a\n\ndata: b\n\n";
        let mut decoder = SseDecoder::new();
        decoder.push(input);

        let first = decoder.next_event().expect("event").expect("some");
        assert_eq!(first.id.as_deref(), Some("1"));
        assert_eq!(decoder.last_event_id(), Some("1"));

        let second = decoder.next_event().expect("event").expect("some");
        assert_eq!(second.id, None);
        assert_eq!(decoder.last_event_id(), Some("1"));
    }

    #[test]
    fn unfinished_event_is_incomplete_not_success() {
        let mut decoder = SseDecoder::new();
        decoder.push(b"data: {\"partial\":true}\n");
        assert_eq!(decoder.next_event().expect("need more"), None);

        let err = decoder.finish().expect_err("incomplete");
        assert_eq!(err.kind, StreamErrorKind::Incomplete);
    }

    #[test]
    fn crlf_stream_works() {
        let input = b"event: x\r\ndata: 1\r\n\r\n";
        let events = decode_all(input, 1);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.as_deref(), Some("x"));
        assert_eq!(events[0].data, "1");
    }

    #[test]
    fn final_lone_cr_dispatches_pending_event() {
        let mut decoder = SseDecoder::new();
        decoder.push(b"data: x\n\r");
        assert_eq!(decoder.next_event().expect("need more"), None);

        let event = decoder.finish().expect("finish").expect("event");
        assert_eq!(event.data, "x");
    }
}

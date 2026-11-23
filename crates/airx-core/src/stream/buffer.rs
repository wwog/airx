//! 字节 → 行的增量缓冲。
//!
//! 负责 SSE 的**行边界**：`LF`、`CRLF`、单独的 `CR` 都是行结束符；
//! `CR` 位于缓冲区末尾时必须等待下一个字节，否则会把 `CRLF` 误判为 `CR` + 空行。
//!
//! UTF-8 以整行校验：行结束符都是 ASCII，不会被多字节字符的续字节（>= 0x80）干扰，
//! 因此跨 chunk 的多字节字符在行结束时必然已完整。

use bytes::BytesMut;

use crate::error::StreamError;

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// 行缓冲：接收任意切分的字节，产出完整行。
#[derive(Debug, Default)]
pub struct LineBuffer {
    buf: BytesMut,
    bom_checked: bool,
}

impl LineBuffer {
    /// 创建空缓冲。
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一段字节。
    pub fn push(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
    }

    /// 当前缓冲的字节数。
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// 缓冲是否为空。
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// 取出下一行（不含行结束符）。
    ///
    /// 返回 `Ok(None)` 表示需要更多输入：
    /// 可能是没有行结束符，也可能是末尾的 `CR` 还在等待 `LF`。
    pub fn next_line(&mut self) -> Result<Option<String>, StreamError> {
        if !self.bom_checked && !self.skip_bom() {
            return Ok(None);
        }

        let bytes = &self.buf[..];
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'\n' => return self.take_line(index, index + 1).map(Some),
                b'\r' => {
                    if index + 1 == bytes.len() {
                        // 可能是 CRLF 的前半，等待更多字节。
                        return Ok(None);
                    }
                    let consume = if bytes[index + 1] == b'\n' {
                        index + 2
                    } else {
                        index + 1
                    };
                    return self.take_line(index, consume).map(Some);
                }
                _ => index += 1,
            }
        }

        Ok(None)
    }

    /// 流结束时冲刷剩余内容。
    ///
    /// 约定：先不断调用 [`Self::next_line`] 直到返回 `Ok(None)`，再调用本方法。
    /// 剩余内容视为最后一行（允许没有行结束符），末尾单独的 `CR` 会被剥离。
    pub fn finish(&mut self) -> Result<Option<String>, StreamError> {
        if !self.bom_checked {
            let _ = self.skip_bom();
        }
        if self.buf.is_empty() {
            return Ok(None);
        }

        let mut line = self.buf.split_to(self.buf.len());
        if line.last() == Some(&b'\r') {
            line.truncate(line.len() - 1);
        }
        let text = std::str::from_utf8(&line).map_err(StreamError::utf8)?;
        Ok(Some(text.to_owned()))
    }

    /// 忽略流开头的 UTF-8 BOM。
    ///
    /// 返回 `false` 表示当前字节是 BOM 的前缀，需要等待更多输入。
    fn skip_bom(&mut self) -> bool {
        if self.buf.len() >= UTF8_BOM.len() {
            if self.buf.starts_with(&UTF8_BOM) {
                let _ = self.buf.split_to(UTF8_BOM.len());
            }
            self.bom_checked = true;
            true
        } else if UTF8_BOM.starts_with(&self.buf[..]) {
            false
        } else {
            self.bom_checked = true;
            true
        }
    }

    fn take_line(&mut self, end: usize, consume: usize) -> Result<String, StreamError> {
        let mut line = self.buf.split_to(consume);
        line.truncate(end);
        let text = std::str::from_utf8(&line).map_err(StreamError::utf8)?;
        Ok(text.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(input: &[u8], chunk_size: usize) -> Vec<String> {
        let mut buffer = LineBuffer::new();
        let mut lines = Vec::new();
        for chunk in input.chunks(chunk_size.max(1)) {
            buffer.push(chunk);
            while let Some(line) = buffer.next_line().expect("line") {
                lines.push(line);
            }
        }
        if let Some(line) = buffer.finish().expect("finish") {
            lines.push(line);
        }
        lines
    }

    #[test]
    fn splits_lf_crlf_and_cr() {
        let input = b"a\nb\r\nc\rd";
        assert_eq!(collect(input, 1), ["a", "b", "c", "d"]);
        assert_eq!(collect(input, 2), ["a", "b", "c", "d"]);
        assert_eq!(collect(input, 100), ["a", "b", "c", "d"]);
    }

    #[test]
    fn cr_at_chunk_boundary_waits_for_lf() {
        let mut buffer = LineBuffer::new();
        buffer.push(b"a\r");
        assert_eq!(buffer.next_line().expect("need more"), None);

        buffer.push(b"\nb");
        assert_eq!(buffer.next_line().expect("line"), Some("a".to_owned()));
        assert_eq!(buffer.finish().expect("finish"), Some("b".to_owned()));
    }

    #[test]
    fn cr_followed_by_other_byte_ends_line() {
        let mut buffer = LineBuffer::new();
        buffer.push(b"a\rb");
        assert_eq!(buffer.next_line().expect("line"), Some("a".to_owned()));
        assert_eq!(buffer.finish().expect("finish"), Some("b".to_owned()));
    }

    #[test]
    fn utf8_split_across_chunks_is_preserved() {
        let text = "你好\n世界";
        let bytes = text.as_bytes();
        assert_eq!(collect(bytes, 1), ["你好", "世界"]);
        assert_eq!(collect(bytes, 2), ["你好", "世界"]);
        assert_eq!(collect(bytes, 3), ["你好", "世界"]);
    }

    #[test]
    fn leading_bom_is_ignored_even_when_split() {
        let mut buffer = LineBuffer::new();
        buffer.push(&UTF8_BOM[..1]);
        assert_eq!(buffer.next_line().expect("need more"), None);
        buffer.push(&UTF8_BOM[1..]);
        buffer.push(b"data: x\n");
        assert_eq!(
            buffer.next_line().expect("line"),
            Some("data: x".to_owned())
        );
    }

    #[test]
    fn finish_strips_trailing_cr() {
        let mut buffer = LineBuffer::new();
        buffer.push(b"tail\r");
        assert_eq!(buffer.next_line().expect("need more"), None);
        assert_eq!(buffer.finish().expect("finish"), Some("tail".to_owned()));
    }

    #[test]
    fn invalid_utf8_is_reported() {
        let mut buffer = LineBuffer::new();
        buffer.push(&[0xFF, b'\n']);
        let err = buffer.next_line().expect_err("invalid utf8");
        assert_eq!(err.kind, crate::error::StreamErrorKind::Utf8);
    }

    #[test]
    fn empty_stream_yields_nothing() {
        let mut buffer = LineBuffer::new();
        assert_eq!(buffer.next_line().expect("none"), None);
        assert_eq!(buffer.finish().expect("none"), None);
    }
}

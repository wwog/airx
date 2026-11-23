//! 通用协议事件流。
//!
//! 组合三层：Transport 字节流 → `airx-core` SSE 解码 → Protocol 事件解码。
//! 只在事件边界做异步，解析全部是纯函数。

use airx_core::AirxError;
use airx_core::stream::SseDecoder;
use airx_protocol::StreamingEndpoint;
use airx_transport::ByteStream;
use futures_util::StreamExt;

/// 已解码的协议事件流。
pub struct ProtocolStream<P: StreamingEndpoint> {
    endpoint: P,
    bytes: ByteStream,
    sse: SseDecoder,
    finished: bool,
}

impl<P: StreamingEndpoint> std::fmt::Debug for ProtocolStream<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProtocolStream")
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}

impl<P: StreamingEndpoint> ProtocolStream<P> {
    pub(crate) fn new(endpoint: P, bytes: ByteStream) -> Self {
        Self {
            endpoint,
            bytes,
            sse: SseDecoder::new(),
            finished: false,
        }
    }

    /// 端点引用。
    pub fn endpoint(&self) -> &P {
        &self.endpoint
    }

    /// 取下一条协议事件。
    ///
    /// 返回 `Ok(None)` 表示流已正常结束；网络断开或事件未闭合会返回错误，
    /// 调用方必须区分二者。
    pub async fn next(&mut self) -> Result<Option<P::Event>, AirxError> {
        loop {
            if self.finished {
                return Ok(None);
            }

            if let Some(raw) = self.sse.next_event()? {
                if let Some(event) = self.endpoint.decode_event(raw)? {
                    return Ok(Some(event));
                }
                continue;
            }

            match self.bytes.next().await {
                Some(Ok(chunk)) => self.sse.push(&chunk),
                Some(Err(error)) => return Err(AirxError::Transport(error)),
                None => {
                    self.finished = true;
                    // 冲刷最后一个未带换行的事件；截断会返回 Incomplete。
                    if let Some(raw) = self.sse.finish()? {
                        if let Some(event) = self.endpoint.decode_event(raw)? {
                            return Ok(Some(event));
                        }
                    }
                    return Ok(None);
                }
            }
        }
    }
}

/// 读完整个响应体（用于非 2xx 流式响应的错误解码）。
pub(crate) async fn collect_body(mut body: ByteStream) -> Result<Vec<u8>, AirxError> {
    let mut buffer = Vec::new();
    while let Some(chunk) = body.next().await {
        buffer.extend_from_slice(&chunk?);
    }
    Ok(buffer)
}

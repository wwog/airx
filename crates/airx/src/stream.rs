//! 通用协议事件流。
//!
//! 组合三层：Transport 字节流 → `airx-core` SSE 解码 → Protocol 事件解码。
//! 只在事件边界做异步，解析全部是纯函数。
//!
//! [`ProtocolStream`] 实现 `futures_core::Stream`，可直接配合
//! `futures_util::TryStreamExt`、`tokio::select!` 使用。

use std::pin::Pin;
use std::task::{Context, Poll};

use airx_core::AirxError;
use airx_core::stream::SseDecoder;
use airx_protocol::StreamingEndpoint;
use airx_transport::ByteStream;
use futures_core::Stream;
use futures_util::StreamExt;

use crate::response::StreamMeta;

/// 已解码的协议事件流。
pub struct ProtocolStream<P: StreamingEndpoint> {
    endpoint: P,
    bytes: ByteStream,
    sse: SseDecoder,
    finished: bool,
    meta: StreamMeta,
}

impl<P: StreamingEndpoint> std::fmt::Debug for ProtocolStream<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProtocolStream")
            .field("finished", &self.finished)
            .field("meta", &self.meta)
            .finish_non_exhaustive()
    }
}

impl<P: StreamingEndpoint> ProtocolStream<P> {
    pub(crate) fn new(endpoint: P, bytes: ByteStream, meta: StreamMeta) -> Self {
        Self {
            endpoint,
            bytes,
            sse: SseDecoder::new(),
            finished: false,
            meta,
        }
    }

    /// 端点引用。
    pub fn endpoint(&self) -> &P {
        &self.endpoint
    }

    /// 建立连接时的 HTTP 元数据。
    pub fn meta(&self) -> &StreamMeta {
        &self.meta
    }

    /// request id（若有）。
    pub fn request_id(&self) -> Option<&str> {
        self.meta.request_id.as_deref()
    }
}

impl<P: StreamingEndpoint + Unpin> ProtocolStream<P> {
    /// 取下一条协议事件。
    ///
    /// 返回 `Ok(None)` 表示流已正常结束；网络断开或事件未闭合会返回错误，
    /// 调用方必须区分二者。
    pub async fn next(&mut self) -> Result<Option<P::Event>, AirxError> {
        std::future::poll_fn(|cx| Stream::poll_next(Pin::new(&mut *self), cx))
            .await
            .transpose()
    }
}

impl<P: StreamingEndpoint + Unpin> Stream for ProtocolStream<P> {
    type Item = Result<P::Event, AirxError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        loop {
            if this.finished {
                return Poll::Ready(None);
            }

            match this.sse.next_event() {
                Ok(Some(raw)) => match this.endpoint.decode_event(raw) {
                    Ok(Some(event)) => return Poll::Ready(Some(Ok(event))),
                    Ok(None) => continue,
                    Err(error) => return Poll::Ready(Some(Err(error))),
                },
                Ok(None) => {}
                Err(error) => {
                    return Poll::Ready(Some(Err(AirxError::Stream(error))));
                }
            }

            match this.bytes.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => this.sse.push(&chunk),
                Poll::Ready(Some(Err(error))) => {
                    return Poll::Ready(Some(Err(AirxError::Transport(error))));
                }
                Poll::Ready(None) => {
                    this.finished = true;
                    match this.sse.finish() {
                        Ok(Some(raw)) => match this.endpoint.decode_event(raw) {
                            Ok(Some(event)) => return Poll::Ready(Some(Ok(event))),
                            Ok(None) => {}
                            Err(error) => return Poll::Ready(Some(Err(error))),
                        },
                        Ok(None) => {}
                        Err(error) => {
                            return Poll::Ready(Some(Err(AirxError::Stream(error))));
                        }
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[cfg(feature = "openai")]
impl ProtocolStream<airx_protocol::openai::chat::ChatEndpoint> {
    /// 消费整个流并返回聚合后的响应。
    pub async fn final_response(
        mut self,
    ) -> Result<airx_protocol::openai::chat::ChatCompletionResponse, AirxError> {
        let mut accumulator = airx_protocol::openai::chat::ChatStreamAccumulator::new();
        while let Some(event) = self.next().await? {
            accumulator.apply(event)?;
        }
        accumulator.finish()
    }

    /// 消费整个流并返回文本。
    pub async fn final_text(self) -> Result<String, AirxError> {
        let response = self.final_response().await?;
        Ok(response.first_content().unwrap_or_default().to_owned())
    }
}

#[cfg(feature = "openai")]
impl ProtocolStream<airx_protocol::openai::responses::ResponsesEndpoint> {
    /// 消费整个流并返回聚合后的响应。
    pub async fn final_response(
        mut self,
    ) -> Result<airx_protocol::openai::responses::ResponsesResponse, AirxError> {
        let mut accumulator = airx_protocol::openai::responses::ResponsesStreamAccumulator::new();
        while let Some(event) = self.next().await? {
            accumulator.apply(event)?;
        }
        accumulator.finish()
    }

    /// 消费整个流并返回文本。
    pub async fn final_text(self) -> Result<String, AirxError> {
        Ok(self.final_response().await?.output_text())
    }
}

#[cfg(feature = "anthropic")]
impl ProtocolStream<airx_protocol::anthropic::messages::MessagesEndpoint> {
    /// 消费整个流并返回聚合后的响应。
    pub async fn final_response(
        mut self,
    ) -> Result<airx_protocol::anthropic::messages::MessagesResponse, AirxError> {
        let mut accumulator = airx_protocol::anthropic::messages::MessagesStreamAccumulator::new();
        while let Some(event) = self.next().await? {
            accumulator.apply(event)?;
        }
        accumulator.finish()
    }

    /// 消费整个流并返回文本。
    pub async fn final_text(self) -> Result<String, AirxError> {
        Ok(self.final_response().await?.text())
    }
}

#[cfg(feature = "gemini")]
impl ProtocolStream<airx_protocol::gemini::generate_content::GenerateContentEndpoint> {
    /// 消费整个流并返回聚合后的响应。
    pub async fn final_response(
        mut self,
    ) -> Result<airx_protocol::gemini::generate_content::GenerateContentResponse, AirxError> {
        let mut accumulator =
            airx_protocol::gemini::generate_content::GenerateContentStreamAccumulator::new();
        while let Some(event) = self.next().await? {
            accumulator.apply(event)?;
        }
        accumulator.finish()
    }

    /// 消费整个流并返回文本。
    pub async fn final_text(self) -> Result<String, AirxError> {
        Ok(self.final_response().await?.text())
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

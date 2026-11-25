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
use airx_core::error::StreamError;
use airx_core::log::{LogHandle, LogLevel, Scope, Timer};
use airx_core::stream::{SseDecoder, StreamStatus};
use airx_protocol::StreamingEndpoint;
use airx_transport::ByteStream;
use futures_core::Stream;
use futures_util::StreamExt;
use serde::de::DeserializeOwned;

use crate::response::StreamMeta;

/// 已解码的协议事件流。
pub struct ProtocolStream<P: StreamingEndpoint> {
    endpoint: P,
    bytes: ByteStream,
    sse: SseDecoder,
    finished: bool,
    meta: StreamMeta,
    log: LogHandle,
    timer: Timer,
    events: u64,
    ended: bool,
}

impl<P: StreamingEndpoint> std::fmt::Debug for ProtocolStream<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProtocolStream")
            .field("finished", &self.finished)
            .field("meta", &self.meta)
            .field("events", &self.events)
            .finish_non_exhaustive()
    }
}

impl<P: StreamingEndpoint> ProtocolStream<P> {
    pub(crate) fn new(endpoint: P, bytes: ByteStream, meta: StreamMeta, log: LogHandle) -> Self {
        Self {
            endpoint,
            bytes,
            sse: SseDecoder::new(),
            finished: false,
            meta,
            log,
            timer: Timer::start(),
            events: 0,
            ended: false,
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

    /// 记录一次流结束；只记录第一次。
    fn log_end(&mut self, failure: Option<&AirxError>) {
        if self.ended {
            return;
        }
        self.ended = true;

        let request_id = self.meta.request_id.as_deref().unwrap_or("-");
        let status = self.meta.status.as_u16();
        match failure {
            Some(error) => {
                airx_core::airx_log!(
                    self.log,
                    Scope::STREAM,
                    LogLevel::Error,
                    "stream failed",
                    status = status,
                    request_id = request_id,
                    events = self.events,
                    error = error,
                    elapsed = self.timer.elapsed(),
                );
                airx_core::airx_log!(
                    self.log,
                    Scope::CALL,
                    LogLevel::Error,
                    "call failed",
                    stream = true,
                    status = status,
                    request_id = request_id,
                    events = self.events,
                    error = error,
                    elapsed = self.timer.elapsed(),
                );
            }
            None => {
                airx_core::airx_log!(
                    self.log,
                    Scope::STREAM,
                    LogLevel::Info,
                    "stream ended",
                    status = status,
                    request_id = request_id,
                    events = self.events,
                    elapsed = self.timer.elapsed(),
                );
                airx_core::airx_log!(
                    self.log,
                    Scope::CALL,
                    LogLevel::Info,
                    "call completed",
                    stream = true,
                    status = status,
                    request_id = request_id,
                    events = self.events,
                    elapsed = self.timer.elapsed(),
                );
            }
        }
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
                    Ok(Some(event)) => {
                        this.events += 1;
                        return Poll::Ready(Some(Ok(event)));
                    }
                    Ok(None) => continue,
                    Err(error) => {
                        this.log_end(Some(&error));
                        return Poll::Ready(Some(Err(error)));
                    }
                },
                Ok(None) => {}
                Err(error) => {
                    let error = AirxError::Stream(error);
                    this.log_end(Some(&error));
                    return Poll::Ready(Some(Err(error)));
                }
            }

            match this.bytes.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => this.sse.push(&chunk),
                Poll::Ready(Some(Err(error))) => {
                    let error = AirxError::Transport(error);
                    this.log_end(Some(&error));
                    return Poll::Ready(Some(Err(error)));
                }
                Poll::Ready(None) => {
                    this.finished = true;
                    let flushed = match this.sse.finish() {
                        Ok(Some(raw)) => match this.endpoint.decode_event(raw) {
                            Ok(Some(event)) => {
                                this.events += 1;
                                Some(event)
                            }
                            Ok(None) => None,
                            Err(error) => {
                                this.log_end(Some(&error));
                                return Poll::Ready(Some(Err(error)));
                            }
                        },
                        Ok(None) => None,
                        Err(error) => {
                            let error = AirxError::Stream(error);
                            this.log_end(Some(&error));
                            return Poll::Ready(Some(Err(error)));
                        }
                    };
                    this.log_end(None);
                    match flushed {
                        Some(event) => return Poll::Ready(Some(Ok(event))),
                        None => return Poll::Ready(None),
                    }
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

    /// 消费整个流，把聚合文本按 JSON 解析为 `T`。
    ///
    /// 只做 serde 解析，不注入/校验 schema。
    pub async fn final_parsed<T: DeserializeOwned>(self) -> Result<T, AirxError> {
        parse_stream_json(self.final_text().await?)
    }

    /// 消费整个流；流内错误 / 截断时保留已累积的部分响应。
    pub async fn final_response_partial(
        mut self,
    ) -> (
        airx_protocol::openai::chat::ChatCompletionResponse,
        Option<AirxError>,
    ) {
        let mut accumulator = airx_protocol::openai::chat::ChatStreamAccumulator::new();
        let mut failure = None;
        loop {
            match self.next().await {
                Ok(Some(event)) => {
                    if let Err(error) = accumulator.apply(event) {
                        failure = Some(error);
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        let (response, status) = accumulator.finish_partial();
        (response, finish_partial_failure(status, failure))
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

    /// 消费整个流，把聚合文本按 JSON 解析为 `T`。
    ///
    /// 只做 serde 解析，不注入/校验 schema。
    pub async fn final_parsed<T: DeserializeOwned>(self) -> Result<T, AirxError> {
        parse_stream_json(self.final_text().await?)
    }

    /// 消费整个流；流内错误 / 截断时保留已累积的部分响应。
    pub async fn final_response_partial(
        mut self,
    ) -> (
        airx_protocol::openai::responses::ResponsesResponse,
        Option<AirxError>,
    ) {
        let mut accumulator = airx_protocol::openai::responses::ResponsesStreamAccumulator::new();
        let mut failure = None;
        loop {
            match self.next().await {
                Ok(Some(event)) => {
                    if let Err(error) = accumulator.apply(event) {
                        failure = Some(error);
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        let (response, status) = accumulator.finish_partial();
        (response, finish_partial_failure(status, failure))
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

    /// 消费整个流，把聚合文本按 JSON 解析为 `T`。
    ///
    /// 只做 serde 解析，不注入/校验 schema。
    pub async fn final_parsed<T: DeserializeOwned>(self) -> Result<T, AirxError> {
        parse_stream_json(self.final_text().await?)
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

    /// 消费整个流，把聚合文本按 JSON 解析为 `T`。
    ///
    /// 只做 serde 解析，不注入/校验 schema。
    pub async fn final_parsed<T: DeserializeOwned>(self) -> Result<T, AirxError> {
        parse_stream_json(self.final_text().await?)
    }

    /// 消费整个流；流内错误 / 截断时保留已累积的部分响应。
    pub async fn final_response_partial(
        mut self,
    ) -> (
        airx_protocol::gemini::generate_content::GenerateContentResponse,
        Option<AirxError>,
    ) {
        let mut accumulator =
            airx_protocol::gemini::generate_content::GenerateContentStreamAccumulator::new();
        let mut failure = None;
        loop {
            match self.next().await {
                Ok(Some(event)) => {
                    if let Err(error) = accumulator.apply(event) {
                        failure = Some(error);
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        let (response, status) = accumulator.finish_partial();
        (response, finish_partial_failure(status, failure))
    }
}

fn parse_stream_json<T: DeserializeOwned>(text: String) -> Result<T, AirxError> {
    serde_json::from_str(&text).map_err(|error| AirxError::Deserialization(error.into()))
}

/// 部分结果收尾的错误归类：只有 `Completed` 才算正常结束。
fn finish_partial_failure(status: StreamStatus, failure: Option<AirxError>) -> Option<AirxError> {
    if failure.is_some() {
        return failure;
    }
    match status {
        StreamStatus::Completed => None,
        StreamStatus::Failed => Some(AirxError::Stream(StreamError::failed("流内错误"))),
        _ => Some(AirxError::Stream(StreamError::incomplete(
            "流在完成信号前结束，结果可能不完整",
        ))),
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

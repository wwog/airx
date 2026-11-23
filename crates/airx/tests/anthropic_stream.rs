//! Anthropic Messages 流式端到端。

#![cfg(all(feature = "anthropic", feature = "stream"))]

use airx::prelude::*;
use airx_core::http::HeaderMap;
use airx_transport::MockTransport;
use bytes::Bytes;

const SSE: &str = concat!(
    "event: message_start\n",
    r#"data: {"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","model":"claude-3-5-sonnet","content":[],"usage":{"input_tokens":10,"output_tokens":0}}}"#,
    "\n\n",
    "event: content_block_start\n",
    r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
    "\n\n",
    "event: ping\n",
    r#"data: {"type":"ping"}"#,
    "\n\n",
    "event: content_block_delta\n",
    r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"你好"}}"#,
    "\n\n",
    "event: content_block_stop\n",
    r#"data: {"type":"content_block_stop","index":0}"#,
    "\n\n",
    "event: message_delta\n",
    r#"data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}"#,
    "\n\n",
    "event: message_stop\n",
    r#"data: {"type":"message_stop"}"#,
    "\n\n",
);

fn split(input: &str, size: usize) -> Vec<Bytes> {
    input
        .as_bytes()
        .chunks(size)
        .map(Bytes::copy_from_slice)
        .collect()
}

#[tokio::test]
async fn messages_stream_skips_ping_and_accumulates() {
    let mock = MockTransport::new();
    mock.push_stream(200, HeaderMap::new(), split(SSE, 6));

    let client = Client::builder()
        .transport(mock.clone())
        .anthropic()
        .api_key("sk-ant-test")
        .build()
        .expect("client");

    let request = MessagesRequest::builder()
        .model("claude-3-5-sonnet")
        .max_tokens(128)
        .user("hello")
        .build()
        .expect("request");

    let mut stream = client.messages().stream(request).await.expect("stream");

    let mut accumulator = MessagesStreamAccumulator::new();
    let mut events = 0;
    while let Some(event) = stream.next().await.expect("event") {
        events += 1;
        accumulator.apply(event).expect("apply");
    }
    assert_eq!(events, 6); // ping 被协议层过滤

    let response = accumulator.finish().expect("finish");
    assert_eq!(response.text(), "你好");
    assert_eq!(response.stop_reason.as_deref(), Some("end_turn"));
    assert_eq!(response.usage.as_ref().map(|u| u.output_tokens), Some(5));

    let sent: serde_json::Value = serde_json::from_slice(&mock.requests()[0].body).expect("body");
    assert_eq!(sent["stream"], true);
}

#[tokio::test]
async fn messages_create_decodes_plain_json() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        r#"{"id":"msg_2","type":"message","role":"assistant","model":"claude","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn"}"#,
    );

    let client = Client::builder()
        .transport(mock.clone())
        .anthropic()
        .api_key("sk-ant-test")
        .build()
        .expect("client");

    let response = client
        .messages()
        .create(
            MessagesRequest::builder()
                .model("claude")
                .max_tokens(16)
                .user("hi")
                .build()
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.text(), "ok");
}

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

/// 流中途带 citations、refusal 详情、容器与 `input_tokens: null`。
const SSE_FIDELITY: &str = concat!(
    "event: message_start\n",
    r#"data: {"type":"message_start","message":{"id":"msg_2","type":"message","role":"assistant","model":"claude","content":[],"usage":{"input_tokens":9,"output_tokens":0}}}"#,
    "\n\n",
    "event: content_block_start\n",
    r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":"","citations":null,"cache_control":{"type":"ephemeral"}}}"#,
    "\n\n",
    "event: content_block_delta\n",
    r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"citations_delta","citation":{"type":"char_location","cited_text":"源文本"}}}"#,
    "\n\n",
    "event: content_block_delta\n",
    r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"引用完成"}}"#,
    "\n\n",
    "event: message_delta\n",
    r#"data: {"type":"message_delta","delta":{"stop_reason":"refusal","stop_details":{"type":"refusal","category":"cyber"},"container":{"id":"container_1"}},"usage":{"input_tokens":null,"output_tokens":4,"output_tokens_details":{"thinking_tokens":3}}}"#,
    "\n\n",
    "event: message_stop\n",
    r#"data: {"type":"message_stop"}"#,
    "\n\n",
);

const SSE_WITH_ERROR: &str = concat!(
    "event: message_start\n",
    r#"data: {"type":"message_start","message":{"id":"msg_3","type":"message","role":"assistant","model":"claude","content":[],"usage":{"input_tokens":3,"output_tokens":0}}}"#,
    "\n\n",
    "event: content_block_start\n",
    r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
    "\n\n",
    "event: content_block_delta\n",
    r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"部分"}}"#,
    "\n\n",
    "event: error\n",
    r#"data: {"type":"error","error":{"type":"overloaded_error","message":"overloaded"}}"#,
    "\n\n",
);

fn split(input: &str, size: usize) -> Vec<Bytes> {
    input
        .as_bytes()
        .chunks(size)
        .map(Bytes::copy_from_slice)
        .collect()
}

fn client(mock: &MockTransport) -> Anthropic {
    Client::builder()
        .transport(mock.clone())
        .anthropic()
        .api_key("sk-ant-test")
        .build()
        .expect("client")
}

fn request() -> MessagesRequest {
    MessagesRequest::builder()
        .model("claude-3-5-sonnet")
        .max_tokens(128)
        .user("hello")
        .build()
        .expect("request")
}

#[tokio::test]
async fn messages_stream_skips_ping_and_accumulates() {
    let mock = MockTransport::new();
    mock.push_stream(200, HeaderMap::new(), split(SSE, 6));

    let mut stream = client(&mock)
        .messages()
        .stream(request())
        .await
        .expect("stream");

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
    assert!(response.stop_reason().is_some());
    assert_eq!(response.usage.as_ref().map(|u| u.output_tokens), Some(5));

    let sent: serde_json::Value = serde_json::from_slice(&mock.requests()[0].body).expect("body");
    assert_eq!(sent["stream"], true);
}

#[tokio::test]
async fn messages_create_rejects_stream_true() {
    let mock = MockTransport::new();
    let request = MessagesRequest::builder()
        .model("claude-3-5-sonnet")
        .max_tokens(128)
        .user("hello")
        .stream(true)
        .build()
        .expect("request");
    let error = client(&mock)
        .messages()
        .create(request)
        .await
        .expect_err("configuration error");

    assert!(
        matches!(error, AirxError::Configuration(_)),
        "got {error:?}"
    );
    assert!(mock.requests().is_empty());
}

#[tokio::test]
async fn stream_preserves_citations_stop_details_container_and_nulls() {
    let mock = MockTransport::new();
    mock.push_stream(200, HeaderMap::new(), split(SSE_FIDELITY, 7));

    let response = client(&mock)
        .messages()
        .stream(request())
        .await
        .expect("stream")
        .final_response()
        .await
        .expect("response");

    assert_eq!(response.text(), "引用完成");
    assert!(response.is_refusal());
    assert_eq!(
        response.stop_details().and_then(|d| d.category.as_deref()),
        Some("cyber")
    );
    assert_eq!(response.container_id(), Some("container_1"));
    assert_eq!(response.thinking_tokens(), 3);
    assert_eq!(
        response.usage.as_ref().and_then(|u| u.input_tokens),
        Some(9)
    );

    match &response.content[0] {
        airx_protocol::anthropic::messages::ContentBlock::Text(block) => {
            assert_eq!(block.citations().len(), 1);
            assert_eq!(block.extensions["cache_control"]["type"], "ephemeral");
        }
        other => panic!("unexpected block: {other:?}"),
    }
}

#[tokio::test]
async fn final_response_partial_keeps_content_after_stream_error() {
    let mock = MockTransport::new();
    mock.push_stream(200, HeaderMap::new(), split(SSE_WITH_ERROR, 5));

    let stream = client(&mock)
        .messages()
        .stream(request())
        .await
        .expect("stream");

    let (response, error) = stream.final_response_partial().await;
    assert!(error.is_some(), "stream error should be surfaced");
    assert_eq!(response.text(), "部分");
}

#[tokio::test]
async fn messages_create_decodes_plain_json() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        r#"{"id":"msg_2","type":"message","role":"assistant","model":"claude","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn"}"#,
    );

    let response = client(&mock)
        .messages()
        .create(request())
        .await
        .expect("response");

    assert_eq!(response.text(), "ok");
}

#[tokio::test]
async fn workspace_and_user_profile_headers_are_sent() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        r#"{"id":"msg_4","model":"claude","content":[{"type":"text","text":"ok"}]}"#,
    );

    let options = RequestOptions::new()
        .workspace("wrkspc_1")
        .expect("workspace")
        .user_profile("profile_1")
        .expect("profile");

    let _ = client(&mock)
        .messages()
        .with_options(options)
        .create(request())
        .await
        .expect("response");

    let sent = &mock.requests()[0];
    assert_eq!(sent.headers["anthropic-workspace-id"], "wrkspc_1");
    assert_eq!(sent.headers["anthropic-user-profile-id"], "profile_1");
    // workspace_id 只走 Header，不写进 body。
    let body: serde_json::Value = serde_json::from_slice(&sent.body).expect("body");
    assert!(body.get("workspace_id").is_none());
}

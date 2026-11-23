//! Chat 流式端到端：Mock 字节流 → SSE → 协议事件 → 聚合。

#![cfg(all(feature = "openai", feature = "stream"))]

use airx::prelude::*;
use airx_core::http::HeaderMap;
use airx_transport::MockTransport;
use bytes::Bytes;
use serde_json::{Value, json};

const SSE: &str = concat!(
    r#"data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"gpt-4o-mini","choices":[{"index":0,"delta":{"role":"assistant","content":"你"}}]}"#,
    "\n\n",
    r#"data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"gpt-4o-mini","choices":[{"index":0,"delta":{"content":"好"}}]}"#,
    "\n\n",
    r#"data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"gpt-4o-mini","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{\"q\":"}}]}}]}"#,
    "\n\n",
    r#"data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"gpt-4o-mini","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"singapore\"}"}}]}}]}"#,
    "\n\n",
    r#"data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"gpt-4o-mini","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#,
    "\n\n",
    r#"data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"gpt-4o-mini","choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#,
    "\n\n",
    "data: [DONE]\n\n",
);

fn split(input: &str, size: usize) -> Vec<Bytes> {
    input
        .as_bytes()
        .chunks(size)
        .map(Bytes::copy_from_slice)
        .collect()
}

#[tokio::test]
async fn streams_text_tool_calls_and_usage_through_accumulator() {
    let mock = MockTransport::new();
    mock.push_stream(200, HeaderMap::new(), split(SSE, 7));

    let client = Client::builder()
        .transport(mock.clone())
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client");

    let request = ChatCompletionRequest::builder()
        .model("gpt-4o-mini")
        .user("weather in Singapore?")
        .build()
        .expect("request");

    let mut stream = client.chat().stream(request).await.expect("stream");

    let mut accumulator = ChatStreamAccumulator::new();
    let mut event_count = 0;
    while let Some(event) = stream.next().await.expect("event") {
        event_count += 1;
        accumulator.apply(event).expect("apply");
    }
    assert_eq!(event_count, 7);

    let response = accumulator.finish().expect("finish");
    assert_eq!(response.first_content(), Some("你好"));
    assert_eq!(response.usage.as_ref().map(|u| u.total_tokens), Some(15));
    assert_eq!(
        response.choices[0].finish_reason.as_deref(),
        Some("tool_calls")
    );

    let calls = response.choices[0]
        .message
        .tool_calls
        .as_ref()
        .expect("tool calls");
    assert_eq!(calls[0].function.arguments, r#"{"q":"singapore"}"#);

    // 请求侧自动带上 stream 与 include_usage。
    let sent: Value = serde_json::from_slice(&mock.requests()[0].body).expect("body");
    assert_eq!(sent["stream"], true);
    assert_eq!(sent["stream_options"]["include_usage"], true);
}

#[tokio::test]
async fn non_success_stream_is_decoded_as_provider_error() {
    let mock = MockTransport::new();
    mock.push_stream(
        400,
        HeaderMap::new(),
        vec![Bytes::from_static(
            br#"{"error":{"message":"bad model","type":"invalid_request_error","code":"model_not_found"}}"#,
        )],
    );

    let client = Client::builder()
        .transport(mock.clone())
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client");

    let error = client
        .chat()
        .stream(
            ChatCompletionRequest::builder()
                .model("m")
                .user("hi")
                .build()
                .expect("request"),
        )
        .await
        .expect_err("provider error");

    match error {
        AirxError::Provider(error) => {
            assert_eq!(error.status, Some(400));
            assert_eq!(error.code.as_deref(), Some("model_not_found"));
        }
        other => panic!("expected provider error, got {other:?}"),
    }
}

#[tokio::test]
async fn malformed_stream_chunk_surfaces_deserialization_error() {
    let mock = MockTransport::new();
    mock.push_stream(
        200,
        HeaderMap::new(),
        vec![Bytes::from_static(b"data: {\"broken\":\n\n")],
    );

    let client = Client::builder()
        .transport(mock.clone())
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client");

    let mut stream = client
        .chat()
        .stream(
            ChatCompletionRequest::builder()
                .model("m")
                .user("hi")
                .build()
                .expect("request"),
        )
        .await
        .expect("stream");

    let error = stream.next().await.expect_err("decode error");
    assert!(matches!(error, AirxError::Deserialization(_)));
}

#[test]
fn sse_fixture_is_well_formed_json_per_event() {
    for line in SSE.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                continue;
            }
            let value: Value = serde_json::from_str(data).expect("json");
            assert_eq!(value["id"], "chatcmpl-1");
            let _ = json!(value);
        }
    }
}

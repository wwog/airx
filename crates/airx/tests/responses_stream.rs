//! Responses 流式端到端。

#![cfg(all(feature = "openai", feature = "stream"))]

use airx::prelude::*;
use airx_core::http::HeaderMap;
use airx_transport::MockTransport;
use bytes::Bytes;
use serde_json::json;

const SSE: &str = concat!(
    "event: response.created\n",
    r#"data: {"type":"response.created","response":{"id":"resp_1","status":"in_progress","output":[]}}"#,
    "\n\n",
    "event: response.output_item.added\n",
    r#"data: {"type":"response.output_item.added","output_index":0,"item":{"type":"message","id":"msg_1","role":"assistant","content":[]}}"#,
    "\n\n",
    "event: response.output_text.delta\n",
    r#"data: {"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"delta":"你"}"#,
    "\n\n",
    "event: response.output_text.delta\n",
    r#"data: {"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"delta":"好"}"#,
    "\n\n",
    "event: response.completed\n",
    r#"data: {"type":"response.completed","response":{"id":"resp_1","status":"completed","output":[]}}"#,
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
async fn responses_stream_accumulates_text() {
    let mock = MockTransport::new();
    mock.push_stream(200, HeaderMap::new(), split(SSE, 5));

    let client = Client::builder()
        .transport(mock.clone())
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client");

    let request = ResponsesRequest::builder()
        .model("gpt-4o-mini")
        .input("hello")
        .build()
        .expect("request");

    let mut stream = client
        .openai()
        .expect("openai")
        .responses()
        .stream(request)
        .await
        .expect("stream");

    let mut accumulator = ResponsesStreamAccumulator::new();
    while let Some(event) = stream.next().await.expect("event") {
        accumulator.apply(event).expect("apply");
    }

    let response = accumulator.finish().expect("finish");
    assert_eq!(response.output_text(), "你好");

    let sent: serde_json::Value = serde_json::from_slice(&mock.requests()[0].body).expect("body");
    assert_eq!(sent["stream"], true);
}

#[tokio::test]
async fn responses_create_decodes_plain_json() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        json!({
            "id": "resp_2",
            "status": "completed",
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "ok", "annotations": []}]
            }]
        })
        .to_string(),
    );

    let client = Client::builder()
        .transport(mock.clone())
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client");

    let response = client
        .openai()
        .expect("openai")
        .responses()
        .create(
            ResponsesRequest::builder()
                .model("m")
                .input("hi")
                .build()
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.output_text(), "ok");
}

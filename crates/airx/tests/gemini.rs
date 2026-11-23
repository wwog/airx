//! Gemini 端到端：Client → Endpoint 编码 → Transport(Mock) → 解码 / 流式聚合。

#![cfg(feature = "gemini")]

use airx::prelude::*;
use airx_core::error::StreamErrorKind;
use airx_core::http::HeaderMap;
use airx_transport::MockTransport;
use bytes::Bytes;
use serde_json::{Value, json};

const RESPONSE: &str = r#"{
  "candidates": [{
    "index": 0,
    "content": {
      "role": "model",
      "parts": [
        {"text": "先想一下", "thought": true},
        {"text": "你好，世界"}
      ]
    },
    "finishReason": "STOP"
  }],
  "usageMetadata": {
    "promptTokenCount": 4,
    "candidatesTokenCount": 6,
    "totalTokenCount": 12,
    "thoughtsTokenCount": 2
  },
  "modelVersion": "gemini-2.5-flash",
  "responseId": "resp-1"
}"#;

fn client(mock: &MockTransport) -> Client {
    Client::builder()
        .transport(mock.clone())
        .gemini()
        .api_key("test-key")
        .build()
        .expect("client")
}

fn request() -> GenerateContentRequest {
    GenerateContentRequest::builder()
        .system("be brief")
        .user("hello")
        .max_output_tokens(64)
        .build()
        .expect("request")
}

#[tokio::test]
async fn generate_content_runs_the_full_pipeline() {
    let mock = MockTransport::new();
    mock.push_json(200, RESPONSE);

    let response = client(&mock)
        .gemini()
        .expect("gemini")
        .generate_content("gemini-2.5-flash")
        .create(request())
        .await
        .expect("response");

    assert_eq!(response.text(), "你好，世界");
    assert_eq!(response.thought_text(), "先想一下");
    assert_eq!(response.finish_reason(), Some("STOP"));
    assert_eq!(
        response
            .usage_metadata
            .as_ref()
            .map(|usage| usage.total_token_count),
        Some(12)
    );
    assert_eq!(response.model_version.as_deref(), Some("gemini-2.5-flash"));

    let sent = &mock.requests()[0];
    assert_eq!(
        sent.uri.path(),
        "/v1beta/models/gemini-2.5-flash:generateContent"
    );
    assert_eq!(sent.headers["x-goog-api-key"], "test-key");
    assert_eq!(sent.headers["content-type"], "application/json");

    let body: Value = serde_json::from_slice(&sent.body).expect("body");
    assert_eq!(body["contents"][0]["role"], "user");
    assert_eq!(body["contents"][0]["parts"][0]["text"], "hello");
    assert_eq!(body["systemInstruction"]["parts"][0]["text"], "be brief");
    assert_eq!(body["generationConfig"]["maxOutputTokens"], 64);
    assert!(body.get("model").is_none());
}

#[tokio::test]
async fn model_prefix_is_normalized() {
    let mock = MockTransport::new();
    mock.push_json(200, RESPONSE);

    client(&mock)
        .gemini()
        .expect("gemini")
        .generate_content("models/gemini-2.5-flash")
        .create(request())
        .await
        .expect("response");

    assert_eq!(
        mock.requests()[0].uri.path(),
        "/v1beta/models/gemini-2.5-flash:generateContent"
    );
}

#[tokio::test]
async fn base_url_override_is_used() {
    let mock = MockTransport::new();
    mock.push_json(200, RESPONSE);

    let client = Client::builder()
        .transport(mock.clone())
        .gemini()
        .api_key("test-key")
        .base_url("https://gateway.example.com/gemini/v1beta")
        .build()
        .expect("client");

    client
        .gemini()
        .expect("gemini")
        .generate_content("gemini-2.5-flash")
        .create(request())
        .await
        .expect("response");

    assert_eq!(
        mock.requests()[0].uri.path(),
        "/gemini/v1beta/models/gemini-2.5-flash:generateContent"
    );
}

#[tokio::test]
async fn provider_error_is_preserved() {
    let mock = MockTransport::new();
    mock.push_json(
        400,
        r#"{"error":{"code":400,"message":"bad request","status":"INVALID_ARGUMENT"}}"#,
    );

    let error = client(&mock)
        .gemini()
        .expect("gemini")
        .generate_content("gemini-2.5-flash")
        .create(request())
        .await
        .expect_err("error");

    match error {
        AirxError::Provider(error) => {
            assert_eq!(error.status, Some(400));
            assert_eq!(error.code.as_deref(), Some("INVALID_ARGUMENT"));
            assert_eq!(error.message, "bad request");
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[tokio::test]
async fn unconfigured_provider_is_a_configuration_error() {
    let client = Client::builder()
        .transport(MockTransport::new())
        .build()
        .expect("client");

    assert!(matches!(client.gemini(), Err(AirxError::Configuration(_))));
}

#[cfg(feature = "stream")]
#[tokio::test]
async fn stream_accumulates_text_and_usage() {
    let mock = MockTransport::new();
    mock.push_stream(
        200,
        HeaderMap::new(),
        [
            Bytes::from_static(
                b"data: {\"candidates\":[{\"index\":0,\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"\xe4\xbd\xa0\"}]}}]}\n\n",
            ),
            Bytes::from_static(
                b"data: {\"candidates\":[{\"index\":0,\"content\":{\"parts\":[{\"text\":\"\xe5\xa5\xbd\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":2,\"candidatesTokenCount\":3,\"totalTokenCount\":5}}\n\n",
            ),
        ],
    );

    let mut stream = client(&mock)
        .gemini()
        .expect("gemini")
        .generate_content("gemini-2.5-flash")
        .stream(request())
        .await
        .expect("stream");

    let mut accumulator = GenerateContentStreamAccumulator::new();
    while let Some(event) = stream.next().await.expect("event") {
        accumulator.apply(event).expect("apply");
    }
    let response = accumulator.finish().expect("response");

    assert_eq!(response.text(), "你好");
    assert_eq!(response.finish_reason(), Some("STOP"));
    assert_eq!(
        response
            .usage_metadata
            .as_ref()
            .map(|usage| usage.total_token_count),
        Some(5)
    );

    let sent = &mock.requests()[0];
    assert_eq!(
        sent.uri.path(),
        "/v1beta/models/gemini-2.5-flash:streamGenerateContent"
    );
    assert_eq!(sent.uri.query(), Some("alt=sse"));
}

#[cfg(feature = "stream")]
#[tokio::test]
async fn truncated_stream_is_an_error() {
    let mock = MockTransport::new();
    mock.push_stream(
        200,
        HeaderMap::new(),
        [Bytes::from_static(
            b"data: {\"candidates\":[{\"index\":0,\"content\":{\"parts\":[{\"text\":\"partial\"}]}}]}\n\n",
        )],
    );

    let mut stream = client(&mock)
        .gemini()
        .expect("gemini")
        .generate_content("gemini-2.5-flash")
        .stream(request())
        .await
        .expect("stream");

    let mut accumulator = GenerateContentStreamAccumulator::new();
    while let Some(event) = stream.next().await.expect("event") {
        accumulator.apply(event).expect("apply");
    }

    match accumulator.finish().expect_err("incomplete") {
        AirxError::Stream(error) => assert_eq!(error.kind, StreamErrorKind::Incomplete),
        other => panic!("unexpected: {other:?}"),
    }
}

#[cfg(feature = "stream")]
#[tokio::test]
async fn stream_http_error_is_surfaced() {
    let mock = MockTransport::new();
    mock.push_stream(
        429,
        HeaderMap::new(),
        [Bytes::from_static(
            br#"{"error":{"code":429,"message":"quota exceeded","status":"RESOURCE_EXHAUSTED"}}"#,
        )],
    );

    let error = client(&mock)
        .gemini()
        .expect("gemini")
        .generate_content("gemini-2.5-flash")
        .stream(request())
        .await
        .expect_err("error");

    match error {
        AirxError::Provider(error) => {
            assert_eq!(error.status, Some(429));
            assert_eq!(error.code.as_deref(), Some("RESOURCE_EXHAUSTED"));
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn gemini_public_types_are_reexported_from_prelude() {
    let tool = GeminiTool::function(
        FunctionDeclaration::new("lookup").with_parameters(json!({"type": "object"})),
    );
    let request = GenerateContentRequest::builder()
        .user("hi")
        .tool(tool)
        .thinking(GeminiThinkingConfig::budget(1024))
        .build()
        .expect("request");

    let value = serde_json::to_value(&request).expect("serialize");
    assert_eq!(
        value["tools"][0]["functionDeclarations"][0]["name"],
        "lookup"
    );
    assert_eq!(
        value["generationConfig"]["thinkingConfig"]["thinkingBudget"],
        1024
    );
}

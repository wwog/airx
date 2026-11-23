//! 端到端竖切：Client → Endpoint 编码 → Transport(Mock) → 解码。
//!
//! 这条路径验证 M3 冻结的三个契约：Protocol / Transport / Client。

#![cfg(feature = "openai")]

use std::time::Duration;

use airx::prelude::*;
use airx_core::http::HeaderMap;
use airx_transport::MockTransport;
use serde_json::{Value, json};

const SUCCESS: &str = r#"{
  "id": "chatcmpl-1",
  "object": "chat.completion",
  "created": 1729000000,
  "model": "gpt-4o-mini",
  "choices": [
    {
      "index": 0,
      "message": { "role": "assistant", "content": "你好!" },
      "finish_reason": "stop"
    }
  ],
  "usage": { "prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7 }
}"#;

fn client(mock: &MockTransport) -> Client {
    Client::builder()
        .transport(mock.clone())
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client")
}

fn request() -> ChatCompletionRequest {
    ChatCompletionRequest::builder()
        .model("gpt-4o-mini")
        .system("be brief")
        .user("hello")
        .build()
        .expect("request")
}

#[tokio::test]
async fn chat_create_runs_the_full_pipeline() {
    let mock = MockTransport::new();
    mock.push_json(200, SUCCESS);

    let response = client(&mock)
        .openai()
        .expect("openai")
        .chat()
        .create(request())
        .await
        .expect("response");

    assert_eq!(response.first_content(), Some("你好!"));
    assert_eq!(
        response.usage.as_ref().map(|usage| usage.total_tokens),
        Some(7)
    );

    let sent = &mock.requests()[0];
    assert_eq!(sent.method.as_str(), "POST");
    assert_eq!(
        sent.uri.to_string(),
        "https://api.openai.com/v1/chat/completions"
    );
    assert_eq!(
        sent.headers["authorization"].to_str().expect("auth"),
        "Bearer sk-test"
    );
    assert_eq!(
        sent.headers["content-type"].to_str().expect("content type"),
        "application/json"
    );

    let body: Value = serde_json::from_slice(&sent.body).expect("body");
    assert_eq!(body["model"], "gpt-4o-mini");
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][1]["content"], "hello");
}

#[tokio::test]
async fn provider_error_surfaces_with_status_and_code() {
    let mock = MockTransport::new();
    mock.push_json(
        400,
        r#"{"error":{"message":"bad model","type":"invalid_request_error","code":"model_not_found"}}"#,
    );

    let error = client(&mock)
        .openai()
        .expect("openai")
        .chat()
        .create(request())
        .await
        .expect_err("provider error");

    assert!(!error.is_retryable());

    match error {
        AirxError::Provider(error) => {
            assert_eq!(error.status, Some(400));
            assert_eq!(error.code.as_deref(), Some("model_not_found"));
            assert!(error.message.contains("bad model"));
            assert!(error.raw_body.is_some());
        }
        other => panic!("expected provider error, got {other:?}"),
    }
}

#[tokio::test]
async fn execution_headers_do_not_override_protocol_headers() {
    let mock = MockTransport::new();
    mock.push_json(200, SUCCESS);

    let mut headers = HeaderMap::new();
    headers.insert("x-execution", "1".parse().expect("header"));
    headers.insert("authorization", "Bearer wrong".parse().expect("header"));

    let client = Client::builder()
        .transport(mock.clone())
        .default_headers(headers)
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client");

    client
        .openai()
        .expect("openai")
        .chat()
        .create(request())
        .await
        .expect("response");

    let sent = &mock.requests()[0];
    assert_eq!(sent.headers["x-execution"], "1");
    assert_eq!(
        sent.headers["authorization"].to_str().expect("auth"),
        "Bearer sk-test"
    );
}

#[tokio::test]
async fn retry_is_disabled_by_default() {
    let mock = MockTransport::new();
    mock.push_json(500, "{}");

    let error = client(&mock)
        .openai()
        .expect("openai")
        .chat()
        .create(request())
        .await
        .expect_err("http error");

    assert!(matches!(error, AirxError::Http(_)));
    assert_eq!(mock.request_count(), 1);
}

#[tokio::test]
async fn retry_policy_retries_rate_limits_for_post() {
    let mock = MockTransport::new();
    mock.push_json(429, "{}");
    mock.push_json(200, SUCCESS);

    let policy = RetryPolicy::standard()
        .with_max_retries(1)
        .with_initial_backoff(Duration::from_millis(1));

    let client = Client::builder()
        .transport(mock.clone())
        .retry_policy(policy)
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client");

    let response = client
        .openai()
        .expect("openai")
        .chat()
        .create(request())
        .await
        .expect("response");

    assert_eq!(response.first_content(), Some("你好!"));
    assert_eq!(mock.request_count(), 2);
}

#[tokio::test]
async fn base_url_override_is_used_for_the_request() {
    let mock = MockTransport::new();
    mock.push_json(200, SUCCESS);

    let client = Client::builder()
        .transport(mock.clone())
        .openai()
        .api_key("sk-test")
        .base_url("https://gateway.example.com/openai")
        .build()
        .expect("client");

    client
        .openai()
        .expect("openai")
        .chat()
        .create(request())
        .await
        .expect("response");

    assert_eq!(
        mock.requests()[0].uri.to_string(),
        "https://gateway.example.com/openai/chat/completions"
    );
}

#[tokio::test]
async fn openai_accessor_requires_provider_configuration() {
    let client = Client::builder()
        .transport(MockTransport::new())
        .build()
        .expect("client");

    assert!(client.openai().is_err());
}

#[tokio::test]
async fn extensions_are_sent_through_to_the_wire_body() {
    let mock = MockTransport::new();
    mock.push_json(200, SUCCESS);

    let request = ChatCompletionRequest::builder()
        .model("gpt-4o-mini")
        .user("hello")
        .extension("response_format", json!({"type": "json_object"}))
        .build()
        .expect("request");

    client(&mock)
        .openai()
        .expect("openai")
        .chat()
        .create(request)
        .await
        .expect("response");

    let body: Value = serde_json::from_slice(&mock.requests()[0].body).expect("body");
    assert_eq!(body["response_format"]["type"], "json_object");
}

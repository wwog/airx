//! SDK 体验层：请求选项、响应元数据、Stream、parse、默认重试。

#![cfg(all(feature = "openai", feature = "stream"))]

use std::time::Duration;

use airx::prelude::*;
use airx_core::http::{HeaderMap, HeaderValue};
use airx_transport::MockTransport;
use bytes::Bytes;
use futures_util::TryStreamExt;
use serde_json::Value;

const CHAT_RESPONSE: &str = r#"{
  "id": "chatcmpl-1",
  "model": "gpt-4o-mini",
  "choices": [{
    "index": 0,
    "message": {"role": "assistant", "content": "你好"},
    "finish_reason": "stop"
  }],
  "usage": {"prompt_tokens": 3, "completion_tokens": 2, "total_tokens": 5}
}"#;

fn request() -> ChatCompletionRequest {
    ChatCompletionRequest::builder()
        .model("gpt-4o-mini")
        .user("hi")
        .build()
        .expect("request")
}

fn client(mock: &MockTransport) -> OpenAi {
    Client::builder()
        .transport(mock.clone())
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client")
}

#[tokio::test]
async fn request_options_add_headers_and_idempotency_key() {
    let mock = MockTransport::new();
    mock.push_json(200, CHAT_RESPONSE);

    let options = RequestOptions::new()
        .try_header("x-trace-id", "trace-1")
        .expect("header")
        .idempotency_key("idem-1");

    client(&mock)
        .chat()
        .with_options(options)
        .create(request())
        .await
        .expect("response");

    let sent = &mock.requests()[0];
    assert_eq!(sent.headers["x-trace-id"], "trace-1");
    assert_eq!(sent.headers["idempotency-key"], "idem-1");
    assert_eq!(sent.headers["accept"], "application/json");
    assert!(
        sent.headers["user-agent"]
            .to_str()
            .expect("ua")
            .starts_with("airx/")
    );
}

#[tokio::test]
async fn create_with_meta_exposes_status_headers_and_request_id() {
    let mock = MockTransport::new();
    let mut headers = HeaderMap::new();
    headers.insert("x-request-id", HeaderValue::from_static("req-42"));
    mock.push_response(200, headers, Bytes::from_static(CHAT_RESPONSE.as_bytes()));

    let response = client(&mock)
        .chat()
        .create_with_meta(request())
        .await
        .expect("response");

    assert_eq!(response.meta.status.as_u16(), 200);
    assert_eq!(response.meta.request_id.as_deref(), Some("req-42"));
    assert!(!response.meta.raw_body.is_empty());
    assert_eq!(response.data.first_content(), Some("你好"));
}

#[tokio::test]
async fn stream_implements_futures_stream() {
    let mock = MockTransport::new();
    mock.push_stream(
        200,
        HeaderMap::new(),
        [
            Bytes::from_static(
                b"data: {\"id\":\"c1\",\"model\":\"gpt-4o-mini\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"\xe4\xbd\xa0\"},\"finish_reason\":null}]}\n\n",
            ),
            Bytes::from_static(
                b"data: {\"id\":\"c1\",\"model\":\"gpt-4o-mini\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"\xe5\xa5\xbd\"},\"finish_reason\":\"stop\"}]}\n\n",
            ),
            Bytes::from_static(b"data: [DONE]\n\n"),
        ],
    );

    let stream = client(&mock)
        .chat()
        .stream(request())
        .await
        .expect("stream");

    let mut accumulator = ChatStreamAccumulator::new();
    let mut stream = stream;
    while let Some(event) = stream.try_next().await.expect("event") {
        accumulator.apply(event).expect("apply");
    }

    assert_eq!(
        accumulator.finish().expect("finish").first_content(),
        Some("你好")
    );
}

#[tokio::test]
async fn stream_final_text_consumes_and_accumulates() {
    let mock = MockTransport::new();
    mock.push_stream(
        200,
        HeaderMap::new(),
        [
            Bytes::from_static(
                b"data: {\"id\":\"c1\",\"model\":\"gpt-4o-mini\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello\"},\"finish_reason\":\"stop\"}]}\n\n",
            ),
            Bytes::from_static(b"data: [DONE]\n\n"),
        ],
    );

    let text = client(&mock)
        .chat()
        .stream(request())
        .await
        .expect("stream")
        .final_text()
        .await
        .expect("text");

    assert_eq!(text, "hello");
}

#[derive(Debug, serde::Deserialize, PartialEq)]
struct Answer {
    text: String,
}

#[tokio::test]
async fn parse_as_deserializes_structured_output() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        r#"{
          "id": "chatcmpl-1",
          "model": "gpt-4o-mini",
          "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "{\"text\":\"hello\"}"},
            "finish_reason": "stop"
          }]
        }"#,
    );

    let answer: Answer = client(&mock)
        .chat()
        .parse_as(request())
        .await
        .expect("parsed");

    assert_eq!(
        answer,
        Answer {
            text: "hello".to_owned()
        }
    );
}

#[tokio::test]
async fn native_default_retries_429() {
    let mock = MockTransport::new();
    let mut headers = HeaderMap::new();
    headers.insert("retry-after", HeaderValue::from_static("0"));
    mock.push_response(429, headers, Bytes::from_static(b"{}"));
    mock.push_json(200, CHAT_RESPONSE);

    let client = Client::builder()
        .transport(mock.clone())
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client");

    client.chat().create(request()).await.expect("response");

    assert_eq!(mock.request_count(), 2);
}

#[test]
fn retry_policy_can_still_be_disabled() {
    let policy = RetryPolicy::disabled().with_initial_backoff(Duration::from_millis(1));
    assert!(!policy.is_enabled());
}

#[tokio::test]
async fn error_exposes_status_request_id_and_headers() {
    let mock = MockTransport::new();
    let mut headers = HeaderMap::new();
    headers.insert("x-request-id", HeaderValue::from_static("req-err"));
    headers.insert("retry-after", HeaderValue::from_static("3"));
    mock.push_response(
        429,
        headers,
        Bytes::from_static(
            br#"{"error":{"message":"rate limited","type":"rate_limit_error","code":"rate_limit"}}"#,
        ),
    );

    let client = Client::builder()
        .transport(mock.clone())
        .retry_policy(RetryPolicy::disabled())
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client");

    let error = client.chat().create(request()).await.expect_err("error");

    assert!(error.is_rate_limit());
    assert_eq!(error.request_id(), Some("req-err"));
    assert_eq!(error.retry_after(), Some(Duration::from_secs(3)));
    assert!(error.response_headers().is_some());
}

#[test]
fn with_meta_data_can_be_extracted() {
    let value = WithMeta {
        data: Value::from(1),
        meta: ResponseMeta {
            status: airx_core::http::StatusCode::OK,
            headers: HeaderMap::new(),
            request_id: None,
            raw_body: Bytes::new(),
        },
    };
    assert_eq!(value.into_data(), Value::from(1));
}

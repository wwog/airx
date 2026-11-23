//! 统一 API 端到端：Provider 选择、Adapter、警告。

#![cfg(feature = "unified")]

use airx::prelude::*;
use airx_transport::MockTransport;
use airx_unified::{Content, Message};
use serde_json::{Value, json};

fn chat_response() -> &'static str {
    r#"{
      "id": "chatcmpl-1",
      "model": "gpt-4o-mini",
      "choices": [{
        "index": 0,
        "message": {"role": "assistant", "content": "你好"},
        "finish_reason": "stop"
      }],
      "usage": {"prompt_tokens": 3, "completion_tokens": 2, "total_tokens": 5}
    }"#
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn generate_routes_to_openai_and_maps_back() {
    let mock = MockTransport::new();
    mock.push_json(200, chat_response());

    let client = Client::builder()
        .transport(mock.clone())
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client");

    let response = client
        .generate(
            GenerateRequest::builder()
                .model("gpt-4o-mini")
                .instructions("be brief")
                .user("hi")
                .temperature(0.2)
                .build()
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.provider, ProviderId::openai());
    assert_eq!(response.text(), "你好");
    assert_eq!(response.usage.map(|usage| usage.total_tokens), Some(5));
    assert!(response.warnings.is_empty());

    let sent: Value = serde_json::from_slice(&mock.requests()[0].body).expect("body");
    assert_eq!(sent["model"], "gpt-4o-mini");
    assert_eq!(sent["messages"][0]["role"], "system");
    assert_eq!(sent["temperature"], 0.2);
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn strict_rejects_unmappable_content_and_best_effort_warns() {
    let mock = MockTransport::new();
    mock.push_json(200, chat_response());

    let client = Client::builder()
        .transport(mock.clone())
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client");

    let request = || {
        GenerateRequest::builder()
            .model("gpt-4o-mini")
            .user("hi")
            .message(Message {
                role: airx_unified::Role::Assistant,
                content: vec![Content::Thinking("hidden".to_owned())],
            })
            .build()
            .expect("request")
    };

    let error = client.generate(request()).await.expect_err("strict");
    assert!(matches!(error, AirxError::Compatibility(_)));

    let response = client
        .generate_best_effort(request())
        .await
        .expect("best effort");
    assert_eq!(response.warnings.len(), 1);
    assert!(response.warnings[0].message.contains("thinking"));
}

#[cfg(feature = "anthropic")]
#[tokio::test]
async fn model_name_infers_anthropic_provider() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        json!({
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet",
            "content": [{"type": "text", "text": "ok"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 1, "output_tokens": 2}
        })
        .to_string(),
    );

    let client = Client::builder()
        .transport(mock.clone())
        .anthropic()
        .api_key("sk-ant-test")
        .build()
        .expect("client");

    let response = client
        .generate(
            GenerateRequest::builder()
                .model("claude-3-5-sonnet")
                .user("hi")
                .max_tokens(64)
                .build()
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.provider, ProviderId::anthropic());
    assert_eq!(response.text(), "ok");
    assert_eq!(mock.requests()[0].uri.path(), "/v1/messages");
    assert_eq!(mock.requests()[0].headers["x-api-key"], "sk-ant-test");
}

#[cfg(feature = "openai")]
#[test]
fn capabilities_are_exposed_on_the_client() {
    let client = Client::builder()
        .transport(MockTransport::new())
        .build()
        .expect("client");

    let caps = client.capabilities(&ProviderId::openai(), &ModelId::new("gpt-4o-mini"));
    assert!(caps.tools.is_supported());
}

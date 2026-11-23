//! Anthropic Messages 协议契约测试。

#![cfg(feature = "anthropic")]

use airx_core::AirxError;
use airx_core::http::{HeaderMap, HttpResponse, Method, StatusCode, header};
use airx_protocol::Endpoint;
use airx_protocol::anthropic::AnthropicConfig;
use airx_protocol::anthropic::messages::{
    ContentBlock, MessagesEndpoint, MessagesRequest, MessagesResponse,
};
use serde_json::{Value, json};

const RESPONSE_FIXTURE: &str = include_str!("fixtures/anthropic/messages/response.json");

fn endpoint() -> MessagesEndpoint {
    MessagesEndpoint::new(AnthropicConfig::new("sk-ant-test"))
}

#[test]
fn encode_uses_messages_path_and_anthropic_headers() {
    let request = MessagesRequest::builder()
        .model("claude-3-5-sonnet")
        .max_tokens(256)
        .system("be brief")
        .user("hello")
        .build()
        .expect("request");

    let http = endpoint().encode(&request).expect("encode");
    assert_eq!(http.method, Method::POST);
    assert_eq!(
        http.uri.to_string(),
        "https://api.anthropic.com/v1/messages"
    );
    assert_eq!(
        http.headers["x-api-key"].to_str().expect("key"),
        "sk-ant-test"
    );
    assert_eq!(
        http.headers["anthropic-version"].to_str().expect("version"),
        "2023-06-01"
    );

    let body: Value = serde_json::from_slice(&http.body).expect("body");
    assert_eq!(body["max_tokens"], 256);
    assert_eq!(body["system"], "be brief");
    assert_eq!(body["messages"][0]["content"], "hello");
}

#[test]
fn decode_keeps_thinking_tool_use_and_server_tool_blocks() {
    let response: MessagesResponse = endpoint()
        .decode(HttpResponse::new(
            StatusCode::OK,
            HeaderMap::new(),
            RESPONSE_FIXTURE.as_bytes(),
        ))
        .expect("decode");

    assert_eq!(response.id, "msg_123");
    assert_eq!(response.text(), "我来查一下。");
    assert_eq!(response.stop_reason.as_deref(), Some("tool_use"));
    assert_eq!(
        response.usage.as_ref().map(|u| u.cache_read_input_tokens),
        Some(Some(8))
    );

    assert!(matches!(response.content[0], ContentBlock::Thinking(_)));
    let calls = response.tool_uses();
    assert_eq!(calls[0].input["q"], "singapore");
    match &response.content[3] {
        ContentBlock::ServerToolUse(block) => {
            assert_eq!(block.id, "srv_1");
            assert_eq!(block.name, "web_search");
        }
        other => panic!("unexpected block: {other:?}"),
    }

    let value = serde_json::to_value(&response).expect("serialize");
    assert_eq!(value["content"][3]["id"], "srv_1");
    assert_eq!(value["content"][3]["type"], "server_tool_use");
}

#[test]
fn provider_error_keeps_type_and_message() {
    let error = endpoint()
        .decode(HttpResponse::new(
            StatusCode::TOO_MANY_REQUESTS,
            HeaderMap::new(),
            br#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#
                .as_slice(),
        ))
        .expect_err("provider error");

    match error {
        AirxError::Provider(error) => {
            assert_eq!(error.status, Some(429));
            assert_eq!(error.code.as_deref(), Some("rate_limit_error"));
            assert!(error.message.contains("slow down"));
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn version_and_base_url_can_be_overridden() {
    let config = AnthropicConfig::new("key")
        .with_base_url("https://gateway.example.com/anthropic")
        .expect("base url")
        .with_api_version("2024-01-01");
    let http = MessagesEndpoint::new(config)
        .encode(
            &MessagesRequest::builder()
                .model("m")
                .max_tokens(1)
                .user("hi")
                .build()
                .expect("request"),
        )
        .expect("encode");

    assert_eq!(
        http.uri.to_string(),
        "https://gateway.example.com/anthropic/v1/messages"
    );
    assert_eq!(http.headers["anthropic-version"], "2024-01-01");
    assert_eq!(http.headers[header::CONTENT_TYPE], "application/json");
    let _ = json!({});
}

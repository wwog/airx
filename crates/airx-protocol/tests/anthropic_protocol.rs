//! Anthropic Messages / Files / Batches 协议契约测试。

#![cfg(feature = "anthropic")]

use airx_core::AirxError;
use airx_core::http::{HeaderMap, HttpResponse, Method, StatusCode, header};
use airx_core::json::Extensions;
use airx_protocol::Endpoint;
use airx_protocol::anthropic::AnthropicConfig;
use airx_protocol::anthropic::batches::{BatchResultsTypedEndpoint, DeleteBatchEndpoint};
use airx_protocol::anthropic::files::{DeleteFileEndpoint, ListFilesEndpoint, ListFilesRequest};
use airx_protocol::anthropic::messages::{
    CacheControl, ContentBlock, DocumentBlock, DocumentSource, Message, MessagesEndpoint,
    MessagesRequest, MessagesResponse, Role, StopReason, TextBlock, Tool, ToolChoice,
};
use serde_json::{Value, json};

const RESPONSE_FIXTURE: &str = include_str!("fixtures/anthropic/messages/response.json");

fn config() -> AnthropicConfig {
    AnthropicConfig::new("sk-ant-test")
}

fn endpoint() -> MessagesEndpoint {
    MessagesEndpoint::new(config())
}

fn ok(body: &'static [u8]) -> HttpResponse {
    HttpResponse::new(StatusCode::OK, HeaderMap::new(), body)
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
    // P0-1: 默认版本对齐官方当前文档面。
    assert_eq!(
        http.headers["anthropic-version"].to_str().expect("version"),
        "2026-10-21"
    );

    let body: Value = serde_json::from_slice(&http.body).expect("body");
    assert_eq!(body["max_tokens"], 256);
    assert_eq!(body["system"], "be brief");
    assert_eq!(body["messages"][0]["content"], "hello");
}

#[test]
fn decode_keeps_thinking_tool_use_and_server_tool_blocks() {
    let response: MessagesResponse = endpoint()
        .decode(ok(RESPONSE_FIXTURE.as_bytes()))
        .expect("decode");

    assert_eq!(response.id, "msg_123");
    assert_eq!(response.text(), "我来查一下。");
    assert_eq!(response.stop_reason.as_deref(), Some("tool_use"));
    assert!(response.is_tool_use());
    assert_eq!(
        response.usage.as_ref().map(|u| u.cache_read_input_tokens),
        Some(Some(8))
    );
    assert_eq!(
        response.usage.as_ref().and_then(|u| u.input_tokens),
        Some(20)
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
fn provider_error_uses_body_request_id_and_error_code() {
    let error = endpoint()
        .decode(HttpResponse::new(
            StatusCode::BAD_REQUEST,
            HeaderMap::new(),
            br#"{"type":"error","request_id":"req_body","error":{"type":"invalid_request_error","message":"too big","details":{"error_code":"request_too_large"}}}"#
                .as_slice(),
        ))
        .expect_err("provider error");

    match error {
        AirxError::Provider(error) => {
            assert_eq!(error.request_id.as_deref(), Some("req_body"));
            assert_eq!(error.code.as_deref(), Some("request_too_large"));
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
}

#[test]
fn system_blocks_and_system_role_are_supported() {
    let request = MessagesRequest::builder()
        .model("m")
        .max_tokens(32)
        .system_blocks([
            TextBlock::new("be brief"),
            TextBlock::new("cached prefix").cached(CacheControl::ephemeral()),
        ])
        .message(Message::system("mid-conversation instruction"))
        .user("hi")
        .build()
        .expect("request");

    let http = endpoint().encode(&request).expect("encode");
    let body: Value = serde_json::from_slice(&http.body).expect("body");
    assert!(body["system"].is_array());
    assert_eq!(body["system"][1]["cache_control"]["type"], "ephemeral");
    assert_eq!(body["messages"][0]["role"], "system");

    let parsed: Message = serde_json::from_value(json!({
        "role": "system", "content": "x"
    }))
    .expect("system message");
    assert_eq!(parsed.role, Role::System);
}

#[test]
fn document_blocks_are_typed_in_requests() {
    let request = MessagesRequest::builder()
        .model("m")
        .max_tokens(32)
        .message(Message::blocks(
            Role::User,
            vec![
                ContentBlock::Text(TextBlock::new("Summarize")),
                ContentBlock::Document(DocumentBlock {
                    source: DocumentSource::file("file_1"),
                    title: Some("invoice.pdf".to_owned()),
                    context: None,
                    citations: None,
                    cache_control: None,
                    extensions: Extensions::new(),
                }),
            ],
        ))
        .build()
        .expect("request");

    let http = endpoint().encode(&request).expect("encode");
    let body: Value = serde_json::from_slice(&http.body).expect("body");
    assert_eq!(body["messages"][0]["content"][1]["type"], "document");
    assert_eq!(body["messages"][0]["content"][1]["source"]["type"], "file");
    assert_eq!(body["messages"][0]["content"][1]["title"], "invoice.pdf");
}

#[test]
fn tool_choice_and_tool_flags_serialize() {
    let request = MessagesRequest::builder()
        .model("m")
        .max_tokens(32)
        .user("hi")
        .tools([Tool::new("lookup", json!({"type": "object"}))
            .strict(true)
            .cached(CacheControl::ephemeral())])
        .tool_choice(ToolChoice::auto().disable_parallel_tool_use(true))
        .build()
        .expect("request");

    let body: Value =
        serde_json::from_slice(&endpoint().encode(&request).expect("encode").body).expect("body");
    assert_eq!(body["tools"][0]["strict"], true);
    assert_eq!(body["tools"][0]["cache_control"]["type"], "ephemeral");
    assert_eq!(body["tool_choice"]["disable_parallel_tool_use"], true);

    let request = MessagesRequest::builder()
        .model("m")
        .max_tokens(32)
        .user("hi")
        .tool_choice(ToolChoice::none())
        .build()
        .expect("request");
    let body: Value =
        serde_json::from_slice(&endpoint().encode(&request).expect("encode").body).expect("body");
    assert_eq!(body["tool_choice"], json!({"type": "none"}));
}

#[test]
fn stop_reason_helpers_cover_all_official_values() {
    let cases = [
        ("end_turn", StopReason::EndTurn),
        ("max_tokens", StopReason::MaxTokens),
        ("stop_sequence", StopReason::StopSequence),
        ("tool_use", StopReason::ToolUse),
        ("pause_turn", StopReason::PauseTurn),
        ("refusal", StopReason::Refusal),
        (
            "model_context_window_exceeded",
            StopReason::ModelContextWindowExceeded,
        ),
    ];
    for (text, expected) in cases {
        let response: MessagesResponse = serde_json::from_value(json!({
            "id": "msg_1", "model": "claude", "content": [], "stop_reason": text
        }))
        .expect("response");
        assert_eq!(response.stop_reason(), Some(expected));
    }

    let paused: MessagesResponse = serde_json::from_value(json!({
        "id": "msg_1", "model": "claude", "content": [], "stop_reason": "pause_turn",
        "container": {"id": "container_1"}
    }))
    .expect("response");
    assert!(paused.is_paused());
    assert_eq!(paused.container_id(), Some("container_1"));
}

#[test]
fn files_list_uses_page_cursor_and_next_page() {
    let endpoint = ListFilesEndpoint::new(config());
    let http = endpoint
        .encode(&ListFilesRequest::builder().limit(10).page("page_1").build())
        .expect("encode");
    assert_eq!(http.method, Method::GET);
    assert_eq!(http.uri.path(), "/v1/files");
    assert_eq!(http.uri.query(), Some("limit=10&page=page_1"));

    let page = endpoint
        .decode(ok(
            br#"{"data":[{"id":"file_1"}],"next_page":"page_2"}"#.as_slice()
        ))
        .expect("decode");
    assert_eq!(page.next_page(), Some("page_2"));

    let http = endpoint
        .encode(
            &ListFilesRequest::builder()
                .page(page.next_page().expect("cursor"))
                .build(),
        )
        .expect("encode");
    assert_eq!(http.uri.query(), Some("page=page_2"));
}

#[test]
fn files_ids_query_is_supported() {
    let http = ListFilesEndpoint::new(config())
        .encode(&ListFilesRequest::by_ids(["file_1", "file_2"]))
        .expect("encode");
    assert_eq!(http.uri.query(), Some("ids[]=file_1&ids[]=file_2"));
}

#[test]
fn file_delete_response_is_id_and_type_only() {
    let response = DeleteFileEndpoint::new(config())
        .decode(ok(br#"{"id":"file_1","type":"file_deleted"}"#.as_slice()))
        .expect("decode");
    assert_eq!(response.id, "file_1");
    assert_eq!(response.kind.as_deref(), Some("file_deleted"));

    let value = serde_json::to_value(&response).expect("serialize");
    assert!(value.get("deleted").is_none());
}

#[test]
fn batch_delete_uses_delete_method() {
    let endpoint = DeleteBatchEndpoint::new(config());
    let http = endpoint.encode(&"msgbatch_1".to_owned()).expect("encode");
    assert_eq!(http.method, Method::DELETE);
    assert_eq!(http.uri.path(), "/v1/messages/batches/msgbatch_1");

    let deleted = endpoint
        .decode(ok(
            br#"{"id":"msgbatch_1","type":"message_batch_deleted"}"#.as_slice()
        ))
        .expect("decode");
    assert_eq!(deleted.id, "msgbatch_1");
}

#[test]
fn typed_batch_results_decode_jsonl() {
    let body = concat!(
        r#"{"custom_id":"1","result":{"type":"succeeded","message":{"id":"msg_1","model":"claude","content":[{"type":"text","text":"ok"}]}}}"#,
        "\n",
        r#"{"custom_id":"2","result":{"type":"errored","error":{"type":"error","error":{"type":"invalid_request_error","message":"bad"},"request_id":"req_1"}}}"#,
        "\n",
    );
    let lines = BatchResultsTypedEndpoint::new(config())
        .decode(ok(body.as_bytes()))
        .expect("decode");
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].custom_id, "1");
}

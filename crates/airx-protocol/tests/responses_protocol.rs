//! OpenAI Responses 协议契约测试。

#![cfg(feature = "openai")]

use airx_core::http::{HeaderMap, HttpResponse, Method, StatusCode, header};
use airx_protocol::Endpoint;
use airx_protocol::openai::OpenAiConfig;
use airx_protocol::openai::responses::{
    OutputItem, ResponsesEndpoint, ResponsesRequest, ResponsesResponse,
};
use serde_json::{Value, json};

const RESPONSE_FIXTURE: &str = include_str!("fixtures/openai/responses/response.json");

fn endpoint() -> ResponsesEndpoint {
    ResponsesEndpoint::new(OpenAiConfig::new("sk-test"))
}

#[test]
fn encode_uses_responses_path_and_body() {
    let request = ResponsesRequest::builder()
        .model("gpt-4o-mini")
        .input("hello")
        .instructions("be brief")
        .max_output_tokens(64)
        .build()
        .expect("request");

    let http = endpoint().encode(&request).expect("encode");
    assert_eq!(http.method, Method::POST);
    assert_eq!(http.uri.to_string(), "https://api.openai.com/v1/responses");
    assert_eq!(
        http.headers[header::AUTHORIZATION].to_str().expect("auth"),
        "Bearer sk-test"
    );

    let body: Value = serde_json::from_slice(&http.body).expect("body");
    assert_eq!(body["input"], "hello");
    assert_eq!(body["instructions"], "be brief");
    assert_eq!(body["max_output_tokens"], 64);
}

#[test]
fn decode_keeps_output_items_and_usage() {
    let response: ResponsesResponse = endpoint()
        .decode(HttpResponse::new(
            StatusCode::OK,
            HeaderMap::new(),
            RESPONSE_FIXTURE.as_bytes(),
        ))
        .expect("decode");

    assert_eq!(response.id, "resp_123");
    assert_eq!(response.output_text(), "你好!");
    assert_eq!(response.usage.as_ref().map(|u| u.total_tokens), Some(20));

    match &response.output[1] {
        OutputItem::FunctionCall(call) => {
            assert_eq!(call.call_id, "call_1");
            assert_eq!(call.name, "lookup");
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn unknown_output_types_survive_roundtrip() {
    let json = json!({
        "id": "resp_x",
        "output": [
            {"type": "web_search_call", "id": "ws_1", "action": {"query": "rust"}},
            {"type": "message", "role": "assistant", "content": [
                {"type": "output_text", "text": "ok", "annotations": []}
            ]}
        ]
    });

    let response: ResponsesResponse = serde_json::from_value(json.clone()).expect("response");
    assert_eq!(response.output_text(), "ok");
    assert_eq!(serde_json::to_value(&response).expect("serialize"), json);
}

#[test]
fn provider_error_is_mapped() {
    let error = endpoint()
        .decode(HttpResponse::new(
            StatusCode::BAD_REQUEST,
            HeaderMap::new(),
            br#"{"error":{"message":"bad input","type":"invalid_request_error","code":"bad"}}"#
                .as_slice(),
        ))
        .expect_err("provider error");

    match error {
        airx_core::AirxError::Provider(error) => {
            assert_eq!(error.status, Some(400));
            assert_eq!(error.code.as_deref(), Some("bad"));
        }
        other => panic!("unexpected: {other:?}"),
    }
}

//! OpenAI Chat Completions 协议契约测试：编码、解码、错误映射。

#![cfg(feature = "openai")]

use airx_core::AirxError;
use airx_core::http::{HeaderMap, HeaderValue, HttpResponse, Method, StatusCode, header};
use airx_protocol::Endpoint;
use airx_protocol::openai::OpenAiConfig;
use airx_protocol::openai::chat::{
    ChatCompletionRequest, ChatCompletionResponse, ChatEndpoint, ChatMessage,
};
use serde_json::{Value, json};

const REQUEST_FIXTURE: &str = include_str!("fixtures/openai/chat/request.json");
const RESPONSE_FIXTURE: &str = include_str!("fixtures/openai/chat/response.json");
const ERROR_FIXTURE: &str = include_str!("fixtures/openai/chat/error.json");

fn endpoint() -> ChatEndpoint {
    ChatEndpoint::new(OpenAiConfig::new("sk-test"))
}

/// 去掉对象中的 `null` 字段，用于比较“语义等价”。
///
/// `Option<T>` 无法区分 `null` 与字段缺省：类型化往返保留语义，
/// 但不保证这两者的区别。需要字节级 / null 级保真时使用 Raw JSON。
fn strip_nulls(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(_, value)| !value.is_null())
                .map(|(key, value)| (key, strip_nulls(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(strip_nulls).collect()),
        other => other,
    }
}

#[test]
fn request_fixture_keeps_unknown_fields() {
    let request: ChatCompletionRequest =
        serde_json::from_str(REQUEST_FIXTURE).expect("deserialize request");

    assert_eq!(request.model.as_str(), "gpt-4o-mini");
    assert_eq!(request.messages.len(), 2);
    assert_eq!(request.messages[0].role(), "system");
    assert!(request.response_format.is_some());
    assert!(request.extensions.contains_key("logit_bias"));

    let value = serde_json::to_value(&request).expect("serialize request");
    assert_eq!(value["response_format"]["type"], "text");
    assert_eq!(value["messages"][1]["content"], "Say hello in Chinese.");
}

#[test]
fn response_fixture_roundtrips_semantically() {
    let fixture: Value = serde_json::from_str(RESPONSE_FIXTURE).expect("fixture");
    let response: ChatCompletionResponse =
        serde_json::from_str(RESPONSE_FIXTURE).expect("deserialize response");

    assert_eq!(response.id, "chatcmpl-123");
    assert_eq!(response.first_content(), Some("你好!"));
    assert_eq!(
        response.usage.as_ref().map(|usage| usage.total_tokens),
        Some(23)
    );
    // 未建模的 service_tier 不丢失。
    assert!(response.extensions.contains_key("service_tier"));

    assert_eq!(
        strip_nulls(serde_json::to_value(&response).expect("serialize response")),
        strip_nulls(fixture)
    );
}

#[test]
fn encode_builds_post_with_auth_and_json_body() {
    let request = ChatCompletionRequest::builder()
        .model("gpt-4o-mini")
        .system("be brief")
        .user("hello")
        .max_tokens(16)
        .build()
        .expect("request");

    let http = endpoint().encode(&request).expect("encode");

    assert_eq!(http.method, Method::POST);
    assert_eq!(
        http.uri.to_string(),
        "https://api.openai.com/v1/chat/completions"
    );
    assert_eq!(
        http.headers[header::AUTHORIZATION]
            .to_str()
            .expect("header"),
        "Bearer sk-test"
    );
    assert_eq!(
        http.headers[header::CONTENT_TYPE].to_str().expect("header"),
        "application/json"
    );

    let body: Value = serde_json::from_slice(&http.body).expect("body");
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][1]["content"], "hello");
    assert_eq!(body["max_tokens"], 16);
}

#[test]
fn base_url_and_provider_headers_can_be_overridden() {
    let config = OpenAiConfig::new("sk-test")
        .with_base_url("https://gateway.example.com/openai/")
        .expect("base url")
        .with_organization("org-1")
        .with_project("proj-1");

    let http = ChatEndpoint::new(config)
        .encode(
            &ChatCompletionRequest::builder()
                .model("m")
                .user("hi")
                .build()
                .expect("request"),
        )
        .expect("encode");

    assert_eq!(
        http.uri.to_string(),
        "https://gateway.example.com/openai/chat/completions"
    );
    assert_eq!(
        http.headers["openai-organization"]
            .to_str()
            .expect("header"),
        "org-1"
    );
    assert_eq!(
        http.headers["openai-project"].to_str().expect("header"),
        "proj-1"
    );
}

#[test]
fn decode_provider_error_keeps_metadata() {
    let mut headers = HeaderMap::new();
    headers.insert("x-request-id", HeaderValue::from_static("req_123"));

    let error = endpoint()
        .decode(HttpResponse::new(
            StatusCode::BAD_REQUEST,
            headers,
            ERROR_FIXTURE.as_bytes(),
        ))
        .expect_err("provider error");

    match error {
        AirxError::Provider(error) => {
            assert_eq!(error.status, Some(400));
            assert_eq!(error.code.as_deref(), Some("model_not_found"));
            assert_eq!(error.request_id.as_deref(), Some("req_123"));
            assert!(error.message.contains("gpt-unknown"));
            assert!(error.raw_body.is_some());
        }
        other => panic!("expected provider error, got {other:?}"),
    }
}

#[test]
fn decode_non_json_failure_is_http_error() {
    let error = endpoint()
        .decode(HttpResponse::new(
            StatusCode::BAD_GATEWAY,
            HeaderMap::new(),
            b"upstream unavailable".as_slice(),
        ))
        .expect_err("http error");

    match error {
        AirxError::Http(error) => {
            assert_eq!(error.status, 502);
            assert!(error.body.is_none());
        }
        other => panic!("expected http error, got {other:?}"),
    }
}

#[test]
fn decode_malformed_success_is_deserialization_error() {
    let error = endpoint()
        .decode(HttpResponse::new(
            StatusCode::OK,
            HeaderMap::new(),
            b"{".as_slice(),
        ))
        .expect_err("deserialization error");

    assert!(matches!(error, AirxError::Deserialization(_)));
}

#[test]
fn unknown_provider_fields_survive_a_second_roundtrip() {
    let json = json!({
        "id": "chatcmpl-9",
        "model": "m",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "ok",
                "audio": {"id": "audio_1"}
            },
            "finish_reason": "stop"
        }],
        "vendor_trace": {"span": "abc"}
    });

    let response: ChatCompletionResponse =
        serde_json::from_value(json.clone()).expect("deserialize");
    assert_eq!(response.first_content(), Some("ok"));
    assert_eq!(
        strip_nulls(serde_json::to_value(&response).expect("serialize")),
        strip_nulls(json)
    );
}

#[test]
fn chat_message_constructors_are_typed() {
    assert_eq!(ChatMessage::assistant("x").role(), "assistant");
    assert_eq!(ChatMessage::system("y").content(), Some("y"));
}

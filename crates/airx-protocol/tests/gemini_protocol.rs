//! Gemini 协议契约测试：编码、解码、错误映射与流事件。

#![cfg(feature = "gemini")]

use airx_core::AirxError;
use airx_core::http::{
    HeaderMap, HeaderName, HeaderValue, HttpResponse, Method, StatusCode, header,
};
use airx_core::stream::RawEvent;
use airx_protocol::Endpoint;
use airx_protocol::StreamingEndpoint;
use airx_protocol::gemini::GeminiConfig;
use airx_protocol::gemini::generate_content::{
    GenerateContentEndpoint, GenerateContentRequest, GenerateContentResponse,
};
use serde_json::{Value, json};

const REQUEST_FIXTURE: &str = include_str!("fixtures/gemini/generate/request.json");
const RESPONSE_FIXTURE: &str = include_str!("fixtures/gemini/generate/response.json");
const ERROR_FIXTURE: &str = include_str!("fixtures/gemini/generate/error.json");

fn endpoint() -> GenerateContentEndpoint {
    GenerateContentEndpoint::new(GeminiConfig::new("test-key"), "gemini-2.5-flash")
}

fn streaming_endpoint() -> GenerateContentEndpoint {
    GenerateContentEndpoint::streaming(GeminiConfig::new("test-key"), "gemini-2.5-flash")
}

fn raw_event(data: impl Into<String>) -> RawEvent {
    RawEvent {
        event: None,
        data: data.into(),
        id: None,
        retry: None,
    }
}

/// 去掉对象中的 `null` 字段，用于比较“语义等价”。
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
    let request: GenerateContentRequest =
        serde_json::from_str(REQUEST_FIXTURE).expect("deserialize request");

    assert_eq!(request.contents.len(), 3);
    assert_eq!(
        request.contents[0].role.as_ref().map(|role| role.as_str()),
        Some("user")
    );
    assert_eq!(request.contents[0].text(), "Hello");
    assert_eq!(
        request
            .system_instruction
            .as_ref()
            .map(|system| system.text()),
        Some("be concise".to_owned())
    );
    assert!(request.tools.is_some());
    assert!(request.generation_config.is_some());
    assert!(request.extensions.contains_key("futureField"));

    let value = serde_json::to_value(&request).expect("serialize request");
    assert_eq!(
        value["contents"][2]["parts"][1]["inlineData"]["mimeType"],
        "image/png"
    );
    assert_eq!(
        value["tools"][0]["functionDeclarations"][0]["name"],
        "get_weather"
    );
    assert_eq!(value["toolConfig"]["functionCallingConfig"]["mode"], "AUTO");
    assert_eq!(
        value["generationConfig"]["thinkingConfig"]["thinkingBudget"],
        1024
    );
    assert_eq!(
        value["safetySettings"][0]["threshold"],
        "BLOCK_MEDIUM_AND_ABOVE"
    );
    assert_eq!(value["cachedContent"], "cachedContents/abc");
    assert_eq!(value["futureField"]["keep"], true);
}

#[test]
fn response_fixture_roundtrips_semantically() {
    let fixture: Value = serde_json::from_str(RESPONSE_FIXTURE).expect("fixture");
    let response: GenerateContentResponse =
        serde_json::from_str(RESPONSE_FIXTURE).expect("deserialize response");

    assert_eq!(response.text(), "It is sunny in Singapore.");
    assert_eq!(response.thought_text(), "Let me think.");
    assert_eq!(response.finish_reason(), Some("STOP"));
    assert_eq!(response.function_calls()[0].name, "get_weather");
    assert_eq!(
        response
            .usage_metadata
            .as_ref()
            .map(|usage| usage.total_token_count),
        Some(33)
    );
    assert_eq!(response.model_version.as_deref(), Some("gemini-2.5-flash"));
    assert!(response.extensions.contains_key("futureTopField"));

    assert_eq!(
        strip_nulls(serde_json::to_value(&response).expect("serialize response")),
        strip_nulls(fixture)
    );
}

#[test]
fn encode_builds_post_with_auth_and_json_body() {
    let request = GenerateContentRequest::builder()
        .system("be brief")
        .user("hi")
        .max_output_tokens(64)
        .build()
        .expect("request");

    let http = endpoint().encode(&request).expect("encode");

    assert_eq!(http.method, Method::POST);
    assert_eq!(
        http.uri.path(),
        "/v1beta/models/gemini-2.5-flash:generateContent"
    );
    assert_eq!(http.uri.query(), None);
    assert_eq!(http.headers["x-goog-api-key"], "test-key");
    assert_eq!(http.headers[header::CONTENT_TYPE], "application/json");
    assert!(http.headers.get(header::AUTHORIZATION).is_none());

    let body: Value = serde_json::from_slice(&http.body).expect("body");
    assert_eq!(body["contents"][0]["parts"][0]["text"], "hi");
    assert_eq!(body["systemInstruction"]["parts"][0]["text"], "be brief");
    assert_eq!(body["generationConfig"]["maxOutputTokens"], 64);
    assert!(body.get("model").is_none());
}

#[test]
fn streaming_encode_uses_sse_alt_query() {
    let request = GenerateContentRequest::builder()
        .user("hi")
        .build()
        .expect("request");

    let http = streaming_endpoint().encode(&request).expect("encode");

    assert_eq!(
        http.uri.path(),
        "/v1beta/models/gemini-2.5-flash:streamGenerateContent"
    );
    assert_eq!(http.uri.query(), Some("alt=sse"));
}

#[test]
fn bare_and_prefixed_models_produce_same_path() {
    let request = GenerateContentRequest::builder()
        .user("hi")
        .build()
        .expect("request");

    let bare = GenerateContentEndpoint::new(GeminiConfig::new("k"), "gemini-2.5-flash")
        .encode(&request)
        .expect("bare");
    let prefixed = GenerateContentEndpoint::new(GeminiConfig::new("k"), "models/gemini-2.5-flash")
        .encode(&request)
        .expect("prefixed");

    assert_eq!(bare.uri, prefixed.uri);
}

#[test]
fn base_url_and_provider_headers_are_configurable() {
    let config = GeminiConfig::new("k")
        .with_base_url("https://gateway.example.com/gemini/v1beta")
        .expect("base url")
        .with_header(
            HeaderName::from_static("x-custom"),
            HeaderValue::from_static("value"),
        );
    let endpoint = GenerateContentEndpoint::new(config, "gemini-2.5-flash");
    let request = GenerateContentRequest::builder()
        .user("hi")
        .build()
        .expect("request");

    let http = endpoint.encode(&request).expect("encode");
    assert_eq!(
        http.uri.path(),
        "/gemini/v1beta/models/gemini-2.5-flash:generateContent"
    );
    assert_eq!(http.headers["x-custom"], "value");
}

#[test]
fn decode_success_parses_response() {
    let response = endpoint()
        .decode(HttpResponse::new(
            StatusCode::OK,
            HeaderMap::new(),
            RESPONSE_FIXTURE,
        ))
        .expect("decode");

    assert_eq!(response.text(), "It is sunny in Singapore.");
}

#[test]
fn decode_error_maps_to_provider_error() {
    let mut headers = HeaderMap::new();
    headers.insert("x-goog-request-id", HeaderValue::from_static("req-42"));

    let error = endpoint()
        .decode(HttpResponse::new(
            StatusCode::BAD_REQUEST,
            headers,
            ERROR_FIXTURE,
        ))
        .expect_err("error");

    match error {
        AirxError::Provider(error) => {
            assert_eq!(error.status, Some(400));
            assert_eq!(error.code.as_deref(), Some("INVALID_ARGUMENT"));
            assert_eq!(
                error.message,
                "API key not valid. Please pass a valid API key."
            );
            assert_eq!(error.request_id.as_deref(), Some("req-42"));
            assert!(error.raw_body.is_some());
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn stream_events_decode_and_empty_data_is_ignored() {
    let endpoint = streaming_endpoint();
    assert!(
        endpoint
            .decode_event(raw_event(""))
            .expect("empty")
            .is_none()
    );

    let event = endpoint
        .decode_event(raw_event(
            json!({
                "candidates": [{
                    "index": 0,
                    "content": {"role": "model", "parts": [{"text": "你"}]}
                }]
            })
            .to_string(),
        ))
        .expect("event")
        .expect("some");

    assert_eq!(event.text(), "你");
}

#[test]
fn stream_error_event_is_an_error() {
    let error = streaming_endpoint()
        .decode_event(raw_event(ERROR_FIXTURE))
        .expect_err("error");

    match error {
        AirxError::Provider(error) => {
            assert_eq!(error.code.as_deref(), Some("INVALID_ARGUMENT"));
        }
        other => panic!("unexpected: {other:?}"),
    }
}

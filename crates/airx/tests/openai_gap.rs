//! OpenAI 差距修复（GAP §8）门面级测试：stored completions、compact、moderations、
//! models.delete、retrieve query、stream 校验、retry 配额分类。

#![cfg(feature = "openai")]

use airx::prelude::*;
use airx_core::AirxError;
use airx_core::http::Method;
use airx_protocol::openai::chat::{
    ChatCompletionListRequest, ChatCompletionUpdateRequest, StreamOptions,
};
use airx_protocol::openai::files::ExpiresAfter;
use airx_protocol::openai::moderations::ModerationRequest;
use airx_protocol::openai::responses::{
    CompactRequest, ResponsesTool, ResponsesToolChoice, RetrieveResponseOptions,
};
use airx_transport::MockTransport;
use serde_json::{Value, json};

fn client(mock: &MockTransport) -> OpenAi {
    Client::builder()
        .transport(mock.clone())
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client")
}

fn chat_request(stream: bool) -> ChatCompletionRequest {
    ChatCompletionRequest::builder()
        .model("gpt-4o-mini")
        .user("hi")
        .stream(stream)
        .build()
        .expect("request")
}

fn responses_request(stream: bool) -> ResponsesRequest {
    ResponsesRequest::builder()
        .model("gpt-5")
        .input("hi")
        .stream(stream)
        .build()
        .expect("request")
}

#[tokio::test]
async fn create_rejects_stream_true_with_configuration_error() {
    let mock = MockTransport::new();
    // 不需要任何响应：请求应在发送前被拒绝。
    let error = client(&mock)
        .chat()
        .create(chat_request(true))
        .await
        .expect_err("configuration error");
    match error {
        AirxError::Configuration(error) => {
            assert!(error.to_string().contains("stream"));
        }
        other => panic!("unexpected: {other:?}"),
    }
    assert_eq!(mock.request_count(), 0);

    let error = client(&mock)
        .responses()
        .create(responses_request(true))
        .await
        .expect_err("configuration error");
    assert!(matches!(error, AirxError::Configuration(_)));
    assert_eq!(mock.request_count(), 0);

    // stream=false 正常发送。
    mock.push_json(200, r#"{"id":"c1","model":"m","choices":[]}"#);
    client(&mock)
        .chat()
        .create(chat_request(false))
        .await
        .expect("response");
    assert_eq!(mock.request_count(), 1);
}

#[tokio::test]
async fn chat_stored_completions_endpoints() {
    let mock = MockTransport::new();
    mock.push_json(200, r#"{"id":"c1","model":"m","choices":[]}"#);
    mock.push_json(200, r#"{"object":"list","data":[],"has_more":false}"#);
    mock.push_json(200, r#"{"id":"c1","model":"m","choices":[]}"#);
    mock.push_json(
        200,
        r#"{"id":"c1","object":"chat.completion.deleted","deleted":true}"#,
    );

    let chat = client(&mock).chat();
    let _ = chat.retrieve("c1").await.expect("retrieve");
    let list = chat
        .list(
            ChatCompletionListRequest::builder()
                .limit(3)
                .order("desc")
                .metadata("trace", "t-1")
                .build(),
        )
        .await
        .expect("list");
    assert!(list.data.is_empty());
    let _ = chat
        .update(ChatCompletionUpdateRequest::new(
            "c1",
            json!({"trace": "t-2"}),
        ))
        .await
        .expect("update");
    let deleted = chat.delete("c1").await.expect("delete");
    assert!(deleted.deleted);

    let requests = mock.requests();
    assert_eq!(requests[0].method, Method::GET);
    assert_eq!(requests[0].uri.path(), "/v1/chat/completions/c1");
    assert_eq!(
        requests[1].uri.query(),
        Some("limit=3&order=desc&metadata[trace]=t-1")
    );
    assert_eq!(requests[2].method, Method::POST);
    let body: Value = serde_json::from_slice(&requests[2].body).expect("body");
    assert_eq!(body["metadata"]["trace"], "t-2");
    assert_eq!(requests[3].method, Method::DELETE);
}

#[tokio::test]
async fn responses_retrieve_with_options_and_compact() {
    let mock = MockTransport::new();
    mock.push_json(200, r#"{"id":"resp_1","output":[]}"#);
    mock.push_json(
        200,
        r#"{"id":"cmp_1","object":"response.compaction","created_at":1,"output":[],"usage":{"input_tokens":1,"output_tokens":0,"total_tokens":1}}"#,
    );

    let responses = client(&mock).responses();
    responses
        .retrieve_with(
            "resp_1",
            RetrieveResponseOptions::default()
                .include("reasoning.encrypted_content")
                .starting_after(9),
        )
        .await
        .expect("retrieve");
    let compacted = responses
        .compact(
            CompactRequest::builder()
                .model("gpt-5")
                .previous_response_id("resp_1")
                .build()
                .expect("request"),
        )
        .await
        .expect("compact");
    assert_eq!(compacted.object.as_deref(), Some("response.compaction"));

    let requests = mock.requests();
    assert_eq!(
        requests[0].uri.query(),
        Some("include[]=reasoning.encrypted_content&starting_after=9")
    );
    assert_eq!(requests[1].uri.path(), "/v1/responses/compact");
    let body: Value = serde_json::from_slice(&requests[1].body).expect("body");
    assert_eq!(body["previous_response_id"], "resp_1");
}

#[tokio::test]
async fn models_delete_and_files_expiry() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        r#"{"id":"ft:gpt-4o-mini","object":"model","deleted":true}"#,
    );
    mock.push_json(200, r#"{"id":"file-1","object":"file","purpose":"batch"}"#);

    let api = client(&mock);
    let deleted = api
        .models()
        .delete("ft:gpt-4o-mini")
        .await
        .expect("delete model");
    assert!(deleted.deleted);

    let _ = api
        .files()
        .upload_with_expires_after(
            "a.jsonl",
            "batch",
            b"x".to_vec(),
            ExpiresAfter::after_seconds(3600),
        )
        .await
        .expect("upload");

    let requests = mock.requests();
    assert_eq!(requests[0].method, Method::DELETE);
    assert_eq!(requests[0].uri.path(), "/v1/models/ft:gpt-4o-mini");
    let body = String::from_utf8_lossy(&requests[1].body);
    assert!(body.contains("expires_after[seconds]"));
}

#[tokio::test]
async fn moderations_endpoint_works_through_facade() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        r#"{"id":"modr-1","model":"omni-moderation-latest","results":[{"flagged":false,"categories":{},"category_scores":{}}]}"#,
    );

    let response = client(&mock)
        .moderations()
        .create(
            ModerationRequest::builder()
                .text("hello")
                .model("omni-moderation-latest")
                .build()
                .expect("request"),
        )
        .await
        .expect("moderation");
    assert_eq!(response.id, "modr-1");
    assert_eq!(mock.requests()[0].uri.path(), "/v1/moderations");
    assert_eq!(mock.requests()[0].method, Method::POST);
}

#[tokio::test]
async fn quota_429_is_not_retried_by_default_policy() {
    let mock = MockTransport::new();
    mock.push_json(
        429,
        r#"{"error":{"message":"quota","type":"insufficient_quota","code":"insufficient_quota"}}"#,
    );

    let error = client(&mock)
        .chat()
        .create(chat_request(false))
        .await
        .expect_err("quota error");
    assert!(error.is_rate_limit());
    assert_eq!(mock.request_count(), 1);
}

#[tokio::test]
async fn chat_production_params_reach_the_wire() {
    let mock = MockTransport::new();
    mock.push_json(200, r#"{"id":"c1","model":"m","choices":[]}"#);

    let request = ChatCompletionRequest::builder()
        .model("gpt-4o-mini")
        .user("hi")
        .n(2)
        .seed(7)
        .store(true)
        .metadata(json!({"trace": "t"}))
        .service_tier("flex")
        .safety_identifier("id-1")
        .prompt_cache_key("cache-1")
        .logprobs(true)
        .top_logprobs(2)
        .frequency_penalty(0.1)
        .presence_penalty(0.2)
        .stream_options(StreamOptions {
            include_usage: false,
            include_obfuscation: Some(false),
        })
        .build()
        .expect("request");

    client(&mock)
        .chat()
        .create(request)
        .await
        .expect("response");

    let body: Value = serde_json::from_slice(&mock.requests()[0].body).expect("body");
    assert_eq!(body["n"], 2);
    assert_eq!(body["seed"], 7);
    assert_eq!(body["store"], true);
    assert_eq!(body["metadata"]["trace"], "t");
    assert_eq!(body["service_tier"], "flex");
    assert_eq!(body["safety_identifier"], "id-1");
    assert_eq!(body["prompt_cache_key"], "cache-1");
    assert_eq!(body["logprobs"], true);
    assert_eq!(body["top_logprobs"], 2);
    assert_eq!(body["frequency_penalty"], 0.1);
    assert_eq!(body["presence_penalty"], 0.2);
}

#[tokio::test]
async fn responses_flat_tools_and_replay_helpers_reach_the_wire() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        r#"{
          "id": "resp_1",
          "output": [
            {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "ok", "annotations": []}]},
            {"type": "function_call", "call_id": "call_1", "name": "lookup", "arguments": "{}"}
          ]
        }"#,
    );
    mock.push_json(200, r#"{"id":"resp_2","output":[]}"#);

    let responses = client(&mock).responses();
    let first = responses
        .create(
            ResponsesRequest::builder()
                .model("gpt-5")
                .tools([ResponsesTool::function(
                    "lookup",
                    json!({"type": "object"}),
                    None,
                )])
                .tool_choice(ResponsesToolChoice::function("lookup"))
                .build()
                .expect("request"),
        )
        .await
        .expect("response");

    let mut items = first.into_input();
    items.push(InputItem::function_call_output("call_1", "42"));
    responses
        .create(
            ResponsesRequest::builder()
                .model("gpt-5")
                .input_items(items)
                .build()
                .expect("request"),
        )
        .await
        .expect("response");

    let first_body: Value = serde_json::from_slice(&mock.requests()[0].body).expect("body");
    assert_eq!(first_body["tools"][0]["name"], "lookup");
    assert_eq!(
        first_body["tool_choice"],
        json!({"type": "function", "name": "lookup"})
    );

    let second_body: Value = serde_json::from_slice(&mock.requests()[1].body).expect("body");
    assert_eq!(second_body["input"][0]["type"], "message");
    assert_eq!(second_body["input"][1]["type"], "function_call");
    assert_eq!(second_body["input"][2]["type"], "function_call_output");
}

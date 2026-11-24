//! Gemini 端到端：Client → Endpoint 编码 → Transport(Mock) → 解码 / 流式聚合。

#![cfg(feature = "gemini")]

use airx::prelude::*;
use airx_core::error::StreamErrorKind;
use airx_core::http::{HeaderMap, Method};
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

fn client(mock: &MockTransport) -> Gemini {
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
    assert_eq!(body["model"], "models/gemini-2.5-flash");
}

#[tokio::test]
async fn model_prefix_is_normalized() {
    let mock = MockTransport::new();
    mock.push_json(200, RESPONSE);

    client(&mock)
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

#[test]
fn missing_api_key_is_a_configuration_error() {
    let result = Client::builder()
        .transport(MockTransport::new())
        .gemini()
        .build();

    assert!(matches!(result, Err(AirxError::Configuration(_))));
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

#[tokio::test]
async fn part_level_signature_survives_the_full_pipeline() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        r#"{
          "candidates": [{
            "index": 0,
            "content": {"role": "model", "parts": [
              {"functionCall": {"name": "get_weather", "args": {"city": "Singapore"}},
               "thoughtSignature": "sig-1"}
            ]},
            "finishReason": "STOP"
          }]
        }"#,
    );

    let response = client(&mock)
        .generate_content("gemini-2.5-flash")
        .create(request())
        .await
        .expect("response");

    let call = &response.function_calls()[0];
    assert_eq!(call.thought_signature(), Some("sig-1"));

    // 原样回传模型回合时，签名仍在 functionCall 兄弟层。
    let model_turn = response
        .first_candidate()
        .and_then(|candidate| candidate.content.clone())
        .expect("content");
    let follow_up = GenerateContentRequest::builder()
        .user("thanks")
        .content(model_turn)
        .build()
        .expect("request");
    let value = serde_json::to_value(&follow_up).expect("serialize");
    assert_eq!(
        value["contents"][1]["parts"][0]["thoughtSignature"],
        "sig-1"
    );
    assert!(
        value["contents"][1]["parts"][0]["functionCall"]
            .get("thoughtSignature")
            .is_none()
    );
}

#[tokio::test]
async fn caches_crud_runs_through_the_client() {
    use airx_protocol::gemini::caches::{
        CreateCachedContentRequest, ListCachedContentsRequest, UpdateCachedContentRequest,
    };

    let mock = MockTransport::new();
    mock.push_json(
        200,
        json!({"name": "cachedContents/abc", "model": "models/gemini-2.5-flash"}).to_string(),
    );
    mock.push_json(
        200,
        json!({
            "cachedContents": [{"name": "cachedContents/abc"}],
            "nextPageToken": "next"
        })
        .to_string(),
    );
    mock.push_json(200, json!({"name": "cachedContents/abc"}).to_string());
    mock.push_json(200, json!({"name": "cachedContents/abc"}).to_string());
    mock.push_json(200, json!({}).to_string());

    let client = client(&mock);
    let caches = client.caches();

    let created = caches
        .create(
            CreateCachedContentRequest::builder()
                .model("models/gemini-2.5-flash")
                .user("long document")
                .ttl("600s")
                .build()
                .expect("request"),
        )
        .await
        .expect("create");
    assert_eq!(created.resource_name(), Some("cachedContents/abc"));

    let listed = caches
        .list(
            ListCachedContentsRequest::builder()
                .page_size(10)
                .page_token("a+b==")
                .build(),
        )
        .await
        .expect("list");
    assert_eq!(listed.cached_contents.len(), 1);
    assert_eq!(listed.next_cursor(), Some("next"));

    let fetched = caches.get("abc").await.expect("get");
    assert_eq!(fetched.resource_name(), Some("cachedContents/abc"));

    let updated = caches
        .update("abc", UpdateCachedContentRequest::ttl("3600s"))
        .await
        .expect("update");
    assert_eq!(updated.resource_name(), Some("cachedContents/abc"));

    caches.delete("abc").await.expect("delete");

    let requests = mock.requests();
    assert_eq!(requests[0].method, Method::POST);
    assert_eq!(requests[0].uri.path(), "/v1beta/cachedContents");
    assert_eq!(requests[1].uri.path(), "/v1beta/cachedContents");
    assert_eq!(
        requests[1].uri.query(),
        Some("pageSize=10&pageToken=a%2Bb%3D%3D")
    );
    assert_eq!(requests[2].uri.path(), "/v1beta/cachedContents/abc");
    assert_eq!(requests[3].method, Method::PATCH);
    assert_eq!(requests[3].uri.query(), Some("updateMask=ttl"));
    assert_eq!(requests[4].method, Method::DELETE);
}

#[tokio::test]
async fn compute_tokens_runs_through_models() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        json!({"tokensInfo": [{"tokenIds": ["1", "2"], "tokens": ["he", "llo"]}]}).to_string(),
    );

    let response = client(&mock)
        .models()
        .compute_tokens(
            "gemini-2.5-flash",
            airx_protocol::gemini::models::ComputeTokensRequest::of_text("hello"),
        )
        .await
        .expect("compute tokens");

    assert_eq!(response.total_tokens(), 2);
    assert_eq!(
        mock.requests()[0].uri.path(),
        "/v1beta/models/gemini-2.5-flash:computeTokens"
    );
}

#[tokio::test]
async fn embed_content_writes_model_and_accepts_array_shape() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        json!({
            "embeddings": [{"values": [0.1, 0.2]}],
            "truncated": false
        })
        .to_string(),
    );

    let response = client(&mock)
        .embed_content("gemini-embedding-001")
        .create(
            EmbedContentRequest::builder()
                .text("hello")
                .build()
                .expect("request"),
        )
        .await
        .expect("embed");

    assert_eq!(response.values(), &[0.1, 0.2]);
    assert_eq!(response.embedding_count(), 1);

    let request = &mock.requests()[0];
    assert_eq!(
        request.uri.path(),
        "/v1beta/models/gemini-embedding-001:embedContent"
    );
    let body: Value = serde_json::from_slice(&request.body).expect("body");
    assert_eq!(body["model"], "models/gemini-embedding-001");
}

#[tokio::test]
async fn list_all_percent_encodes_opaque_page_tokens() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        json!({
            "models": [{"name": "models/a"}],
            "nextPageToken": "a+b/c=="
        })
        .to_string(),
    );
    mock.push_json(200, json!({"models": [{"name": "models/b"}]}).to_string());

    let models = client(&mock)
        .models()
        .list_all(GeminiListModelsRequest::builder().page_size(1).build())
        .await
        .expect("list all");

    assert_eq!(
        models
            .iter()
            .map(|model| model.name.as_str())
            .collect::<Vec<_>>(),
        ["models/a", "models/b"]
    );
    assert_eq!(
        mock.requests()[1].uri.query(),
        Some("pageSize=1&pageToken=a%2Bb%2Fc%3D%3D")
    );
}

#[tokio::test]
async fn caches_list_all_follows_cursors() {
    use airx_protocol::gemini::caches::ListCachedContentsRequest;

    let mock = MockTransport::new();
    mock.push_json(
        200,
        json!({"cachedContents": [{"name": "cachedContents/a"}], "nextPageToken": "next"})
            .to_string(),
    );
    mock.push_json(
        200,
        json!({"cachedContents": [{"name": "cachedContents/b"}]}).to_string(),
    );

    let caches = client(&mock)
        .caches()
        .list_all(ListCachedContentsRequest::default())
        .await
        .expect("list all");

    assert_eq!(caches.len(), 2);
    assert_eq!(mock.requests()[1].uri.query(), Some("pageToken=next"));
}

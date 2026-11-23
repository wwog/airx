//! Models / countTokens 端到端：路径、查询参数、认证 Header 与响应解码。

#![cfg(all(feature = "openai", feature = "anthropic", feature = "gemini"))]

use airx::prelude::*;
use airx_transport::MockTransport;
use serde_json::{Value, json};

#[tokio::test]
async fn openai_models_list_and_retrieve() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        json!({
            "object": "list",
            "data": [{"id": "gpt-4o-mini", "object": "model", "owned_by": "system"}]
        })
        .to_string(),
    );
    mock.push_json(
        200,
        json!({"id": "gpt-4o-mini", "owned_by": "system"}).to_string(),
    );

    let client = Client::builder()
        .transport(mock.clone())
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client");

    let list = client.models().list().await.expect("list");
    assert_eq!(list.data[0].id, "gpt-4o-mini");

    let model = client
        .models()
        .retrieve("gpt-4o-mini")
        .await
        .expect("retrieve");
    assert_eq!(model.id, "gpt-4o-mini");

    let requests = mock.requests();
    assert_eq!(requests[0].method.as_str(), "GET");
    assert_eq!(requests[0].uri.path(), "/v1/models");
    assert_eq!(requests[0].headers["authorization"], "Bearer sk-test");
    assert_eq!(requests[1].uri.path(), "/v1/models/gpt-4o-mini");
}

#[tokio::test]
async fn anthropic_models_list_uses_cursor_and_bearer_headers() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        json!({
            "data": [{"id": "claude-sonnet-4-5", "type": "model"}],
            "has_more": false
        })
        .to_string(),
    );

    let client = Client::builder()
        .transport(mock.clone())
        .anthropic()
        .api_key("sk-ant-test")
        .build()
        .expect("client");

    let page = client
        .models()
        .list(
            AnthropicListModelsRequest::builder()
                .limit(10)
                .after_id("cursor-1")
                .build(),
        )
        .await
        .expect("list");

    assert_eq!(page.data[0].id, "claude-sonnet-4-5");

    let request = &mock.requests()[0];
    assert_eq!(request.uri.path(), "/v1/models");
    assert_eq!(request.uri.query(), Some("limit=10&after_id=cursor-1"));
    assert_eq!(request.headers["x-api-key"], "sk-ant-test");
    assert_eq!(request.headers["anthropic-version"], "2023-06-01");
}

#[tokio::test]
async fn anthropic_models_list_all_follows_cursor() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        json!({
            "data": [{"id": "a"}, {"id": "b"}],
            "has_more": true,
            "last_id": "b"
        })
        .to_string(),
    );
    mock.push_json(
        200,
        json!({"data": [{"id": "c"}], "has_more": false}).to_string(),
    );

    let client = Client::builder()
        .transport(mock.clone())
        .anthropic()
        .api_key("sk-ant-test")
        .build()
        .expect("client");

    let all = client
        .models()
        .list_all(AnthropicListModelsRequest::builder().limit(2).build())
        .await
        .expect("list all");

    assert_eq!(
        all.iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    assert_eq!(mock.requests()[1].uri.query(), Some("limit=2&after_id=b"));
}

#[tokio::test]
async fn anthropic_count_tokens_uses_messages_path() {
    let mock = MockTransport::new();
    mock.push_json(200, json!({"input_tokens": 12}).to_string());

    let client = Client::builder()
        .transport(mock.clone())
        .anthropic()
        .api_key("sk-ant-test")
        .build()
        .expect("client");

    let response = client
        .messages()
        .count_tokens(
            CountTokensRequest::builder()
                .model("claude-sonnet-4-5")
                .system("be brief")
                .user("hi")
                .build()
                .expect("request"),
        )
        .await
        .expect("count");

    assert_eq!(response.input_tokens, 12);

    let request = &mock.requests()[0];
    assert_eq!(request.uri.path(), "/v1/messages/count_tokens");
    let body: Value = serde_json::from_slice(&request.body).expect("body");
    assert_eq!(body["model"], "claude-sonnet-4-5");
    assert_eq!(body["system"], "be brief");
}

#[tokio::test]
async fn gemini_models_list_and_get() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        json!({
            "models": [{"name": "models/gemini-2.5-flash", "displayName": "Gemini 2.5 Flash"}],
            "nextPageToken": "next"
        })
        .to_string(),
    );
    mock.push_json(200, json!({"name": "models/gemini-2.5-flash"}).to_string());

    let client = Client::builder()
        .transport(mock.clone())
        .gemini()
        .api_key("test-key")
        .build()
        .expect("client");

    let page = client
        .models()
        .list(
            GeminiListModelsRequest::builder()
                .page_size(5)
                .page_token("cursor")
                .build(),
        )
        .await
        .expect("list");
    assert_eq!(page.models[0].name, "models/gemini-2.5-flash");
    assert_eq!(page.next_cursor(), Some("next"));

    let model = client.models().get("gemini-2.5-flash").await.expect("get");
    assert_eq!(model.name, "models/gemini-2.5-flash");

    let requests = mock.requests();
    assert_eq!(requests[0].uri.path(), "/v1beta/models");
    assert_eq!(requests[0].uri.query(), Some("pageSize=5&pageToken=cursor"));
    assert_eq!(requests[0].headers["x-goog-api-key"], "test-key");
    assert_eq!(requests[1].uri.path(), "/v1beta/models/gemini-2.5-flash");
}

#[tokio::test]
async fn gemini_count_tokens_uses_model_path() {
    let mock = MockTransport::new();
    mock.push_json(200, json!({"totalTokens": 9}).to_string());

    let client = Client::builder()
        .transport(mock.clone())
        .gemini()
        .api_key("test-key")
        .build()
        .expect("client");

    let response = client
        .generate_content("gemini-2.5-flash")
        .count_tokens(
            GeminiCountTokensRequest::builder()
                .system("be brief")
                .user("hi")
                .build()
                .expect("request"),
        )
        .await
        .expect("count");

    assert_eq!(response.total_tokens, 9);

    let request = &mock.requests()[0];
    assert_eq!(
        request.uri.path(),
        "/v1beta/models/gemini-2.5-flash:countTokens"
    );
    let body: Value = serde_json::from_slice(&request.body).expect("body");
    assert_eq!(body["contents"][0]["parts"][0]["text"], "hi");
    assert_eq!(body["systemInstruction"]["parts"][0]["text"], "be brief");
}

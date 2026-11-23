//! 文件、批处理、Embeddings、Responses 资源与底层接入能力。

#![cfg(all(feature = "openai", feature = "anthropic", feature = "gemini"))]

use std::time::Duration;

use airx::prelude::*;
use airx_core::error::TransportError;
use airx_core::http::{HeaderMap, HttpRequest, HttpResponse, Method, StatusCode};
use airx_protocol::anthropic::messages::Message;
use airx_transport::{BoxFuture, MockTransport, Transport};
use bytes::Bytes;
use serde_json::{Value, json};

fn openai(mock: &MockTransport) -> OpenAi {
    Client::builder()
        .transport(mock.clone())
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client")
}

fn anthropic(mock: &MockTransport) -> Anthropic {
    Client::builder()
        .transport(mock.clone())
        .anthropic()
        .api_key("sk-ant-test")
        .build()
        .expect("client")
}

fn gemini(mock: &MockTransport) -> Gemini {
    Client::builder()
        .transport(mock.clone())
        .gemini()
        .api_key("test-key")
        .build()
        .expect("client")
}

#[tokio::test]
async fn per_request_retry_policy_overrides_client_default() {
    let mock = MockTransport::new();
    mock.push_json(
        429,
        r#"{"error":{"message":"slow down","type":"rate_limit_error","code":"rate_limit"}}"#,
    );

    let client = openai(&mock);
    let error = client
        .chat()
        .with_options(RequestOptions::new().retry_policy(RetryPolicy::disabled()))
        .create(
            ChatCompletionRequest::builder()
                .model("gpt-4o-mini")
                .user("hi")
                .build()
                .expect("request"),
        )
        .await
        .expect_err("error");

    assert!(error.is_rate_limit());
    assert_eq!(mock.request_count(), 1);
}

struct SlowTransport;

impl Transport for SlowTransport {
    fn execute(
        &self,
        _request: HttpRequest,
    ) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
        Box::pin(async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(HttpResponse::new(
                StatusCode::OK,
                HeaderMap::new(),
                Bytes::from_static(b"{}"),
            ))
        })
    }

    fn execute_stream(
        &self,
        _request: HttpRequest,
    ) -> BoxFuture<'_, Result<airx_transport::HttpStreamResponse, TransportError>> {
        Box::pin(async {
            Err(TransportError::new(
                airx_core::error::TransportErrorKind::Other,
                "not used",
            ))
        })
    }
}

#[tokio::test]
async fn per_request_timeout_is_enforced() {
    let client = Client::builder()
        .transport(SlowTransport)
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client");

    let error = client
        .chat()
        .with_options(RequestOptions::new().timeout(Duration::from_millis(1)))
        .create(
            ChatCompletionRequest::builder()
                .model("gpt-4o-mini")
                .user("hi")
                .build()
                .expect("request"),
        )
        .await
        .expect_err("timeout");

    assert!(matches!(error, AirxError::Transport(_)));
}

#[tokio::test]
async fn raw_execute_reuses_client_configuration() {
    let mock = MockTransport::new();
    mock.push_json(200, r#"{"id":"model-1"}"#);

    let client = openai(&mock);
    let request = HttpRequest::get("https://api.openai.com/v1/models".parse().expect("uri"));
    let response = client.execute(request).await.expect("execute");

    assert_eq!(response.status, 200);
    assert_eq!(&response.body[..], br#"{"id":"model-1"}"#);
}

#[tokio::test]
async fn openai_files_upload_list_retrieve_delete_content() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        json!({"id":"file-1","object":"file","filename":"a.jsonl","purpose":"batch"}).to_string(),
    );
    mock.push_json(200, json!({"object":"list","data":[]}).to_string());
    mock.push_json(200, json!({"id":"file-1","object":"file"}).to_string());
    mock.push_json(
        200,
        json!({"id":"file-1","object":"file","deleted":true}).to_string(),
    );
    mock.push_response(200, HeaderMap::new(), Bytes::from_static(b"line1\nline2\n"));

    let client = openai(&mock);
    let file = client
        .files()
        .upload("a.jsonl", "batch", b"content".to_vec())
        .await
        .expect("upload");
    assert_eq!(file.id, "file-1");

    let list = client
        .files()
        .list(
            ListFilesRequest::builder()
                .purpose("batch")
                .limit(5)
                .build(),
        )
        .await
        .expect("list");
    assert!(list.data.is_empty());

    let retrieved = client.files().retrieve("file-1").await.expect("retrieve");
    assert_eq!(retrieved.id, "file-1");

    let deleted = client.files().delete("file-1").await.expect("delete");
    assert!(deleted.deleted);

    let content = client.files().content("file-1").await.expect("content");
    assert_eq!(&content[..], b"line1\nline2\n");

    let requests = mock.requests();
    assert_eq!(requests[0].uri.path(), "/v1/files");
    assert!(
        requests[0].headers["content-type"]
            .to_str()
            .expect("content-type")
            .starts_with("multipart/form-data; boundary=")
    );
    let body = String::from_utf8_lossy(&requests[0].body);
    assert!(body.contains("name=\"purpose\""));
    assert!(body.contains("filename=\"a.jsonl\""));

    assert_eq!(requests[1].uri.query(), Some("purpose=batch&limit=5"));
    assert_eq!(requests[2].method, Method::GET);
    assert_eq!(requests[2].uri.path(), "/v1/files/file-1");
    assert_eq!(requests[3].method, Method::DELETE);
    assert_eq!(requests[4].uri.path(), "/v1/files/file-1/content");
}

#[tokio::test]
async fn openai_batches_create_list_cancel() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        json!({"id":"batch_1","object":"batch","status":"validating"}).to_string(),
    );
    mock.push_json(200, json!({"object":"list","data":[]}).to_string());
    mock.push_json(
        200,
        json!({"id":"batch_1","status":"cancelling"}).to_string(),
    );

    let client = openai(&mock);
    let batch = client
        .batches()
        .create(
            CreateBatchRequest::builder()
                .input_file_id("file-1")
                .endpoint("/v1/chat/completions")
                .completion_window("24h")
                .build()
                .expect("request"),
        )
        .await
        .expect("create");
    assert_eq!(batch.id, "batch_1");

    let _ = client
        .batches()
        .list(ListBatchesRequest {
            limit: Some(10),
            after: Some("batch_1".to_owned()),
        })
        .await
        .expect("list");

    let _ = client.batches().cancel("batch_1").await.expect("cancel");

    let requests = mock.requests();
    assert_eq!(requests[0].uri.path(), "/v1/batches");
    let body: Value = serde_json::from_slice(&requests[0].body).expect("body");
    assert_eq!(body["input_file_id"], "file-1");
    assert_eq!(body["completion_window"], "24h");
    assert_eq!(requests[1].uri.query(), Some("limit=10&after=batch_1"));
    assert_eq!(requests[2].method, Method::POST);
    assert_eq!(requests[2].uri.path(), "/v1/batches/batch_1/cancel");
}

#[tokio::test]
async fn openai_embeddings_create() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        json!({
            "object": "list",
            "data": [{"object": "embedding", "embedding": [0.1, 0.2], "index": 0}],
            "model": "text-embedding-3-small",
            "usage": {"prompt_tokens": 2, "total_tokens": 2}
        })
        .to_string(),
    );

    let response = openai(&mock)
        .embeddings()
        .create(
            EmbeddingRequest::builder()
                .model("text-embedding-3-small")
                .input("hello")
                .build()
                .expect("request"),
        )
        .await
        .expect("embedding");

    assert_eq!(response.data[0].embedding, vec![0.1, 0.2]);
    assert_eq!(mock.requests()[0].uri.path(), "/v1/embeddings");
}

#[tokio::test]
async fn responses_lifecycle_and_input_tokens() {
    let mock = MockTransport::new();
    mock.push_json(200, json!({"id":"resp_1","output":[]}).to_string());
    mock.push_json(200, json!({"id":"resp_1","status":"cancelled"}).to_string());
    mock.push_json(
        200,
        json!({"id":"resp_1","object":"response","deleted":true}).to_string(),
    );
    mock.push_json(
        200,
        json!({
            "object": "list",
            "data": [{"type": "reasoning", "id": "rs_1", "summary": []}],
            "has_more": false
        })
        .to_string(),
    );
    mock.push_json(
        200,
        json!({"object":"response.input_tokens","input_tokens":7}).to_string(),
    );

    let client = openai(&mock);
    let responses = client.responses();
    let _ = responses.retrieve("resp_1").await.expect("retrieve");
    let _ = responses.cancel("resp_1").await.expect("cancel");
    let _ = responses.delete("resp_1").await.expect("delete");

    let items = responses
        .input_items(ListInputItemsRequest::new("resp_1"))
        .await
        .expect("input items");
    assert_eq!(items.data.len(), 1);

    let tokens = responses
        .input_tokens_count(
            InputTokensCountRequest::builder()
                .model("gpt-5")
                .input("hi")
                .build()
                .expect("request"),
        )
        .await
        .expect("tokens");
    assert_eq!(tokens.input_tokens, 7);

    let requests = mock.requests();
    assert_eq!(requests[0].uri.path(), "/v1/responses/resp_1");
    assert_eq!(requests[1].uri.path(), "/v1/responses/resp_1/cancel");
    assert_eq!(requests[2].method, Method::DELETE);
    assert_eq!(requests[3].uri.path(), "/v1/responses/resp_1/input_items");
    assert_eq!(requests[4].uri.path(), "/v1/responses/input_tokens");
}

#[tokio::test]
async fn anthropic_files_and_batches() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        json!({"id":"file_1","type":"file","filename":"a.pdf"}).to_string(),
    );
    mock.push_json(200, json!({"data":[],"has_more":false}).to_string());
    mock.push_json(
        200,
        json!({"id":"msgbatch_1","type":"message_batch","processing_status":"in_progress"})
            .to_string(),
    );
    mock.push_json(
        200,
        json!({"id":"msgbatch_1","type":"message_batch"}).to_string(),
    );
    mock.push_response(
        200,
        HeaderMap::new(),
        Bytes::from_static(b"{\"custom_id\":\"1\"}\n"),
    );

    let client = anthropic(&mock);
    let file = client
        .files()
        .upload("a.pdf", "application/pdf", b"pdf".to_vec())
        .await
        .expect("upload");
    assert_eq!(file.id, "file_1");

    let _ = client
        .files()
        .list(AnthropicListFilesRequest::builder().limit(10).build())
        .await
        .expect("list");

    let request = MessagesRequest::builder()
        .model("claude-sonnet-4-5")
        .max_tokens(64)
        .user("hi")
        .build()
        .expect("messages");
    let _ = client
        .batches()
        .create(AnthropicCreateBatchRequest::new([BatchRequestItem {
            custom_id: "1".to_owned(),
            params: request,
        }]))
        .await
        .expect("batch");

    let _ = client.batches().cancel("msgbatch_1").await.expect("cancel");
    let results = client
        .batches()
        .results("msgbatch_1")
        .await
        .expect("results");
    assert!(!results.is_empty());

    let requests = mock.requests();
    assert_eq!(requests[0].uri.path(), "/v1/files");
    assert!(
        requests[0].headers["content-type"]
            .to_str()
            .expect("content-type")
            .starts_with("multipart/form-data")
    );
    assert_eq!(requests[2].uri.path(), "/v1/messages/batches");
    assert_eq!(
        requests[3].uri.path(),
        "/v1/messages/batches/msgbatch_1/cancel"
    );
    assert_eq!(
        requests[4].uri.path(),
        "/v1/messages/batches/msgbatch_1/results"
    );
}

#[tokio::test]
async fn gemini_embeddings_single_and_batch() {
    let mock = MockTransport::new();
    mock.push_json(200, json!({"embedding":{"values":[0.1,0.2]}}).to_string());
    mock.push_json(
        200,
        json!({"embeddings":[{"values":[0.3]},{"values":[0.4]}]}).to_string(),
    );

    let client = gemini(&mock);
    let embeddings = client.embed_content("gemini-embedding-001");
    let response = embeddings
        .create(
            EmbedContentRequest::builder()
                .text("hello")
                .task_type("RETRIEVAL_QUERY")
                .build()
                .expect("request"),
        )
        .await
        .expect("embed");
    assert_eq!(
        response.embedding.expect("embedding").values,
        vec![0.1, 0.2]
    );

    let batch = embeddings
        .batch([
            EmbedContentRequest::builder()
                .text("a")
                .build()
                .expect("request"),
            EmbedContentRequest::builder()
                .text("b")
                .build()
                .expect("request"),
        ])
        .await
        .expect("batch");
    assert_eq!(batch.embeddings.len(), 2);

    let requests = mock.requests();
    assert_eq!(
        requests[0].uri.path(),
        "/v1beta/models/gemini-embedding-001:embedContent"
    );
    assert_eq!(
        requests[1].uri.path(),
        "/v1beta/models/gemini-embedding-001:batchEmbedContents"
    );
    let body: Value = serde_json::from_slice(&requests[1].body).expect("body");
    assert_eq!(body["requests"][0]["model"], "models/gemini-embedding-001");
    assert_eq!(body["requests"][0]["content"]["parts"][0]["text"], "a");
}

#[tokio::test]
async fn responses_input_items_accept_complete_history() {
    let mock = MockTransport::new();
    mock.push_json(200, json!({"id":"resp_1","output":[]}).to_string());

    let client = openai(&mock);
    let _ = client
        .responses()
        .create(
            ResponsesRequest::builder()
                .model("gpt-5")
                .input_items(vec![
                    InputItem::Message {
                        role: "user".to_owned(),
                        content: InputMessageContent::Text("hi".to_owned()),
                    },
                    InputItem::Reasoning {
                        id: Some("rs_1".to_owned()),
                        summary: Some(vec![ReasoningSummary::text("thought")]),
                        encrypted_content: Some("cipher".to_owned()),
                        extensions: Default::default(),
                    },
                ])
                .build()
                .expect("request"),
        )
        .await
        .expect("response");

    let body: Value = serde_json::from_slice(&mock.requests()[0].body).expect("body");
    assert_eq!(body["input"][0]["type"], "message");
    assert_eq!(body["input"][1]["type"], "reasoning");
    assert_eq!(body["input"][1]["encrypted_content"], "cipher");
}

#[tokio::test]
async fn message_list_builders_accept_full_history() {
    let mock = MockTransport::new();
    mock.push_json(
        200,
        json!({"id":"c","model":"gpt-4o-mini","choices":[]}).to_string(),
    );
    mock.push_json(
        200,
        json!({"id":"m","model":"claude","content":[]}).to_string(),
    );
    mock.push_json(200, json!({"candidates":[]}).to_string());

    let chat = openai(&mock)
        .chat()
        .create(
            ChatCompletionRequest::builder()
                .model("gpt-4o-mini")
                .messages([
                    ChatMessage::user("one"),
                    ChatMessage::assistant("two"),
                    ChatMessage::user("three"),
                ])
                .build()
                .expect("request"),
        )
        .await;
    assert!(chat.is_ok());

    let messages = anthropic(&mock)
        .messages()
        .create(
            MessagesRequest::builder()
                .model("claude-sonnet-4-5")
                .max_tokens(64)
                .messages([Message::user("one"), Message::assistant("two")])
                .build()
                .expect("request"),
        )
        .await;
    assert!(messages.is_ok());

    let gemini_response = gemini(&mock)
        .generate_content("gemini-2.5-flash")
        .create(
            GenerateContentRequest::builder()
                .contents([
                    Content::user_text("one"),
                    Content::model_text("two"),
                    Content::user_text("three"),
                ])
                .build()
                .expect("request"),
        )
        .await;
    assert!(gemini_response.is_ok());

    let requests = mock.requests();
    let chat_body: Value = serde_json::from_slice(&requests[0].body).expect("body");
    assert_eq!(chat_body["messages"].as_array().expect("messages").len(), 3);
    let messages_body: Value = serde_json::from_slice(&requests[1].body).expect("body");
    assert_eq!(
        messages_body["messages"]
            .as_array()
            .expect("messages")
            .len(),
        2
    );
    let gemini_body: Value = serde_json::from_slice(&requests[2].body).expect("body");
    assert_eq!(
        gemini_body["contents"].as_array().expect("contents").len(),
        3
    );
}

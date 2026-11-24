//! OpenAI GAP §8 的协议契约测试：Responses 扁平工具、Embeddings base64、
//! custom tool call、流式字段、Output → Input 回传。

#![cfg(feature = "openai")]

use airx_core::http::{HeaderMap, HttpResponse, Method, StatusCode};
use airx_protocol::Endpoint;
use airx_protocol::openai::OpenAiConfig;
use airx_protocol::openai::chat::{
    ChatCompletionChunk, ChatCompletionResponse, ChatEndpoint, ChatStreamAccumulator,
    ChatStreamEvent,
};
use airx_protocol::openai::embeddings::{EmbeddingVector, EmbeddingsEndpoint};
use airx_protocol::openai::responses::{
    InputItem, ResponsesEndpoint, ResponsesRequest, ResponsesResponse, ResponsesTool,
    ResponsesToolChoice,
};
use serde_json::{Value, json};

fn config() -> OpenAiConfig {
    OpenAiConfig::new("sk-test")
}

#[test]
fn responses_flat_tools_and_choice_encode() {
    let request = ResponsesRequest::builder()
        .model("gpt-5")
        .input("hi")
        .tools([
            ResponsesTool::function("lookup", json!({"type": "object"}), Some("d".to_owned())),
            ResponsesTool::web_search(),
        ])
        .tool_choice(ResponsesToolChoice::required())
        .build()
        .expect("request");

    let http = ResponsesEndpoint::new(config())
        .encode(&request)
        .expect("encode");
    assert_eq!(http.method, Method::POST);
    assert_eq!(http.uri.path(), "/v1/responses");

    let body: Value = serde_json::from_slice(&http.body).expect("body");
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["name"], "lookup");
    assert_eq!(body["tools"][0]["parameters"]["type"], "object");
    assert_eq!(body["tools"][0]["description"], "d");
    assert!(body["tools"][0].get("function").is_none());
    assert_eq!(body["tools"][1], json!({"type": "web_search"}));
    assert_eq!(body["tool_choice"], "required");
}

#[test]
fn embedding_base64_decodes_through_the_endpoint() {
    // little-endian f32 [1.0, 1.0]
    let encoded = "AACAPwAAgD8=";
    let body = json!({
        "object": "list",
        "data": [{"object": "embedding", "embedding": encoded, "index": 0}],
        "model": "text-embedding-3-small"
    });

    let response = EmbeddingsEndpoint::new(config())
        .decode(HttpResponse::new(
            StatusCode::OK,
            HeaderMap::new(),
            body.to_string(),
        ))
        .expect("decode");

    let vector = &response.data[0].embedding;
    assert!(matches!(vector, EmbeddingVector::Base64(_)));
    let decoded = vector.to_f32().expect("decode");
    assert_eq!(decoded, vec![1.0, 1.0]);
}

#[test]
fn chat_custom_tool_call_response_decodes_and_roundtrips() {
    let json = json!({
        "id": "chatcmpl-1",
        "model": "m",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [
                    {"id": "call_1", "type": "function", "function": {"name": "lookup", "arguments": "{}"}},
                    {"id": "call_2", "type": "custom", "custom": {"name": "shell", "input": "ls"}}
                ]
            },
            "finish_reason": "tool_calls"
        }]
    });

    let response: ChatCompletionResponse = ChatEndpoint::new(config())
        .decode(HttpResponse::new(
            StatusCode::OK,
            HeaderMap::new(),
            json.to_string(),
        ))
        .expect("decode");

    let calls = response.choices[0]
        .message
        .tool_calls
        .as_ref()
        .expect("calls");
    assert_eq!(calls[0].name(), Some("lookup"));
    assert_eq!(calls[1].name(), Some("shell"));
    assert_eq!(calls[1].arguments(), Some("ls"));
    assert_eq!(calls[1].custom.as_ref().expect("custom").input, "ls");

    let value = serde_json::to_value(&response).expect("serialize");
    assert_eq!(
        value["choices"][0]["message"]["tool_calls"][1]["type"],
        "custom"
    );
    assert_eq!(
        value["choices"][0]["message"]["tool_calls"][1]["custom"]["input"],
        "ls"
    );
}

#[test]
fn streaming_chunk_fields_accumulate_into_typed_response() {
    let chunks = [
        json!({
            "id": "chatcmpl-1",
            "model": "m",
            "system_fingerprint": "fp_1",
            "service_tier": "flex",
            "choices": [{"index": 0, "delta": {"role": "assistant", "refusal": "no"}}]
        }),
        json!({
            "id": "chatcmpl-1",
            "model": "m",
            "choices": [{"index": 0, "delta": {"refusal": "pe"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        }),
    ];

    let mut accumulator = ChatStreamAccumulator::new();
    for chunk in chunks {
        let chunk: ChatCompletionChunk = serde_json::from_value(chunk).expect("chunk");
        accumulator
            .apply(ChatStreamEvent::Chunk(chunk))
            .expect("apply");
    }
    accumulator.apply(ChatStreamEvent::Done).expect("done");

    let response = accumulator.finish().expect("finish");
    assert_eq!(response.system_fingerprint.as_deref(), Some("fp_1"));
    assert_eq!(response.choices[0].message.refusal.as_deref(), Some("nope"));
    assert_eq!(
        response
            .extensions
            .get("service_tier")
            .and_then(Value::as_str),
        Some("flex")
    );
    assert_eq!(
        response.usage.as_ref().map(|usage| usage.total_tokens),
        Some(2)
    );
}

#[test]
fn responses_output_replays_as_input() {
    let response: ResponsesResponse = serde_json::from_value(json!({
        "id": "resp_1",
        "output": [
            {
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "ok", "annotations": []}]
            },
            {
                "type": "custom_tool_call",
                "call_id": "call_1",
                "name": "shell",
                "input": "ls"
            },
            {
                "type": "compaction",
                "id": "cmp_1",
                "encrypted_content": "cipher"
            }
        ]
    }))
    .expect("response");

    let items = response.into_input();
    assert_eq!(items.len(), 3);
    assert!(matches!(items[0], InputItem::Message { .. }));
    assert!(matches!(items[1], InputItem::Other(_)));
    assert!(matches!(items[2], InputItem::Other(_)));

    let value = serde_json::to_value(&items).expect("serialize");
    assert_eq!(value[0]["content"], "ok");
    assert_eq!(value[1]["type"], "custom_tool_call");
    assert_eq!(value[2]["type"], "compaction");
}

#[test]
fn compact_input_text_message_replays_without_loss() {
    let response: ResponsesResponse = serde_json::from_value(json!({
        "id": "resp_1",
        "output": [{
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": "Create a landing page"}]
        }]
    }))
    .expect("response");

    let items = response.into_input();
    assert_eq!(items.len(), 1);
    let value = serde_json::to_value(&items[0]).expect("serialize");
    assert_eq!(value["type"], "message");
    assert_eq!(value["content"][0]["type"], "input_text");
    assert_eq!(value["content"][0]["text"], "Create a landing page");
}

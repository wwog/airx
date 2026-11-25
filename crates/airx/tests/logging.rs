//! 日志系统竖切：按 scope 开启、call/net/retry/protocol/stream 日志点。
//!
//! 覆盖三类关注点：
//! 1. `{net: Info, call: Info}` 能收到请求与调用日志，其他 scope 保持静默；
//! 2. 关闭状态完全静默，行为与不传 logger 一致；
//! 3. 日志不泄露认证 Header、请求体与 query。

#![cfg(feature = "openai")]

use std::sync::{Arc, Mutex};

use airx::prelude::*;
use airx_core::error::{StreamErrorKind, TransportErrorKind};
use airx_transport::MockTransport;
use bytes::Bytes;

const SUCCESS: &str = r#"{
  "id": "chatcmpl-1",
  "object": "chat.completion",
  "created": 1729000000,
  "model": "gpt-4o-mini",
  "choices": [
    {
      "index": 0,
      "message": { "role": "assistant", "content": "ok" },
      "finish_reason": "stop"
    }
  ],
  "usage": { "prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7 }
}"#;

/// 收集日志文本；同时保留级别与 scope 便于断言。
#[derive(Default)]
struct Collector {
    records: Mutex<Vec<(LogLevel, String, String)>>,
}

impl Collector {
    fn lines(&self) -> Vec<String> {
        self.records
            .lock()
            .expect("lock")
            .iter()
            .map(|(_, scope, body)| format!("[{scope}] {body}"))
            .collect()
    }

    fn levels(&self) -> Vec<LogLevel> {
        self.records
            .lock()
            .expect("lock")
            .iter()
            .map(|(level, _, _)| *level)
            .collect()
    }
}

impl Logger for Collector {
    fn log(&self, record: &Record<'_>) {
        self.records.lock().expect("lock").push((
            record.level,
            record.scope.as_str().to_owned(),
            format!("{} {}", record.message, Fields(record.fields)),
        ));
    }
}

/// 与 `LogHandle` 内部一致的字段渲染，用于断言完整文本。
struct Fields<'a>(&'a [airx::log::Field<'a>]);

impl std::fmt::Display for Fields<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, field) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{}", field)?;
        }
        Ok(())
    }
}

fn request() -> ChatCompletionRequest {
    ChatCompletionRequest::builder()
        .model("gpt-4o-mini")
        .user("hello")
        .build()
        .expect("request")
}

fn client_with(mock: &MockTransport, config: LogConfig) -> (OpenAi, Arc<Collector>) {
    let collector = Arc::new(Collector::default());
    let client = Client::builder()
        .transport(mock.clone())
        .logger(collector.clone())
        .log_config(config)
        .openai()
        .api_key("sk-secret-value")
        .base_url("https://api.example.com/v1")
        .build()
        .expect("client");
    (client, collector)
}

#[tokio::test]
async fn net_and_call_scopes_are_enabled_independently() {
    let mock = MockTransport::new();
    mock.push_json(200, SUCCESS);
    let config = LogConfig::new()
        .scope(Scope::NET, LogLevel::Info)
        .scope(Scope::CALL, LogLevel::Info);
    let (client, collector) = client_with(&mock, config);

    client.chat().create(request()).await.expect("response");

    let lines = collector.lines();
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("[net] sending request")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("[net] response received")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("[call] call completed")),
        "{lines:?}"
    );
    // 未配置的 scope 不输出。
    assert!(
        !lines.iter().any(|line| line.contains("[stream]")),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("[protocol]")),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("[retry]")),
        "{lines:?}"
    );
}

#[tokio::test]
async fn only_configured_scope_logs() {
    let mock = MockTransport::new();
    mock.push_json(200, SUCCESS);
    let (client, collector) =
        client_with(&mock, LogConfig::new().scope(Scope::CALL, LogLevel::Info));

    client.chat().create(request()).await.expect("response");

    let lines = collector.lines();
    assert!(!lines.is_empty());
    assert!(
        lines.iter().all(|line| line.starts_with("[call]")),
        "{lines:?}"
    );
}

#[tokio::test]
async fn without_log_config_nothing_is_emitted() {
    let mock = MockTransport::new();
    mock.push_json(200, SUCCESS);
    let collector = Arc::new(Collector::default());
    let client = Client::builder()
        .transport(mock.clone())
        .logger(collector.clone())
        .openai()
        .api_key("sk-test")
        .build()
        .expect("client");

    client.chat().create(request()).await.expect("response");

    assert!(collector.lines().is_empty());
}

#[tokio::test]
async fn non_success_response_logs_warn_and_failure_logs_error() {
    let mock = MockTransport::new();
    mock.push_json(400, r#"{"error":{"message":"bad request"}}"#);
    let config = LogConfig::new()
        .scope(Scope::NET, LogLevel::Info)
        .scope(Scope::CALL, LogLevel::Info);
    let (client, collector) = client_with(&mock, config);

    assert!(client.chat().create(request()).await.is_err());

    let lines = collector.lines();
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("[net] non-success response")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("[call] call failed")),
        "{lines:?}"
    );
    assert!(collector.levels().contains(&LogLevel::Warn));
    assert!(collector.levels().contains(&LogLevel::Error));
}

#[tokio::test]
async fn transport_failure_is_logged_as_error() {
    let mock = MockTransport::new();
    mock.push_error(TransportErrorKind::Connect, "connection refused");
    let config = LogConfig::new()
        .scope(Scope::NET, LogLevel::Info)
        .scope(Scope::CALL, LogLevel::Error);
    let (client, collector) = client_with(&mock, config);

    assert!(client.chat().create(request()).await.is_err());

    let lines = collector.lines();
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("[net] request failed")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("[call] call failed")),
        "{lines:?}"
    );
}

#[tokio::test]
async fn logs_never_contain_credentials_body_or_query() {
    let mock = MockTransport::new();
    mock.push_json(200, SUCCESS);
    let config = LogConfig::all(LogLevel::Trace);
    let collector = Arc::new(Collector::default());
    let client = Client::builder()
        .transport(mock.clone())
        .logger(collector.clone())
        .log_config(config)
        .openai()
        .api_key("sk-secret-value")
        .build()
        .expect("client");

    // 绝对路径带 query：日志只保留 path。
    let chat = client
        .chat_at("https://api.example.com/v1/chat/completions?api_key=query-secret")
        .expect("chat");
    chat.create(request()).await.expect("response");

    let joined = collector.lines().join("\n");
    assert!(!joined.contains("sk-secret-value"), "{joined}");
    assert!(!joined.contains("query-secret"), "{joined}");
    assert!(!joined.contains("api_key="), "{joined}");
    assert!(!joined.contains("hello"), "请求体不应进入日志: {joined}");
    // 只记录 path，不记录完整 URI。
    assert!(joined.contains("/v1/chat/completions"), "{joined}");
}

#[cfg(feature = "stream")]
#[tokio::test]
async fn stream_scope_logs_open_and_end() {
    let mock = MockTransport::new();
    let sse = concat!(
        r#"data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"gpt-4o-mini","choices":[{"index":0,"delta":{"content":"hi"}}]}"#,
        "\n\n",
        "data: [DONE]\n\n",
    );
    mock.push_stream(
        200,
        airx_core::http::HeaderMap::new(),
        vec![Bytes::from_static(sse.as_bytes())],
    );

    let config = LogConfig::new().scope(Scope::STREAM, LogLevel::Info);
    let (client, collector) = client_with(&mock, config);

    let stream = client.chat().stream(request()).await.expect("stream");
    let text = stream.final_text().await.expect("text");
    assert_eq!(text, "hi");

    let lines = collector.lines();
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("[stream] stream opened")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("[stream] stream ended")),
        "{lines:?}"
    );
    // stream scope 之外不输出。
    assert!(
        !lines.iter().any(|line| line.contains("[net]")),
        "{lines:?}"
    );
}

#[cfg(feature = "stream")]
#[tokio::test]
async fn truncated_stream_logs_failure() {
    let mock = MockTransport::new();
    // 没有空行结尾 → 事件未闭合，finish() 返回 Incomplete。
    let sse = r#"data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"gpt-4o-mini","choices":[{"index":0,"delta":{"content":"hi"}}]}"#;
    mock.push_stream(
        200,
        airx_core::http::HeaderMap::new(),
        vec![Bytes::from_static(sse.as_bytes())],
    );

    let config = LogConfig::new()
        .scope(Scope::STREAM, LogLevel::Info)
        .scope(Scope::CALL, LogLevel::Info);
    let (client, collector) = client_with(&mock, config);

    let mut stream = client.chat().stream(request()).await.expect("stream");
    let error = loop {
        match stream.next().await {
            Ok(Some(_)) => continue,
            Ok(None) => panic!("截断流不应正常结束"),
            Err(error) => break error,
        }
    };
    match &error {
        AirxError::Stream(err) => assert_eq!(err.kind, StreamErrorKind::Incomplete),
        other => panic!("期望 Stream(Incomplete)，实际 {other:?}"),
    }

    let lines = collector.lines();
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("[stream] stream failed")),
        "{lines:?}"
    );
}

#[tokio::test]
async fn protocol_decode_failure_is_logged() {
    let mock = MockTransport::new();
    // 2xx 但 body 不是合法的 chat completion。
    mock.push_json(200, r#"{"unexpected":"shape"}"#);
    let config = LogConfig::new().scope(Scope::PROTOCOL, LogLevel::Error);
    let (client, collector) = client_with(&mock, config);

    assert!(client.chat().create(request()).await.is_err());

    let lines = collector.lines();
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("[protocol] protocol encode/decode failed")),
        "{lines:?}"
    );
    assert!(lines[0].contains("phase=decode"), "{lines:?}");
}

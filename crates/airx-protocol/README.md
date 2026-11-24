# airx-protocol

`airx` 的协议层：OpenAI Chat Completions / Responses、Anthropic Messages、
Gemini `generateContent` 的强类型请求/响应模型与纯编解码。不依赖传输层，
可以直接用来构造请求、序列化 body、解析响应与 SSE 事件。

[文档](https://docs.rs/airx-protocol) · [仓库](https://github.com/wwog/airx) · [协议支持](https://github.com/wwog/airx/blob/main/docs/protocol-support.md)

## 安装

```toml
[dependencies]
airx-protocol = { version = "0.1", features = ["openai"] }
```

Feature：`openai` / `anthropic` / `gemini` / `stream`；默认不启用任何协议。

## 使用

```rust
use airx_protocol::openai::chat::ChatCompletionRequest;
use airx_protocol::Endpoint;

let request = ChatCompletionRequest::builder()
    .model("gpt-4o-mini")
    .user("Hello")
    .build()?;

let http = airx_protocol::openai::chat::ChatEndpoint::new(
    airx_protocol::openai::OpenAiConfig::new("sk-..."),
)
.encode(&request)?;

assert_eq!(http.method.as_str(), "POST");
assert_eq!(http.uri.path(), "/v1/chat/completions");
# Ok::<(), airx_core::AirxError>(())
```

## 设计

- **纯编解码**：`Endpoint::encode` / `Endpoint::decode`，不发网络请求。
- **未知字段保留**：统一 `extensions: Extensions`，重新序列化不丢字段。
- **协议各自独立**：Chat 与 Responses、不同 Provider 不共享请求模型，避免形状混淆。
- **流式**：每个协议自己的事件枚举与 `*StreamAccumulator`，完成信号严格判定。

## License

MIT OR Apache-2.0

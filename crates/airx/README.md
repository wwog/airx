# airx

多协议 LLM 请求工具包：OpenAI Chat Completions / Responses、Anthropic Messages、
Gemini `generateContent` 的强类型请求、流式解码与统一客户端。**不是 agent 框架**——
只做「一次请求 = 一个响应」的协议整合，工具调用、循环、会话状态由业务自己管理。

[文档](https://docs.rs/airx) · [仓库](https://github.com/wwog/airx) · [协议支持](https://github.com/wwog/airx/blob/main/docs/protocol-support.md)

## 安装

```toml
[dependencies]
airx = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

默认启用 `native + rustls-tls + compression + all-providers + stream`。
裁剪示例：

```toml
airx = { version = "0.1", default-features = false, features = ["native", "rustls-tls", "openai", "stream"] }
```

## 快速开始

```rust
use airx::prelude::*;

#[tokio::main]
async fn main() -> Result<(), AirxError> {
    let client = Client::openai()
        .api_key(std::env::var("OPENAI_API_KEY").unwrap())
        .build()?;

    let response = client
        .chat()
        .create(
            ChatCompletionRequest::builder()
                .model("gpt-4o-mini")
                .user("用一句话解释 Rust 所有权。")
                .build()?,
        )
        .await?;

    println!("{}", response.first_content().unwrap_or_default());
    Ok(())
}
```

## 主要能力

- **三协议统一入口**：`Client::openai()` / `Client::anthropic()` / `Client::gemini()`，也可用
  `client.chat()` / `messages()` / `generate_content(model)` 直接取端点。
- **流式**：`ProtocolStream` 实现 `futures_core::Stream`；`final_text()` / `final_response()` /
  `final_parsed::<T>()` 一次性收尾。
- **结构化输出**：由请求参数（`response_format` / `text.format` / `output_config.format` /
  `responseJsonSchema`）控制；非流式用 `parse_as::<T>()`，流式用 `final_parsed::<T>()`。
- **完整历史输入**：`.messages(...)` / `.input_items(...)` / `.contents(...)` 直接接列表，适合业务循环。
- **Files / Batches / Embeddings / Models / Caches**：类型化端点，未知字段通过 `extensions` 透传。
- **逃生通道**：`OpenAi/Anthropic/Gemini::execute(HttpRequest)` 与原生 `Endpoint`。
- **按 scope 分级日志**：业务层注入 `Logger`，用 `{net: Info, call: Info}` 精确打开，
  默认零输出、零全局状态。

## 日志

```rust
use std::sync::Arc;
use airx::prelude::*;

struct MyLogger;

impl Logger for MyLogger {
    fn log(&self, record: &Record<'_>) {
        // 同步、快速、不阻塞；需要异步落盘时投递到 channel 后立即返回。
        eprintln!("[{}][{}] {}", record.scope, record.level, record.message);
    }
}

let client = Client::builder()
    .logger(Arc::new(MyLogger))
    .log_config(
        LogConfig::new()
            .scope(Scope::NET, LogLevel::Info)
            .scope(Scope::CALL, LogLevel::Info),
    )
    .openai()
    .api_key("sk-...")
    .build()?;
# Ok::<(), AirxError>(())
```

- 级别：`Error` / `Warn` / `Info` / `Debug` / `Trace`；配置某级别输出该级别及更严重的级别。
- scope：`net` / `call` / `stream` / `retry` / `protocol`，也可 `Scope::new("custom")`。
- 默认完全静默：只传 `logger` 不传 `log_config` 不会产生输出。
- `Info` 及以下不记录 Header 与 body；`net=Debug` 输出 headers（自动脱敏）与截断 body
  （`LogConfig::body_limit`，默认 4096 字节），`net=Trace` 额外补一条完整 body；SSE 逐帧用
  `stream=Trace`。
- Header 默认自动脱敏；`LogConfig::header_redactor(|headers| ...)` 完全替换自动脱敏，
  回调拿到全量 `HeaderMap`。body 不做自动脱敏。
- URI 只记录 path（不含 query）。
- `tracing` feature 提供 `TracingLogger` 适配器，scope 映射为 `airx::net` / `airx::call` 等 target。

## Feature

| Feature | 作用 | 默认 |
| --- | --- | --- |
| `native` / `wasm` | Reqwest 或浏览器 Fetch 传输 | `native` |
| `rustls-tls` / `native-tls` | TLS 后端（二选一） | `rustls-tls` |
| `compression` | gzip / brotli / deflate | ✅ |
| `openai` / `anthropic` / `gemini` | 协议模块 | ✅（`all-providers`） |
| `stream` | 流式 API | ✅ |
| `tracing` | `TracingLogger` 适配器 | — |

## 子 crate

- [`airx-core`](https://crates.io/crates/airx-core)：错误、JSON 保真、SSE、HTTP 数据结构
- [`airx-protocol`](https://crates.io/crates/airx-protocol)：各协议类型与纯编解码
- [`airx-transport`](https://crates.io/crates/airx-transport)：HTTP 执行、重试、流式读取

## License

MIT OR Apache-2.0

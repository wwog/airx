# airx

多协议 AI API 基础设施库（Rust）。One Rust SDK. Multiple AI protocols. No feature loss.

- **Native API**：OpenAI Chat Completions / Responses、Anthropic Messages、
  Gemini `generateContent` / `streamGenerateContent`，
  完整保留各协议自己的请求、响应、事件与错误形状。
- **可替换传输**：Reqwest（native）、Fetch（wasm）、Mock，协议层零 HTTP 依赖。
- **流式**：SSE 解析一次，事件各自建模，独立 Accumulator，截断流不会被当作成功。

## 结构

| Crate | 职责 |
| --- | --- |
| `airx-core` | 错误、模型标识、JSON 保真、SSE、增量 JSON、HTTP 数据结构 |
| `airx-protocol` | OpenAI / Anthropic / Gemini 强类型模型与纯编解码（DeepSeek 走兼容端点） |
| `airx-transport` | HTTP、流式读取、超时取消、幂等感知重试、同步中间件、Mock |
| `airx` | Client 构建入口 / Provider 客户端 / typed endpoint / prelude |

依赖严格单向：`airx → {core, protocol, transport}`，`protocol/transport → core`。
当前实现架构见 [docs/architecture.md](docs/architecture.md)，协议覆盖见
[docs/protocol-support.md](docs/protocol-support.md)，兼容规则见 [docs/compatibility.md](docs/compatibility.md)。

协议接入指南：

| 协议 | 快速接入 / 详细使用 / 扩展 |
| --- | --- |
| OpenAI Chat + Responses | [docs/openai.md](docs/openai.md) |
| Anthropic Messages | [docs/anthropic.md](docs/anthropic.md) |
| Gemini generateContent | [docs/gemini.md](docs/gemini.md) |

## 快速开始

```toml
[dependencies]
airx = "0.1"
```

```rust,ignore
use airx::prelude::*;

// build() 直接得到已定性的 OpenAI 客户端，可复用
let client = Client::openai().api_key("sk-...").build()?;

// Native API（Chat）
let response = client
    .chat()
    .create(
        ChatCompletionRequest::builder()
            .model("gpt-4o-mini")
            .system("be concise")
            .user("Hello")
            .build()?,
    )
    .await?;

// 流式（累积为完整响应）
let mut stream = client
    .chat()
    .stream(
        ChatCompletionRequest::builder().model("gpt-4o-mini").user("Hello").build()?,
    )
    .await?;
let mut accumulator = ChatStreamAccumulator::new();
while let Some(event) = stream.next().await? {
    accumulator.apply(event)?;
}
let streamed = accumulator.finish()?;
# Ok::<(), AirxError>(())
```

```rust,ignore
// Gemini：模型是路径的一部分，进入 API 时指定
let gemini = Client::gemini().api_key("...").build()?;
let response = gemini
    .generate_content("gemini-2.5-flash")
    .create(
        GenerateContentRequest::builder()
            .system("be concise")
            .user("Hello")
            .max_output_tokens(256)
            .build()?,
    )
    .await?;
println!("{}", response.text());
# Ok::<(), AirxError>(())
```

## SDK 体验

单次请求选项与响应元数据：

```rust,ignore
let chat = client.chat().with_options(
    RequestOptions::new()
        .try_header("x-trace-id", "trace-1")?
        .idempotency_key("idem-1"),
);

let response = chat.create_with_meta(request).await?;
println!("status={} request_id={:?}",
    response.meta.status,
    response.meta.request_id);
let data = response.data;
```

流式：`ProtocolStream` 实现 `futures_core::Stream`，并提供一次性收尾：

```rust,ignore
use futures_util::TryStreamExt;

let mut stream = client.chat().stream(request).await?;
while let Some(event) = stream.try_next().await? {
    // 增量消费
}

// 或直接拿到聚合结果
let text = client.chat().stream(request).await?.final_text().await?;
```

结构化输出、模型列表与计 token：

```rust,ignore
let openai = Client::openai().api_key("...").build()?;
let anthropic = Client::anthropic().api_key("...").build()?;

let parsed: MyType = openai.chat().parse_as(request).await?;

let models = openai.models().list().await?;

let page = anthropic
    .models()
    .list(AnthropicListModelsRequest::builder().limit(20).build())
    .await?;

let tokens = anthropic
    .messages()
    .count_tokens(CountTokensRequest::builder()
        .model("claude-sonnet-4-5")
        .user("hi")
        .build()?)
    .await?;
```

结构化输出由请求参数控制（`response_format` / `text.format` / `output_config.format` /
`responseSchema`），不是 feature。`parse_as` 只用于非流式；流式时先用 `final_text()`
收齐文本，再自行 `serde_json::from_str`。

默认策略对齐官方 SDK：native 2 次重试、10 秒连接超时、gzip / brotli / deflate 解压、
`User-Agent: airx/<version>`；整体请求超时默认关闭，避免截断长流。
`RequestOptions` 可临时覆盖 Header、幂等键、超时与重试策略；Provider 客户端提供
`execute(HttpRequest)` 作为自定义接口的逃生通道。

请求输入直接接受完整历史列表，适合循环：

```rust,ignore
let request = ChatCompletionRequest::builder()
    .model("gpt-4o-mini")
    .messages(history)          // Vec<ChatMessage>，而不是逐条 .user()
    .build()?;
```

文件 / 批处理 / Embeddings：

```rust,ignore
let file = openai.files().upload("input.jsonl", "batch", bytes).await?;
let batch = openai.batches().create(
    CreateBatchRequest::builder()
        .input_file_id(&file.id)
        .endpoint("/v1/chat/completions")
        .completion_window("24h")
        .build()?,
).await?;

let embedding = openai.embeddings().create(
    EmbeddingRequest::builder().model("text-embedding-3-small").input("hello").build()?,
).await?;
```

Anthropic 提供 Files / Batches；Gemini 提供 Embeddings（文件/批处理使用原生 Endpoint）。

## Feature

默认启用 `native + rustls-tls + compression + all-providers + stream`，即三协议开箱即用。

| Feature | 功能 | 默认 |
| --- | --- | --- |
| `native` | Reqwest 原生 HTTP 传输（连接池、超时、TLS） | ✅ |
| `rustls-tls` | Reqwest 使用 rustls TLS 后端 | ✅ |
| `native-tls` | Reqwest 使用系统 native-tls；与 `rustls-tls` 二选一 | — |
| `compression` | gzip / brotli / deflate 响应解压 | ✅ |
| `wasm` | 浏览器 Fetch 传输（含 `ReadableStream` 流式；用于 wasm32） | — |
| `all-providers` | 启用全部协议：`openai` + `anthropic` + `gemini` | ✅ |
| `openai` | OpenAI Chat Completions + Responses | ✅（经 `all-providers`） |
| `anthropic` | Anthropic Messages | ✅（经 `all-providers`） |
| `gemini` | Gemini `generateContent` + `streamGenerateContent` | ✅（经 `all-providers`） |
| `stream` | 流式 API：`ProtocolStream`、各协议事件、Accumulator | ✅ |

按需裁剪（只保留 OpenAI + 流式）：

```toml
[dependencies]
airx = { version = "0.1", default-features = false, features = ["native", "rustls-tls", "openai", "stream"] }
```

WASM：

```toml
[dependencies]
airx = { version = "0.1", default-features = false, features = ["wasm", "all-providers", "stream"] }
```

TLS 后端只选一个；`native` 与 `wasm` 按目标平台选择。

## 示例

官方 OpenAI 示例：

```bash
OPENAI_API_KEY=sk-... cargo run -p airx --example chat
OPENAI_API_KEY=sk-... cargo run -p airx --example chat_stream
```

Gemini 示例（原生 `generateContent`）：

```bash
export GEMINI_API_KEY=...     # 或写入 .env（见 .env.example）
cargo run -p airx --features gemini --example gemini
cargo run -p airx --features gemini,stream --example gemini_stream
```

OpenAI-Compatible 网关示例（OpenCode Zen，两个模型 `hy4-flash-free`）：

```bash
cp .env.example .env      # 填入 OPENCODE_API_KEY；.env 已被 git 忽略
cargo run -p airx --example opencode_text     # 非流式：输出文字海报 SVG
cargo run -p airx --example opencode_stream   # 流式：SSE → 事件 → 聚合器
cargo run -p airx --example opencode_retry    # 重试：注入 429 + Retry-After
cargo run -p airx --example opencode_full     # 结构化输出 + 工具调用
```

DeepSeek（OpenAI / Anthropic 兼容部署，JSON mode）：

```bash
DEEPSEEK_API_KEY=sk-... cargo run -p airx --example deepseek_json
```

生成物写到 `target/opencode-*.svg` 与 `target/opencode-*.txt`。

环境变量：

| 变量 | 说明 |
| --- | --- |
| `OPENCODE_BASE_URL` | 默认 `https://opencode.ai/zen/go/v1` |
| `OPENCODE_API_KEY` | 必填 |
| `OPENCODE_MODELS` | 逗号分隔，默认 `hy4-flash-free,deepseek-v4.1-flash` |

网关要求 `x-opencode-session` Header，示例通过执行级 `default_headers` 注入。

## 已知边界

- `JsonMode::Canonical`（RFC 8785 JCS）尚未实现，会返回明确错误，而不是给出不可靠的规范化结果。
- 中间件是同步的；需要异步（如 Token 刷新）时请实现自定义 `Transport`。
- 流式请求不重试（已消费事件无法重放）；首次事件前重试留待后续。
- WASM 传输已通过 `wasm32-unknown-unknown` 编译检查，浏览器运行时行为尚未纳入 CI。
- Provider 参数随模型演进：Anthropic 已移除 `temperature` / `top_p` / `top_k`，
  OpenAI `max_tokens` 已弃用。DeepSeek 是 OpenAI / Anthropic 兼容部署，不设独立协议，
  结构化输出走 JSON mode（`response_format=json_object`）。对齐基准与来源见
  [docs/protocol-support.md](docs/protocol-support.md) 与 [docs/compatibility.md](docs/compatibility.md)。
- Gemini 覆盖 `generateContent` / `streamGenerateContent` / `countTokens` / 模型列表 /
  Embeddings；Vertex AI、Live API、Files、Batches 尚未封装，可用 Raw 扩展字段、
  `execute(HttpRequest)` 或原生 `Endpoint` 逃生通道。
  Gemini 流没有 `[DONE]` 哨兵，聚合器以 `finishReason`（或 `promptFeedback.blockReason`）
  判定完成，缺失时返回 `Incomplete`。
- 已封装：OpenAI Files / Batches / Embeddings、Anthropic Files / Batches、Gemini Embeddings。
  图片/音频、Gemini Files / Batches、部分 MCP/服务端工具 item 尚未类型化，可用
  `extensions`、原生 `Endpoint` 或 `execute(HttpRequest)` 扩展。
- 服务端联网工具（OpenAI `web_search`、Anthropic `web_search` / `web_fetch`、DeepSeek
  `deepseek_web_search`）由 Provider 后台执行；airx 只声明工具并解析引用/调用记录，
  不代发本地网络请求。

## 开发

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
./scripts/check-deps.sh
cargo check -p airx --target wasm32-unknown-unknown \
  --no-default-features --features wasm,all-providers,stream
```

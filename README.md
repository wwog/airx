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
| `airx` | Client / Builder / typed endpoint / prelude |

依赖严格单向：`airx → {core, protocol, transport}`，`protocol/transport → core`。
完整设计与决策见 [docs/architecture.md](docs/architecture.md)，协议覆盖见
[docs/protocol-support.md](docs/protocol-support.md)，兼容规则见 [docs/compatibility.md](docs/compatibility.md)。

## 快速开始

```toml
[dependencies]
airx = "0.1"
```

```rust,ignore
use airx::prelude::*;

let client = Client::builder().openai().api_key("sk-...").build()?;

// Native API（Chat）
let response = client
    .openai()?
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
    .openai()?
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
let client = Client::builder().gemini().api_key("...").build()?;
let response = client
    .gemini()?
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

## Feature

| Feature | 内容 |
| --- | --- |
| `native` | Reqwest 传输 |
| `rustls-tls` / `native-tls` | TLS 后端（默认 rustls） |
| `wasm` | 浏览器 Fetch 传输（含 ReadableStream 流式） |
| `openai` | OpenAI Chat + Responses |
| `anthropic` | Anthropic Messages |
| `gemini` | Gemini `generateContent` + `streamGenerateContent` |
| `stream` | 流式 API（`ProtocolStream`、事件、Accumulator） |
| `all-providers` | 全部 Provider |

默认：`native + rustls-tls + openai + stream`。

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
- Gemini 当前覆盖 `generateContent` / `streamGenerateContent`；`countTokens`、
  模型列表与 Vertex AI 专用参数尚未封装，可用 Raw 扩展字段或原生 `Endpoint` 逃生通道。
  Gemini 流没有 `[DONE]` 哨兵，聚合器以 `finishReason`（或 `promptFeedback.blockReason`）
  判定完成，缺失时返回 `Incomplete`。
- 服务端联网工具（OpenAI `web_search`、Anthropic `web_search` / `web_fetch`、DeepSeek
  `deepseek_web_search`）由 Provider 后台执行；airx 只声明工具并解析引用/调用记录，
  不代发本地网络请求。

## 开发

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --features airx/anthropic,airx/gemini -- -D warnings
cargo test --workspace --features airx/anthropic,airx/gemini
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
./scripts/check-deps.sh
cargo check -p airx --target wasm32-unknown-unknown \
  --no-default-features --features wasm,openai,stream,gemini
```

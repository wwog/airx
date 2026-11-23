# airx 架构（当前实现）

> One Rust SDK. Multiple AI protocols. No feature loss.

本文根据仓库**当前代码**整理，描述已经落地的分层、契约与运行时行为，不是路线图或设计提案。
与代码冲突时以代码为准。协议能力清单见 [protocol-support.md](protocol-support.md)，
跨 Provider 的显式行为约定见 [compatibility.md](compatibility.md)。

---

## 1. 定位与实现约束

airx 是**多协议原生 API 客户端库**。它不提供跨 Provider 的统一请求 / 响应模型，
而是为每个原生 API 保留独立的强类型请求、响应、事件与错误形状。

当前实现覆盖：

| Provider | API |
| --- | --- |
| OpenAI | Chat Completions、Responses |
| Anthropic | Messages |
| Gemini | `generateContent`、`streamGenerateContent` |

DeepSeek 不是独立协议模块，而是通过“同一协议客户端 + `base_url`”接入的兼容部署
（OpenAI-Compatible / Anthropic-Compatible）。

实现层面的三条硬约束：

1. **Native First**：每个协议拥有完整、独立的数据模型，不经过统一中转。
2. **No Silent Loss**：未建模字段进入 `extensions`，未知事件 / 内容块保留为
   `Unknown(Value)` / `Other(Value)`，不静默丢弃。
3. **Effects at the Edge**：编码、解码、SSE 语法、事件聚合都是纯函数；
   网络 IO 只存在于 `airx-transport`。

crate 级约束：

| crate | 允许做的事 | 明确不做 |
| --- | --- | --- |
| `airx-core` | 错误形状、标识类型、HTTP 数据形状、JSON 保真、SSE 语法 | 任何 IO、任何 Provider 语义 |
| `airx-protocol` | 强类型模型、URL / Header 构造、编解码、错误体解析、流事件与聚合 | 网络请求、连接池、重试 |
| `airx-transport` | HTTP 执行、流式读取、超时、重试、中间件、Mock | 认证 Header、Provider 错误解析 |
| `airx` | 构建入口、Provider 客户端、typed endpoint、执行器、prelude | 协议字段定义、底层 IO 实现 |

---

## 2. Workspace 结构与依赖方向

### 2.1 Crate 与依赖图

```text
airx (facade)
  ├── airx-protocol ──► airx-core
  ├── airx-transport ─► airx-core
  └────────────────────► airx-core
```

- `airx-core` 不依赖任何 workspace crate。
- `airx-protocol` 和 `airx-transport` 互不依赖，只共享 `airx-core` 的数据契约。
- `airx` 依赖三者并完成组合。

### 2.2 依赖规则

`scripts/check-deps.sh` 用 `cargo metadata` 校验普通依赖（不校验 dev-dependencies），
允许的直接依赖为：

```text
airx-core      → {}
airx-protocol  → {airx-core}
airx-transport → {airx-core}
airx           → {airx-core, airx-protocol, airx-transport}
```

CI 中该脚本与 `cargo fmt`、`clippy`、`test`、`doc`、feature 组合检查一起运行。

### 2.3 平台依赖

- Native：`airx-transport/native` 引入 `reqwest`，超时/退避使用 `tokio::time`。
- WASM：`airx-transport/wasm` 引入 `gloo-net`、`wasm-bindgen`、`wasm-streams`、`web-sys`。
- TLS 后端由 feature 选择：`rustls-tls` 或 `native-tls`。

---

## 3. 运行时链路

### 3.1 构建

```text
Client::openai()                 // 命名空间 + 执行配置起点
  .api_key("...")                // Provider 配置
  .base_url(...) / .chat_path(...)
  .timeout(...) / .retry_policy(...) / .transport(...)
  .build()                       // 校验配置、创建 Executor、返回 OpenAi
```

`Client` 只是入口命名空间；`ClientBuilder` 保存执行级配置（Transport、超时、重试、默认 Header），
各 Provider 构建器保存协议级配置（API Key、base URL、模式路径、API 版本等）。
`build()` 把执行配置固化为共享的 `Arc<Executor>`，并返回**已定性的 Provider 客户端**：

| 类型 | 职责 |
| --- | --- |
| `OpenAi` | 持有 `Arc<Executor>` + `OpenAiConfig`，入口为 `chat()` / `responses()` / `models()` |
| `Anthropic` | 持有 `Arc<Executor>` + `AnthropicConfig`，入口为 `messages()` / `models()` |
| `Gemini` | 持有 `Arc<Executor>` + `GeminiConfig`，入口为 `generate_content(model)` / `models()` |
| `ApiClient<P>` | 绑定一个端点与执行器，顺序执行 encode → execute → decode |
| `Executor` | 应用执行级默认 Header，调用可替换的 `Transport` |

Provider 客户端可克隆、可复用；每次调用只克隆协议配置并构造端点，不重建 Transport。

### 3.2 非流式请求

```text
Chat::create(&request)
  └─ ApiClient::create
       1. endpoint.encode(request)  → HttpRequest（URL、认证 Header、JSON body）
       2. Executor.execute(http)    → Transport::execute
       3. endpoint.decode(response) → 2xx 反序列化；非 2xx 解析错误体
```

- 编码失败 → `AirxError::Serialization` / `AirxError::Configuration`。
- 传输失败 → `AirxError::Transport`。
- 非 2xx 且可识别 Provider 错误体 → `AirxError::Provider`；否则 `AirxError::Http`。
- 2xx 解析失败 → `AirxError::Deserialization`。

### 3.3 流式请求

```text
Chat::stream(&request)              // 门面自动设置 stream = true
  └─ ApiClient::stream
       1. endpoint.encode(request)
       2. Executor.execute_stream   → HttpStreamResponse（状态码 + Header + ByteStream）
       3. 2xx  → ProtocolStream::new(endpoint, body)
          非 2xx → 读完 body，构造 HttpResponse 交给 endpoint.decode，返回 Provider/Http 错误
```

`ProtocolStream::next()` 的循环：

```text
ProtocolStream
  └─ SseDecoder::next_event   (字节 → RawEvent)
       └─ endpoint.decode_event (RawEvent → Option<Event>)
            ├─ Some(event) → 返回给调用方
            └─ None        → 协议层可忽略事件（ping 等），继续读
```

流结束时 `SseDecoder::finish()` 冲刷未分发内容；事件没有以空行闭合时返回
`StreamErrorKind::Incomplete`，不会被当作正常完成。

### 3.4 逃生通道

- `Chat::endpoint()` / `Responses::endpoint()` / `Messages::endpoint()` /
  `GenerateContent::endpoint()` 暴露端点，可自行 `encode`。
- `ApiClient::execute(HttpRequest)` 直接发送预编码请求。
- 未封装字段通过请求模型的 `extensions` 透传。

---

## 4. 核心契约

### 4.1 Endpoint / StreamingEndpoint

`airx-protocol::endpoint` 只约定编解码，不包含 IO：

```rust
pub trait Endpoint {
    type Request;
    type Response;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError>;
    fn decode(&self, response: HttpResponse) -> Result<Self::Response, AirxError>;
}

pub trait StreamingEndpoint: Endpoint {
    type Event;

    fn decode_event(&self, event: RawEvent) -> Result<Option<Self::Event>, AirxError>;
}
```

- 非流式与流式分成两个 trait，只有支持流式的 API 才实现 `StreamingEndpoint`。
- `decode_event` 返回 `Ok(None)` 表示该事件在协议层可忽略（例如 Anthropic 的 `ping`）。
- 端点是轻量值类型，持有 Provider 配置（Gemini 还持有模型与调用模式）。

### 4.2 Transport

`airx-transport::Transport` 是对象安全的可替换传输：

```rust
pub trait Transport {
    fn execute(&self, request: HttpRequest)
        -> BoxFuture<'_, Result<HttpResponse, TransportError>>;

    fn execute_stream(&self, request: HttpRequest)
        -> BoxFuture<'_, Result<HttpStreamResponse, TransportError>>;
}
```

- `BoxFuture` 与 `ByteStream` 在 Native 下要求 `Send`，WASM 下不要求，
  以适配浏览器 Future / Stream 的约束。
- `HttpStreamResponse` 即使非 2xx 也保留状态码、Header 与 body 流，
  错误体由协议层解码，传输层不丢弃内容。
- `&T` 与 `Arc<T>` 自动实现 `Transport`，便于共享与包装。

实现：

| 类型 | 用途 |
| --- | --- |
| `ReqwestTransport` | Native HTTP，支持整体超时，可将后端错误映射为 `TransportErrorKind` |
| `WasmTransport` | 浏览器 Fetch；流式基于 `ReadableStream` |
| `MockTransport` | 预置响应 / 流 / 错误，记录请求，克隆共享状态 |
| `RetryTransport<T>` | 应用 `RetryPolicy`（仅非 WASM） |
| `MiddlewareTransport<T>` | 同步中间件链 |

native 默认启用 2 次重试（`RetryPolicy::standard()`），WASM 默认关闭；
可用 `RetryPolicy::disabled()` 显式关闭。决策：

| 情况 | 是否重试 |
| --- | --- |
| 连接失败（请求未发出） | 任意方法 |
| 超时 / 5xx | 仅幂等方法 |
| 429 | 任意方法（含 POST），优先使用 `Retry-After` |
| 取消 | 不重试 |
| 流式请求 | 不重试 |

退避为指数增长并受 `max_backoff` 限制。中间件的 `on_request` 按注册顺序执行，
`on_response` 按逆序执行；流式请求只执行 `on_request`，不调用 `on_response`。

### 4.3 错误模型

`AirxError` 是统一错误入口，但每个变体保留原始形状：

| 变体 | 内容 |
| --- | --- |
| `Transport(TransportError)` | 连接、超时、TLS、代理、body、取消等；保留 source 链 |
| `Http(HttpError)` | 非 2xx 且无法解析 Provider 错误体；保留状态码与原始 body |
| `Serialization(SerializationError)` | 请求序列化失败 |
| `Deserialization(DeserializationError)` | 响应 / 事件反序列化失败；保留 serde 错误 |
| `Provider(Box<ProviderError>)` | 可识别的 Provider 错误（装箱避免撑大枚举） |
| `Stream(StreamError)` | SSE 语法、UTF-8、截断、明确失败、非法状态 |
| `Configuration(ConfigurationError)` | 客户端配置问题 |

`ProviderError` 保留 `provider`、`status`、`code`、`message`、`request_id`、
`headers`、`raw_body`。
`AirxError::is_retryable()` 只做中立判定：连接、超时、429、5xx；
最终是否重试由传输层结合 HTTP 方法与策略决定。
`AirxError` 还提供 `request_id()`、`is_rate_limit()`、`retry_after()`、
`response_headers()` 等访问器，避免调用方重复解析。

### 4.4 JSON 保真

四种表示按场景分工：

| 表示 | 用途 |
| --- | --- |
| 强类型 struct | 协议请求 / 响应 / 事件 |
| `serde_json::Value` | 动态结构、未知块、原始扩展 |
| `RawJson`（`Box<RawValue>`） | 错误体 / 原样转发，保留原始字节 |
| `bytes::Bytes` | 网络原始数据 |

- 未知字段统一进入 `Extensions`（`BTreeMap<String, Value>`），通过 `#[serde(flatten)]`
  双向保留；`BTreeMap` 提供确定顺序，但不代表协议语义。
- 未知内容块 / 事件使用 `Other(Value)` / `Unknown(Value)`。
- `JsonMode` 提供 `Compact`、`Pretty`、`Preserve`；`Canonical`（RFC 8785 JCS）
  尚未实现，调用会返回 `AirxError::Configuration`。
- 已知保真缺口：`Option<T>` 无法区分 `null` 与字段缺省；需要 null 或字节级保真时
  使用 `RawJson` 或保留原始响应体。

### 4.5 标识类型

- `ProviderId`：Provider 标签，只用于错误归属与展示，不决定协议。
- `ModelId`：模型标识。
- `ApiVersion`：Provider 接口版本（如 Anthropic `2023-06-01`），影响路径 / Header。

三者都是字符串新类型，新增 Provider 不需要修改 `airx-core`。

---

## 5. 协议层实现

### 5.1 总览

| Provider | 模块 | 默认 base URL | 端点路径 | 认证 |
| --- | --- | --- | --- | --- |
| OpenAI Chat | `openai::chat` | `https://api.openai.com/v1` | `/chat/completions` | `Authorization: Bearer` |
| OpenAI Responses | `openai::responses` | 同上 | `/responses` | `Authorization: Bearer` |
| Anthropic Messages | `anthropic::messages` | `https://api.anthropic.com` | `/v1/messages` | `x-api-key` + `anthropic-version` |
| Gemini | `gemini::generate_content` | `https://generativelanguage.googleapis.com/v1beta` | `/models/{model}:generateContent` | `x-goog-api-key` |

OpenAI 的 `chat` 与 `responses` 是独立模块：请求 / 响应 / 事件模型不同，不共享类型。
OpenAI 的共享部分只放在 `openai::common`（错误体、`Usage`、`ReasoningEffort` / `Verbosity`）。

### 5.2 OpenAI Chat Completions

- 请求：`ChatCompletionRequest`，覆盖 model、messages、采样参数、
  `max_completion_tokens`（以及已弃用的 `max_tokens`）、`reasoning_effort`、`verbosity`、
  tools / tool_choice / parallel_tool_calls、`response_format`、stream / stream_options。
- 消息：system / user（文本或多模态）/ assistant（文本 + tool_calls）/ tool。
- 响应：choices、usage、system_fingerprint 与扩展字段。
- 流事件：`ChatStreamEvent::Chunk(ChatCompletionChunk)` 与 `ChatStreamEvent::Done`；
  `[DONE]` 由端点解码为 `Done`，chunk 内的错误对象直接映射为 `AirxError::Provider`。
- 聚合：`ChatStreamAccumulator` 合并文本、`reasoning_content`、工具调用增量与 usage；
  只有收到 `Done` 才完成。

### 5.3 OpenAI Responses

- 请求：`ResponsesRequest`，输入支持纯文本或 `InputItem` 列表（message / function_call_output）。
- 输出 item：message（output_text / refusal）、function_call、web_search_call、
  以及未知 item 的 `Other(Value)`。
- 流事件：created、output_item.added/done、output_text.delta/done、
  function_call_arguments.delta/done、completed、failed、incomplete、error；
  未识别事件保留为 `Unknown(Value)`。
- 端点忽略空 data 与 `[DONE]`；`type == "error"` 映射为 `ProviderError`。
- 聚合：`ResponsesStreamAccumulator` 按 `output_index` 维护 item；
  `response.completed` 才完成，`response.failed` / `error` 记为失败。

### 5.4 Anthropic Messages

- 请求：`MessagesRequest`；`max_tokens` 必填；支持 `output_config`
  （`effort` / 结构化输出格式）、`thinking`、tools / tool_choice、`stop_sequences`。
- 内容块：text、image、tool_use、tool_result、thinking、redacted_thinking、
  server_tool_use、web_search_tool_result；未知块保留为 `ContentBlock::Other(Value)`。
- 错误体映射为 `ProviderError`，保留 `type` 与 `message`。
- 流事件：message_start、content_block_start/delta/stop、message_delta、message_stop、
  ping、error、未知事件；端点层过滤 `ping`。
- 聚合：`MessagesStreamAccumulator` 按内容块 index 合并；工具参数增量在
  `finish()` 时解析（严格模式下非法 JSON 报错）；只有收到 `message_stop` 才完成。

### 5.5 Gemini generateContent

- 模型在路径中：端点持有 `ModelId`，请求体不含模型字段。
  - `gemini-2.5-flash` → `models/gemini-2.5-flash`
  - `models/…` / `tunedModels/…` / `publishers/…` 原样保留
  - 空模型、非法字符、`.` / `..` 段会被拒绝
- 请求：`contents`、`systemInstruction`、`tools` / `toolConfig`、
  `generationConfig`（采样、结构化输出、thinking、模态等）、`safetySettings`、`cachedContent`。
- 内容片段：text（含 `thought` / `thoughtSignature`）、inlineData、fileData、
  functionCall、functionResponse、executableCode、codeExecutionResult、`Other(Value)`。
- 响应：candidates、promptFeedback、usageMetadata、modelVersion、responseId 与扩展字段。
- 流式：使用 `alt=sse`；每个 SSE data 就是一个 `GenerateContentResponse` 增量。
  没有 `[DONE]`；聚合器以候选 `finishReason` 或 `promptFeedback.blockReason` 判定完成，
  缺失时返回 `Incomplete`；`FINISH_REASON_UNSPECIFIED` 不视为完成。

### 5.6 未知结构策略

| 位置 | 策略 |
| --- | --- |
| 请求 / 响应 / chunk 的未知字段 | `extensions`（flatten） |
| OpenAI Responses 未知 item | `OutputItem::Other(Value)` |
| OpenAI Responses 未知事件 | `ResponsesEvent::Unknown(Value)` |
| Anthropic 未知内容块 | `ContentBlock::Other(Value)` |
| Anthropic 未知事件 | `MessagesEvent::Unknown(Value)` |
| Gemini 未知 part | `Part::Other(Value)` |

---

## 6. 流式基础设施

### 6.1 三层职责

| 层 | 位置 | 职责 |
| --- | --- | --- |
| 网络 | `airx-transport` | 响应字节流与背压 |
| SSE 语法 | `airx-core::stream` | 字节 → 行 → 字段 → `RawEvent` |
| 协议事件 | `airx-protocol` | `RawEvent` → Provider 原生事件 |
| 聚合 | `airx-protocol` | 增量事件 → 完整响应 |

SSE 层只认语法，不认 `[DONE]` 等协议约定。

### 6.2 LineBuffer

- 处理 `LF`、`CRLF`、单独 `CR`；末尾 `CR` 等待下一字节，避免把 `CRLF` 误判为 `CR` + 空行。
- 忽略流开头 UTF-8 BOM，BOM 跨 chunk 也能识别。
- 按整行校验 UTF-8；行结束符均为 ASCII，跨 chunk 的多字节字符在行结束时已完整。

### 6.3 SseDecoder

- 维护 `event`、多行 `data`、`id`、`retry` 与 `last_event_id`。
- 只有以空行结束且 `data` 非空的事件才会分发；注释与空事件被忽略。
- `finish()` 区分“最后一个事件已完整”与“事件未闭合”；后者返回 `Incomplete`。
- `RawEvent { event, data, id, retry }` 是协议层的唯一输入。

### 6.4 ProtocolStream

- 组合 `StreamingEndpoint + ByteStream + SseDecoder`。
- `next()` 在事件边界等待字节；EOF 时先冲刷 SSE，再返回 `Ok(None)`。
- 字节流错误映射为 `AirxError::Transport`；事件未闭合映射为 `AirxError::Stream`。
- 非 2xx 流式响应先读完 body，由协议层解码，避免错误信息丢失。

### 6.5 聚合器

| Provider | 类型 | 完成条件 | 失败条件 |
| --- | --- | --- | --- |
| OpenAI Chat | `ChatStreamAccumulator` | 收到 `Done`（`[DONE]`） | 终态后继续 apply → `InvalidState` |
| OpenAI Responses | `ResponsesStreamAccumulator` | `response.completed` | `response.failed` / `error` → `Failed`；未完成 → `Incomplete` |
| Anthropic | `MessagesStreamAccumulator` | `message_stop` | `error` → `Failed`；未完成 → `Incomplete` |
| Gemini | `GenerateContentStreamAccumulator` | `finishReason` 或 `blockReason` | 其它结束方式 → `Incomplete` |

`StreamStatus`（`Active` / `Completed` / `Failed` / `Incomplete` / `Cancelled`）统一表达
生命周期；只有 `Completed` 是成功。

---

## 7. 配置、URL 与 Header

### 7.1 配置分层

| 层 | 持有者 | 内容 |
| --- | --- | --- |
| 执行级 | `ClientBuilder` → `Executor` | Transport、连接 / 整体超时、`RetryPolicy`、执行级默认 Header |
| 单次请求 | `RequestOptions` | 临时 Header、`Idempotency-Key`、timeout、retry override |
| Provider 级 | `OpenAiConfig` / `AnthropicConfig` / `GeminiConfig` | API Key、base URL、模式路径、API 版本、组织 / 项目、Provider 默认 Header |

Provider 配置的 `Debug` 会隐藏 API Key。

### 7.2 URL 与模式路径

`base_url` 必须是绝对地址，末尾 `/` 会被去掉。模式路径与 `base_url` 解耦：

| Provider | 模式 | 默认路径 | 构建时覆盖 | 运行时覆盖 |
| --- | --- | --- | --- | --- |
| OpenAI | chat | `/chat/completions` | `.chat_path(...)` | `chat_at(...)` |
| OpenAI | responses | `/responses` | `.responses_path(...)` | `responses_at(...)` |
| Anthropic | messages | `/v1/messages` | `.messages_path(...)` | `messages_at(...)` |
| Gemini | generateContent | `/models/{model}:generateContent` | 由模型决定 | — |
| Gemini | stream | `/models/{model}:streamGenerateContent?alt=sse` | 由模型决定 | — |

路径解析规则（`airx-protocol::internal::url`）：

- 相对路径补前导 `/`，拼接到 `base_url`。
- 完整 `http(s)://` URL 原样使用，忽略 `base_url`。
- 拒绝空路径、协议相对地址、非法 URI。

### 7.3 Header 优先级

从上到下优先级降低：

1. **单次请求 `RequestOptions`**：最后写入，覆盖同名 Header；可设置 `Idempotency-Key`。
2. **Provider 配置的 `default_headers`**：编码阶段最后写入，同名校验为覆盖。
3. **协议层写入**：`content-type`、认证 Header（`Authorization` / `x-api-key` /
   `x-goog-api-key`）、`anthropic-version`、`openai-organization` / `openai-project`。
4. **执行级 `default_headers`**：仅填充缺失 Header（`or_insert`），不覆盖已有值。
5. **缺省 Header**：`User-Agent: airx/<version>`；`Accept` 由 `ApiClient` 按调用模式
   设置为 `application/json` 或 `text/event-stream`（均不覆盖已有值）。

网关需要的额外 Header（如 OpenCode Zen 的 `x-opencode-session`）通过执行级
`ClientBuilder::default_headers` 注入。Provider 级默认 Header 通过各 Config 的
`with_header` 提供，不经过门面构建器。

### 7.4 错误归属与 request id

- Provider 错误体由各协议模块解析；无法解析时退化为 `HttpError`，保留原始 body。
- `request_id` 从 `x-request-id` / `request-id` / `x-amzn-requestid` 提取。
- 错误结构保留状态码、错误码、request id、响应头与原始响应体；
  是否记录敏感 Header 由错误形状决定，认证 Header 不进入日志。

---

## 8. Feature 与平台

### 7.5 SDK 体验层

协议层之外，门面提供与官方 SDK 对齐的体验层：

| 能力 | API | 位置 |
| --- | --- | --- |
| 响应元数据 | `create_with_meta` → `WithMeta<T> { data, meta }`；`ResponseMeta` 含 status / headers / request id / raw body | `airx/src/response.rs` |
| 流式元数据 | `ProtocolStream::meta()` / `request_id()` | 同上 |
| 单次请求选项 | `with_options(RequestOptions)`；Header、`Idempotency-Key`、timeout、retry override | `airx/src/options.rs` |
| `futures::Stream` | `ProtocolStream` 实现 `futures_core::Stream`，支持 `TryStreamExt` / `select!` | `airx/src/stream.rs` |
| 流式收尾 | 各协议 `final_response()` / `final_text()`，内部驱动对应 Accumulator | 同上 |
| 结构化输出 | `parse_as::<T>()`（Chat / Responses / Messages / Gemini），只做 serde 解析 | 各 Provider 客户端 |
| 模型列表 | `client.models().list() / retrieve()`；Anthropic / Gemini 支持 `list_all()` 翻页 | 各 Provider 客户端 |
| 计 token | Anthropic `messages().count_tokens()`；Gemini `generate_content(m).count_tokens()` | 同上 |
| 文件 / 批处理 / Embeddings | OpenAI `files/batches/embeddings`、Anthropic `files/batches`、Gemini `embed_content` | 各 Provider 客户端 |
| 完整历史输入 | Chat `.messages(...)`、Messages `.messages(...)`、Gemini `.contents(...)`、Responses `.input_items(...)` | 各请求构建器 |
| raw execute | `OpenAi/Anthropic/Gemini::execute(HttpRequest)` | 各 Provider 客户端 |

默认策略对齐官方 SDK：native 2 次重试、10 秒连接超时、gzip / brotli / deflate 解压、
`User-Agent: airx/<version>`。整体超时默认关闭，避免截断长流；需要时通过
`ClientBuilder::timeout` 设置。

### 8.1 门面 feature

`airx` 默认启用 `native + rustls-tls + compression + all-providers + stream`。

| Feature | 作用 |
| --- | --- |
| `native` | Reqwest 传输 |
| `rustls-tls` / `native-tls` | TLS 后端，二选一 |
| `compression` | gzip / brotli / deflate 响应解压 |
| `wasm` | 浏览器 Fetch 传输 |
| `openai` / `anthropic` / `gemini` | 启用对应协议模块 |
| `all-providers` | 三者全开 |
| `stream` | 门面流式 API（`ProtocolStream`、`stream()` 方法） |
| `json` | 预留开关（`JsonMode` / `RawJson` 当前始终可用） |

按需裁剪示例：

```toml
airx = { version = "0.1", default-features = false, features = ["native", "rustls-tls", "openai", "stream"] }
```

### 8.2 传输 feature

- `airx-transport` 默认 `native + rustls-tls`。
- `native` 引入 reqwest；`wasm` 引入浏览器依赖；两者按目标平台选择。
- TLS 后端只选一个。

### 8.3 占位 feature

`airx-core` 的 `stream` / `json`、`airx-protocol` 的 `stream`、
`airx-transport` 的 `stream` 目前是空 feature：

```toml
stream = []
json = []
```

它们不会裁剪模块；流式与 JSON 工具在对应 crate 中始终编译。真正控制门面流式 API 的
是 `airx` 的 `stream` feature。

### 8.4 平台约束

- WASM 下不提供 `RetryTransport`；客户端默认关闭重试，显式启用会在构建时返回配置错误。
- Native 的 `Send + Sync` 约束在 WASM 下放宽，以适配浏览器 Future / Stream。
- WASM 传输已通过 `wasm32-unknown-unknown` 编译检查；浏览器运行时行为未纳入 CI。

---

## 9. 测试与质量保障

### 9.1 测试分层

| 层 | 位置 | 覆盖 |
| --- | --- | --- |
| 单元测试 | `airx-core/src/**`、`airx-protocol/src/**`、`airx-transport/src/**` | SSE 边界、错误分类、路径校验、重试规则、聚合状态机 |
| 协议集成测试 | `airx-protocol/tests/*_protocol.rs` + `tests/fixtures/**` | 编码 Header / URL、响应解析、未知字段往返、错误体映射 |
| 端到端竖切 | `airx/tests/*.rs` | Mock Transport → 编码 → 执行 → 解码 / 流式聚合 |
| 路径覆盖 | `airx/tests/endpoint_paths.rs` | base_url 与模式路径的构建时 / 运行时覆盖 |
| SDK 体验 | `airx/tests/sdk_experience.rs`、`models_and_tokens.rs` | 请求选项、响应元数据、`Stream`、parse、默认重试、models / countTokens |
| 资源与底层接入 | `airx/tests/resources.rs` | Files / Batches / Embeddings、Responses 生命周期、multipart、per-request timeout/retry、raw execute |

当前共 **283** 个测试，`cargo test --workspace` 全绿。

关键断言：

- SSE 在每个字节边界切分都能稳定解码。
- 未知字段经过一次反序列化 / 序列化往返仍保留。
- 非 2xx 流式响应能还原 Provider 错误。
- 截断流不会被当作成功。
- 执行级 Header 不覆盖协议 Header；单次请求选项可覆盖。
- native 默认重试按幂等性与 429 规则执行；显式禁用后只发一次。

### 9.2 CI

`.github/workflows/ci.yml` 执行：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
./scripts/check-deps.sh
cargo check -p airx --no-default-features
cargo check -p airx --no-default-features --features gemini
cargo check -p airx --target wasm32-unknown-unknown \
  --no-default-features --features wasm,all-providers,stream
```

---

## 10. 已知边界

- `Option<T>` 无法区分 `null` 与字段缺省；需要 null 或字节级保真时使用 `RawJson`。
- `JsonMode::Canonical`（RFC 8785 JCS）尚未实现，会返回明确错误。
- 中间件是同步的；需要异步操作（如刷新 Token）时应实现自定义 `Transport`。
  流式响应不会调用 `on_response`。
- 流式请求不重试；首个事件前重试尚未实现。
- WASM 不支持自动重试（默认关闭）；native 默认 2 次。
- WASM 传输只做编译检查，浏览器运行时未纳入 CI。
- Gemini 已封装 `generateContent` / `streamGenerateContent` / `countTokens` / 模型列表 /
  Embeddings；Vertex AI、Live API、Files、Batches 未封装。
- 已封装 OpenAI Files / Batches / Embeddings、Anthropic Files / Batches。
  图片/音频、Vector Store、Fine-tuning、Assistants/Threads 仍不做。
- 服务端联网工具由 Provider 后台执行；airx 只声明工具并解析引用 / 调用记录，
  不发起本地网络请求。
- Provider 参数随模型演进；能力与对齐来源以
  [protocol-support.md](protocol-support.md) 和 [compatibility.md](compatibility.md) 为准。

---

## 附录 A：模块地图

```text
crates/
├── airx-core/src/
│   ├── error.rs              # AirxError / ProviderError / TransportError / StreamError
│   ├── http.rs               # HttpRequest / HttpResponse，无客户端
│   ├── model.rs              # ProviderId / ModelId / ApiVersion
│   ├── json/
│   │   ├── format.rs         # JsonMode / to_string
│   │   ├── raw.rs            # RawJson / RawExtension / parse_raw
│   │   └── value.rs          # Extensions（BTreeMap<String, Value>）
│   └── stream/
│       ├── buffer.rs         # 字节 → 行（LF / CRLF / CR、BOM、UTF-8）
│       ├── decoder.rs        # 行 → RawEvent
│       ├── sse.rs            # SSE 字段语法
│       ├── status.rs         # StreamStatus
│       └── json_accumulator.rs # 增量 JSON 结构完整性判断
├── airx-protocol/src/
│   ├── endpoint.rs           # Endpoint / StreamingEndpoint
│   ├── internal/url.rs       # 模式路径与 URL 解析
│   ├── openai/
│   │   ├── config.rs         # OpenAiConfig
│   │   ├── models.rs         # 模型列表 / 详情
│   │   ├── files.rs          # Files（multipart 上传 / 内容）
│   │   ├── batches.rs        # Batches
│   │   ├── embeddings.rs     # Embeddings
│   │   ├── common/           # 错误体 / Usage / ReasoningEffort / Verbosity
│   │   ├── chat/             # 请求 / 响应 / chunk / event / tool / endpoint / accumulator
│   │   └── responses/        # 请求 / 响应 / item / event / tool / resource / endpoint / accumulator
│   ├── anthropic/
│   │   ├── config.rs         # AnthropicConfig
│   │   ├── common.rs         # 错误体 / Usage
│   │   ├── files.rs          # Files
│   │   ├── batches.rs        # Messages Batches
│   │   ├── models.rs         # 模型列表 / 详情（cursor 分页）
│   │   └── messages/         # 请求 / 响应 / 内容块 / event / endpoint / accumulator / count_tokens
│   └── gemini/
│       ├── config.rs         # GeminiConfig
│       ├── common.rs         # 错误体 / SafetyRating / UsageMetadata
│       ├── embeddings.rs     # embedContent / batchEmbedContents
│       ├── models.rs         # 模型列表 / 详情（pageToken 分页）
│       └── generate_content/ # 请求 / 响应 / content / generation / tool / endpoint / accumulator / count_tokens
├── airx-transport/src/
│   ├── transport.rs          # Transport / BoxFuture
│   ├── native.rs             # ReqwestTransport
│   ├── wasm.rs               # WasmTransport
│   ├── mock.rs               # MockTransport
│   ├── retry.rs              # RetryPolicy / RetryTransport
│   ├── middleware.rs         # Middleware / MiddlewareTransport
│   ├── response.rs           # HttpStreamResponse / ResponseMetadata
│   └── stream.rs             # ByteStream / byte_stream
└── airx/src/
    ├── client.rs             # Client 命名空间
    ├── builder.rs            # ClientBuilder / Provider 构建器
    ├── executor.rs           # Executor（执行级 Header + Transport + 请求选项）
    ├── api.rs                # ApiClient（encode → execute → decode / meta）
    ├── options.rs            # RequestOptions（Header / Idempotency-Key / timeout / retry）
    ├── multipart.rs          # boundary 生成
    ├── response.rs           # ResponseMeta / StreamMeta / WithMeta / parse_json
    ├── stream.rs             # ProtocolStream + Stream impl + final_* 助手
    ├── openai.rs             # OpenAi / Chat / Responses / Models / Files / Batches / Embeddings
    ├── anthropic.rs          # Anthropic / Messages / Models / Files / Batches
    ├── gemini.rs             # Gemini / GenerateContent / Models / Embeddings
    └── prelude.rs            # 常用类型导出
```

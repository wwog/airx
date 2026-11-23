# OpenAI 接入指南（Chat Completions / Responses）

> **对齐基准：2026-11-23。** 参数随模型与官方 SDK 演进；接入前核对
> [protocol-support.md](protocol-support.md)、[compatibility.md](compatibility.md)
> 与官方文档。本文描述仓库当前实现，与代码冲突时以代码为准。

`airx` 为 OpenAI 原生 API 提供强类型请求、响应、流式事件与错误形状，不做跨 Provider
的统一请求模型。本文覆盖 Chat Completions、Responses 与 Models；DeepSeek 等
OpenAI-Compatible 部署通过 `base_url` 接入，不使用独立协议模块。

## 1. 适用边界

airx 负责到**协议与 HTTP 边界**：

| airx 负责 | 边界之外（不提供） |
| --- | --- |
| 请求构造、URL / 认证 Header、JSON 编解码 | Agent 工具执行循环：只返回 `tool_calls` / `function_call`，不执行、不循环 |
| 连接池、TLS、超时、重试、流式读取 | 会话 / 状态管理；Vector Store、Realtime、Fine-tuning、Assistants / Threads |
| 响应 / SSE / 错误体解析、Accumulator、Files / Batches / Embeddings | 图片 / 音频、Vector Store、Fine-tuning、Assistants / Threads |
| `RequestOptions`、`ResponseMeta`、`futures::Stream`、`parse_as`、raw execute | 服务端工具由 OpenAI 后台执行，airx 只声明与解析 |

函数工具的 HTTP 请求、参数校验、结果回传由应用完成；airx 支持把结果作为历史消息回传，
但不维护循环。服务端工具（Responses `web_search`）的调用记录与引用由 airx 解析，不代发请求。

## 2. 快速接入

```toml
[dependencies]
airx = { version = "0.1", default-features = false, features = ["native", "rustls-tls", "openai", "stream"] }
airx-protocol = "0.1" # 直接使用 ContentPart / Tool / JsonSchemaSpec 等协议类型时
serde_json = "1"      # 示例中的 json! 宏
futures-util = "0.3"  # 使用 TryStreamExt 时需要
tokio = { version = "1", features = ["macros", "rt-multi-thread"] } # #[tokio::main]
```

| 场景 | features |
| --- | --- |
| 默认（三协议 + 流式） | `native + rustls-tls + all-providers + stream` |
| 最小 OpenAI | `native + rustls-tls + openai + stream` |
| 浏览器 / WASM | `wasm + openai + stream`（不启用 native） |
| 只要非流式 | 去掉 `stream`；`.stream()` 与 `ProtocolStream` 不可用 |

`Client::openai().api_key(...).build()?` 返回**已定性的 `OpenAi` 客户端**，可克隆复用；
后续请求不需要再次选择 Provider。

```rust
use airx::prelude::*;

#[tokio::main]
async fn main() -> Result<(), AirxError> {
    let client = Client::openai().api_key("sk-...").build()?;

    let request = ChatCompletionRequest::builder()
        .model("gpt-4o-mini")
        .system("Be concise.")
        .user("Hello")
        .build()?;
    let response = client.chat().create(request).await?;
    println!("{}", response.first_content().unwrap_or_default());

    // 最小流式：final_text() 会消费整个流并聚合
    let request = ChatCompletionRequest::builder()
        .model("gpt-4o-mini")
        .user("用一句话解释 Rust 的所有权。")
        .build()?;
    let text = client.chat().stream(request).await?.final_text().await?;
    println!("{text}");
    Ok(())
}
```

## 3. 配置

### 3.1 基地址与模式路径

官方地址是默认值：`https://api.openai.com/v1`。**只有兼容网关才需要 `base_url`**。

| 模式 | 默认路径 | 构建时覆盖 | 单次覆盖 |
| --- | --- | --- | --- |
| Chat Completions | `/chat/completions` | `.chat_path(...)` | `client.chat_at(path)?` |
| Responses | `/responses` | `.responses_path(...)` | `client.responses_at(path)?` |

相对路径补前导 `/` 后拼接到 `base_url`；完整 `http(s)://` URL 原样使用并忽略 `base_url`；
空路径、协议相对地址（如 `//host/path`）与非法 URI 会被拒绝。

```rust
let gateway = Client::openai()
    .api_key(key)
    .base_url("https://gateway.example.com/v1")
    .chat_path("v2/chat")                                // 相对路径
    .responses_path("https://other.example.com/respond") // 完整 URL
    .build()?;

gateway.chat_at("/override/chat")?.create(request).await?; // 单次覆盖，不修改客户端
```

### 3.2 组织与项目

`.organization("org-...")` / `.project("proj-...")` 分别写入 `OpenAI-Organization` /
`OpenAI-Project` Header。

### 3.3 Header 分层

优先级从高到低：

| 层 | API | 合并行为 | 典型用途 |
| --- | --- | --- | --- |
| 单次请求 | `RequestOptions` | 覆盖同名 Header | trace id、`Idempotency-Key` |
| Config 层 | `OpenAiConfig::with_header` | 编码阶段最后写入，覆盖同名协议 Header | 原生端点替换认证等 |
| 协议层 | 编码器写入 | `Authorization`、`content-type`、组织 / 项目 | 自动完成 |
| 执行级 | `ClientBuilder::default_headers` | `or_insert`，**不覆盖**已有 Header | 网关 session Header |
| 缺省 | `ApiClient` / `Executor` | `Accept`（按调用模式）、`User-Agent: airx/<version>` | 自动完成 |

`with_header` 属于 `OpenAiConfig`，**不通过 `Client::openai()` 构建器暴露**；门面客户端
请用执行级 `default_headers`（HeaderMap 来自 `airx-core`，与 `http` crate 类型一致）。

### 3.4 超时与重试

| 项 | 默认值 | 设置方式 |
| --- | --- | --- |
| 连接超时 | 10 秒（仅内置 native Transport） | `Client::builder().connect_timeout(...)` |
| 整体请求超时 | 关闭 | `.timeout(...)`；**会截断长流** |
| 重试 | native 2 次；WASM 0 次 | `.retry_policy(RetryPolicy::...)` |
| `User-Agent` | `airx/<version>` | 自动写入 |

`Client::openai()` 的执行配置只有 `.timeout/.retry_policy/.transport`；`connect_timeout` / `default_headers` 先用 `Client::builder()`。

### 3.5 单次请求选项

```rust
let chat = client.chat().with_options(
    RequestOptions::new().try_header("x-trace-id", "trace-1")?.idempotency_key("idem-1"),
);
let response = chat.create_with_meta(request).await?;
```

`RequestOptions` 同样适用于 `responses()` 与 `models()`；非法 Header / 幂等键立即返回
`AirxError::Configuration`。

## 4. Chat Completions

### 4.1 请求构建

```rust
let request = ChatCompletionRequest::builder()
    .model("gpt-4o-mini")          // 必填
    .system("You are concise.")
    .user("Hello")
    .max_completion_tokens(256)
    .build()?;                     // model 与至少一条 message 必填
```

| 方法 | 请求字段 | 说明 |
| --- | --- | --- |
| `.model(...)` | `model` | 必填，`ModelId` |
| `.system / .user / .assistant / .message` | `messages` | 按调用顺序追加 |
| `.messages(iter)` | `messages` | 一次设置完整历史（`Vec<ChatMessage>`），适合 Agent 循环 |
| `.temperature(...)` / `.top_p(...)` | 同名 | 采样参数 |
| `.max_completion_tokens(u32)` | `max_completion_tokens` | **推荐**，含推理 token |
| `.max_tokens(u32)` | `max_tokens` | 已弃用；o-series 不兼容 |
| `.reasoning_effort(...)` / `.verbosity(...)` | 同名 | 取值见 `ReasoningEffort`（none/minimal/low/medium/high/xhigh/max）/ `Verbosity`（low/medium/high） |
| `.stop(...)` / `.user_id(...)` | `stop` / `user` | 用户标识方法名与字段名不同 |
| `.tools(...)` / `.tool_choice(...)` / `.parallel_tool_calls(bool)` | 同名 | 客户端函数工具 |
| `.response_format(...)` | `response_format` | `text` / `json_object` / `json_schema` |
| `.stream(...)` / `.stream_options(...)` / `.extension(k, v)` | 同名 / 顶层字段 | 流式设置、未建模参数透传 |

未列出的参数（如 `logit_bias`、`n`、`seed`、`presence_penalty`）用 `.extension(...)` 透传。

### 4.2 多模态 user 消息

当前 `ContentPart` 只类型化 `text` 与 `image_url`；`input_audio` / `file` 未建模且无 `Other` 变体，需走原生 Endpoint。

```rust
use airx_protocol::openai::chat::{ContentPart, ImageUrl};

let message = ChatMessage::user_parts(vec![
    ContentPart::Text { text: "这张图里是什么？".to_owned() },
    ContentPart::ImageUrl { image_url: ImageUrl {
        url: "https://example.com/cat.png".to_owned(),
        detail: Some("auto".to_owned()), // low / high / auto
        extensions: Default::default(),
    }},
]);
let request = ChatCompletionRequest::builder()
    .model("gpt-4o-mini").message(message).build()?;
```

### 4.3 工具声明与工具结果回传

```rust
let tools = [Tool::function(
    "get_weather", serde_json::json!({"type": "object"}), None,
)];

let request = ChatCompletionRequest::builder()
    .model("gpt-4o-mini")
    .user("查询新加坡天气，必须调用工具。")
    .tools(tools)
    .tool_choice(ToolChoice::auto())
    .build()?;

let calls = client.chat().create(request).await?
    .choices.first()
    .and_then(|choice| choice.message.tool_calls.clone())
    .unwrap_or_default();

// 工具由应用执行，airx 只负责把结果作为历史消息发回；假设 calls 非空。
let follow_up = ChatCompletionRequest::builder()
    .model("gpt-4o-mini")
    .message(ChatMessage::assistant_tool_calls(calls.clone()))
    .message(ChatMessage::tool_result(&calls[0].id, "31°C，多云"))
    .build()?;
```

`ToolChoice` 支持 `auto()` / `none()` / `required()` / `function(name)`。服务端工具用
`Tool::server("tool_type")` 或 `Tool::raw(value)` 原样声明；是否执行由 Provider / 网关决定。

### 4.4 结构化输出与响应访问

```rust
use airx_protocol::openai::chat::{JsonSchemaSpec, ResponseFormat};

let request = ChatCompletionRequest::builder()
    .model("gpt-4o-mini")
    .response_format(ResponseFormat::JsonSchema {
        json_schema: JsonSchemaSpec {
            name: "poster".to_owned(),
            description: None,
            schema: serde_json::json!({"type": "object"}),
            strict: Some(true),
            extensions: Default::default(),
        },
    })
    .build()?;

let parsed: MyPoster = client.chat().parse_as(request).await?;
```

`parse_as::<T>` 只做 serde 解析：读取第一个候选的文本并反序列化，**不会**替你设置
`response_format`；空内容返回 `AirxError::Deserialization`。响应侧 `first_content()`
返回首个候选文本，`ResponseMessage` 保留 `refusal`、`tool_calls`、`annotations` 等字段。

## 5. Responses

### 5.1 请求构建

```rust
let request = ResponsesRequest::builder()
    .model("gpt-4o-mini")                 // 必填
    .instructions("You are concise.")     // 系统指令
    .input("用一句话解释 Rust 所有权。")
    .max_output_tokens(256)
    .reasoning_effort(ReasoningEffort::Low)
    .text_format(TextFormat::JsonObject)
    .build()?;

let response = client.responses().create(request).await?;
println!("{}", response.output_text());
```

| 方法 | 请求字段 | 说明 |
| --- | --- | --- |
| `.model(...)` | `model` | 必填 |
| `.input(...)` / `.input_items(Vec<InputItem>)` / `.instructions(...)` | `input` / `instructions` | 纯文本、item 列表与系统指令 |
| `.max_output_tokens(u32)` / `.temperature(...)` / `.top_p(...)` | 同名 | 输出长度与采样 |
| `.tools(...)` / `.tool_choice(...)` / `.parallel_tool_calls(bool)` | 同名 | 函数工具与服务端工具 |
| `.reasoning_effort(...)` | `reasoning.effort` | 推理投入 |
| `.text_format(TextFormat)` | `text.format` | `text` / `json_object` / `json_schema` |
| `.stream(bool)` / `.extension(key, value)` | `stream` / 顶层字段 | 流式标记、未建模参数 |

`TextFormat::JsonSchema { name, description, schema, strict }` 用于结构化输出；
`parse_as::<T>` 读取 `output_text()` 后做 serde 解析。

### 5.2 输入 item 与输出 item

| 方向 | 已类型化 | 未知 / 未类型化 |
| --- | --- | --- |
| 输入 | `InputItem::Message { role, content }`（content 仅 String）/ `FunctionCallOutput { call_id, output }` | 多模态 content、`function_call` / `reasoning` 等输入 item |
| 输出 | `OutputItem::Message`（`OutputText` / `Refusal`） | `reasoning`、`file_search_call`、`code_interpreter_call` 等 → `Other(Value)` |
| 输出 | `OutputItem::FunctionCall` / `WebSearchCall` | 未知 item → `Other(Value)`；未知标注 → `Annotation::Other(Value)` |

```rust
use airx_protocol::openai::responses::InputItem;

let request = ResponsesRequest::builder()
    .model("gpt-4o-mini")
    .input_items(vec![
        InputItem::Message { role: "user".into(), content: "查询新加坡天气。".into() },
        // 应用执行完工具后，把结果作为 item 回传。
        InputItem::FunctionCallOutput { call_id: "call_1".into(), output: "31°C，多云".into() },
    ])
    .build()?;
```

### 5.3 item / 事件覆盖与剩余缺口

已类型化：

- 请求字段：`previous_response_id`、`store`、`background`、`include`、`truncation`、
  `conversation`、`metadata`、`service_tier`、`prompt_cache_key`、`safety_identifier`、
  `max_tool_calls`、`reasoning.summary`、`text.verbosity`。
- 输入 item：`message`（文本或多模态 `input_text` / `input_image` / `input_file`）、
  `function_call`、`function_call_output`、`reasoning`、`item_reference`；未知 item 保留为
  `InputItem::Other(Value)`。
- 输出 item：`message`、`function_call`、`reasoning`、`web_search_call`、
  `file_search_call`、`code_interpreter_call`、`image_generation_call`、`mcp_call`；
  其余保留为 `OutputItem::Other(Value)`。
- 流式事件：`created` / `in_progress` / `content_part.added·done` / `output_text.delta·done` /
  `refusal.delta·done` / `reasoning_summary_text.delta·done` /
  `function_call_arguments.delta·done` / 服务端工具生命周期（`ServerToolCall`）/
  `completed` / `failed` / `incomplete` / `error`；其余保留为 `Unknown(Value)`。

剩余缺口：

- `mcp_list_tools` / `mcp_approval_request`、音频与自定义工具 item 仍落在 `Other`。
- 标注除 `url_citation` 外仍为 `Annotation::Other(Value)`。
- `conversation` 以 `Value` 透传，未拆成强类型对象。
- 服务端工具生命周期事件统一为 `ServerToolCallEvent`（`kind` 区分具体事件）。

扩展路径：未知字段继续用 `.extension(...)`；需要完全控制 body 时使用
`openai.responses().execute(HttpRequest)` 或自定义 Endpoint。输出 item / 事件需要更细类型时，
在 `airx-protocol` 增加 typed endpoint / 变体。

## 6. 响应与流式

### 6.1 响应元数据

```rust
let response = client.chat().create_with_meta(request).await?;
println!("{} {:?}", response.meta.status.as_u16(), response.meta.request_id);
let data = response.data; // 类型化响应
```

`ResponseMeta` 字段：`status`、`headers`、`request_id`（从 `x-request-id` / `request-id` /
`x-amzn-requestid` 提取）、`raw_body`（**原始响应体**）；`WithMeta<T>` 可 `.into_data()`。

### 6.2 流式入口与 `futures::Stream`

| 入口 | 自动行为 |
| --- | --- |
| `client.chat().stream(req)` | `stream=true`；未设置 `stream_options` 时自动 `include_usage=true` |
| `client.responses().stream(req)` | `stream=true` |

`ProtocolStream` 实现 `futures_core::Stream<Item = Result<Event, AirxError>>`，
可直接使用 `TryStreamExt`、`select!`；也提供 `next()`、`meta()`、`request_id()`。

```rust
use futures_util::TryStreamExt;

let mut stream = client.chat().stream(request).await?;
println!("request_id={:?}", stream.request_id());
while let Some(event) = stream.try_next().await? {
    if let ChatStreamEvent::Chunk(chunk) = event {
        for choice in chunk.choices {
            if let Some(text) = choice.delta.content { print!("{text}"); }
        }
    }
}
```

一次性收尾会消费整个流：`final_response()` 返回完整响应，`final_text()` 返回文本；
Chat 与 Responses 的 `ProtocolStream` 都提供这两个方法。

### 6.3 Accumulator 与完成语义

| API | 完成条件 | 失败 / 截断 |
| --- | --- | --- |
| `ChatStreamAccumulator` | 收到 `ChatStreamEvent::Done`（`[DONE]`） | 未收到 → `Incomplete`；终态后继续 `apply` chunk → `InvalidState` |
| `ResponsesStreamAccumulator` | 收到 `response.completed` | `response.failed` / `error` → `Failed`；`response.incomplete` 或 EOF → `Incomplete` |

Chat 聚合器合并文本、工具参数、usage，并把 `reasoning_content` 合并到
`message.extensions.reasoning_content`；Responses 聚合器按 `output_index` 维护 item。
**完成 ≠ 未被截断**：Chat 的 `finish_reason == "length"` 仍可能收到 `[DONE]`；
Responses 的 `status` / `incomplete_details` 需要单独检查。

## 7. 错误与可观测

`ProviderError` 保留 `provider`、`status`（`Option<u16>`）、`code`、`message`、
`request_id`、`headers`、`raw_body`（原始 JSON）。`AirxError` 访问器：

| 方法 | 返回 | 说明 |
| --- | --- | --- |
| `http_status()` | `Option<u16>` | `Http` / `Provider` 变体 |
| `request_id()` / `response_headers()` | `Option<&str>` / `Option<&HeaderMap>` | 仅 `Provider` 变体 |
| `is_rate_limit()` | `bool` | 状态码 429 |
| `retry_after()` | `Option<Duration>` | 仅秒数形式，`Provider` 变体 |
| `is_retryable()` | `bool` | 中立判定：连接、超时、429、5xx |

```rust
if let Err(e) = client.chat().create(request).await {
    eprintln!("{:?} {:?} {} {:?}", e.http_status(), e.request_id(), e.is_rate_limit(), e.retry_after());
    if let AirxError::Provider(p) = &e { eprintln!("{:?} {:?}", p.code, p.raw_body); }
}
```

native 默认重试规则：

| 结果 | 是否重试 |
| --- | --- |
| 429 | 任意方法（含 POST），优先 `Retry-After`（秒） |
| 5xx / 超时 | 仅幂等方法（GET / HEAD / OPTIONS / TRACE / PUT / DELETE） |
| 连接失败 | 任意方法（请求未发出） |
| 取消 / 流式请求 | 不重试 |

WASM 目标默认关闭重试；显式启用会返回配置错误。

## 8. 模型列表

`GET /models` 与 `GET /models/{model}`，使用 `Authorization: Bearer`，**没有分页**。

```rust
let list = client.models().list().await?;
for model in &list.data {
    println!("{} owned_by={:?}", model.id, model.owned_by);
}
let model = client.models().retrieve("gpt-4o-mini").await?;
```

`list()` 返回 `ModelList { object, data, extensions }`；`retrieve(id)` 返回
`Model { id, object, created, owned_by, extensions }`。模型 id 只允许 ASCII 字母数字与
`- _ . : /`，非法值在编码阶段返回配置错误。

## 9. 文件 / 批处理 / Embeddings

这些接口都是普通 typed endpoint，复用同一客户端的认证、超时、重试与 `RequestOptions`。

### 9.1 Files

```rust
let file = openai.files()
    .upload("input.jsonl", "batch", bytes)
    .await?;

let page = openai.files()
    .list(ListFilesRequest::builder().purpose("batch").limit(20).build())
    .await?;
let meta = openai.files().retrieve(&file.id).await?;
let content = openai.files().content(&file.id).await?; // Bytes
openai.files().delete(&file.id).await?;
```

上传使用 `multipart/form-data`；`execute(HttpRequest)` 可绕过 typed 方法直接发任意文件接口。

### 9.2 Batches

```rust
let batch = openai.batches().create(
    CreateBatchRequest::builder()
        .input_file_id(&file.id)
        .endpoint("/v1/chat/completions")
        .completion_window("24h")
        .build()?,
).await?;

let page = openai.batches()
    .list(ListBatchesRequest { limit: Some(20), after: None })
    .await?;
let current = openai.batches().retrieve(&batch.id).await?;
openai.batches().cancel(&batch.id).await?;
```

结果文件通过 `batches().retrieve(&id)` 的 `output_file_id` / `error_file_id`，再用
`files().content(...)` 下载。

### 9.3 Embeddings

```rust
let response = openai.embeddings().create(
    EmbeddingRequest::builder()
        .model("text-embedding-3-small")
        .input(vec!["hello".to_owned(), "world".to_owned()])
        .dimensions(256)
        .build()?,
).await?;
```

### 9.4 Responses 生命周期与计 token

```rust
let responses = openai.responses();
let current = responses.retrieve("resp_...").await?;
responses.cancel("resp_...").await?;
responses.delete("resp_...").await?;

let items = responses
    .input_items(ListInputItemsRequest::new("resp_..."))
    .await?;

let tokens = responses.input_tokens_count(
    InputTokensCountRequest::builder()
        .model("gpt-5")
        .input_items(history)
        .build()?,
).await?;
```

## 10. 扩展点

### 10.1 `extensions` 透传未知字段

Chat 请求用 `.extension("logit_bias", serde_json::json!({"50256": -100}))` 透传未建模字段；
Responses 同理（如 `.extension("store", json!(true))`）。只用于**未建模字段**；不要与同名
typed 字段同时设置，否则序列化会出现重复键。

### 10.2 自定义 Endpoint 与 Transport

`OpenAi` 暴露 raw execute，复用客户端的认证、重试、超时、压缩与 `RequestOptions`：

```rust
use airx_core::http::HttpRequest;

let request = HttpRequest::get("https://api.openai.com/v1/models".parse()?);
let response = openai.execute(request).await?;
let raw = response.body;
```

需要自定义编码 / 解码时：

1. **复用已编码请求**：`endpoint().encode(&request)?` 得到带认证 Header 的 `HttpRequest`，
   交给自定义 `Transport` 执行，再用 `endpoint().decode(response)` 解码。
2. **自定义 `Endpoint`**：实现 `encode` / `decode`（流式再实现 `StreamingEndpoint`），
   自行构造 URL、认证 Header 与 body。

```rust
use airx::{Endpoint, Transport}; // trait 需在作用域内

let http = openai.chat().endpoint().encode(&request)?;
let response = my_transport.execute(http).await?;
let typed = openai.chat().endpoint().decode(response)?;
```

`OpenAiConfig::apply_headers` 仍是 crate-private；自定义 Endpoint 需要自己持有 key 并写入
Header。长期方案是在 `airx-protocol` 增加 typed endpoint，并在 `airx` 门面暴露入口。

### 10.3 OpenAI-Compatible 网关（DeepSeek 示例）

DeepSeek 不是独立协议，只是 OpenAI-Compatible 部署：

```rust
use airx_protocol::openai::chat::ResponseFormat;
let openai = Client::openai()
    .api_key(deepseek_key)
    .base_url("https://api.deepseek.com")
    .build()?;
let request = ChatCompletionRequest::builder()
    .model("deepseek-flash")
    .user("以 json 返回问题和答案，示例：{\"question\":\"q\",\"answer\":\"a\"}")
    .response_format(ResponseFormat::JsonObject)
    .max_tokens(512) // 兼容部署按网关文档选择 max_tokens / max_completion_tokens
    .build()?;
```

- DeepSeek 不支持 `json_schema`；JSON mode 使用 `response_format={"type":"json_object"}`，
  提示词必须包含 “json” 与示例。
- 网关额外 Header（如 OpenCode Zen 的 `x-opencode-session`）用执行级
  `ClientBuilder::default_headers` 注入；网关未实现的字段 / 工具需真实调用验证。

### 10.4 明确不提供 Agent / 工具循环

airx 不会轮询 `tool_calls`、不会执行函数、不会维护多轮会话，也不会自动把工具结果拼回
下一次请求。上面的 `assistant_tool_calls` + `tool_result` 由调用方显式构造；循环、
超时、并发与错误策略都属于应用层。

## 11. 最佳实践 / 常见坑

- **`max_tokens` 已弃用**：新代码使用 `max_completion_tokens`（含推理 token）；o-series 与 `max_tokens` 不兼容。
- **流式不重试**：429 / 5xx 在流式请求上直接返回错误。
- **整体超时会截断长流**：`ClientBuilder::timeout` 是总时长；长流通常只依赖连接超时。
- **Chat 聚合依赖 `[DONE]`**：网关省略 `[DONE]` 时 `finish()` 返回 `Incomplete`；可改为逐事件消费。
- **usage chunk 的 `choices` 为空**：开启 `include_usage` 后，最后一个 chunk 只带 usage。
- **`ResponseMeta.raw_body` 是原始 body**：类型化响应是解码结果；需要字节级保真时用它。
- **`Option<T>` 无法区分 `null` 与缺省**：需要 null 保真时使用 `RawJson` 或保留原始响应体。
- **`parse_as` 只做 serde**：schema / `response_format` / `text_format` 仍由调用方设置；空内容直接报 `Deserialization`。

## 12. API 速查表

| 类型 / 方法 | 一句话 |
| --- | --- |
| `Client::openai()` | 进入 `OpenAiBuilder`；`.api_key(...)` / `.base_url(...)` / `.build()?` |
| `.chat_path(...)` / `.responses_path(...)` | 构建时覆盖模式路径 |
| `OpenAi::chat()` / `chat_at(path)?` | Chat 入口；`_at` 仅本次覆盖路径 |
| `OpenAi::responses()` / `responses_at(path)?` | Responses 入口；`_at` 仅本次覆盖路径 |
| `OpenAi::models()` / `embeddings()` / `files()` / `batches()` | 模型 / 向量 / 文件 / 批处理入口 |
| `OpenAi::execute(HttpRequest)` | 自定义接口逃生通道，复用客户端配置 |
| `Chat::create / create_with_meta / parse_as::<T> / stream / with_options / endpoint` | Chat 全部 SDK 入口 |
| `Responses::create / retrieve / cancel / delete / input_items / input_tokens_count` | Responses 生命周期与计 token |
| `OpenAiModels::list()` / `retrieve(id)` | 模型列表（无分页）/ 单个模型 |
| `OpenAiFiles::upload / list / retrieve / delete / content` | 文件上传、列表、下载 |
| `OpenAiBatches::create / list / retrieve / cancel` | 批处理 |
| `OpenAiEmbeddings::create` | 向量 |
| `ChatCompletionRequest::builder()` / `ResponsesRequest::builder()` | 请求构建器；`build()` 校验必填字段 |
| `RequestOptions::new().try_header(k, v)?.idempotency_key(k).timeout(d).retry_policy(p)` | 单次 Header / 幂等键 / 超时 / 重试 |
| `WithMeta<T> { data, meta }` / `ResponseMeta` | 类型化数据 + HTTP 元数据（status / headers / request_id / raw_body） |
| `ProtocolStream::next()` / `final_text()` / `final_response()` | 逐事件读取 / 一次性收尾 |
| `ChatStreamAccumulator` / `ResponsesStreamAccumulator` | `apply` / `finish` / `snapshot` / `status` |
| `AirxError::request_id / is_rate_limit / retry_after / response_headers` | 错误可观测性 |

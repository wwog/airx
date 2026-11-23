# Anthropic 接入指南（Messages）

本文说明 airx 的 Anthropic Messages 接入方式、配置与边界。实现位于
`crates/airx/src/anthropic.rs` 与 `crates/airx-protocol/src/anthropic/`；能力表见
[protocol-support.md](protocol-support.md)，跨 Provider 约定见 [compatibility.md](compatibility.md)。
对齐基准：2026-11-23，参数以仓库当前实现为准。

## 1. 适用边界

airx 是**请求工具**：一次 `messages().create(...)` 拿到 `MessagesResponse`，就是 airx 的全部职责。

| airx 负责 | 边界之外（不提供） |
| --- | --- |
| 请求构造、URL / 认证 Header、JSON 编解码 | Agent 工具执行循环：只解析 `tool_use`，不执行、不循环 |
| 连接池、TLS、超时、重试、流式读取 | 会话 / 状态管理；Skills、Memory、Vector Store、Realtime、Fine-tuning |
| 响应 / SSE / 错误体解析、Accumulator、Files / Batches | Agent 编排、多轮工具结果回传策略 |
| `RequestOptions`、`ResponseMeta`、`futures::Stream`、`parse_as`、raw execute | 结构化输出只做 serde，不校验 schema / thinking 预算 |

服务端工具（`web_search` 等）由 Anthropic 后台联网：airx 只发送工具声明、解析
`server_tool_use` / `web_search_tool_result`，不代发、不代理任何本地网络请求。

## 2. 快速接入

默认 feature 已含 `anthropic` + `stream`。裁剪时保留最小集合（`airx-protocol` /
`airx-core` 版本需与 `airx` 对齐，用于内容块与 `Extensions`）：

```toml
[dependencies]
airx = { version = "0.1", default-features = false, features = ["native", "rustls-tls", "anthropic", "stream"] }
airx-protocol = "0.1"
airx-core = "0.1"
serde_json = "1"     # 示例中的 json! 宏
futures-util = "0.3" # 使用 TryStreamExt 时需要
```

| feature | 作用 |
| --- | --- |
| `anthropic` | 启用 `Client::anthropic()` 与 Messages / Models 端点 |
| `stream` | 启用 `messages().stream()`、`ProtocolStream`、`final_text()` / `final_response()` |
| `native` + `rustls-tls` | Reqwest 传输与 TLS 后端（与 `native-tls` 二选一） |

`Client::anthropic()` 返回构建器，`build()` 直接得到已定性的 `Anthropic` 客户端：

```rust,ignore
use airx::prelude::*;

let client = Client::anthropic().api_key("sk-ant-...").build()?;
let request = MessagesRequest::builder()
    .model("claude-sonnet-4-5").max_tokens(512).system("be concise").user("Hello")
    .build()?;
let response = client.messages().create(request).await?;
println!("{}", response.text());
```

最小流式调用（`stream()` 会自动把请求的 `stream` 置为 `true`）：

```rust,ignore
use futures_util::TryStreamExt;

let mut accumulator = MessagesStreamAccumulator::new();
let mut stream = client.messages().stream(request.clone()).await?;
while let Some(event) = stream.try_next().await? { accumulator.apply(event)?; }
let response = accumulator.finish()?;

let text = client.messages().stream(request).await?.final_text().await?; // 或一次性收尾
```

`airx::prelude` 已导出 `Client`、`MessagesRequest`、`MessagesResponse`、
`MessagesStreamAccumulator`、`ThinkingConfig`、`OutputEffort`、`JsonOutputFormat`、
`CountTokensRequest`、`AnthropicListModelsRequest`。内容块与工具需显式引入，例如
`use airx_protocol::anthropic::messages::{ContentBlock, Message, Role, TextBlock, Tool, ToolChoice, ToolResultBlock};`、
`use airx_core::json::Extensions;`、`serde_json::json!`。

## 3. 配置

### 3.1 地址、路径与版本

| 项 | 默认值 | 构建时覆盖 | 单次覆盖 |
| --- | --- | --- | --- |
| `base_url` | `https://api.anthropic.com`（不含 `/v1`） | `.base_url(...)` | — |
| Messages 路径 | `/v1/messages` | `.messages_path(...)` | `.messages_at(...)` |
| `api_version` | `2023-06-01`，写入 `anthropic-version` | `.api_version(...)` | `RequestOptions` 覆盖同名 Header |
| countTokens | `{messages_path}/count_tokens` | 跟随 Messages 路径 | 跟随 |
| Models | `{base_url}/v1/models` | 无覆盖入口 | 无 |

官方地址是默认值：`Client::anthropic().api_key(...)` 不需要 `base_url`，**只有兼容网关才
需要覆盖**。`base_url` 必须是绝对地址，末尾 `/` 会被去掉；路径可为相对路径（补前导 `/`
拼到 `base_url`）或完整 URL（忽略 `base_url`）。

```rust,ignore
use std::time::Duration;

let client = Client::builder()
    .connect_timeout(Duration::from_secs(5))   // 只在 Client::builder() 上提供
    .anthropic()
    .api_key("sk-ant-...")
    .base_url("https://gateway.example.com/anthropic")
    .messages_path("v1/messages")
    .api_version("2023-06-01")
    .timeout(Duration::from_secs(600))         // 整体超时，长流会被截断
    .retry_policy(RetryPolicy::standard())
    .build()?;

let messages = client.messages_at("/gateway/v1/messages")?; // 单次覆盖，客户端不变
```

- `count_tokens` 路径由当前 `Messages` 实例推导（`messages_at` 后调用同样跟随）；Models 路径
  固定为 `{base_url}/v1/models`，不跟随 `messages_path`，网关下 404 属预期。

### 3.2 API 版本与 beta Header

`api_version` 只影响 `anthropic-version` Header。Beta Header 无专用字段，三种注入方式：

```rust,ignore
use airx_core::http::{HeaderMap, HeaderName, HeaderValue};
use airx_protocol::anthropic::AnthropicConfig;

const BETA: &str = "context-management-2026-01-01";

// 1) 单次请求：优先级最高，覆盖同名 Header
let messages = client.messages().with_options(RequestOptions::new().try_header("anthropic-beta", BETA)?);

// 2) 执行级：只填充缺失 Header；协议层不写 anthropic-beta，因此不会被覆盖
let mut headers = HeaderMap::new();
headers.insert("anthropic-beta", HeaderValue::from_static(BETA));
let client = Client::builder().default_headers(headers).anthropic().api_key(key).build()?;

// 3) Provider 级：协议层 Config（门面构建器未暴露，用法见 8.3）
let config = AnthropicConfig::new(key).with_header(
    HeaderName::from_static("anthropic-beta"), HeaderValue::from_static(BETA),
);
```

Header 优先级（高到低）：单次 `RequestOptions` → Provider 配置 `default_headers` → 协议层
（`x-api-key`、`anthropic-version`、`content-type`）→ 执行级 `default_headers` → 缺省
`User-Agent: airx/<version>` 与 `Accept`。

### 3.3 超时与重试

| 项 | 默认 | 入口 |
| --- | --- | --- |
| 连接超时 | 10 秒（native） | `Client::builder().connect_timeout(...)` |
| 整体超时 | 关闭，避免截断长流 | `.timeout(...)`（两类构建器均有） |
| 重试 | native 2 次，500ms 起、8s 上限 | `.retry_policy(...)` / `RetryPolicy::disabled()` |

Provider 构建器只镜像 `transport` / `timeout` / `retry_policy`；`connect_timeout` 与
`default_headers` 必须写在 `.anthropic()` 之前。默认 `User-Agent: airx/<version>`；
WASM 默认关闭重试，显式启用会报配置错误。

### 3.4 单次请求选项

`RequestOptions` 只影响本次调用，不修改客户端，也作用于该 `Messages` 实例的 `count_tokens`：
`try_header("x-trace-id", "trace-1")` 追加 Header，`idempotency_key("idem-1")` 写
`Idempotency-Key`，`timeout(...)` / `retry_policy(...)` 覆盖客户端默认值；
组合后传给 `messages().with_options(...)`（beta Header 示例见 3.2）。

## 4. Messages 请求

### 4.1 构建器与字段

`build()` 返回 `Result<MessagesRequest, AirxError>`：缺少 `model`、缺少 `max_tokens`、
`messages` 为空都会得到 `AirxError::Configuration`。

| 构建器方法 | JSON 字段 | 说明 |
| --- | --- | --- |
| `.model(impl Into<ModelId>)` | `model` | 必填 |
| `.max_tokens(u32)` | `max_tokens` | 必填，无默认值 |
| `.system(impl Into<String>)` | `system` | 单个字符串，不是消息数组 |
| `.message(Message)` / `.user(str)` / `.assistant(str)` | `messages` | 可重复调用 |
| `.messages(iter)` | `messages` | 一次设置完整历史（`Vec<Message>`），适合 Agent 循环 |
| `.stop_sequences(iter)` | `stop_sequences` | 字符串数组 |
| `.tools(iter)` / `.tool_choice(ToolChoice)` | `tools` / `tool_choice` | 见 4.4 |
| `.thinking(ThinkingConfig)` | `thinking` | 见 4.2 |
| `.output_effort(OutputEffort)` | `output_config.effort` | `low` / `medium` / `high` / `xhigh` / `max` |
| `.output_format(JsonOutputFormat)` | `output_config.format` | `{"type":"json_schema","schema":...}` |
| `.stream(bool)` | `stream` | 一般不用手动设置，`stream()` 会自动置位 |
| `.extension(key, value)` | 原样透传 | 见 8.1 |

**`temperature` / `top_p` / `top_k` 已从最新 Messages API 移除**，airx 也没有对应字段。
推理投入用 `output_config.effort`，结构化输出用 `output_config.format`（可同时设置）：

```rust,ignore
let request = MessagesRequest::builder()
    .model("claude-sonnet-4-5").max_tokens(4096).user("Plan the migration.")
    .thinking(ThinkingConfig::enabled(2048)).output_effort(OutputEffort::High)
    .build()?;
```

### 4.2 thinking 的四种形态

| 形态 | 构造 | 序列化要点 |
| --- | --- | --- |
| 固定预算 | `ThinkingConfig::enabled(2048)` | `{"type":"enabled","budget_tokens":2048}` |
| 展示控制 | `ThinkingConfig::Enabled { budget_tokens, display: Some(ThinkingDisplay::Omitted) }` | `display`：`summarized` / `omitted` |
| 关闭 | `ThinkingConfig::disabled()` | `{"type":"disabled"}` |
| 工具间思考 | `ThinkingConfig::between_tools()` | `{"type":"between_tools"}` |
| 自适应 | `ThinkingConfig::adaptive()` | `{"type":"adaptive"}`，可带 `display` |

`budget_tokens` 官方要求不小于 1024 且小于 `max_tokens`；airx 不校验，违反时由 API
返回错误（`ProviderError`，通常 400）。

### 4.3 内容块

| 变体 | JSON `type` | 关键字段 |
| --- | --- | --- |
| `Text(TextBlock)` | `text` | `text` |
| `Image(ImageBlock)` | `image` | `source`（base64 / url，原样 `Value`） |
| `ToolUse(ToolUseBlock)` | `tool_use` | `id` / `name` / `input` |
| `ToolResult(ToolResultBlock)` | `tool_result` | `tool_use_id` / `content` / `is_error` |
| `Thinking(ThinkingBlock)` | `thinking` | `thinking` / `signature` |
| `RedactedThinking(RedactedThinkingBlock)` | `redacted_thinking` | `data` |
| `ServerToolUse(ServerToolUseBlock)` | `server_tool_use` | `id` / `name` / `input` / `caller` |
| `WebSearchToolResult(WebSearchToolResultBlock)` | `web_search_tool_result` | `tool_use_id` / `content` / `caller` |
| `Other(Value)` | 任意未知值 | 原样保留 |

每个具体块都带 `extensions`（未建模字段，双向保留），构造时用 `Extensions::new()` 或
`Default::default()`。未知 `type` 解码为 `Other(Value)`，再序列化仍写回原形状。

```rust,ignore
let request = MessagesRequest::builder()
    .model("claude-sonnet-4-5").max_tokens(1024)
    .message(Message::blocks(Role::User, vec![
        ContentBlock::Text(TextBlock { text: "Where is order 42?".into(), extensions: Extensions::new() }),
    ]))
    .build()?;
let response = client.messages().create(request).await?; // response.tool_uses() 取工具调用

// 下一轮：执行结果作为 user 消息里的 tool_result 块回传
let result = ContentBlock::ToolResult(ToolResultBlock {
    tool_use_id: "toolu_1".to_owned(), content: Some(json!("shipped")),
    is_error: None, extensions: Extensions::new(),
});
```

`Message::user` / `Message::assistant` 生成纯文本内容；需要块结构（图片、工具结果、
thinking）时用 `Message::blocks(role, blocks)`。多轮回传时不要丢弃 `signature`（thinking）
与 `redacted_thinking` 块。

### 4.4 客户端工具与服务端工具

| 类型 | 声明 | 执行者 | 解析结果 |
| --- | --- | --- | --- |
| 客户端工具 | `Tool::new(name, input_schema)` | 你的应用 | `ContentBlock::ToolUse` |
| 联网搜索 | `Tool::web_search()` | Anthropic 后台 | `ServerToolUse` + `WebSearchToolResult` |
| 其他服务端工具 | `Tool::raw(json!({"type": "web_fetch_20250910", "name": "web_fetch"}))` | Anthropic 后台 | `ServerToolUse`，未知结果块为 `Other(Value)` |

`Tool::web_search()` 默认声明 `web_search_20260318`；固定旧版本时构造
`Tool::WebSearch(WebSearchTool { kind: WebSearchKind::V20260209, ..Default::default() })`
（可选 `V20250305` / `V20260209` / `V20260318`），或用 `Tool::raw` 写完整对象。
airx 不代发网络请求，也不保证网关透传服务端工具；接入前用真实网关验证。

## 5. 响应与流式

### 5.1 非流式

| 方法 | 返回 | 说明 |
| --- | --- | --- |
| `create(request)` | `MessagesResponse` | 2xx 反序列化；非 2xx 映射为 `ProviderError` / `HttpError` |
| `create_with_meta(request)` | `WithMeta<MessagesResponse>` | 额外提供 status / headers / request_id / raw_body |
| `parse_as::<T>(request)` | `T` | 把 `response.text()` 当 JSON 做 serde 解析 |

`MessagesResponse` 字段：`id`、`type`、`role`、`model`、`content`、`stop_reason`、
`stop_sequence`、`usage`、`extensions`；`text()` 拼接文本块（不含 thinking），
`tool_uses()` 收集 `ToolUseBlock`。`create_with_meta` 返回
`WithMeta { data, meta }`，`parse_as::<T>` 把文本解析为 `T`（`T` 由调用方定义）。

`parse_as` 只做 serde：不校验 schema、不重试，空文本直接返回 `AirxError::Deserialization`；
schema 仍要通过 `output_format(JsonOutputFormat::new(schema))` 交给模型。

### 5.2 流式

`messages().stream(request)` 返回 `ProtocolStream<MessagesEndpoint>`：

- 实现 `futures_core::Stream`，`Item = Result<MessagesEvent, AirxError>`，可用
  `TryStreamExt`、`tokio::select!`；inherent `async fn next()` 的 `Ok(None)` 表示正常结束；
- `meta()` / `request_id()` 暴露建连元数据；`final_text()` / `final_response()` 内部驱动
  `MessagesStreamAccumulator` 一次性收尾。

```rust,ignore
use futures_util::TryStreamExt;

let mut accumulator = MessagesStreamAccumulator::new();
let mut stream = client.messages().stream(request).await?;
while let Some(event) = stream.try_next().await? {
    accumulator.apply(event)?;
    let _partial = accumulator.snapshot(); // 当前部分结果，用于增量展示
}
let response = accumulator.finish()?;      // 收到 message_stop 才算完成
```

| 事件 | 载荷 | 聚合器行为 |
| --- | --- | --- |
| `message_start` | message（id / model / usage 初值） | 初始化响应 |
| `content_block_start` | `index` + 初始块 | 建立块状态 |
| `content_block_delta` | `text_delta` / `input_json_delta` / `thinking_delta` / `signature_delta` | 按 index 追加 |
| `content_block_stop` | `index` | 标记块结束 |
| `message_delta` | `stop_reason` / `stop_sequence` / `usage.output_tokens` | 收尾字段，补缓存 token |
| `message_stop` | — | 状态置 `Completed` |
| `ping` | — | **协议层过滤**，不会出现在事件流里 |
| `error` | `error.type` / `error.message` | 端点直接返回 `AirxError::Provider`，流中断 |
| 未知 `type` | 原样 | `MessagesEvent::Unknown(Value)` |

`MessagesEvent` 仍保留 `Ping` / `Error` 变体（直接 decode 时可用），但 `ProtocolStream`
不产出 `ping`；`error` 以 `Err` 形式出现在 `try_next()`，`ProviderError.code` 为
`error.type`，`status` 为空（流内错误没有 HTTP 状态码）。

| `finish()` 的结束方式 | 结果 |
| --- | --- |
| 收到 `message_stop` | `Ok(MessagesResponse)` |
| 正常 EOF 但没收到 `message_stop` | `AirxError::Stream(Incomplete)` |
| 手动 `apply` 了 `MessagesEvent::Error`（error 事件） | `AirxError::Stream(Failed)`；SSE 未闭合时由 `ProtocolStream` 直接报 `Stream` |

工具参数增量在 `finish()` 时拼接并解析；非法 JSON 返回 `AirxError::Deserialization`，
不会静默变成字符串。`snapshot()` 用宽松模式，便于展示。

## 6. 计 token 与模型列表

### 6.1 count_tokens

`POST {messages_path}/count_tokens`，请求体不含 `max_tokens`：

```rust,ignore
let tokens = client.messages().count_tokens(CountTokensRequest::builder()
    .model("claude-sonnet-4-5").system("be brief").user("hi").build()?).await?;
println!("input_tokens={}", tokens.input_tokens);
```

请求字段：`model` / `messages` 必填（messages 非空），`system`、`tools`、`thinking`
可选（类型与 Messages 相同），未知字段走 `extensions`。响应为
`CountTokensResponse { input_tokens, extensions }`；路径跟随当前 `Messages` 实例的
Messages 路径（含 `messages_at` 覆盖），认证与版本 Header 相同。

### 6.2 模型列表

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `models().list(ListModelsRequest)` | `GET /v1/models` | 单页，cursor 分页 |
| `models().retrieve(id)` | `GET /v1/models/{id}` | 单个模型；id 含非法字符时本地报错 |
| `models().list_all(ListModelsRequest)` | 同上 | 自动跟随 `last_id` 翻页，返回 `Vec<Model>` |

请求由 `AnthropicListModelsRequest::builder().limit(20).after_id("...").build()` 构造；
`list_all` 传同一个请求即自动翻页。

| 位置 | 字段 |
| --- | --- |
| 请求 | `limit`、`after_id`、`before_id` |
| `ModelList` | `data`、`has_more`、`first_id`、`last_id`、`extensions`；`next_cursor()` 取下一页游标 |
| `Model` | `id`、`type`、`display_name`、`created_at`、`extensions` |

`list_all` 在 `has_more` 为真且游标前进时继续，游标不再变化即停止，避免死循环。

### 6.3 Files 与 Batches

```rust,ignore
let file = client.files()
    .upload("input.pdf", "application/pdf", bytes)
    .await?;
let page = client.files()
    .list(AnthropicListFilesRequest::builder().limit(20).build())
    .await?;
let content = client.files().content(&file.id).await?; // Bytes
client.files().delete(&file.id).await?;

let batch = client.batches().create(AnthropicCreateBatchRequest::new([
    BatchRequestItem { custom_id: "1".to_owned(), params: messages_request },
])).await?;
let current = client.batches().retrieve(&batch.id).await?;
let results = client.batches().results(&batch.id).await?; // JSONL Bytes
```

- Files 路径 `/v1/files`，上传为 multipart；Batches 路径跟随 `messages_path`：
  `{messages_path}/batches`。
- 结果以 JSONL 返回；逐行解析由调用方完成。
- Skills / Memory / Vector Store / Realtime / Fine-tuning 仍未封装。

## 7. 错误与可观测

| `AirxError` 变体 | 触发场景 |
| --- | --- |
| `Provider(Box<ProviderError>)` | 可识别的 Anthropic 错误体（含流内 `error` 事件） |
| `Http(HttpError)` | 非 2xx 且错误体无法解析，保留状态码与原始 body |
| `Transport(TransportError)` | 连接、TLS、超时、取消 |
| `Stream(StreamError)` | SSE 语法、UTF-8、事件未闭合、`Incomplete` / `Failed` |
| `Serialization` / `Deserialization` | 请求序列化、响应或事件反序列化失败 |
| `Configuration` | 缺少 / 空 api_key、非法 base_url 或路径、构建期校验失败 |

`ProviderError` 字段：`provider`、`status: Option<u16>`、`code: Option<String>`（Anthropic
`error.type`）、`message`、`request_id`（来自 `request-id` / `x-request-id`）、
`headers: HeaderMap`、`raw_body: Option<RawJson>`。

| `AirxError` 访问器 | 返回 | 备注 |
| --- | --- | --- |
| `http_status()` | `Option<u16>` | `Provider` / `Http` 均有值 |
| `is_retryable()` | `bool` | 中立判定：连接失败、超时、429、5xx |
| `request_id()` | `Option<&str>` | 仅 `Provider` 变体 |
| `is_rate_limit()` | `bool` | 状态码 429 |
| `retry_after()` | `Option<Duration>` | 仅秒数形式的 `Retry-After` |
| `response_headers()` | `Option<&HeaderMap>` | 仅 `Provider` 变体 |

重试规则（native，默认 2 次）：

| 结果 | 是否重试 |
| --- | --- |
| 连接失败（请求未发出） | 任意方法 |
| 超时 / 5xx | 仅幂等方法（GET / HEAD / OPTIONS / TRACE / PUT / DELETE） |
| 429 | 任意方法，含 POST；优先使用 `Retry-After`（秒） |
| 取消 / 流式请求 | 不重试 |

`messages().create` 是 POST：429 与连接失败会自动重试，**5xx 与超时不会**；更强保证需在应用层包一层，用 `request_id()` 对账。

## 8. 扩展点

### 8.1 extensions 透传

`MessagesRequest` / `CountTokensRequest` / `OutputConfig` / 各内容块的 `extensions` 都是
`BTreeMap<String, Value>`，通过 `#[serde(flatten)]` 双向保留。账号级或尚未建模的参数
（如 `service_tier`、`context_management`、`container`、`inference_geo`、`speed`、
`workspace_id`）**没有类型化字段**，一律走 extensions，语义与校验由官方负责：

```rust,ignore
let request = MessagesRequest::builder()
    .model("claude-sonnet-4-5").max_tokens(256).user("hi")
    .extension("service_tier", json!("auto"))
    .extension("context_management", json!({"edits": []}))
    .build()?;
request.extensions.insert("inference_geo".to_owned(), json!("us"));
```

响应中的未知字段同样进入 `extensions`，重新序列化不丢失。

### 8.2 未知工具与未知块

`Tool::raw(json!({...}))` 可声明任何服务端工具（`web_fetch`、`code_execution`、
`computer_use` 等），airx 不解释其语义；`ContentBlock::Other(Value)` 保留未知内容块，
既能读也能原样回写。

### 8.3 自定义 Endpoint 与 Transport

`Anthropic` 暴露 `execute(HttpRequest)` / `execute_with(HttpRequest, &RequestOptions)`，
复用认证、版本 Header、重试、超时与压缩；适合未封装接口。

需要完全自定义编码 / 解码时：

1. 实现 `airx_protocol::Endpoint`（`encode` / `decode`），流式再加 `StreamingEndpoint`
   的 `decode_event`，产出 / 消费 `HttpRequest` / `HttpResponse`；
2. 用 `airx_transport` 的 `ReqwestTransport`（或自实现 `Transport`）发送；流式场景自行
   组合 `SseDecoder` 与事件解码。

已有的 Messages 端点也可这样使用，从而让 `AnthropicConfig::with_header` 生效：

```rust,ignore
use airx::{Endpoint, Transport};
use airx_protocol::anthropic::messages::MessagesEndpoint;
use airx_transport::ReqwestTransport;

let transport = ReqwestTransport::builder().build()?;
let endpoint = MessagesEndpoint::new(config);   // config 含 beta Header
let http = endpoint.encode(&request)?;
let response = transport.execute(http).await?;
let data = endpoint.decode(response)?;
```

新增官方接口的正路是向 `airx-protocol` 贡献 typed endpoint（`Endpoint` + 请求 / 响应
类型），再在 `airx` 门面接客户端方法；airx 不提供“任意 JSON 塞进通用请求”的伪通用接口。

### 8.4 Anthropic-Compatible 部署

同一协议客户端 + `base_url` 即可接入兼容部署（DeepSeek 为例）；官方地址不需要 `base_url`：

```rust,ignore
let anthropic = Client::anthropic()
    .api_key(key).base_url("https://api.deepseek.com/anthropic").build()?;

let response = anthropic.messages().create(MessagesRequest::builder()
    .model("deepseek-flash").max_tokens(512)
    .system("你只输出 json。").user("以 json 返回 question / answer 两个字段")
    .build()?).await?;
```

兼容部署是否支持 thinking、工具、结构化输出由网关决定，airx 不内置 provider 专属策略。
完整示例见 [deepseek_json.rs](../crates/airx/examples/deepseek_json.rs)。

### 8.5 明确未封装

| 能力 | 现状 | 需要时怎么办 |
| --- | --- | --- |
| 工具循环 / Agent 编排 | 不提供 | 应用自己维护 `tool_use` → 执行 → `tool_result` |
| Skills / Memory / Vector Store / Realtime / Fine-tuning | 未封装 | 走原生 `Endpoint` + `Transport`，或换用对应 SDK |
| 服务端工具本地执行 | 不提供 | Anthropic 后台执行，airx 只声明与解析 |
| 采样参数 `temperature` / `top_p` / `top_k` | 已从 API 移除 | 用 `output_config.effort` |
| 流式重试 | 不提供 | 应用层重建请求，事件不可重放 |

## 9. 最佳实践 / 常见坑

1. **`max_tokens` 必填**：`build()` 会校验；流式请求同样需要。
2. **不要再传 `temperature` / `top_p` / `top_k`**：最新 API 不接受，airx 也没有字段。
3. **thinking 预算与 `max_tokens` 联动**：`budget_tokens` ≥ 1024 且小于 `max_tokens`，
   设得太小会被 API 拒绝，airx 不替你调整。
4. **流式不重试**：`stream()` 走 `execute_stream`，已消费事件无法重放；首事件前的重试
   尚未实现。
5. **整体 timeout 会截断长流**：默认关闭。设置 `.timeout(...)` 后长回答可能在中途被掐断，
   表现为 `Transport(Timeout)`；通常只调连接超时。
6. **`ping` 看不到、`error` 是 `Err`**：协议层过滤 `ping`；流内 `error` 由 `try_next()`
   直接返回 `AirxError::Provider`，此时 `status` 为空，只有 `code` / `message` / `raw_body`。
7. **缓存 token 可选**：`cache_creation_input_tokens` / `cache_read_input_tokens` 为
   `Option<u32>`，未启用缓存时为 `None`，不要当 0 参与计费展示。
8. **`count_tokens` 跟随 `messages_path`，`models` 不跟随**：网关下模型列表可能 404，
   属于路径设计而非 bug。
9. **`parse_as` 不做 schema 校验**：模型文本必须先是合法 JSON，空文本直接报
   `Deserialization`；结构化输出请同时设置 `output_format`。
10. **`Tool` 重名**：`airx::prelude` 的 `Tool` / `ToolChoice` 是 OpenAI Chat 的同名类型；
    Anthropic 的需显式 import（显式 import 会遮蔽 glob，不会冲突），建议用别名。

## 10. API 速查表

| 类型 / 方法 | 一句话 |
| --- | --- |
| `Client::anthropic()` | 进入 Anthropic 构建器（feature `anthropic`） |
| `AnthropicBuilder::{api_key, base_url, messages_path, api_version, transport, timeout, retry_policy, build}` | 构造可复用的 `Anthropic` |
| `Anthropic::{messages, messages_at, models, files, batches, config}` | 调用入口与配置查看 |
| `Anthropic::execute(HttpRequest)` | 自定义接口逃生通道 |
| `Messages::{create, create_with_meta, parse_as, stream, count_tokens, with_options, endpoint}` | Messages 全部能力 |
| `AnthropicModels::{list, retrieve, list_all, with_options}` | 模型列表与详情 |
| `AnthropicFiles::{upload, list, retrieve, delete, content}` | 文件上传、列表、下载 |
| `AnthropicBatches::{create, list, retrieve, cancel, results}` | 消息批处理与结果 |
| `MessagesRequest::builder()` | 请求构建器（`model` / `max_tokens` 必填） |
| `MessagesResponse::{text, tool_uses}` | 拼文本、取工具调用 |
| `MessagesEvent` | `message_start` / `content_block_*` / `message_delta` / `message_stop` / `error` / `Unknown` |
| `MessagesStreamAccumulator::{new, apply, snapshot, finish, status}` | 事件聚合与完成判定 |
| `ProtocolStream::{next, final_text, final_response, meta, request_id}` | 流消费与收尾 |
| `Tool::{new, with_description, web_search, raw}` | 自定义 / 服务端工具声明 |
| `ToolChoice::{Auto, Any, Tool { name }}` | 工具选择 |
| `ThinkingConfig::{enabled, adaptive, disabled, between_tools}` | thinking 形态 |
| `OutputEffort` / `JsonOutputFormat::new(schema)` | 推理投入 / 结构化输出 |
| `CountTokensRequest::builder()` | 计 token 请求（`model` + `messages` 必填） |
| `AnthropicListModelsRequest::builder()` / `ModelList::next_cursor()` | 模型分页 |
| `RequestOptions::{new, try_header, header, headers, idempotency_key, timeout, retry_policy}` | 单次请求选项 |
| `WithMeta::{data, meta, into_data}` | 响应 + HTTP 元数据 |

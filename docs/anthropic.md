# Anthropic 接入指南（Messages）

本文说明 airx 的 Anthropic Messages 接入方式、配置与边界。实现位于
`crates/airx/src/anthropic.rs` 与 `crates/airx-protocol/src/anthropic/`；能力表见
[protocol-support.md](protocol-support.md)，跨 Provider 约定见 [compatibility.md](compatibility.md)。
对齐基准：`@anthropic-ai/sdk@1.13.0`（TypeScript SDK）与官方文档快照 2026-11-24；
默认 `anthropic-version` 为 `2026-10-21`。

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
`CountTokensRequest`、`AnthropicListModelsRequest`、`RequestOptions`。内容块、工具、文档块、
停止原因与新增的请求辅助类型需显式引入，例如
`use airx_protocol::anthropic::messages::{CacheControl, ContentBlock, DocumentBlock, DocumentSource, Message, Role, StopReason, SystemPrompt, TextBlock, Tool, ToolChoice, ToolResultBlock};`、
`use airx_protocol::anthropic::models::ModelLifecycle;`、`use airx_core::json::Extensions;`、
`serde_json::json!`。

## 3. 配置

### 3.1 地址、路径与版本

| 项 | 默认值 | 构建时覆盖 | 单次覆盖 |
| --- | --- | --- | --- |
| `base_url` | `https://api.anthropic.com`（不含 `/v1`） | `.base_url(...)` | — |
| Messages 路径 | `/v1/messages` | `.messages_path(...)` | `.messages_at(...)` |
| `api_version` | `2026-10-21`，写入 `anthropic-version` | `.api_version(...)` | `RequestOptions` 覆盖同名 Header |
| countTokens | `{messages_path}/count_tokens` | 跟随 Messages 路径 | 跟随 |
| Models | `{base_url}/v1/models` | 无覆盖入口 | 无 |

官方地址是默认值：`Client::anthropic().api_key(...)` 不需要 `base_url`，**只有兼容网关才
需要覆盖**。`base_url` 必须是绝对地址，末尾 `/` 会被去掉；路径可为相对路径（补前导 `/`
拼到 `base_url`）或完整 URL（忽略 `base_url`）。

> **版本头是全局开关。** 默认 `2026-10-21` 覆盖 `output_config` / `thinking.display` /
> `stop_reason=refusal` 等字段；兼容网关（DeepSeek Anthropic 兼容端点等）未必接受该值，
> 接网关时请显式指定它支持的版本，例如 `.api_version("2023-06-01")`。

```rust,ignore
use std::time::Duration;

let client = Client::builder()
    .connect_timeout(Duration::from_secs(5))   // 只在 Client::builder() 上提供
    .anthropic()
    .api_key("sk-ant-...")
    .base_url("https://gateway.example.com/anthropic")
    .messages_path("v1/messages")
    .api_version("2023-06-01")                 // 网关场景显式固定，官方地址可省略
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

// 官方 beta 名，注意日期：context-management-2025-06-27。
const BETA: &str = "context-management-2025-06-27";

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

**GA 与 beta 的归属**：airx 只实现 GA 面。`context_management`、`compaction`、
`mcp_servers`、`fallbacks` 等都是 beta 字段，GA `MessageCreateParams` 里没有它们；
通过 `extensions` 透传时必须同时带对应的 beta Header，否则会被端点拒绝或忽略。
beta 字段的字段名与日期以官方 `anthropic-beta` 文档为准，不要照抄旧示例。

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

两个 Anthropic 专有 Header 有具名助手（避免记 Header 名，也避免写进 body）：

```rust,ignore
// 未绑定 Workspace 的 key 必须带 anthropic-workspace-id
let options = RequestOptions::new().workspace("wrkspc_011CZkZaBF1tNoB5wlCeusgy")?;
// 代表第三方发起请求时指定 anthropic-user-profile-id
let options = options.user_profile("profile_0123")?;
let response = client.messages().with_options(options).create(request).await?;
```

`workspace_id` / `user_profile_id` 是**请求头**，不是 body 字段：写进 `extensions`
会落到 JSON body 里而无效。同样的选项对 `files()` / `models()` / `batches()` 都可用。

## 4. Messages 请求

### 4.1 构建器与字段

`build()` 返回 `Result<MessagesRequest, AirxError>`：缺少 `model`、缺少 `max_tokens`、
`messages` 为空都会得到 `AirxError::Configuration`。

| 构建器方法 | JSON 字段 | 说明 |
| --- | --- | --- |
| `.model(impl Into<ModelId>)` | `model` | 必填 |
| `.max_tokens(u32)` | `max_tokens` | 必填，无默认值 |
| `.system(impl Into<SystemPrompt>)` | `system` | 字符串或文本块数组；`.system("x")` 仍可编译 |
| `.system_blocks(iter)` | `system` | 文本块数组，可给某块挂 `cache_control` |
| `.message(Message)` / `.user(str)` / `.assistant(str)` / `.system_message(str)` | `messages` | 可重复调用；`Role::System` 对话中途插入系统指令 |
| `.messages(iter)` | `messages` | 一次设置完整历史（`Vec<Message>`），适合 Agent 循环 |
| `.stop_sequences(iter)` | `stop_sequences` | 字符串数组 |
| `.tools(iter)` / `.tool_choice(ToolChoice)` | `tools` / `tool_choice` | 见 4.4 |
| `.thinking(ThinkingConfig)` | `thinking` | 见 4.2 |
| `.output_effort(OutputEffort)` | `output_config.effort` | `low` / `medium` / `high` / `xhigh` / `max` |
| `.output_format(JsonOutputFormat)` | `output_config.format` | `{"type":"json_schema","schema":...}` |
| `.cache_control(CacheControl)` | `cache_control` | 顶层自动缓存断点 |
| `.user_id(str)` / `.metadata(Metadata)` | `metadata.user_id` | 调用方用户标识 |
| `.service_tier(RequestServiceTier)` | `service_tier` | `auto` / `standard_only` |
| `.speed(Speed)` | `speed` | `standard` / `fast`（`fast` 可能需 beta Header） |
| `.inference_geo(str)` | `inference_geo` | 数据驻留区域 |
| `.diagnostics(DiagnosticsParam)` | `diagnostics.previous_message_id` | 缓存未命中诊断 |
| `.container(ContainerRef)` | `container` | 容器 ID 或 `ContainerParam`（skills） |
| `.stream(bool)` | `stream` | `stream()` 会自动置位；`create()` 拒绝 `true`（见 5.2） |
| `.extension(key, value)` | 原样透传 | 见 8.1；与已建模字段重名时 `build()` 报错 |

**`temperature` / `top_p` / `top_k` 已从最新 Messages API 移除**，airx 也没有对应字段。
推理投入用 `output_config.effort`，结构化输出用 `output_config.format`（可同时设置）：

```rust,ignore
let request = MessagesRequest::builder()
    .model("claude-sonnet-4-5").max_tokens(4096).user("Plan the migration.")
    .thinking(ThinkingConfig::enabled(2048)).output_effort(OutputEffort::High)
    .build()?;
```

`system` 官方类型是 `string | TextBlockParam[]`。字符串形式最简单；需要给 system
挂缓存断点、拆多块，或给某个块加 `citations` 时用块数组：

```rust,ignore
use airx_protocol::anthropic::messages::{CacheControl, CacheTtl, TextBlock};

let request = MessagesRequest::builder()
    .model("claude-sonnet-4-5").max_tokens(1024)
    .system_blocks([
        TextBlock::new("你是一个严谨的审阅者。"),
        TextBlock::new(LONG_RULES).cached(CacheControl::ephemeral().with_ttl(CacheTtl::OneHour)),
    ])
    .user("开始审阅")
    .build()?;
```

`Role::System` / `Message::system("...")` 用于在对话中间插入系统指令，与顶层
`system` 并存；`text()` 拼接响应文本不受影响。

结构化输出是请求参数 `output_config.format`，可以与流式入口 `messages().stream()` 组合
（流式取值见 5.2）。官方限制：它与 citations、assistant 预填充不兼容；切换
`output_config.format` 会使 prompt cache 失效，并额外注入一段 system prompt，增加输入 token。
airx 不拦截 `stream` 与该参数的组合，被网关或模型拒绝时按普通 `ProviderError` 返回。

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
| `Text(TextBlock)` | `text` | `text`（`cache_control` / `citations` 存于 `extensions`，用 `cached()` / `citations()`） |
| `Image(ImageBlock)` | `image` | `source`（base64 / url，原样 `Value`） |
| `Document(DocumentBlock)` | `document` | `source` / `title` / `context` / `citations` / `cache_control` |
| `ToolUse(ToolUseBlock)` | `tool_use` | `id` / `name` / `input` / `caller` / `toolset_name` |
| `ToolResult(ToolResultBlock)` | `tool_result` | `tool_use_id` / `content` / `is_error` / `toolset_name` |
| `Thinking(ThinkingBlock)` | `thinking` | `thinking` / `signature` |
| `RedactedThinking(RedactedThinkingBlock)` | `redacted_thinking` | `data` |
| `ServerToolUse(ServerToolUseBlock)` | `server_tool_use` | `id` / `name` / `input` / `caller` |
| `WebSearchToolResult(WebSearchToolResultBlock)` | `web_search_tool_result` | `tool_use_id` / `content` / `caller` |
| `Other(Value)` | 任意未知值 | 原样保留 |

`DocumentSource` 覆盖官方 5 种来源，未知来源落 `Other(Value)`：

| 来源 | 构造 | JSON |
| --- | --- | --- |
| base64 PDF | `DocumentSource::base64_pdf(data)` | `{"type":"base64","media_type":"application/pdf","data":...}` |
| 纯文本 | `DocumentSource::plain_text(text)` | `{"type":"text","media_type":"text/plain","data":...}` |
| URL | `DocumentSource::url(url)` | `{"type":"url","url":...}` |
| Files API | `DocumentSource::file(file_id)` | `{"type":"file","file_id":...}` |
| 内联内容 | `DocumentSource::Content { content }` | `{"type":"content","content":...}` |

每个具体块都带 `extensions`（未建模字段，双向保留），构造时用 `Extensions::new()` 或
`Default::default()`。未知 `type` 解码为 `Other(Value)`，再序列化仍写回原形状。

```rust,ignore
let request = MessagesRequest::builder()
    .model("claude-sonnet-4-5").max_tokens(1024)
    .message(Message::blocks(Role::User, vec![
        ContentBlock::Text(TextBlock::new("Where is order 42?")),
        ContentBlock::Document(DocumentBlock::new(DocumentSource::file("file_1"))),
    ]))
    .build()?;
let response = client.messages().create(request).await?; // response.tool_uses() 取工具调用

// 下一轮：执行结果作为 user 消息里的 tool_result 块回传
let result = ContentBlock::ToolResult(ToolResultBlock {
    tool_use_id: "toolu_1".to_owned(), content: Some(json!("shipped")),
    is_error: None, toolset_name: None, extensions: Extensions::new(),
});
```

`Message::user` / `Message::assistant` 生成纯文本内容；需要块结构（图片、文档、工具结果、
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

自定义工具的可选开关（均为链式方法，只作用于 `Tool::Client`）：

| 方法 | JSON 字段 | 用途 |
| --- | --- | --- |
| `.strict(true)` | `strict` | 严格模式：保证工具名与输入按 schema 校验（结构化输出的工具侧对偶） |
| `.cached(CacheControl)` | `cache_control` | 工具级缓存断点，大工具集场景复用 |
| `.input_examples([...])` | `input_examples` | 输入示例，提升调用准确率 |
| `.eager_input_streaming(true)` | `eager_input_streaming` | 参数细粒度流式增量 |
| `.defer_loading(true)` | `defer_loading` | 延迟加载，只在 tool search 返回 `tool_reference` 后注入 |
| `.allowed_callers(["direct"])` | `allowed_callers` | 限定调用方（`direct` / `code_execution_*`） |

`ToolChoice` 覆盖官方 4 个变体，并提供 `disable_parallel_tool_use`：

```rust,ignore
use airx_protocol::anthropic::messages::ToolChoice;

let choice = ToolChoice::auto();                       // {"type":"auto"}
let choice = ToolChoice::none();                       // {"type":"none"}，本次禁用工具
let choice = ToolChoice::any().disable_parallel_tool_use(true);   // 强制串行
let choice = ToolChoice::tool("lookup").disable_parallel_tool_use(true);
```

## 5. 响应与流式

### 5.1 非流式

| 方法 | 返回 | 说明 |
| --- | --- | --- |
| `create(request)` | `MessagesResponse` | 2xx 反序列化；非 2xx 映射为 `ProviderError` / `HttpError`；`stream: true` 直接报 `Configuration` |
| `create_with_meta(request)` | `WithMeta<MessagesResponse>` | 额外提供 status / headers / request_id / raw_body |
| `parse_as::<T>(request)` | `T` | 把 `response.text()` 当 JSON 做 serde 解析（**不做 schema 注入 / 校验**） |

`MessagesResponse` 字段：`id`、`type`、`role`、`model`、`content`、`stop_reason`、
`stop_sequence`、`stop_details`、`container`、`diagnostics`、`usage`、`extensions`；
`text()` 拼接文本块（不含 thinking），`tool_uses()` 收集 `ToolUseBlock`。
`create_with_meta` 返回 `WithMeta { data, meta }`，`parse_as::<T>` 把文本解析为 `T`
（`T` 由调用方定义）。

`stop_reason` 字段保留原始字符串（兼容旧代码），判定请用助手方法：

| 助手 | 含义 |
| --- | --- |
| `stop_reason()` | `Option<StopReason>` 枚举（`EndTurn` / `MaxTokens` / `StopSequence` / `ToolUse` / `PauseTurn` / `Refusal` / `ModelContextWindowExceeded` / `Other`） |
| `is_tool_use()` | 模型调用了工具 |
| `is_paused()` | `pause_turn`：服务端工具轮次被暂停，需把 content 原样回传续跑 |
| `is_refusal()` | 分类器拒绝；细类见 `stop_details().category` |
| `is_truncated()` | `max_tokens` 或 `model_context_window_exceeded` |
| `stop_details()` | `Option<&RefusalStopDetails>`（`category` / `explanation`） |
| `thinking_tokens()` | `usage.output_tokens_details.thinking_tokens`（按 output 计费） |
| `container_id()` | 容器 ID（续跑容器类工具时回传） |

airx 只告诉业务「为什么停了」，是否续跑由业务决定（见 8.5）。

**如何处理 `stop_reason`**（不做循环，只做判定）：

| 取值 | 业务动作 |
| --- | --- |
| `end_turn` | 正常完成，取 `text()` |
| `tool_use` | 取 `tool_uses()` → 自己执行 → `Message::blocks(Role::User, [ToolResultBlock])` → 再发一次 |
| `max_tokens` / `model_context_window_exceeded` | 视为截断：提高 `max_tokens`、裁剪历史，或分段继续 |
| `pause_turn` | 把 `response.content` 原样作为 assistant 消息回传续跑；需要时带上 `container_id()` |
| `stop_sequence` | 命中自定义停止序列，`stop_sequence` 字段给出具体串 |
| `refusal` | 读 `stop_details().category` / `explanation`，不要重试同一 prompt |

`parse_as` 只做 serde：不校验 schema、不重试，空文本直接返回 `AirxError::Deserialization`；
schema 仍要通过 `output_format(JsonOutputFormat::new(schema))` 交给模型。官方
`messages.parse` 会注入 schema 并返回 `parsed_output`，airx 的入口同名但语义更窄。

### 5.2 流式

`messages().stream(request)` 返回 `ProtocolStream<MessagesEndpoint>`：

- 实现 `futures_core::Stream`，`Item = Result<MessagesEvent, AirxError>`，可用
  `TryStreamExt`、`tokio::select!`；inherent `async fn next()` 的 `Ok(None)` 表示正常结束；
- `meta()` / `request_id()` 暴露建连元数据；`final_text()` / `final_response()` 内部驱动
  `MessagesStreamAccumulator` 一次性收尾。

流式结构化输出需要先累积完整文本（`final_text()`，或自行累积 `text_delta`）再
`serde_json::from_str`；`parse_as` 只用于非流式请求。

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
| `content_block_start` | `index` + 初始块 | 建立块状态，**保留块的原始 `extensions`** |
| `content_block_delta` | `text_delta` / `input_json_delta` / `thinking_delta` / `signature_delta` / `citations_delta` | 按 index 追加；citations 累积到文本块 |
| `content_block_stop` | `index` | 标记块结束 |
| `message_delta` | `stop_reason` / `stop_sequence` / `stop_details` / `container` / `usage` | 收尾字段；`usage` 是累计值，`input_tokens: null` 不再解码失败 |
| `message_stop` | — | 状态置 `Completed` |
| `ping` | — | **协议层过滤**，不会出现在事件流里 |
| `error` | `error.type` / `error.message` | 端点直接返回 `AirxError::Provider`，流中断 |
| 未知 `type` | 原样 | `MessagesEvent::Unknown(Value)` |

`MessageDelta` 现在是 `{stop_reason, stop_sequence, stop_details, container, extensions}`：
refusal 细类与容器 id 在流式路径上也能拿到，聚合结果与 `create()` 一致。

`MessagesEvent` 仍保留 `Ping` / `Error` 变体（直接 decode 时可用），但 `ProtocolStream`
不产出 `ping`；`error` 以 `Err` 形式出现在 `try_next()`，`ProviderError.code` 为
`error.type`，`status` 为空（流内错误没有 HTTP 状态码）。

| `finish()` 的结束方式 | 结果 |
| --- | --- |
| 收到 `message_stop` | `Ok(MessagesResponse)` |
| 正常 EOF 但没收到 `message_stop` | `AirxError::Stream(Incomplete)` |
| 手动 `apply` 了 `MessagesEvent::Error`（error 事件） | `AirxError::Stream(Failed)`；SSE 未闭合时由 `ProtocolStream` 直接报 `Stream` |

需要「部分结果 + 错误」时用 `finish_partial()`（聚合器，永不失败）或
`ProtocolStream::final_response_partial()`（返回 `(MessagesResponse, Option<AirxError>)`）：

```rust,ignore
let (partial, error) = client.messages().stream(request).await?
    .final_response_partial().await;
println!("已收到 {} 字，错误: {:?}", partial.text(), error);
```

工具参数增量在 `finish()` 时拼接并解析；非法 JSON 返回 `AirxError::Deserialization`，
不会静默变成字符串。`snapshot()` 用宽松模式，便于展示。

`create()` 收到 `stream: true` 会直接返回 `Configuration` 错误：把 SSE 响应体交给 JSON
解码只会得到难以定位的 `Deserialization`。

## 6. 计 token 与模型列表

### 6.1 count_tokens

`POST {messages_path}/count_tokens`，请求体不含 `max_tokens`：

```rust,ignore
let tokens = client.messages().count_tokens(CountTokensRequest::builder()
    .model("claude-sonnet-4-5").system("be brief").user("hi").build()?).await?;
println!("input_tokens={}", tokens.input_tokens);
```

请求字段对齐官方 11 个：`model` / `messages` 必填（messages 非空），`system`、`tools`、
`tool_choice`、`thinking`、`output_config`、`cache_control`、`speed` 可选（类型与 Messages
相同，`tool_choice` 与 `output_config` 会影响计数结果），`workspace_id` / `user_profile_id`
是 Header（见 3.4）。未知字段走 `extensions`，与已建模字段重名时 `build()` 报错。响应为
`CountTokensResponse { input_tokens, extensions }`；路径跟随当前 `Messages` 实例的
Messages 路径（含 `messages_at` 覆盖），认证与版本 Header 相同。

```rust,ignore
let tokens = client.messages().count_tokens(CountTokensRequest::builder()
    .model("claude-sonnet-4-5").system("be brief").user("hi")
    .tools([tool.clone()]).tool_choice(ToolChoice::auto())
    .output_effort(OutputEffort::High)
    .build()?).await?;
```

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
| 请求 | `limit`、`after_id`、`before_id`、`lifecycle`（`active` / `deprecated` / `retired`，最多 3 个） |
| `ModelList` | `data`、`has_more`、`first_id`、`last_id`、`extensions`；`next_cursor()` 取下一页游标 |
| `Model` | `id`、`type`、`display_name`、`created_at`、`lifecycle`、`max_tokens`、`max_input_tokens`、`capabilities`、`deprecated_at`、`retires_at`、`extensions` |

`lifecycle` 省略时官方返回 `active` + `deprecated`；要拿 `retired` 必须显式请求。
`list_all` 在 `has_more` 为真且游标前进时继续，游标不再变化即停止，避免死循环。

### 6.3 Files 与 Batches

```rust,ignore
let file = client.files()
    .upload("input.pdf", "application/pdf", bytes)
    .await?;
// 亦可指定过期时间（官方区间 3600–7776000 秒）：
let temp = client.files()
    .upload_with_expiry("input.pdf", "application/pdf", bytes, 3600)
    .await?;

// Files 分页是 page 游标，不是 after_id / before_id：
let mut page = client.files()
    .list(AnthropicListFilesRequest::builder().limit(20).build())
    .await?;
while let Some(cursor) = page.next_page() {
    page = client.files()
        .list(AnthropicListFilesRequest::builder().limit(20).page(cursor).build())
        .await?;
}

// 也可以一次按 ID 集合查询（与 page / limit 互斥）：
let selected = client.files()
    .list(AnthropicListFilesRequest::by_ids(["file_1", "file_2"]))
    .await?;

let content = client.files().content(&file.id).await?; // Bytes
let deleted = client.files().delete(&file.id).await?;   // {id, type}

let batch = client.batches().create(AnthropicCreateBatchRequest::new([
    BatchRequestItem { custom_id: "1".to_owned(), params: messages_request },
])).await?;
let current = client.batches().retrieve(&batch.id).await?;
let raw = client.batches().results(&batch.id).await?;          // JSONL Bytes（逃生通道）
let lines = client.batches().results_typed(&batch.id).await?;  // Vec<BatchResultLine>
client.batches().delete(&batch.id).await?;                     // DELETE，返回 {id, type}
```

- Files 路径 `/v1/files`，上传为 multipart；列表响应是 `{data, next_page}`，
  把 `next_page` 回填到 `page` 取下一页。`FileDeleteResponse` 只有 `{id, type}`，
  旧的自造 `deleted: bool` 已移除（官方从不返回）。
- `FileMetadata.expires_at` 与 `UploadFileRequest::with_expires_in_seconds` 对应官方
  `expires_in_seconds`；`AnthropicFiles::upload_request` 接受完整请求（自定义 boundary）。
- Batches 路径跟随 `messages_path`：`{messages_path}/batches`；删除为
  `DELETE {messages_path}/batches/{id}`。
- `results_typed` 逐行解析为 `BatchResultLine { custom_id, result }`，
  `BatchResult` 覆盖 `succeeded` / `errored` / `canceled` / `expired`；`errored.error`
  是官方 `ErrorResponse` 信封（`{type:"error", error:{...}, request_id}`），
  用 `error.error.code()` 取错误码。
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

`ProviderError` 字段：`provider`、`status: Option<u16>`、`code: Option<String>`（优先
`error.details.error_code` 如 `storage_quota_exceeded`，否则 Anthropic `error.type`）、
`message`、`request_id`（优先 `request-id` / `x-request-id` Header，缺失时回退到错误体
顶层的 `request_id`）、`headers: HeaderMap`、`raw_body: Option<RawJson>`。

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
`BTreeMap<String, Value>`，通过 `#[serde(flatten)]` 双向保留。尚未建模的 body 字段可以走
extensions，语义与校验由官方负责：

```rust,ignore
let request = MessagesRequest::builder()
    .model("claude-sonnet-4-5").max_tokens(256).user("hi")
    .extension("future_field", json!({"x": 1}))   // 仅限未建模字段
    .build()?;
```

三条规则：

1. **已建模字段用类型化入口。** `model` / `max_tokens` / `messages` / `system` /
   `output_config` / `stop_sequences` / `tools` / `tool_choice` / `thinking` /
   `cache_control` / `metadata` / `service_tier` / `speed` / `inference_geo` /
   `diagnostics` / `container` / `stream` 都不能再写进 `extensions`：`flatten` 会与结构体
   字段各写一次，产出重复 JSON 键。`build()` 会直接返回 `Configuration` 错误拦截这种写法。
2. **Header 参数不是 body 字段。** `workspace_id` / `user_profile_id` 用
   `RequestOptions::{workspace, user_profile}`（见 3.4），写进 extensions 无效。
3. **beta 字段要带 beta Header。** `context_management` / `compaction` / `mcp_servers`
   等是 beta 面字段，必须同时注入对应 `anthropic-beta`（见 3.2）。

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
    .api_key(key)
    .base_url("https://api.deepseek.com/anthropic")
    .api_version("2023-06-01")   // 网关未必接受 2026-10-21，显式固定它支持的版本
    .build()?;

let response = anthropic.messages().create(MessagesRequest::builder()
    .model("deepseek-flash").max_tokens(512)
    .system("你只输出 json。").user("以 json 返回 question / answer 两个字段")
    .build()?).await?;
```

兼容部署是否支持 thinking、工具、结构化输出、新版本头由网关决定，airx 不内置 provider
专属策略。完整示例见 [deepseek_json.rs](../crates/airx/examples/deepseek_json.rs)。

认证方式同样可切换：`AnthropicConfig::with_bearer_token(token)` 发送
`Authorization: Bearer <token>`（OAuth / WIF 短期 token），默认仍发 `x-api-key`；
门面构建器未暴露该入口，需要时用 8.3 的 Endpoint 直连方式。

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
   `Option<u32>`，未启用缓存时为 `None`，不要当 0 参与计费展示；
   `Usage::thinking_tokens()` 与 `Usage::cache_creation`（5m / 1h 拆分）用于成本归因。
8. **`count_tokens` 跟随 `messages_path`，`models` 不跟随**：网关下模型列表可能 404，
   属于路径设计而非 bug。
9. **`parse_as` 不做 schema 校验**：模型文本必须先是合法 JSON，空文本直接报
   `Deserialization`；结构化输出请同时设置 `output_format`。
10. **`Tool` 重名**：`airx::prelude` 的 `Tool` / `ToolChoice` 是 OpenAI Chat 的同名类型；
    Anthropic 的需显式 import（显式 import 会遮蔽 glob，不会冲突），建议用别名。
11. **`stop_reason` 必须被处理**：`pause_turn` 需把 assistant content 原样回传（用
    `is_paused()` / `container_id()` 判断与取容器），`refusal` 读 `stop_details().category`，
    `model_context_window_exceeded` 视为截断（`is_truncated()`）。
12. **`extensions` 不是万能口**：已建模字段走类型化入口，Header 参数走 `RequestOptions`，
    否则 `build()` 会报错或请求体出现重复键。

## 10. API 速查表

| 类型 / 方法 | 一句话 |
| --- | --- |
| `Client::anthropic()` | 进入 Anthropic 构建器（feature `anthropic`） |
| `AnthropicBuilder::{api_key, base_url, messages_path, api_version, transport, timeout, retry_policy, build}` | 构造可复用的 `Anthropic` |
| `Anthropic::{messages, messages_at, models, files, batches, config}` | 调用入口与配置查看 |
| `Anthropic::execute(HttpRequest)` | 自定义接口逃生通道 |
| `Messages::{create, create_with_meta, parse_as, stream, count_tokens, with_options, endpoint}` | Messages 全部能力 |
| `AnthropicModels::{list, retrieve, list_all, with_options}` | 模型列表与详情（含 `lifecycle` 过滤） |
| `AnthropicFiles::{upload, upload_with_expiry, upload_request, list, retrieve, delete, content}` | 文件上传、列表、下载 |
| `AnthropicBatches::{create, list, retrieve, cancel, delete, results, results_typed}` | 消息批处理与结果 |
| `MessagesRequest::builder()` | 请求构建器（`model` / `max_tokens` 必填） |
| `SystemPrompt` / `.system_blocks(...)` / `Role::System` | system 字符串或多块、对话中 system |
| `MessagesResponse::{text, tool_uses, stop_reason, is_tool_use, is_paused, is_refusal, is_truncated, stop_details, thinking_tokens, container_id}` | 文本、工具调用与停止原因判定 |
| `MessagesEvent` | `message_start` / `content_block_*` / `message_delta` / `message_stop` / `error` / `Unknown` |
| `MessagesStreamAccumulator::{new, apply, snapshot, finish, finish_partial, failure_message, status}` | 事件聚合与完成判定 |
| `ProtocolStream::{next, final_text, final_response, final_response_partial, meta, request_id}` | 流消费与收尾 |
| `Tool::{new, with_description, strict, cached, input_examples, eager_input_streaming, defer_loading, allowed_callers, web_search, raw}` | 自定义 / 服务端工具声明 |
| `ToolChoice::{auto, any, tool, none, disable_parallel_tool_use}` | 工具选择（含本次禁用工具） |
| `CacheControl::{ephemeral, with_ttl}` / `CacheTtl` | prompt caching 断点（5m / 1h） |
| `DocumentBlock` / `DocumentSource::{base64_pdf, plain_text, url, file}` | 文档内容块 |
| `ThinkingConfig::{enabled, adaptive, disabled, between_tools}` | thinking 形态 |
| `OutputEffort` / `JsonOutputFormat::new(schema)` | 推理投入 / 结构化输出 |
| `CountTokensRequest::builder()` | 计 token 请求（`model` + `messages` 必填） |
| `AnthropicListModelsRequest::builder()` / `ModelList::next_cursor()` | 模型分页 |
| `AnthropicListFilesRequest::{builder, by_ids}` / `FileList::next_page()` | 文件分页（`page` 游标） |
| `BatchResultLine` / `BatchResult` | 批处理结果（JSONL 逐行类型化） |
| `RequestOptions::{new, try_header, header, headers, workspace, user_profile, idempotency_key, timeout, retry_policy}` | 单次请求选项 |
| `WithMeta::{data, meta, into_data}` | 响应 + HTTP 元数据 |

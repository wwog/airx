# 兼容性与行为约定

本文记录跨 Provider 的显式约定，避免“看起来统一、实际有损”的隐式行为。

## 1. Null 与字段缺省

类型化模型使用 `Option<T>`，**无法区分 `null` 与字段缺省**：

- 请求：`None` 不序列化（符合各 API 的缺省语义）。
- 响应：`None` 不重新序列化；若原始响应显式给出 `null`，重新序列化后会变成缺省。
- 需要 null / 字节级保真时，使用 `RawJson`（`Box<RawValue>`）或直接保留原始响应体。

这是当前唯一的已知保真缺口，测试中以“语义等价（忽略 null）”断言。

## 2. 重试

native 默认 2 次（`RetryPolicy::standard()`），WASM 默认关闭；可用
`RetryPolicy::disabled()` 显式关闭。启用后按幂等性决策：

| 结果 | 重试条件 |
| --- | --- |
| 连接失败（Connect） | 任意方法（请求未发出） |
| 超时 | 仅幂等方法：GET / HEAD / OPTIONS / TRACE / PUT / DELETE |
| 429 | 任意方法（含 POST），优先使用 `Retry-After`（秒） |
| 5xx | 仅幂等方法 |
| 取消（Cancelled） | 不重试 |
| 流式请求 | 不重试 |

退避为指数增长（`initial × 2^n`，上限 `max_backoff`）。WASM 目标暂不支持自动重试，
显式启用会返回配置错误。默认连接超时 10 秒；整体超时默认关闭，避免截断长流。

## 3. Header 优先级

从高到低：

1. 单次请求 `RequestOptions`（覆盖同名 Header，可设置 `Idempotency-Key`）。
2. Provider 配置的 `default_headers`（覆盖同名协议 Header）。
3. 协议层写入（认证、`content-type`、Provider 版本 / Beta Header）。
4. 执行级 `default_headers`（**不覆盖**已存在的 Header）。
5. 缺省 `User-Agent: airx/<version>` 与按调用模式设置的 `Accept`。

敏感 Header（`authorization`、`x-api-key` 等）不会出现在日志中；错误结构保留
状态码、错误码、request id、响应头与原始响应体。

## 4. base_url 约定

Provider 是「协议 × 部署配置」，不是协议本身。同一协议客户端通过 `base_url` 指向不同部署。

| 协议形态 | 默认值 | 端点拼接 |
| --- | --- | --- |
| OpenAI | `https://api.openai.com/v1` | `{base}/chat/completions`、`{base}/responses` |
| Anthropic | `https://api.anthropic.com` | `{base}/v1/messages` |
| Gemini | `https://generativelanguage.googleapis.com/v1beta` | `{base}/models/{model}:generateContent`、`…:streamGenerateContent?alt=sse` |
| DeepSeek (OpenAI-Compatible) | `https://api.deepseek.com` | `{base}/chat/completions` |
| DeepSeek (Anthropic-Compatible) | `https://api.deepseek.com/anthropic` | `{base}/v1/messages` |

`base_url` 必须为绝对地址；末尾 `/` 会被去掉。使用网关时按上表保留或省略 `/v1`。

**官方地址是默认值**：`Client::openai().api_key(...)` / `Client::anthropic().api_key(...)`
无需传 `base_url`，只有接入三方兼容网关时才覆盖。

模式路径与 `base_url` 解耦，可独立覆盖（相对路径拼接到 `base_url`，完整 URL 直接使用）：

| Provider | 模式 | 默认路径 | 构建时覆盖 | 运行时覆盖 |
| --- | --- | --- | --- | --- |
| OpenAI | chat | `/chat/completions` | `.chat_path(...)` | `openai.chat_at(...)` |
| OpenAI | responses | `/responses` | `.responses_path(...)` | `openai.responses_at(...)` |
| Anthropic | messages | `/v1/messages` | `.messages_path(...)` | `anthropic.messages_at(...)` |

```rust
// 兼容网关：base_url + 各模式路径都可覆盖；build 后复用
let gateway = Client::openai()
    .api_key(key)
    .base_url("https://gateway.example.com/v1")
    .chat_path("v2/chat")                                // 相对路径
    .responses_path("https://other.example.com/respond") // 完整 URL
    .build()?;

gateway.chat().create(request).await?;                    // 复用构建时配置
gateway.chat_at("/override/chat").create(request).await?; // 仅本次覆盖
```

## 5. DeepSeek 兼容部署与 JSON mode

DeepSeek 没有独立协议模块，接入方式是“同一协议 + base_url”：

```rust
// OpenAI-Compatible：build() 返回可复用的 OpenAi 客户端
let openai = Client::openai()
    .api_key(key)
    .base_url("https://api.deepseek.com")
    .build()?;

// Anthropic-Compatible：build() 返回可复用的 Anthropic 客户端
let anthropic = Client::anthropic()
    .api_key(key)
    .base_url("https://api.deepseek.com/anthropic")
    .build()?;
```

JSON mode（官方指南）：

- `response_format = {"type": "json_object"}`；不支持 `json_schema`。
- 提示词必须包含 “json” 并给出输出示例。
- 非 JSON 内容会被替换为空格，可能出现空 content 但 `finish_reason=stop`。
- 设置足够的 `max_tokens`，空 content 时重试或兜底。
- 完整示例：[deepseek_json.rs](../crates/airx/examples/deepseek_json.rs)。

Anthropic-Compatible 部署请使用 Anthropic Native API。`thinking` /
`reasoning_effort` 通过 Chat 字段或 `extensions.thinking` 传入；DeepSeek 的
`temperature` 在 thinking 模式下无效、`tool_choice=required` 在 thinking 模式下被拒绝，
这些由调用方按提示与官方文档处理，airx 不再内置 provider 专属策略。

## 6. Gemini 行为约定

- **模型在路径中**：`gemini.generate_content(model)` 决定
  `/models/{model}:generateContent`；请求体不含 `model`。裸模型名会补 `models/`
  前缀，`models/…` / `tunedModels/…` / `publishers/…` 原样使用。
- **认证**：使用 `x-goog-api-key` Header，不把 key 放进 query，避免出现在日志与 URL 中。
- **流式**：请求 `alt=sse`；每个 SSE data 是一个 `GenerateContentResponse` 增量。
  没有 `[DONE]`：完成信号是候选的 `finishReason`，输入被拦截时是
  `promptFeedback.blockReason`；两者都没有时聚合器返回 `Incomplete`。
- **thinking**：`text` 片段可带 `thought: true`；`Content::text()` 只拼接非 thinking
  文本，`thought_text()` 单独提供。多轮工具调用需要回传 `thoughtSignature`（已建模）。
- **工具**：`functionCall.args` 在 Gemini 中是完整 JSON 对象（不是增量字符串）；
  聚合器按片段原样保留。服务端工具通过 `Tool::server` 声明，airx 不代发网络请求。
- **安全拦截**：HTTP 200 + `promptFeedback.blockReason` 是正常响应，不是错误；
  调用方应检查 `prompt_feedback` 与 `finish_reason`。
- **未知结构**：未知 `Part` / 字段保留为 `Part::Other` 或 `extensions`，重新序列化不丢失。
- **未封装**：Gemini Files、Batches、Vertex AI 专属参数、Live API（可用 `extensions`、
  `execute(HttpRequest)` 或原生 `Endpoint` 逃生通道）。

## 7. 服务端联网工具

服务端工具（OpenAI Responses `web_search`、Anthropic `web_search_*`、DeepSeek
`deepseek_web_search` / `deepseek_read_url`）的联网请求发生在 **Provider 后台**：

- airx 只负责声明工具、解析 `web_search_call` / `server_tool_use` / `web_search_tool_result` /
  `annotations` 等结果。
- SDK 不会为了这些工具发起任何本地 HTTP 请求，也不代理搜索结果。
- 客户端 function tool 相反：模型返回 `tool_calls`，由应用执行并回传结果。
- 网关是否透传服务端工具由网关决定，需用真实请求验证。

## 8. 参数随模型变化（2026-11 对齐）

- **Anthropic**：最新 Messages API 已移除 `temperature` / `top_p` / `top_k`。
  用 `output_config.effort` 控制推理投入，用 `output_config.format` 做结构化输出。
  `thinking` 支持 `enabled`（预算）/ `disabled` / `between_tools` / `adaptive`。
- **OpenAI Chat**：`max_tokens` 已标记 deprecated，且与 o-series 不兼容；
  新代码使用 `max_completion_tokens`（包含推理 token）。`reasoning_effort` 取值
  `none|minimal|low|medium|high|xhigh|max`（模型相关），`verbosity` 取值 `low|medium|high`。
- **OpenAI Responses**：使用 `max_output_tokens`；推理用 `reasoning.effort`；
  结构化输出用 `text.format`（`json_schema`）。
- **DeepSeek**：当前文档模型为 `deepseek-flash` / `deepseek-v4-pro`；
  `reasoning_effort` 取 `none|low|high|max`（`minimal→low`，`medium/xhigh→high`）；
  `response_format` 不支持 `json_schema`。
- **Gemini**：2.5 系列用 `thinkingConfig.thinkingBudget`（`-1` 动态、`0` 关闭），
  3 系列用 `thinkingLevel`；结构化输出用 `generationConfig.responseMimeType =
  application/json` + `responseSchema` / `responseJsonSchema`。采样参数
  （`temperature` / `topP` / `topK`）仍在 `generationConfig` 中。

参数对齐会随官方版本演进；发现模型行为变化时，优先更新本节与
[protocol-support.md](protocol-support.md)，再改代码。

## 9. SDK 体验层

在不引入统一请求模型的前提下，门面提供与官方 SDK 对齐的便利能力：

```rust
// 单次请求选项（Header / 幂等键 / timeout / retry 覆盖）
let chat = client.chat().with_options(
    RequestOptions::new()
        .try_header("x-trace-id", "trace-1")?
        .idempotency_key("idem-1")
        .timeout(Duration::from_secs(30))
        .retry_policy(RetryPolicy::disabled()),
);

// 响应元数据（status / headers / request id / raw body）
let response = chat.create_with_meta(request).await?;
println!("{} {}", response.meta.status, response.meta.request_id.unwrap_or_default());

// 结构化输出
let parsed: MyType = client.chat().parse_as(request).await?;

// 流：futures::Stream + 一次性收尾
use futures_util::TryStreamExt;
let mut stream = client.chat().stream(request).await?;
while let Some(event) = stream.try_next().await? { /* ... */ }
```

- `ProtocolStream` 实现 `futures_core::Stream`，可直接使用 `TryStreamExt`、`select!`。
- 各协议提供 `final_response()` / `final_text()`，内部驱动对应 Accumulator。
- 模型列表：`client.models().list() / retrieve()`；Anthropic / Gemini 支持 `list_all()`。
- 计 token：Anthropic `messages().count_tokens()`、Gemini `generate_content(m).count_tokens()`。
- 文件 / 批处理 / Embeddings：OpenAI `files/batches/embeddings`、Anthropic `files/batches`、
  Gemini `embed_content`；上传使用 multipart。
- 完整历史输入：Chat / Messages 的 `.messages(...)`、Gemini `.contents(...)`、
  Responses `.input_items(...)`。
- raw execute：`OpenAi/Anthropic/Gemini::execute(HttpRequest)` 复用认证、重试、超时与压缩。
- 结构化输出是请求参数（`response_format` / `text.format` / `output_config.format` /
  `responseSchema`），不是 feature；可与各协议的流式入口组合，流式需先累积完整文本再
  反序列化，没有增量类型化解析。
- `parse_as<T>` 只负责 serde 解析，且仅支持非流式请求；schema 仍由调用方通过各协议字段
  设置。流式要么用 `final_text()` / `final_response()` 收尾后 `serde_json::from_str`，
  要么自行累积 text delta 再解析。

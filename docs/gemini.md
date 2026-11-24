# Gemini 接入指南（generateContent）

> 面向 Google AI Gemini API 的原生客户端。本文与**当前代码**一致；协议清单见
> [protocol-support.md](protocol-support.md)，跨 Provider 约定见
> [compatibility.md](compatibility.md)，分层设计见 [architecture.md](architecture.md)。
> 代码片段假定位于返回 `Result<(), AirxError>` 的 async 函数中。

## 1. 适用边界

airx 负责单次请求的完整链路：请求构造 → JSON 编码 → HTTP 传输 → 响应解码 → SSE
增量解析 → 聚合 → 错误形状。它止步于“模型返回了什么”，不介入“拿到结果之后做什么”。

| airx 负责 | 边界之外（不提供） |
| --- | --- |
| 请求构造、模型路径、`alt=sse` | Agent 工具执行循环：只返回 `functionCall`，**不自动执行工具、不循环** |
| 官方 JSON 字段双向编解码、未知字段保留 | 会话 / 状态管理：多轮 `contents` 历史由调用方维护 |
| SSE、增量事件、`GenerateContentStreamAccumulator` | Vector Store / Corpora / Semantic Retrieval |
| 连接池、TLS、超时、重试、单次请求 Header | Live / Realtime API（WebSocket） |
| 状态码、响应头、request id、原始 body、Embeddings | Files 上传、Batches、Fine-tuning |
| Gemini `error` 错误体 → `ProviderError` | Vertex AI（端点与 OAuth 认证均不同） |
| `extensions` 透传、`Part::Other`、服务端工具声明 | — |

## 2. 快速接入

默认启用 `native + rustls-tls + all-providers + stream`；Gemini 开箱即用，裁剪时注意
`gemini` 与 `stream` 两个 feature。

| Feature | 作用 | 默认 |
| --- | --- | --- |
| `gemini` | Gemini 协议模块与 `Client::gemini()` | ✅（经 `all-providers`） |
| `stream` | `stream()`、`ProtocolStream`、`final_text()` | ✅ |
| `native` / `wasm` | Reqwest 或浏览器 Fetch；TLS 由 `rustls-tls` / `native-tls` 选一个，WASM 无自动重试 | ✅ native |

最小依赖：

```toml
[dependencies]
airx = { version = "0.1", default-features = false, features = ["native", "rustls-tls", "gemini", "stream"] }
airx-protocol = "0.1" # 直接使用 TextPart / FunctionResponsePart 等具体结构时
serde_json = "1"      # 示例中的 json! 宏
futures-util = "0.3"  # 使用 TryStreamExt 时需要
```

构建客户端（`Client::gemini()` 是静态入口，直接返回 `GeminiBuilder`；
没有 `client.gemini()?` 这种形态）：

```rust
use airx::prelude::*;

let gemini = Client::gemini().api_key(api_key).build()?;
```

非流式：

```rust
let response = gemini
    .generate_content("gemini-2.5-flash")
    .create(
        GenerateContentRequest::builder()
            .system("你是简洁的助手")
            .user("用一句话解释 Rust 所有权")
            .max_output_tokens(256)
            .build()?,
    )
    .await?;
println!("{} / {:?}", response.text(), response.finish_reason());
```

流式：

```rust
let mut stream = gemini
    .generate_content("gemini-2.5-flash")
    .stream(
        GenerateContentRequest::builder()
            .user("用三句话解释 Rust 所有权")
            .build()?,
    )
    .await?;

let mut accumulator = GenerateContentStreamAccumulator::new();
while let Some(chunk) = stream.next().await? {
    print!("{}", chunk.text());
    accumulator.apply(chunk)?;
}
// 没有 finishReason / blockReason 时返回 Incomplete
let response = accumulator.finish()?;
```

一次性收尾：`.stream(request).await?.final_text().await?`（内部驱动同一聚合器）。

## 3. 配置

### 3.1 地址、认证与路径

| 项 | 默认值 / 说明 |
| --- | --- |
| `base_url` | `https://generativelanguage.googleapis.com/v1beta`（含版本号；末尾 `/` 会被去掉） |
| 认证 | `x-goog-api-key: <key>`；不把 key 放进 URL / query |
| generateContent | `{base}/models/{model}:generateContent`（模型在路径中，请求体无 `model`） |
| 流式 | `{base}/models/{model}:streamGenerateContent?alt=sse` |

官方地址是默认值，只有接兼容网关时才覆盖（必须为绝对地址）：

```rust
let gemini = Client::gemini()
    .api_key(key)
    .base_url("https://gateway.example.com/gemini/v1beta")
    .build()?;
```

### 3.2 模型路径规范化

`generate_content(model)` / `models().get(model)` / `count_tokens` 共用同一套规则：

| 输入 | 解析结果 |
| --- | --- |
| `gemini-2.5-flash` | `models/gemini-2.5-flash` |
| `models/gemini-2.5-flash` | 原样保留 |
| `tunedModels/my-model` | 原样保留 |
| `publishers/google/models/gemini-2.5-flash` | 原样保留 |
| 空串、`../x`、`models//x`、`gemini:generateContent`、中文 | `AirxError::Configuration` |

只允许 ASCII 字母数字与 `-` `_` `.` `/`，每段非空且不能是 `.` / `..`。
校验发生在编码阶段：`generate_content(model)` 只保存 `ModelId`，非法值在
`create` / `stream` / `count_tokens` / `models().get` 时才报错。

### 3.3 超时与重试

| 项 | 默认 | 设置 |
| --- | --- | --- |
| 连接超时 | 10 秒 | `Client::builder().connect_timeout(...)` |
| 整体超时 | 关闭 | `Client::builder().timeout(...)`；会截断长流 |
| 重试 | native 2 次，500ms 起、指数退避、单次上限 8s | `retry_policy(RetryPolicy::standard())` / `disabled()` |
| WASM 重试 | 关闭；显式启用报配置错误 | — |

`GeminiBuilder` 直接暴露 `transport` / `timeout` / `retry_policy`；
`connect_timeout` 与执行级 Header 要从 `Client::builder()` 进入：

```rust
let gemini = Client::builder()
    .connect_timeout(Duration::from_secs(5))
    .retry_policy(RetryPolicy::standard().with_max_retries(3))
    .gemini()
    .api_key(key)
    .build()?;
```

### 3.4 单次请求选项

```rust
let call = gemini.generate_content("gemini-2.5-flash").with_options(
    RequestOptions::new()
        .try_header("x-trace-id", "trace-1")?
        .idempotency_key("idem-1"),
);
```

`models()` 也支持 `with_options`。Header 优先级（高 → 低）：单次 `RequestOptions` →
Provider 级默认 Header（`GeminiConfig::with_header`，需直接用 `airx-protocol` 构造端点）→
协议层（`x-goog-api-key`、`content-type`）→ 执行级 `default_headers`（只填充缺失值）→
`User-Agent` / `Accept`。`Idempotency-Key` 原样发送；当前重试判定按 HTTP 方法进行，
不读取该 Header。

## 4. generateContent 请求

### 4.1 构建器

| 方法 | 写入 |
| --- | --- |
| `.content(Content)` | 追加一轮 `contents`（role + parts） |
| `.user(text)` / `.model(text)` | 追加 user / model 文本轮 |
| `.part(Part)` | 追加片段到最后一轮；没有内容时新建 user 轮 |
| `.system(text)` | `systemInstruction`（不带 role） |
| `.tools(...)` / `.tool(Tool)` | `tools` |
| `.tool_config(ToolConfig)` | `toolConfig` |
| `.generation_config(...)` | 整体覆盖 `generationConfig`（可直接 `GenerationConfig::default()` 构造） |
| `.temperature` / `.max_output_tokens` / `.thinking` | `generationConfig` 常用快捷方式 |
| `.json_schema(Value)` | `responseMimeType=application/json` + `responseSchema` |
| `.safety_setting(SafetySetting)` | 追加 `safetySettings` |
| `.cached_content(name)` | 引用已存在的缓存资源名 |
| `.build()` | `contents` 为空时报 `Configuration` |

### 4.2 Part 类型

Gemini 的 `Part` 没有判别字段，**JSON 中出现的键决定变体**：

| 变体 | 构造 | JSON 形状 |
| --- | --- | --- |
| 文本 | `Part::text` / `Part::thought` | `{"text": ...}`，可带 `thought`、`thoughtSignature` |
| 内联二进制 | `Part::inline_image(mime, base64)` | `{"inlineData": {"mimeType","data"}}` |
| 文件引用 | `Part::file(mime, uri)` | `{"fileData": {"mimeType","fileUri"}}` |
| 工具调用 | `Part::function_call(name, args)` | `{"functionCall": {"name","args"}}` |
| 工具结果 | `Part::function_response(name, response)` | `{"functionResponse": {...}}` |
| 代码 / 结果 | `Part::ExecutableCode(..)` / `Part::CodeExecutionResult(..)` | `{"executableCode": {...}}` / `{"codeExecutionResult": {...}}` |
| 未知片段 | `Part::Other(Value)` | 原样透传，重新序列化不丢失 |

`thoughtSignature` 在 `TextPart` / `FunctionCallPart` 上，多轮工具调用需回传（§9）。
`TextPart` / `ExecutableCodePart` / `CodeExecutionResultPart` 不在 prelude；
通常从响应读取，手工构造需直接依赖 `airx-protocol`。

### 4.3 工具

```rust
let weather = GeminiTool::function(
    FunctionDeclaration::new("get_weather")
        .with_description("查询城市天气")
        .with_parameters(json!({
            "type": "object",
            "properties": { "city": { "type": "string" } },
            "required": ["city"]
        })),
);

let request = GenerateContentRequest::builder()
    .user("北京今天天气如何？")
    .tool(weather)
    .tool_config(ToolConfig::new(
        FunctionCallingConfig::mode(FunctionCallingMode::Auto),
    ))
    .build()?;
```

`FunctionCallingMode` 序列化为 `AUTO` / `ANY` / `NONE`；`ANY` 可再 `.allow(["get_weather"])`
限定函数名。服务端工具用 `GeminiTool::server("googleSearch", json!({}))` 声明
（`codeExecution` / `urlContext` 同理），只声明，airx 不代发网络请求。

### 4.4 generationConfig

| 字段 | 说明 |
| --- | --- |
| `temperature` / `top_p` / `top_k` | 采样 |
| `candidate_count` / `max_output_tokens` | 候选数、输出上限（含 thinking） |
| `stop_sequences` | 停止序列 |
| `response_mime_type` | `text/plain` / `application/json` |
| `response_schema` | OpenAPI 子集 Schema（兼容性最好） |
| `response_json_schema` | 标准 JSON Schema（支持情况以官方为准） |
| `presence_penalty` / `frequency_penalty` / `seed` | 惩罚与随机种子 |
| `response_modalities` | 如 `["TEXT"]`、`["TEXT","IMAGE"]` |
| `media_resolution` / `speech_config` / `image_config` | 前者字符串，后两者 `Value` 原样保留 |
| `thinking_config` | 见 §4.5 |
| `extensions` | 未建模字段 |

### 4.5 thinking

| 构造 | 效果 |
| --- | --- |
| `GeminiThinkingConfig::budget(n)` | `thinkingBudget = n`，并设 `includeThoughts = true` |
| `GeminiThinkingConfig::dynamic()` | `budget(-1)`，动态预算 |
| `GeminiThinkingConfig::disabled()` | `thinkingBudget = 0`，不设 `includeThoughts` |
| `GeminiThinkingConfig::level("low" \| "high")` | `thinkingLevel`（Gemini 3），并设 `includeThoughts = true` |

```rust
.thinking(GeminiThinkingConfig::budget(-1))   // Gemini 2.5：动态 / 0 关闭 / n 为上限
.thinking(GeminiThinkingConfig::level("high")) // Gemini 3：等级
```

读取侧：`response.thought_text()` 只取 thinking 片段，`response.text()` 只拼接可见文本。

### 4.6 结构化输出

```rust
let request = GenerateContentRequest::builder()
    .user("总结这篇文章：……")
    .json_schema(json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "bullets": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["title", "bullets"]
    }))
    .build()?;

let summary: Summary = gemini
    .generate_content("gemini-2.5-flash")
    .parse_as(request)
    .await?;
```

`parse_as::<T>` 只对首个候选的可见文本做 serde 解析，**不设置 schema、不修复内容**；
被拦截或空内容返回 `AirxError::Deserialization`。需要 `responseJsonSchema` 时直接写
`GenerationConfig { response_mime_type: Some("application/json".into()), response_json_schema: Some(schema), ..Default::default() }`。

schema 本身是请求参数（`generationConfig.responseMimeType` + `responseSchema` / `responseJsonSchema`），
`:streamGenerateContent` 同样适用；流式片段是部分 JSON 字符串，必须累积后再解析。注意
`minLength`、`maxLength`、`uniqueItems` 等约束在受限解码时服务端并不强制，客户端仍需自行校验。

## 5. 响应与流式

### 5.1 三种取值方式

| 方法 | 返回 | 用途 |
| --- | --- | --- |
| `.create(request)` | `GenerateContentResponse` | 只要协议数据 |
| `.create_with_meta(request)` | `WithMeta<GenerateContentResponse>` | 附带 `status` / `headers` / `request_id` / `raw_body` |
| `.parse_as::<T>(request)` | `T` | 首个候选文本解析为 JSON |

```rust
let with_meta = gemini.generate_content("gemini-2.5-flash").create_with_meta(request).await?;
println!("{} {:?}", with_meta.meta.status, with_meta.meta.request_id);
let response = with_meta.data;
```

### 5.2 响应字段

| 字段 | 说明 |
| --- | --- |
| `candidates: Vec<Candidate>` | 候选；安全拦截时可能为空 |
| `prompt_feedback: Option<PromptFeedback>` | `block_reason` + `safety_ratings` |
| `usage_metadata: Option<UsageMetadata>` | prompt / candidates / total / thoughts / cached / tool_use_prompt，另有未类型化的 details |
| `model_version` / `response_id` | 实际模型版本、部分模型返回的响应 ID |
| `extensions` | 未知顶层字段，原样保留 |

`Candidate`：`index`、`content`、`finish_reason`（`STOP` / `MAX_TOKENS` / `SAFETY`…）、
`finish_message`、`safety_ratings`、`token_count`、`avg_logprobs`，以及未类型化且原样保留的
`citation_metadata` / `grounding_metadata` / `url_context_metadata` / `logprobs_result`；
未知字段进入自身 `extensions`。

便捷读取：`response.text()`（可见文本）、`response.thought_text()`、`response.finish_reason()`、
`response.first_candidate()`、`response.function_calls()`；`Content` 上有同类方法。

### 5.3 流式语义

- 请求带 `?alt=sse` 与 `Accept: text/event-stream`；每个 `data:` 是一个
  `GenerateContentResponse` 增量。**没有 `[DONE]` 哨兵**，空 `data` 被忽略；
  `data` 中出现顶层 `error` 立即返回 `AirxError::Provider`（无 status / headers）。
- 完成信号：候选 `finishReason` 非空且不是 `FINISH_REASON_UNSPECIFIED`，
  或输入被拦截的 `promptFeedback.blockReason`；两者都没有时 `finish()` 返回
  `AirxError::Stream`（`Incomplete`），截断不会伪装成成功。
- 非 2xx：`stream()` 先读完 body 再解码，**直接返回 `Err`**，不会返回“正常流”。

| API | 说明 |
| --- | --- |
| `ProtocolStream::next().await` / `impl futures_core::Stream` | 固有方法返回 `Result<Option<GenerateContentResponse>, AirxError>`；也可用 `TryStreamExt::try_next`、`select!` |
| `meta()` / `request_id()` | 建立连接时的状态码、响应头、request id |
| `final_response()` / `final_text()` | 消费整条流并返回聚合结果 |

`GenerateContentStreamAccumulator`：`new()` → `apply(response)` → `finish()`；
`snapshot()` 可取部分结果且不要求完成，`status()` 返回 `airx-core` 的 `StreamStatus`。
文本按 thought 标记分段拼接，`functionCall` 原样追加；终态后只吸收 usage 与扩展字段。

结构化输出的流式取值方式与普通文本相同（`final_text()` + serde），不要逐片段反序列化。

## 6. 计 token、模型列表与 Embeddings

### 6.1 countTokens

```rust
let tokens = gemini
    .generate_content("gemini-2.5-flash")
    .count_tokens(
        GeminiCountTokensRequest::builder()
            .system("be brief")
            .user("hi")
            .build()?,
    )
    .await?;
println!("{}", tokens.total_tokens);
```

路径为 `POST {base}/models/{model}:countTokens`，模型同样在路径中。请求字段：
`contents` / `systemInstruction` / `tools` / `generationConfig`；响应字段：
`total_tokens`、`cached_content_token_count`、`prompt_tokens_details`（`Value`）。

### 6.2 模型列表

```rust
let page = gemini
    .models()
    .list(GeminiListModelsRequest::builder().page_size(20).build())
    .await?;
for model in &page.models {
    println!("{}", model.name);
}

let all = gemini
    .models()
    .list_all(GeminiListModelsRequest::builder().page_size(50).build())
    .await?;
```

| API | 路径 | 说明 |
| --- | --- | --- |
| `models().list(request)` | `GET /models?pageSize=&pageToken=` | 一页，`next_cursor()` 取游标 |
| `models().get(model)` | `GET /models/{model}` | 单个模型 |
| `models().list_all(request)` | 同上，循环翻页 | 自动翻页，直到无游标或游标重复 |

`Model` 字段：`name`、`base_model_id`、`version`、`display_name`、`description`、
`input_token_limit`、`output_token_limit`、`supported_generation_methods`、
`temperature` / `top_p` / `top_k`、`extensions`。

### 6.3 Embeddings

```rust
let embeddings = gemini.embed_content("gemini-embedding-001");

let response = embeddings.create(
    EmbedContentRequest::builder()
        .text("hello")
        .task_type("RETRIEVAL_QUERY")
        .output_dimensionality(256)
        .build()?,
).await?;

let batch = embeddings.batch([
    EmbedContentRequest::builder().text("a").build()?,
    EmbedContentRequest::builder().text("b").build()?,
]).await?;
```

路径为 `POST {base}/{model}:embedContent` 与 `:batchEmbedContents`；批量请求会自动为每条
item 写入 `model`。

## 7. 错误与可观测

Gemini 错误体 `{"error": {"code", "message", "status", "details"}}` 映射为 `ProviderError`：

| 来源 | 去向 |
| --- | --- |
| HTTP 状态码 | `ProviderError.status` |
| `error.status`（如 `INVALID_ARGUMENT`） | `ProviderError.code`；缺失时回退为 `error.code` 字符串 |
| `error.message` | `ProviderError.message` |
| `error.details` | 不单独建模，保留在 `raw_body` |
| `x-goog-request-id` / `x-request-id` | `ProviderError.request_id` |
| 非 2xx 响应头 | `ProviderError.headers`（流内错误没有） |

非 JSON 错误体退化为 `AirxError::Http`（保留状态码与原始 body）。流内错误对象同样是
`AirxError::Provider`，但没有 status 与 headers。

`AirxError` 访问器：`http_status()`、`request_id()`、`is_rate_limit()`、
`retry_after()`（`Retry-After` 仅支持秒数）、`response_headers()`、`is_retryable()`；
`request_id` / `retry_after` / `response_headers` 只在 `Provider` 变体有值。

重试规则（native，`RetryPolicy::standard()` 为 2 次）：

| 情况 | 非流式 generateContent（POST） | `models()`（GET） |
| --- | --- | --- |
| 连接失败（请求未发出） | 重试 | 重试 |
| 429 | 重试，优先使用 `Retry-After` | 同左 |
| 5xx | 不重试（POST 非幂等） | 重试 |
| 超时 | 不重试 | 重试 |
| 流式请求 | 从不重试 | — |

## 8. 扩展点

- **extensions**：请求 / 响应 / 候选 / `Content` / `GenerationConfig` / `Tool` 等都有
  `extensions`，经 `#[serde(flatten)]` 双向保留；请求侧用 `.extension(key, json!(...))`。
  未类型化字段（grounding / citation / logprobs / `error.details`）自行解析 `Value`。
- **Part::Other**：未知片段原样保留，重新序列化不丢失。
- **服务端工具**：`GeminiTool::server(...)` 原样写入 `tools`；联网与代码执行发生在
  Provider 后台，airx 只解析结果（`executableCode` / `codeExecutionResult` 为强类型，
  grounding / citation 为 `Value`），**不发本地 HTTP 请求**。
- **自定义 Endpoint + Transport**：`Gemini` 暴露 `execute(HttpRequest)` /
  `execute_with(HttpRequest, &RequestOptions)`，复用认证、重试、超时与压缩。
  需要自定义编码 / 解码时，先 `use airx::Endpoint;`，再用
  `endpoint.encode(&request)?` → 自定义 `Transport::execute` → `endpoint.decode(raw)?`；
  后者会绕过执行级 Header 与内置重试。自定义 Transport 需直接依赖
  `airx-core` / `airx-transport`；流式未建模接口需自己读 SSE，或在 `airx-protocol`
  增加 typed endpoint 并暴露客户端方法。

| 未封装 | 现状与替代路径 |
| --- | --- |
| cachedContents CRUD | 可用 `cached_content("cachedContents/...")` 引用已有缓存；创建 / 读取 / 删除需自建 Endpoint |
| Files API | 可用 `Part::file(mime, fileUri)` 引用已有文件、`Part::inline_image` 传 base64；上传 / 列举未封装 |
| Batches | 未封装，自建 Endpoint |
| Vertex AI | 端点与认证不同（OAuth），`x-goog-api-key` 不适用 |
| Live / Realtime、Corpora / Semantic Retrieval | 不在 Transport / 本库范围，边界之外 |

## 9. 最佳实践 / 常见坑

| 坑 | 做法 |
| --- | --- |
| 模型必须进路径 | `generate_content(model)` 决定 URL，请求体无 `model`；没有 `*_at` 式覆盖，且非法字符在 `create` / `stream` / `count_tokens` 时才报 `Configuration` |
| 流式没有 `[DONE]` | 不要等哨兵；用 `finish()` / `final_text()`，提前 EOF 得到 `Incomplete` |
| `FINISH_REASON_UNSPECIFIED` 不是完成 | 聚合器不把它当成功；只在 `STOP` / `MAX_TOKENS` / `SAFETY` 等终态结束 |
| 安全拦截是 200 | `promptFeedback.blockReason` 出现在正常响应里，`text()` 可能为空；先查 `prompt_feedback` 与 `finish_reason` |
| `thoughtSignature` 要回传 | 多轮工具调用把模型返回的内容原样加回 `contents`，不要重建纯文本回合 |
| 整体超时会截断长流 | `timeout(...)` 是总时长；长流优先只调 `connect_timeout` |
| `parse_as` 只解析 | 不校验 schema、不修复 JSON；空内容或非 JSON 直接报 `Deserialization` |
| `pageToken` 字符受限 | 只接受 `[A-Za-z0-9._:-]`；网关返回其它字符时 `list` / `list_all` 报配置错误 |

多轮回传示意：

```rust
let response = gemini.generate_content("gemini-2.5-flash").create(first).await?;
let model_turn = response.first_candidate()
    .and_then(|candidate| candidate.content.clone()).expect("模型返回了内容");

let next = GenerateContentRequest::builder()
    .user("北京今天天气如何？") // 实际项目保留完整 contents 历史
    .content(model_turn)        // 原样回传，保留 functionCall 与 thoughtSignature
    .content(Content::parts(
        GeminiRole::User,
        vec![Part::function_response("get_weather", json!({ "temp_c": 26 }))],
    ))
    .tool(GeminiTool::function(FunctionDeclaration::new("get_weather")))
    .build()?;
```

`functionCall.args` 是完整 JSON 对象（非增量字符串）；`functionResponse` 的 `id` 可选，
需要与调用 ID 对应时用 `Part::Other`，或直接构造 `FunctionResponsePart`（需 `airx-protocol`）。

## 10. API 速查表

| 类型 / 方法 | 一句话 |
| --- | --- |
| `Client::gemini()` | 静态入口，返回 `GeminiBuilder` |
| `GeminiBuilder::api_key / base_url / transport / timeout / retry_policy / build` | 构建 `Gemini` |
| `Gemini::generate_content(model)` | 进入 `GenerateContent`，模型在此确定 |
| `Gemini::models()` / `embed_content(model)` / `config()` | 模型 API / 向量 / 查看配置 |
| `Gemini::execute(HttpRequest)` | 自定义接口逃生通道 |
| `GenerateContent::create` / `create_with_meta` / `parse_as::<T>` | 非流式 / 带元数据 / 解析为 `T` |
| `GenerateContent::count_tokens` / `stream` | 计 token / 流式（需 `stream`） |
| `GenerateContent::with_options` / `endpoint` / `stream_endpoint` | 单次选项 / 取端点 |
| `GeminiModels::list` / `get` / `list_all` / `with_options` | 模型列表、详情、自动翻页 |
| `GeminiEmbeddings::create` / `batch` | 单条 / 批量向量 |
| `GenerateContentRequest::builder()` | `user` / `system` / `tools` / `thinking` / `json_schema` … |
| `Content::user_text` / `model_text` / `parts` | 构造一轮内容 |
| `Part::text` / `thought` / `inline_image` / `file` / `function_call` / `function_response` | 构造片段 |
| `GeminiTool::function` / `functions` / `server` | 客户端工具 / 多个工具 / 服务端工具 |
| `FunctionDeclaration::new` / `with_description` / `with_parameters` | 函数声明 |
| `ToolConfig::new` + `FunctionCallingConfig::mode` / `allow` | 工具选择 |
| `GenerationConfig` / `GeminiThinkingConfig` | 采样、结构化输出、模态、thinking |
| `SafetySetting::new(category, threshold)` | 安全设置 |
| `GenerateContentResponse` | `text()` / `thought_text()` / `finish_reason()` / `function_calls()` |
| `Candidate` / `UsageMetadata` / `SafetyRating` | 候选、用量、安全评级 |
| `GenerateContentStreamAccumulator` | `new` / `apply` / `snapshot` / `finish` / `status` |
| `ProtocolStream` | `next` / `Stream` / `meta` / `request_id` / `final_text` / `final_response` |
| `GeminiCountTokensRequest::builder()` | `content` / `user` / `system` / `tools` / `generation_config` |
| `GeminiListModelsRequest::builder()` | `page_size` / `page_token` / `build` |
| `Model` / `ModelList` | 模型字段与 `next_cursor()` |
| `RequestOptions::new` / `try_header` / `idempotency_key` / `timeout` / `retry_policy` | 单次请求选项 |
| `WithMeta { data, meta }` / `ResponseMeta` / `StreamMeta` | 数据 + HTTP 元数据 |
| `RetryPolicy::standard` / `disabled` / `with_max_retries` | 重试策略 |
| `AirxError::request_id` / `is_rate_limit` / `retry_after` / `response_headers` | 错误访问器 |

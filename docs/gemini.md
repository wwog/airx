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
| `cachedContents` 创建 / 读取 / 续期 / 删除 | 图像 / 视频 `predict*` 与长任务编排 |
| Gemini `error` 错误体 → `ProviderError` | Vertex AI（端点与 OAuth 认证均不同） |
| `extensions` 透传、`Part::Other`、服务端工具声明 | Interactions API（新一代默认接口，见 §11） |

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
| generateContent | `{base}/models/{model}:generateContent`（模型在路径中，请求体同时补写 `model`） |
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
| `publishers/google/models/gemini-2.5-flash` | 原样保留（Vertex 形状，Gemini API 不接受） |
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
| `.contents([Content])` | 一次性追加多轮（复用历史） |
| `.user(text)` / `.of_user(text)` | 追加 user 文本轮 |
| `.model(text)` | 追加 model 文本轮 |
| `.part(Part)` | 追加片段到最后一轮；没有内容时新建 user 轮 |
| `.system(text)` | `systemInstruction`（不带 role） |
| `.tools(...)` / `.tool(Tool)` | `tools` |
| `.tool_config(ToolConfig)` | `toolConfig` |
| `.generation_config(...)` | 整体覆盖 `generationConfig`（可直接 `GenerationConfig::default()` 构造） |
| `.temperature` / `.max_output_tokens` / `.thinking` | `generationConfig` 常用快捷方式 |
| `.json_schema(Value)` | `responseMimeType=application/json` + `responseSchema`（OpenAPI 子集） |
| `.json_schema_std(Value)` | `responseMimeType=application/json` + `responseJsonSchema`（标准 JSON Schema） |
| `.safety_setting(SafetySetting)` | 追加 `safetySettings` |
| `.cached_content(name)` | 引用已存在的缓存资源名 |
| `.service_tier(ServiceTier)` | 请求顶层 `serviceTier`（`unspecified` / `standard` / `flex` / `priority`） |
| `.store(bool)` | 请求顶层 `store`（日志开关） |
| `.label(k, v)` / `.labels(...)` / `.safety_identifier(id)` | 请求顶层 `labels`；后者写 `labels.safety_identifier` |
| `.continuation_token(token)` | 请求顶层 `continuationToken`（续写） |
| `.model_id(name)` | 覆盖请求体里的 `model`（默认由端点按路径写入） |
| `.extension(key, json!(...))` | 未建模字段透传 |
| `.build()` | `contents` 为空时报 `Configuration` |

### 4.2 Part 类型

Gemini 的 `Part` 没有判别字段，**JSON 中出现的键决定变体**。官方 `Part` 是
“一个 part 级对象 + 互斥的 data 成员”：`thoughtSignature`、`videoMetadata`、
`partMetadata`、`mediaResolution` 是 **part 级兄弟键**，不属于 `functionCall` /
`inlineData` 内部。

| 变体 | 构造 | JSON 形状 |
| --- | --- | --- |
| 文本 | `Part::text` / `Part::thought` | `{"text": ...}`，可带 `thought` |
| 内联二进制 | `Part::inline_image(mime, base64)` | `{"inlineData": {"mimeType","data","displayName"?}}` |
| 文件引用 | `Part::file(mime, uri)` | `{"fileData": {"mimeType","fileUri","displayName"?}}` |
| 工具调用 | `Part::function_call(name, args)` | `{"functionCall": {"name","args","id"?}}` |
| 工具结果 | `Part::function_response(name, response)` | `{"functionResponse": {"name","response","id"?,"parts"?}}` |
| 多模态工具结果 | `Part::function_response_parts(name, [FunctionResponseMedia])` | `functionResponse.parts[]` + `response: {"$ref": displayName}` |
| 代码 / 结果 | `Part::ExecutableCode(..)` / `Part::CodeExecutionResult(..)` | `{"executableCode": {...}}` / `{"codeExecutionResult": {...}}` |
| 未知片段 | `Part::Other(Value)` | 原样透传，重新序列化不丢失 |

Part 级字段与访问器：

| 字段 | 读取 | 构造 |
| --- | --- | --- |
| `thoughtSignature` | `Part::thought_signature()`；`FunctionCallPart::thought_signature()` | `Part::with_thought_signature(sig)` |
| `videoMetadata` | `Part::video_metadata()`（`VideoMetadata { start_offset, end_offset, fps }`） | 写变体的 `video_metadata` 字段 |
| `partMetadata` | `Part::part_metadata()`（自由 `Value`） | 写变体的 `part_metadata` 字段 |
| `mediaResolution` | `Part::media_resolution()` | 写变体的 `media_resolution` 字段 |
| 其余兄弟键 | `Part::extensions()` | 写变体的 `extensions` 字段 |

`thoughtSignature` 是 **Part 级兄弟键**（官方 `FunctionCall` 接口没有该字段）：
序列化为 `{"functionCall": {...}, "thoughtSignature": "..."}`，反序列化时从兄弟层读回。
Gemini 3/4 的多轮函数调用必须原样回传签名，缺失会报 400。所有已知变体的
其它 part 级兄弟键（`videoMetadata` / `partMetadata` / `mediaResolution` / 未知键）
往返都不再丢失（落在该变体的 `extensions`，可用 `Part::extensions()` 读取）；
未知 Part 仍走 `Part::Other` 原样保留。data 成员内部（如 `inlineData` 里）的未知键
也会被提升到兄弟层 —— 数据不丢，但位置可能变化。
`toolCall` / `toolResponse`（服务端工具载荷）保持 `Part::Other`，用
`Part::tool_call()` / `Part::tool_response()` 读取后原样 echo 回 API。
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
    .tool_config(ToolConfig::new(FunctionCallingConfig::auto()))
    .build()?;
```

`FunctionCallingMode` 序列化为 `AUTO` / `ANY` / `NONE` / `VALIDATED` /
`MODE_UNSPECIFIED`；`ANY` 可再 `.allow(["get_weather"])` 限定函数名。
常用构造：`FunctionCallingConfig::{auto,any,none,validated}()`。

`FunctionDeclaration` 支持 `parametersJsonSchema` / `responseJsonSchema`
（标准 JSON Schema）与 `behavior`（如 `NON_BLOCKING`），分别是
`.with_parameters_json_schema(...)` / `.with_response_json_schema(...)` / `.with_behavior(...)`；
`parameters` 与 `parametersJsonSchema` 二选一。

`ToolConfig::include_server_side_invocations(true)` 写
`includeServerSideToolInvocations`，是让服务端工具调用（`toolCall` / `toolResponse`）
出现在结果里的开关。服务端工具用命名构造器：`GeminiTool::google_search()` /
`code_execution()` / `url_context()` / `google_maps()` / `file_search([store])`，
或逃生通道 `GeminiTool::server("googleSearch", json!({}))`；
只声明，airx 不代发网络请求。

### 4.4 generationConfig

| 字段 | 说明 |
| --- | --- |
| `temperature` / `top_p` / `top_k` | 采样（Gemini 3.6+ / 4 已忽略并回 deprecation 头，未来会 400，见 §9） |
| `candidate_count` / `max_output_tokens` | 候选数、输出上限（含 thinking） |
| `stop_sequences` | 停止序列 |
| `response_mime_type` | `text/plain` / `application/json` / `text/x.enum` |
| `response_schema` | OpenAPI 子集 Schema（**SDK 3.6.1 已标 deprecated**，见 §4.6） |
| `response_json_schema` | 标准 JSON Schema（结构化输出的推荐字段） |
| `presence_penalty` / `frequency_penalty` / `seed` | 惩罚与随机种子 |
| `response_logprobs` / `logprobs` | 是否导出 logprobs、每步 top logprobs 数量（`0..=20`） |
| `response_modalities` | 如 `["TEXT"]`、`["TEXT","IMAGE"]` |
| `media_resolution` | 媒体分辨率（字符串） |
| `speech_config` | 结构化 `SpeechConfig`（`voice_config.prebuilt_voice_config.voice_name`、`language_code`…） |
| `image_config` | 结构化 `ImageConfig`（`aspect_ratio`、`image_size`、`mime_type`…） |
| `translation_config` | `TranslationConfig`（`target_language_code`、`echo_target_language`） |
| `audio_transcription_config` | `AudioTranscriptionConfig`（语言提示、词级时间戳、说话人分离） |
| `enable_affective_dialog` / `enable_enhanced_civic_answers` | 情绪自适应 / 增强公民回答 |
| `thinking_config` | 见 §4.5 |
| `extensions` | 未建模字段（如 `responseFormat`、`modelSelectionConfig`、Vertex 专属字段） |

### 4.5 thinking

| 构造 | 效果 |
| --- | --- |
| `GeminiThinkingConfig::budget(n)` | `thinkingBudget = n`，并设 `includeThoughts = true` |
| `GeminiThinkingConfig::dynamic()` | `budget(-1)`，动态预算 |
| `GeminiThinkingConfig::disabled()` | `thinkingBudget = 0`，不设 `includeThoughts` |
| `GeminiThinkingConfig::level("minimal" \| "low" \| "medium" \| "high" \| "xhigh")` | `thinkingLevel`（Gemini 3），并设 `includeThoughts = true` |

`includeThoughts` 默认不发送（官方默认 false）；`budget` / `level` 会强制打开。
Gemini 4 同时发送 `thinkingBudget` 与 `thinkingLevel` 会返回 400 —— 构造器会自动清空
另一个字段，直接写结构体字面量时需自行保证互斥；`thinkingBudget` 已 deprecated，
只发它会映射到最近的 level。`thinkingLevel` 的大小写以官方示例为准（`"xhigh"` / `"low"`）。

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
被拦截或空内容返回 `AirxError::Deserialization`。

两条 schema 路径：

| 构建器 | 写入 | 说明 |
| --- | --- | --- |
| `.json_schema(Value)` | `responseSchema` | OpenAPI 子集；SDK 3.6.1 已标 **deprecated**（推荐 `responseFormat`），REST 仍在用，能跑但属退役通道 |
| `.json_schema_std(Value)` | `responseJsonSchema` | 标准 JSON Schema，新代码优先 |

也可以直接构造
`GenerationConfig { response_mime_type: Some("application/json".into()), response_json_schema: Some(schema), ..Default::default() }`。
`text/x.enum` 需要手写 `response_mime_type`。

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
| `prompt_feedback: Option<PromptFeedback>` | `block_reason` + `block_reason_message` + `safety_ratings` |
| `usage_metadata: Option<UsageMetadata>` | prompt / candidates / total / thoughts / cached / tool_use_prompt，另有未类型化的 details |
| `model_version` / `response_id` | 实际模型版本、部分模型返回的响应 ID |
| `create_time` / `model_status` | 时间戳与模型状态（`Value`，原样保留） |
| `extensions` | 未知顶层字段，原样保留 |

`Candidate`：`index`、`content`、`finish_reason`（`STOP` / `MAX_TOKENS` / `SAFETY`…）、
`finish_message`、`safety_ratings`、`token_count`、`avg_logprobs`、`continuation_token`，
以及未类型化且原样保留的
`citation_metadata` / `grounding_metadata` / `url_context_metadata` / `logprobs_result`；
未知字段进入自身 `extensions`。

便捷读取与分类：

| 方法 | 语义 |
| --- | --- |
| `text()` / `thought_text()` / `function_calls()` | 可见文本 / thinking 文本 / 工具调用 |
| `finish_reason()` / `finish_reason_typed()` | 原始字符串 / `FinishReason` 枚举 |
| `is_blocked()` | 输入 `blockReason`，或候选终态属于 `SAFETY` / `RECITATION` / `BLOCKLIST` / `PROHIBITED_CONTENT` / `SPII` / `IMAGE_SAFETY` / `NO_IMAGE` / `MALFORMED_*` / `MISSING_THOUGHT_SIGNATURE` 等 |
| `is_truncated()` | `MAX_TOKENS` |
| `requires_continuation()` | `CONTINUATION` 时返回 `continuationToken` |
| `block_message()` | `promptFeedback.blockReasonMessage` 可读原因 |
| `candidate_count()` | 候选数量 |

### 5.3 流式语义

- 请求带 `?alt=sse` 与 `Accept: text/event-stream`；每个 `data:` 是一个
  `GenerateContentResponse` 增量。官方 generateContent **没有 `[DONE]` 哨兵**，空 `data`
  被忽略；部分网关会发 `data: [DONE]`，该帧被跳过而不是报错；
  `data` 中出现顶层 `error` 立即返回 `AirxError::Provider`（无 status / headers）。
- 完成信号：候选 `finishReason` 非空且不是 `FINISH_REASON_UNSPECIFIED`，且**不是
  `CONTINUATION`**（续写未完成）；或输入被拦截的 `promptFeedback.blockReason`。
  两者都没有时 `finish()` 返回 `AirxError::Stream`（`Incomplete`），截断不会伪装成成功。
- 多候选（`candidateCount > 1`）：等所有候选都进入终态才置为完成；终态之后到达的
  候选内容不会被丢弃（若又出现未完成的候选，状态会回退为进行中）。
- `CONTINUATION`：`finish()` 返回 `Incomplete`，用 `requires_continuation()` 取游标，
  交给下一次请求的 `.continuation_token(...)`（airx 不自动重发）。
- 非 2xx：`stream()` 先读完 body 再解码，**直接返回 `Err`**，不会返回“正常流”。

| API | 说明 |
| --- | --- |
| `ProtocolStream::next().await` / `impl futures_core::Stream` | 固有方法返回 `Result<Option<GenerateContentResponse>, AirxError>`；也可用 `TryStreamExt::try_next`、`select!` |
| `meta()` / `request_id()` | 建立连接时的状态码、响应头、request id |
| `final_response()` / `final_text()` | 消费整条流并返回聚合结果 |

`GenerateContentStreamAccumulator`：`new()` → `apply(response)` → `finish()`；
`snapshot()` 可取部分结果且不要求完成，`status()` 返回 `airx-core` 的 `StreamStatus`，
`requires_continuation()` 取续写游标。文本按 thought 标记分段拼接，`functionCall`
原样追加；终态后仍会吸收迟到的候选内容与扩展字段。

`inlineData` 默认按出现顺序追加为独立片段；`with_inline_data_merge(true)` 会尝试把
连续的同类 base64 分片拼接（前一段没有结尾填充时）。官方快照未规定 generateContent
的图片流式分片规则，若上游“每个 chunk 一张完整图片”，拼接会破坏数据，故默认关闭 ——
接真机验证后再决定是否开启。

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

路径为 `POST {base}/models/{model}:countTokens`，模型在路径中。请求字段：
`contents` / `systemInstruction` / `tools` / `generationConfig`，或整体形态
`generateContentRequest`（`GeminiCountTokensRequest::of(&generate_request)`，此时
`contents` 被忽略，嵌套 body 里的 `model` 会自动补写）；响应字段：
`total_tokens`、`cached_content_token_count`、`prompt_tokens_details`（`Value`）。

`computeTokens` 是更细的接口（返回 token ID 与文本）：

```rust
let info = gemini
    .models()
    .compute_tokens(
        "gemini-2.5-flash",
        airx_protocol::gemini::models::ComputeTokensRequest::of_text("hello"),
    )
    .await?;
println!("{}", info.total_tokens());
```

`ComputeTokensRequest` / `ComputeTokensResponse` / `TokensInfo` 与缓存相关类型已从
`airx::prelude::*` 导出；直接用 `airx_protocol::gemini::models` / `gemini::caches`
也可以。

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
| `models().retrieve(model)` | 同上 | `get` 的别名（与其它 Provider 命名对齐） |
| `models().list_all(request)` | 同上，循环翻页 | 自动翻页，直到无游标或游标重复 |
| `models().compute_tokens(model, request)` | `POST /models/{model}:computeTokens` | token ID 与文本 |

`Model` 字段：`name`、`base_model_id`、`version`、`display_name`、`description`、
`input_token_limit`、`output_token_limit`、`supported_generation_methods`、
`temperature` / `max_temperature` / `top_p` / `top_k`、`thinking`、`extensions`。

`pageToken` 是官方不透明串，按百分号编码写入 query（`+` / `/` / `=` 不再被拒绝）。

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

路径为 `POST {base}/{model}:embedContent` 与 `:batchEmbedContents`；单条与批量请求
都会在 body 里写入 `model`（批量还会为每条 item 写入）。

响应两种形状都认：REST 的单数 `embedding` 与 SDK 类型的数组 `embeddings[]`，
另有 `truncated` 与 `usageMetadata`。读取用 `values()`（第一条向量，`f64`）、
`first_embedding()`、`embeddings()`、`embedding_count()`；`Embedding` 同时保留
`shape` 与未识别字段。

### 6.4 缓存（cachedContents）

```rust
let cached = gemini
    .caches()
    .create(
        CreateCachedContentRequest::builder()
            .model("models/gemini-2.5-flash")
            .user("很长的文档……")
            .ttl("600s")
            .build()?,
    )
    .await?;

let next = GenerateContentRequest::builder()
    .user("总结上面的文档")
    .cached_content(cached.name.expect("name"))
    .build()?;

gemini.caches().update("abc", UpdateCachedContentRequest::ttl("3600s")).await?;
gemini.caches().delete("abc").await?;
```

| API | 路径 |
| --- | --- |
| `caches().create(request)` | `POST /cachedContents` |
| `caches().get(name)` | `GET /cachedContents/{id}` |
| `caches().list(request)` / `list_all(request)` | `GET /cachedContents`（`pageToken` 百分号编码） |
| `caches().update(name, request)` | `PATCH /cachedContents/{id}?updateMask=ttl\|expireTime` |
| `caches().delete(name)` | `DELETE /cachedContents/{id}` |

缓存名可带或不带 `cachedContents/` 前缀；只做 CRUD，不解释缓存内容。

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
- **Part 级元数据**：已知变体的兄弟键有 typed 访问器（`thought_signature` /
  `video_metadata` / `part_metadata` / `media_resolution`），其余进
  `Part::extensions()`；未知 Part 仍走 `Part::Other`。
- **Part::Other**：未知片段原样保留，重新序列化不丢失。
- **服务端工具**：`GeminiTool::server(...)` / 命名构造器原样写入 `tools`；联网与代码执行发生在
  Provider 后台，airx 只解析结果（`executableCode` / `codeExecutionResult` 为强类型，
  grounding / citation 为 `Value`，`toolCall` / `toolResponse` 用 `Part::tool_call()` 读取），
  **不发本地 HTTP 请求**。
- **自定义 Endpoint + Transport**：`Gemini` 暴露 `execute(HttpRequest)` /
  `execute_with(HttpRequest, &RequestOptions)`，复用认证、重试、超时与压缩。
  需要自定义编码 / 解码时，先 `use airx::Endpoint;`，再用
  `endpoint.encode(&request)?` → 自定义 `Transport::execute` → `endpoint.decode(raw)?`；
  后者会绕过执行级 Header 与内置重试。自定义 Transport 需直接依赖
  `airx-core` / `airx-transport`；流式未建模接口需自己读 SSE，或在 `airx-protocol`
  增加 typed endpoint 并暴露客户端方法。

| 未封装 | 现状与替代路径 |
| --- | --- |
| Files API | 可用 `Part::file(mime, fileUri)` 引用已有文件、`Part::inline_image` 传 base64；上传 / 列举未封装 |
| Batches | 未封装，自建 Endpoint |
| `responseFormat`（结构化输出新形状） | 走 `GenerationConfig::extensions`，等 REST 稳定后再 typed |
| Vertex AI | 端点与认证不同（OAuth），`x-goog-api-key` 不适用 |
| Live / Realtime、Corpora / Semantic Retrieval | 不在 Transport / 本库范围，边界之外 |
| Interactions API | 本周期不实现，见 §11 |

## 9. 最佳实践 / 常见坑

| 坑 | 做法 |
| --- | --- |
| 模型在路径，也在 body | `generate_content(model)` 决定 URL；端点同时把 `model` 写进请求体（官方 REST 标 required），要覆盖用 `.model_id(...)` |
| 流式没有 `[DONE]` | 不要等哨兵；用 `finish()` / `final_text()`，提前 EOF 得到 `Incomplete`（收到 `[DONE]` 时跳过） |
| `FINISH_REASON_UNSPECIFIED` / `CONTINUATION` 不是完成 | 聚合器都不当成功；`CONTINUATION` 用 `requires_continuation()` 取游标后续写 |
| 安全拦截是 200 | `promptFeedback.blockReason` 出现在正常响应里，`text()` 可能为空；先查 `is_blocked()` / `block_message()` / `finish_reason` |
| `MAX_TOKENS` 是截断 | `is_truncated()` 为真时不要当作完整结果 |
| `thoughtSignature` 要回传 | 它是 **Part 级**字段；多轮工具调用把模型返回的内容原样加回 `contents`，不要重建纯文本回合 |
| 采样参数在退役通道 | Gemini 3.6+ / 4 忽略 `temperature` / `topP` / `topK` 并回 deprecation 头，未来 400；保持默认 |
| `responseSchema` 已被 SDK 标 deprecated | 新代码用 `.json_schema_std(...)`（`responseJsonSchema`） |
| 整体超时会截断长流 | `timeout(...)` 是总时长；长流优先只调 `connect_timeout` |
| `parse_as` 只解析 | 不校验 schema、不修复 JSON；空内容或非 JSON 直接报 `Deserialization` |
| `pageToken` 是不透明串 | 按百分号编码写入 query，`+` / `/` / `=` 都可以正常翻页 |

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

`functionCall.args` 是完整 JSON 对象（非增量字符串）；`functionResponse` 的 `id` 可选。
多模态工具结果用
`Part::function_response_parts("render_chart", [FunctionResponseMedia::inline("image/png", b64).with_display_name("chart_image")])`，
`response` 会自动写成 `{"$ref": "chart_image"}`；引用已有附件时用
`Part::function_response_ref("render_chart", "chart_image")`。

## 10. API 速查表

| 类型 / 方法 | 一句话 |
| --- | --- |
| `Client::gemini()` | 静态入口，返回 `GeminiBuilder` |
| `GeminiBuilder::api_key / base_url / transport / timeout / retry_policy / build` | 构建 `Gemini` |
| `Gemini::generate_content(model)` | 进入 `GenerateContent`，模型在此确定 |
| `Gemini::models()` / `caches()` / `embed_content(model)` / `config()` | 模型 API / 缓存 CRUD / 向量 / 查看配置 |
| `Gemini::execute(HttpRequest)` | 自定义接口逃生通道 |
| `GenerateContent::create` / `create_with_meta` / `parse_as::<T>` | 非流式 / 带元数据 / 解析为 `T` |
| `GenerateContent::count_tokens` / `stream` | 计 token / 流式（需 `stream`） |
| `GenerateContent::with_options` / `endpoint` / `stream_endpoint` | 单次选项 / 取端点 |
| `GeminiModels::list` / `get` / `retrieve` / `list_all` / `compute_tokens` / `with_options` | 模型列表、详情、自动翻页、token ID |
| `GeminiCaches::create` / `get` / `list` / `list_all` / `update` / `delete` | 缓存 CRUD（续期用 `UpdateCachedContentRequest::ttl`） |
| `GeminiEmbeddings::create` / `batch` | 单条 / 批量向量 |
| `GenerateContentRequest::builder()` | `user` / `system` / `tools` / `thinking` / `json_schema` / `service_tier` … |
| `Content::user_text` / `model_text` / `parts` | 构造一轮内容 |
| `Part::text` / `thought` / `inline_image` / `file` / `function_call` / `function_response` | 构造片段 |
| `Part::function_response_parts` / `function_response_ref` / `with_thought_signature` | 多模态工具结果 / `$ref` 引用 / Part 级签名 |
| `Part::thought_signature` / `video_metadata` / `part_metadata` / `media_resolution` / `extensions` | Part 级元数据读取 |
| `Part::tool_call` / `tool_response` / `other` | 未建模片段的读取器 |
| `GeminiTool::function` / `functions` / `server` / `google_search` / `code_execution` / `url_context` / `file_search` | 客户端工具 / 服务端工具 |
| `FunctionDeclaration::new` / `with_description` / `with_parameters` / `with_parameters_json_schema` / `with_behavior` | 函数声明 |
| `ToolConfig::new` + `FunctionCallingConfig::{auto,any,none,validated}` / `allow` / `include_server_side_invocations` | 工具选择 |
| `GenerationConfig` / `GeminiThinkingConfig` / `SpeechConfig` / `ImageConfig` | 采样、结构化输出、模态、thinking、语音、图片 |
| `SafetySetting::new(category, threshold)` | 安全设置 |
| `GenerateContentResponse` | `text()` / `thought_text()` / `finish_reason()` / `finish_reason_typed()` / `function_calls()` / `is_blocked()` / `is_truncated()` / `requires_continuation()` / `block_message()` |
| `Candidate` / `UsageMetadata` / `SafetyRating` | 候选、用量、安全评级 |
| `GenerateContentStreamAccumulator` | `new` / `with_inline_data_merge` / `apply` / `snapshot` / `finish` / `status` / `requires_continuation` |
| `ProtocolStream` | `next` / `Stream` / `meta` / `request_id` / `final_text` / `final_response` |
| `GeminiCountTokensRequest::builder()` / `of(&request)` | `content` / `user` / `system` / `tools` / `generation_config`；整体形态 |
| `CreateCachedContentRequest::builder()` / `UpdateCachedContentRequest` | 缓存创建 / 续期 |
| `GeminiListModelsRequest::builder()` | `page_size` / `page_token` / `build` |
| `Model` / `ModelList` | 模型字段与 `next_cursor()` |
| `RequestOptions::new` / `try_header` / `idempotency_key` / `timeout` / `retry_policy` | 单次请求选项 |
| `WithMeta { data, meta }` / `ResponseMeta` / `StreamMeta` | 数据 + HTTP 元数据 |
| `RetryPolicy::standard` / `disabled` / `with_max_retries` | 重试策略 |
| `AirxError::request_id` / `is_rate_limit` / `retry_after` / `response_headers` | 错误访问器 |

## 11. 未做：Interactions API（观察项）

官方已把 Interactions 作为新项目推荐接口，`generateContent` 被称作 legacy 但仍
“fully supported”。airx 本周期**不实现** Interactions：它的核心增量是服务端会话状态
（`previousInteractionId` / `store`）、step 可观测、后台任务与 WebSocket/RTC，
与“只做请求载荷、不做 agent 层”的定位冲突。

切换触发条件（任一满足再评估）：

1. 目标模型只在 Interactions 可用；
2. `generateContent` 收不到需要的新能力（新的多模态 / 工具能力只落在 Interactions）；
3. 官方宣布 `generateContent` 的下线与时间表。

届时最小实现边界：请求编码 + SSE 增量（含 `data: [DONE]` 哨兵）+ `interaction.completed`，
只透传 `previousInteractionId` / `store`，不实现会话状态语义。

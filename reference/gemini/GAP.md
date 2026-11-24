# Gemini：官方行为 vs airx 差距分析

- 基准 1（SDK 类型与端点清单）：`@google/genai@3.6.1`（npm `latest`），快照生成时间 `2026-11-24T02:01:33Z`
  —— 见 `reference/gemini/api/SUMMARY.md:3`、`reference/gemini/api/manifest.json` 的 `version` / `generated_at`。
- 基准 2（官方文档原文）：`aistudio.google.com` 官方 markdown 快照，抓取时间 `2026-11-24T01:56:21Z`
  —— 见 `reference/gemini/docs/meta.json`。
- airx 侧对齐日期：`docs/protocol-support.md:6`（2026-11-23）。
- 快照规模：24 个资源 / 104 个方法（87 个有 HTTP 端点）/ 154 个请求参数类型 / 1324 个其余类型
  —— `reference/gemini/api/SUMMARY.md:12-18`。
- **引用约定**：airx 仓库文档只有 `docs/gemini.md`、`docs/protocol-support.md`、`docs/compatibility.md`、
  `docs/architecture.md` 四个；其它以 `docs/` 或 `docs__` 开头、形如 `docs__xxx--generateContent.md` /
  `docs/api-reference__xxx.md` 的引用一律是快照文件（即 `reference/gemini/docs/` 下同名文件）。
  SDK 类型写作 `dist/genai.d.ts`（即
  `reference/gemini/sdk/@google-genai@3.6.1/dist/genai.d.ts`）。

**分析范围**：`models.generateContent` / `generateContentStream`（含 `contents`、`systemInstruction`、`tools`、
`toolConfig`、`safetySettings`、`cachedContent`、`generationConfig` 及其 `thinkingConfig` / `responseSchema`、
`responseModalities`）、`models.countTokens`、`models.list` / `models.get`、`models.embedContent` /
`models.batchEmbedContents`；上下游类型：`Part` / `Content` / `Candidate` / `GenerateContentResponse` /
`UsageMetadata` / `SafetyRating` / `Model`；流式聚合与 `alt=sse` 语义；airx 门面（`Client::gemini()` /
`GenerateContent` / `GeminiModels` / `GeminiEmbeddings`）的接入形状。

**不在范围**：Live API（WebSocket）、Interactions WebSocket/RTC（`interactions.connect`）、托管 Agent 资源
（agents / triggers / environments / credentials / voices / webhooks / decisions / authTokens）、Tuning、
FileSearchStores（RAG 服务）、图像/视频 `predict*` 系列、Batches 作业编排、以及一切"拿到结果之后做什么"
的逻辑（工具执行、循环、会话状态）。

**判定口径**（全文统一）：

| 分类 | 含义 |
| --- | --- |
| (a) 缺失 | airx 没有对应端点 / 字段，且无法通过 `extensions` 表达 |
| (b) 仅透传 | 无 typed 字段，但可通过 `#[serde(flatten)] extensions`（请求侧）或 `Part::Other(Value)`（Part 级）wire 兼容 |
| (c) typed 不完整/不正确 | 有字段但形状、必填性、枚举值或嵌套层级与官方不符 |
| (d) 行为差异 | 默认值、完成语义、错误映射、流式组装等运行时行为不同 |
| (e) 文档/代码不一致 | 仓库文档描述与代码或官方事实不符 |

---

## 0. 一页结论

| # | 差距 | 影响 | 优先级 | 建议动作 |
| --- | --- | --- | --- | --- |
| 1 | **Part 级元数据错位且会丢**：`thoughtSignature` 被塞进 `functionCall` 内部、解析时 Part 级签名与其它兄弟键被丢弃 | Gemini 3/4 多轮函数调用 400；`partMetadata` / `videoMetadata` 等往返消失，违背"未知字段保留"承诺 | **P0** | 签名提到 Part 层；`Part::inner` 把剩余兄弟键收进各变体 `extensions` |
| 2 | `FunctionResponse.parts`（多模态工具结果）未建模 | 工具要回图片/PDF 只能用 `Part::Other` 手拼 JSON | P1 | `FunctionResponsePart.parts` + `Part::function_response_parts(...)` |
| 3 | 请求体省略 `model`（官方 REST 标 required）；`embedContent` 响应只认单数 `embedding`（SDK 类型已是 `embeddings[]`） | 严格网关 400；上游返回数组时静默给出空向量 | P1 | 端点自动写 `model`（与 `BatchEmbedItem::from_request` 一致）；响应两种形状都认 |
| 4 | `pageToken` 白名单拒绝 `+` `/` `=`；`FunctionCallingMode` 缺 `VALIDATED` | 官方 token 是不透明 base64，翻页可能直接报 `Configuration`；`VALIDATED` 模式无法发送 | P1 | token 改百分号编码；枚举补 `Validated` / `ModeUnspecified` |
| 5 | 完成语义只判"有 `finishReason`"：`CONTINUATION` 当成功，`SAFETY` / `NO_IMAGE` / `MALFORMED_*` 无分类 | 长输出静默截断；空结果被当成功 | P1 | `is_blocked()` / `is_truncated()` / `requires_continuation()`；`CONTINUATION` 判未完成 |
| 6 | 结构化输出主路径被 SDK 标 deprecated：`responseMimeType` / `responseSchema` → `responseFormat` | 未来移除会打断 `parse_as` 业务；`.json_schema()` 正好写的是这对字段 | P1 | 增加 `responseJsonSchema` 快捷方式与文档预警；`responseFormat` 待 REST 稳定再 typed |
| 7 | `generationConfig` 与请求顶层新字段未 typed（`serviceTier` / `store` / `labels` / `continuationToken` / `logprobs` / `translationConfig` …） | 只能 `extensions` 透传，无补全与拼写保护 | P2 | 优先 `serviceTier` / `store` / `labels` / `continuationToken` |
| 8 | 官方已把 `generateContent` 称为 legacy、新能力优先落在 Interactions（战略风险，非缺口） | 新模型/新能力可能只在 Interactions 可用 | 观察项 | 本周期不实现；记录触发条件（§7.1） |

---

## 1. 端点覆盖矩阵

官方清单：24 资源 / 104 方法（`reference/gemini/api/SUMMARY.md:12-18`，逐资源见 `SUMMARY.md:23-259`）。
airx 的 Gemini 实现只覆盖 **models** 资源中的 6 个方法 + 1 个 alt 路由，即 **6/104（5.8%）**；models 资源内部为 **6/15（40%）**。

### 1.1 按资源汇总

| 官方资源 | 方法数 | airx 状态 | 备注 | 优先级 |
| --- | ---: | --- | --- | --- |
| `models` | 15 | **部分实现 6**：`generateContent`、`generateContentStream`、`countTokens`、`embedContent`（含 alt `batchEmbedContents`）、`get`、`list` | 见 §1.2 明细 | — |
| `files` | 7 | 缺失 | 上传/列举/删除/下载；`Part::file` 只能引用已存在文件（`docs/gemini.md:465`） | P2 |
| `caches` | 5 | 缺失 | 只能通过 `cachedContent("cachedContents/…")` 引用已建缓存（`docs/gemini.md:464`），无法创建/续期 | P2 |
| `batches` | 6 | 缺失 | `batchGenerateContent` / `asyncBatchEmbedContent` 等作业编排 | P2 |
| `operations` | 2 | 缺失 | 仅服务于长任务（视频/批处理） | Out（依赖 Batches/视频） |
| `interactions` (+ `.authTokens`) | 5 + 2 | 缺失 | 新一代默认接口；含 SSE 流与 WebSocket/RTC `connect` | 观察项（§7） |
| `live` (+ `live.music`, `live.session`) | 1 + 1 + 5 | 缺失 | WebSocket 双向实时 | Out |
| `fileSearchStores` (+ `.documents`) | 7 + 3 | 缺失 | 托管 RAG 服务 | Out |
| `tunings` | 5 | 缺失 | 微调作业 | Out |
| `agents` / `triggers` / `environments`(+files) / `credentials` / `decisions` / `voices` / `webhooks` / `authTokens` / `chat` / `chats` | 4 / 7 / 4+3 / 5 / 1 / 4 / 7 / 1 / 3 / 1 | 缺失 | 托管 Agent 平台与会话封装（`chat` 由 SDK 维护历史） | Out（§9） |

### 1.2 `models` 资源明细（15 个方法，`SUMMARY.md:193-209`）

| 官方方法 | 端点 | airx 状态 | 证据 | 优先级 |
| --- | --- | --- | --- | --- |
| `generateContent` | `POST /v1beta/models/{model}:generateContent` | 已实现 | `gemini/generate_content/endpoint.rs:21`、`:131-162` | — |
| `generateContentStream` | `POST …:streamGenerateContent?alt=sse` | 已实现（硬编码 `alt=sse`） | `endpoint.rs:23`、`:164-183` | — |
| `countTokens` | `POST …:countTokens` | 已实现 | `gemini/generate_content/count_tokens.rs:160-212` | — |
| `embedContent` | `POST …:embedContent` | 已实现 | `gemini/embeddings.rs:192-227` | — |
| `embedContent`（alt） | `POST …:batchEmbedContents` | 已实现 | `gemini/embeddings.rs:229-264` | — |
| `get` | `GET /v1beta/{name}` | 已实现 | `gemini/models.rs:177-208` | — |
| `list` | `GET /v1beta/models` | 已实现 | `gemini/models.rs:132-175` | — |
| `computeTokens` | `POST …:computeTokens` | 缺失 | 无对应文件符号 | P2 |
| `delete` / `update` | `DELETE /v1beta/{name}` / `PATCH /v1beta/models/{model}` | 缺失 | 模型管理面 | Out（管理面） |
| `editImage` / `generateImages` / `recontextImage` / `segmentImage` / `upscaleImage` | `POST …:predict` | 缺失 | 图像生成/编辑 | P2 / Out |
| `generateVideos` | `POST …:predictLongRunning` | 缺失 | 需要 operations 轮询 | Out |

> 说明：`reference/gemini/api/SUMMARY.md:199` 把 `batchEmbedContents` 记为 `embedContent` 的 `alt:` 路径，
> 并非独立 method；airx 把它做成独立端点（`embeddings.rs:229`），与 REST 参考一致
> （`docs/api-reference__embeddings__batch-embed-contents.md:6-8`）。

---

## 2. 请求参数与类型缺口

### 2.1 请求体字段覆盖（`GenerateContentRequest`）

官方 REST 请求体共 12 个顶层字段（`docs/api-reference__generate-content__generate-content.md:30-42` 起，末尾 5 个见同文件 `:674-711`）：

| 官方字段 | airx | 位置 | 判定 |
| --- | --- | --- | --- |
| `model`（标 required） | 未发送（模型在路径） | `generate_content/request.rs:17-63`（无该字段）；`endpoint.rs:76-93` 拼路径 | (c)/(d)：可 `.extension("model", …)` 补，但默认缺失 |
| `contents` | `Vec<Content>`（必填，空则 `Configuration`） | `request.rs:21`、`:223-241` | 一致 |
| `systemInstruction` | `Option<Content>` | `request.rs:23-28` | 一致 |
| `tools` | `Option<Vec<Tool>>` | `request.rs:30-31` | 一致 |
| `toolConfig` | `Option<ToolConfig>` | `request.rs:33-38` | 一致 |
| `safetySettings` | `Option<Vec<SafetySetting>>` | `request.rs:46-52` | 一致（见 §2.4） |
| `generationConfig` | `Option<GenerationConfig>` | `request.rs:39-45` | 一致（见 §2.2） |
| `cachedContent` | `Option<String>` | `request.rs:53-59` | 一致 |
| `serviceTier` | 无 typed（extensions） | — | (b) |
| `store` | 无 typed（extensions） | — | (b) |
| `continuationToken` | 无 typed（extensions） | — | (b) |
| `labels` | 无 typed（extensions） | — | (b) |

结论：**7/12 typed**；`model` 是唯一"官方 required 但默认不发"的字段，其余 4 个可 `extensions` 透传。

### 2.2 `generationConfig`

官方 `GenerationConfig` 在 SDK 类型里 26 个字段（`reference/gemini/api/manifest.json` →
`other_types[GenerationConfig]`；`dist/genai.d.ts:7836-7891`），REST 参考另有 `serviceTier` / `store` /
`continuationToken` / `labels` 等新字段
（`docs/api-reference__generate-content__generate-content.md:427`、`:608`、`:626`、`:682`、`:699`、`:705`、`:711`）。

airx `GenerationConfig` typed 17 个字段（`gemini/generate_content/generation.rs:13-68`）：
`temperature`、`topP`、`topK`、`candidateCount`、`maxOutputTokens`、`stopSequences`、`responseMimeType`、
`responseSchema`、`responseJsonSchema`、`presencePenalty`、`frequencyPenalty`、`seed`、`responseModalities`、
`thinkingConfig`、`mediaResolution`、`speechConfig`(Value)、`imageConfig`(Value)。

| 官方字段 | airx | 判定 | 优先级 |
| --- | --- | --- | --- |
| `responseMimeType` / `responseSchema` | typed ✓ | **(c) 语义漂移**：SDK 对该对字段标注 "Deprecated: Use `response_format` instead"（`dist/genai.d.ts:7866`、`:7870`），而 REST 参考仍按现行字段文档（`:180`）。airx 的 `.json_schema()`（`request.rs:190-199`）写的正是这对字段 | P1（观察） |
| `responseFormat` | 缺失（extensions） | (b)：官方新形状为 `{audio?, image?, text?, video?}`（`manifest.json` → `other_types[ResponseFormat]`） | P1（随上一条） |
| `logprobs` / `responseLogprobs` | 缺失（extensions） | (b)：`Candidate.logprobs_result` 已 typed 为 `Value`（`response.rs:107-108`），请求侧却没有开关 | P2 |
| `translationConfig` / `audioTranscriptionConfig` | 缺失（extensions） | (b) | P2 |
| `enableAffectiveDialog` / `enableEnhancedCivicAnswers` | 缺失（extensions） | (b) | P2 |
| `modelSelectionConfig` | 缺失（extensions） | (b)：SDK 侧选型 | P2 |
| `audioTimestamp` / `routingConfig` | 缺失 | **非缺口**：SDK 注释明确 "not supported in Gemini API"（`dist/genai.d.ts:7846`、`:7872`） | — |
| `speechConfig` / `imageConfig` | `Option<Value>` | (c)：官方分别是 3 / 7 字段结构（`manifest.json` → `SpeechConfig`、`ImageConfig`）；`speechConfig` 走 `VoiceConfig`，业务要手写嵌套 JSON | P2 |

请求顶层：`serviceTier`（`unspecified|standard|flex|priority`，`:682-697`）、`store`（`:699`）、`labels`（`:711`）
全部只能 `extensions`；`labels` 官方建议用于 `safety_identifier`（`:717`），属合规场景，建议优先 typed。

### 2.3 `tools` / `toolConfig` / `functionDeclarations`

| 类型 | 官方字段数 | airx typed | 判定 | 位置 |
| --- | ---: | --- | --- | --- |
| `Tool` | 13（manifest `other_types[Tool]`；`dist/genai.d.ts:18226-18290`） | 1（`functionDeclarations`） | (b)：其余 12 种（`googleSearch` / `codeExecution` / `urlContext` / `fileSearch` / `googleMaps` / `mcpServers` / `computerUse` / `enterpriseWebSearch` / `parallelAiSearch` / `googleSearchRetrieval` / `exaAiSearch` / `retrieval`）靠 `Tool::server(kind, config)` 原样写入（`tool.rs:39-47`） | `tool.rs:11-48` |
| `FunctionDeclaration` | 7 | 4（`name` / `description` / `parameters` / `response`） | (b)：缺 `parametersJsonSchema`、`responseJsonSchema`、`behavior`。标准 JSON Schema 目前只能塞进 `parameters`（官方语义是 OpenAPI 子集）或 `.extension(...)` | `tool.rs:51-95` |
| `ToolConfig` | 3 | 1（`functionCallingConfig`） | (b)：缺 `retrievalConfig`、`includeServerSideToolInvocations`（后者是"观察服务端工具调用"的唯一开关，`docs/api-reference__generate-content__generate-content.md:126`） | `tool.rs:98-117` |
| `FunctionCallingConfig` | 3（+`streamFunctionCallArguments`） | 2（`mode` + `allowedFunctionNames`） | (b)；`streamFunctionCallArguments` 为 Vertex 专属（`dist/genai.d.ts:6653-6656`），**非缺口** | `tool.rs:120-150` |
| `FunctionCallingMode` | 5 值：`MODE_UNSPECIFIED` / `AUTO` / `ANY` / `NONE` / `VALIDATED`（`dist/genai.d.ts:6660-6681`；REST 参考 `:74-77`） | 3 值 | **(c) 硬缺口**：`VALIDATED` 无法发送；`serde` 解析到 `VALIDATED` 直接失败（`tool.rs:152-162`，反序列化由 `#[derive]` + `rename_all` 决定） | `tool.rs:152-162` |

> `VALIDATED` 难以用 `extensions` 绕过：`FunctionCallingConfig.mode` 已是 typed 字段，
> 把 `"mode"` 塞进 `extensions` 会序列化出重复键，只能整包自建 JSON 走 `Gemini::execute`。

### 2.4 `safetySettings` / `cachedContent`

- `SafetySetting` 官方 3 字段：`category`、`method`、`threshold`（`dist/genai.d.ts:16417-16426`）。airx typed 2 个
  （`generation.rs:170-191`），且两者都是**必填** `String`。
  - `method`（`HarmBlockMethod`）= **非缺口**：SDK 注明 "This field is not supported in Gemini API"（`dist/genai.d.ts:16421`）。
  - 但官方 `category` / `threshold` 都是 optional（d.ts 同为 `?`），airx 设成必填 → 反序列化官方形状的
    `{"category":"…"}`（用默认阈值）会失败；这属 (c)，影响面小，P2。
- `cachedContent` typed ✓（`request.rs:53-59`），但 `caches` 资源 5 个方法全缺（§1.1）→ 只能引用别人创建的缓存。

### 2.5 `countTokens`

官方请求体支持两种互斥形态（`docs/api-reference__tokens__count-tokens.md:862-864`）：
`model` + `contents`（+/`systemInstruction`/`tools`/`generationConfig`），或整体 `generateContentRequest`。

airx：`contents` / `systemInstruction` / `tools` / `generationConfig` = **4/5**（`count_tokens.rs:24-47`），
`generateContentRequest` 缺失（(b)：可用 `extensions` 手工塞 `Value`，但类型上是 any，业务收益低）。
`model` 同样未写入 body（官方 required，`:19-26`）。响应 `totalTokens` / `cachedContentTokenCount` /
`promptTokensDetails` 3/3（`count_tokens.rs:143-158`，details 为 `Value`）。

### 2.6 Embeddings

| 项目 | 官方 | airx | 判定 |
| --- | --- | --- | --- |
| `embedContent` 请求体 | `model`(required)、`content`(required)、`title`、`taskType`、`autoTruncate`、`outputDimensionality`、`documentOcr`、`audioTrackExtraction`（`docs/api-reference__embeddings__embed-content.md:30-40`、`:1021-1070`） | `content` / `taskType` / `title` / `outputDimensionality` = 4/8（`embeddings.rs:19-36`） | (b) 缺 4 个（`model` 同 §0 第 3 条） |
| `batchEmbedContents` 请求 | `requests[]`，每项 `model`(required) + 同单条字段（`docs/api-reference__embeddings__batch-embed-contents.md:30-46`） | `BatchEmbedContentsRequest` + `BatchEmbedItem`（含 `model`，由 `from_request` 自动填） | 一致（`embeddings.rs:134-179`） |
| `embedContent` 响应 | REST：`embedding` + `usageMetadata`（`:967-979`）；SDK 类型：`embeddings[]` + `metadata` + `truncated` + `usageMetadata`（`manifest.json` → `EmbedContentResponse`） | 只解析 `embedding`（`embeddings.rs:123-132`） | **(c)/(d) 漂移风险** |
| `Embedding.values` | `number[]`，另有 `shape`（`:990-1000`） | `Vec<f32>`，`shape` 进 extensions | (c) 精度/字段（P2） |

> 这里两种官方来源互相矛盾（REST 参考单数、SDK 类型数组）。airx 选了单数；当上游按 SDK 形状返回
> `embeddings[]` 时，`EmbedContentResponse.embedding == None`，向量被静默收进 `extensions`。

### 2.7 `models.list` / `models.get`

- `ListModelsRequest`：`pageSize` + `pageToken` ✓（`models.rs:82-130`），与官方一致
  （`docs/api-reference__models__list-models.md:18-24`）。
- 但 query 编码只放行 `[A-Za-z0-9._:-]`（`models.rs:224-234`），官方 `nextPageToken` 是不透明串，
  含 `+` `/` `=` 时 `list` / `list_all` 直接报 `Configuration`（该限制被写进文档 `docs/gemini.md:481`，
  属"文档一致、与官方不一致"）。
- `Model` typed 11/13 个 REST 字段（`models.rs:20-58` vs `docs/api-reference__models__list-models.md:134-214`），
  缺 `maxTemperature`、`thinking`（extensions 可读）；SDK 类型另有 `labels` / `endpoints` / `tunedModelInfo`
  / `checkpoints` / `supportedActions`。

---

## 3. 响应字段、Part 与流式增量缺口

### 3.1 响应顶层与 Candidate

| 类型 | 官方字段 | airx typed | 缺口 | 位置 |
| --- | ---: | --- | --- | --- |
| `GenerateContentResponse` | 8 个 wire 字段（9 含 `sdkHttpResponse`）：`candidates` / `createTime` / `automaticFunctionCallingHistory` / `modelVersion` / `promptFeedback` / `responseId` / `usageMetadata` / `modelStatus` | 5 | `createTime`、`modelStatus`（extensions）；`automaticFunctionCallingHistory` 属 SDK 自动函数调用历史 → **不做** | `response.rs:14-36` |
| `Candidate` | 12 | 11 | `continuationToken`（与 `finishReason=CONTINUATION` 配套） | `response.rs:72-112` |
| `PromptFeedback` | 3：`blockReason` / `blockReasonMessage` / `safetyRatings` | 2 | `blockReasonMessage`（业务排障最需要的可读原因） | `response.rs:137-150` |
| `UsageMetadata` | 12 | 8 typed（2 个 details 为 `Value`） | `cacheTokensDetails`、`toolUsePromptTokensDetails`、`trafficType`、`initialPromptTokenCount` | `common.rs:97-128` |
| `SafetyRating` | 7 | 6 | `overwrittenThreshold` | `common.rs:130-154` |

### 3.2 `Part`：20 个 part 级字段，airx typed 9

官方 `Part` 是"一个 part 级对象 + 互斥的 data 成员"（`dist/genai.d.ts:14750-14860`，REST 参考
`docs/api-reference__generate-content__generate-content.md:3868-4560`）。part 级字段清单（含 data 成员共 20 项）：
`text`、`thought`、`thoughtSignature`、`inlineData`、`fileData`、`functionCall`、`functionResponse`、
`executableCode`、`codeExecutionResult`、`videoMetadata`、`partMetadata`、`mediaResolution`、
`toolCall`、`toolResponse`、`audioTranscription`、`mediaProcessing`、`speechMetadata`、`documentMetadata`、
`spreadsheetMetadata`、`audioMetadata`。

| airx | 覆盖 | 说明 |
| --- | --- | --- |
| `Part::Text(TextPart)` | `text` / `thought` / `thoughtSignature` | 唯一在 **part 级**捕获未知兄弟键的变体（`TextPart` 直接对整段 JSON 反序列化，`content.rs:237-254`、`:400-402`） |
| `Part::InlineData` / `FileData` / `ExecutableCode` / `CodeExecutionResult` | 4 个 data 成员 | 兄弟键全部丢弃（见 §3.3） |
| `Part::FunctionCall(FunctionCallPart)` | `functionCall` 的 3/5 子字段（`id` / `name` / `args`） | 签名错位（§3.3）；`partialArgs` / `willContinue` 为 Vertex 专属（`dist/genai.d.ts:6643-6650`），非缺口 |
| `Part::FunctionResponse(FunctionResponsePart)` | `functionResponse` 的 3/6 子字段 | 缺 `parts`（多模态）/ `scheduling` / `willContinue`；后两者为非阻塞函数（Vertex/异步工具）语义 |
| `Part::Other(Value)` | 其余 11 个 part 级成员 | wire 保留，但无 typed 访问器（`content.rs:156`） |

`toolCall` / `toolResponse` 值得单独说明：官方描述为"服务端工具调用/结果，客户端应原样 echo 回 API"
（`dist/genai.d.ts:14753-14758`）。这是纯协议载荷（不要求 airx 执行工具），但当前只能走 `Part::Other`。

### 3.3 P0：`thoughtSignature` 层级错误 + 兄弟键丢失

官方结构（`docs/thought-signatures--generateContent.md:240-262`，REST 参考 `:4534-4540`）：

```json
{ "functionCall": { "name": "check_flight", "args": { "flight": "AA100" } },
  "thoughtSignature": "<Signature A>" }
```

`thoughtSignature` 是 **Part 级**兄弟键；官方 `FunctionCall` 接口只有 `args` / `id` / `name` / `partialArgs`
/ `willContinue`，**没有** `thoughtSignature`（`dist/genai.d.ts:6636-6651`）。

airx 现状：

1. `FunctionCallPart` 声明了 `thought_signature`（`content.rs:302-307`），序列化时被包进 `functionCall`
   对象内部（`content.rs:365` → `serialize_wrapped` `:377-386`），产出
   `{"functionCall":{…,"thoughtSignature":"…"}}` —— 层级错误，官方不认。
2. 反序列化 `{"functionCall":{…},"thoughtSignature":"…"}` 时，`Part::inner` 只 `remove("functionCall")`
   并把其余键**丢弃**（`content.rs:427-432`、`:444-447`），所以真实签名根本没进模型。
3. 后果与官方文档描述的失败模式吻合：官方类型与方法只认 Part 级签名，airx 发出的嵌套字段等价于
   "没带签名"（解码侧则是把真实签名丢了）。Gemini 3/4 多轮函数调用必须回传签名，缺失即报错
   （`docs__thought-signatures--generateContent.md:98-140`，其中明确 "If you omit a thought_signature for the
   first functionCall part in any … you will error out"，"400 error"）。官方还提供了专门的
   `finishReason = MISSING_THOUGHT_SIGNATURE`（`dist/genai.d.ts:6492-6540`）。
4. 同一机制导致所有**非 text 变体**的 part 级兄弟键丢失：`{"inlineData":{…},"videoMetadata":{…}}`、
   `{"functionResponse":{…},"partMetadata":{…}}` 往返后元数据消失 —— 与 `docs/gemini.md:196`、
   `docs/compatibility.md:128` 承诺的"未知结构原样保留"相矛盾（`Part::Other` 才保留）。

现有测试没有覆盖"函数调用 + 签名"的组合：`content.rs:470-499` 的 roundtrip 只给 text part 加了
`thoughtSignature`。这正是该 bug 能长期存在的原因。

### 3.4 流式增量组装

官方：每个 SSE `data:` 是一个 `GenerateContentResponse` 增量，参考示例固定带 `?alt=sse`
（`docs/api-reference__generate-content__stream-generate-content.md:731`、`:845`），参考文档中没有 `[DONE]`
哨兵 —— 快照全文检索里 `[DONE]` 只出现在 Interactions 示例（`docs__streaming.md:195`、
`docs__thinking.md:666-667`、`docs__migrate-to-interactions.md:3428-3429`），即 `event: done` +
`data: [DONE]` 的命名事件流。

airx 聚合器（`generate_content/accumulator.rs`）：

| 行为 | 现状 | 判定 |
| --- | --- | --- |
| text 增量 | 按 `thought` 标记分段拼接（`:182-203`） | 正确 |
| `thoughtSignature` | 只在 text 增量上补写（`:189-191`）；functionCall 的签名根本到不了这里 | P0（同 §3.3） |
| `inlineData` 增量 | 每个 chunk 追加为独立 `Part::InlineData`，不拼接 base64（`:182-203` 的 `other => target.push(other)`） | (d) 待验证：若按 chunk 流式返回图片分片，聚合结果不可用。快照文档未明确 generateContent 的图片流式分片规则，标为"待真机验证"（P2） |
| 多候选 | 任一候选出现终态 `finishReason` 即置 `Completed` 并**停止吸收后续内容**（`:89-100`、`:44-53`） | (d) `candidateCount > 1` 时后到候选内容被丢；且 `finish()` 提前成功（P2） |
| 完成判定 | 任何非空且非 `FINISH_REASON_UNSPECIFIED` 的 `finishReason` 都算成功（`:133-135`） | (d) 见 §4.2（`CONTINUATION`、`SAFETY`、`NO_IMAGE` 等一律 Completed） |
| `promptFeedback.blockReason` | 视为完成并保留 feedback（`:89-100`） | 正确（与 `docs/compatibility.md:126-127` 一致） |
| 终态后再到的事件 | 只吸收 `usageMetadata` 与顶层 extensions（`:44-53`） | 与文档一致（`docs/gemini.md:341`） |
| `StreamStatus::Failed` 分支 | `finish()` 有分支（`:114`），但 `apply` 从不置 `Failed` → 死代码 | 代码清晰度（P2） |
| `[DONE]` 帧 | `decode_event` 直接 `serde_json::from_str`，`[DONE]` 会变成 `Deserialization` 错误（`endpoint.rs:167-182`） | (d) 官方 generateContent 不发；网关/未来协议可能发 → 建议跳过（P2） |

---

## 4. 行为差异

### 4.1 默认值与隐式行为

| 项 | 官方 | airx | 证据 |
| --- | --- | --- | --- |
| `includeThoughts` 默认 | 默认 false（不返回思考内容） | `ThinkingConfig::budget(n)` / `level(l)` 强制 `includeThoughts = true`（`dynamic()` 同样） | `generation.rs:132-167`；`docs/gemini.md:246-251` 已写明 |
| `thinkingBudget` + `thinkingLevel` 同时出现 | Gemini 4：同时发送返回 **400**；`thinkingBudget` 已 deprecated，只发它会映射到最近的 level 并告警 | `ThinkingConfig` 允许同时为 `Some`（公开字段，`generation.rs:115-130`），构造器会自动清另一个，但结构体字面量可以造出 400 组合 | `docs/thinking--generateContent.md:939` |
| `thinkingLevel` 取值 | SDK 枚举为 `MINIMAL|LOW|MEDIUM|HIGH|XHIGH`（`dist/genai.d.ts:18024-18050`）；REST 示例用小写 `"xhigh"` / `"low"`（`docs/thinking--generateContent.md:750`、`:892`） | `Option<String>`，不校验（`generation.rs:126`） | 保持 String 可接受，但应文档化大小写 |
| 采样参数 | `temperature` / `topP` / `topK`：Gemini 4 与 3.6+ **忽略并返回 deprecation 警告头，未来 400**；更早的 Gemini 3 也建议保持默认 | builder 仍提供 `.temperature()`，`compatibility.md:157-160` 称"仍在 generationConfig 中"（暗示有效） | `docs/api-reference__generate-content__generate-content.md:164`、`:168`、`:172`；`docs/text-generation--generateContent.md:534-542` |
| `model` 进 body | REST 把 `model` 标 required（generateContent `:32-36`、stream `:28-32`、countTokens `:19-26`、embedContent `:30-40`、batch item `:36-40`） | 只在路径里；单条 embed 不写，批量为每项写 | `endpoint.rs:101-129`、`embeddings.rs:157-169` |

### 4.2 完成语义

官方 `FinishReason` 有 24 个值（`dist/genai.d.ts:6492-6540`），其中大量是"调用完成但结果不可用"：
`SAFETY`、`RECITATION`、`BLOCKLIST`、`PROHIBITED_CONTENT`、`SPII`、`IMAGE_SAFETY`、`NO_IMAGE`、
`MALFORMED_FUNCTION_CALL`、`UNEXPECTED_TOOL_CALL`、`TOO_MANY_TOOL_CALLS`、`MISSING_THOUGHT_SIGNATURE`、
`MALFORMED_RESPONSE`、`MODEL_ARMOR`、`CONTINUATION`。

airx 把它们一律当 `Completed`（`accumulator.rs:133-135`；非流式响应也只有一个
`finish_reason() -> Option<&str>`，`response.rs:58-61`）：

- `CONTINUATION`：官方语义是"用 `continuationToken` 续写"，SDK 默认
  `automaticContinuation = true` 会自动重发（`dist/genai.d.ts:7373-7378`；REST 字段
  `docs/api-reference__generate-content__generate-content.md:703-709`）。airx 视为成功 → **长输出静默截断**。
  （注意：SDK 的自动续写是客户端循环，airx 不应默认实现，但必须能识别并暴露 token。）
- `SAFETY` / `NO_IMAGE` / `MALFORMED_*`：`final_text()` 返回空串或半截内容，`finish()` 返回 `Ok`。
  这与"截断不伪装成成功"（`docs/protocol-support.md:93-95`）只覆盖了 EOF 场景，没覆盖 finishReason 场景。

### 4.3 错误映射

| 场景 | 官方形状 | airx | 判定 |
| --- | --- | --- | --- |
| 非 2xx + JSON `error` | `{error:{code,message,status,details}}` | → `ProviderError`（status / code=status、回退 code / message / requestId / headers / raw_body） | 一致（`common.rs:35-63`） |
| 流内错误对象 | 部分网关在 SSE data 里直接给 `{"error":…}` | → `ProviderError`，无 status/headers | 一致、已文档化（`endpoint.rs:175-177`、`common.rs:66-78`、`docs/gemini.md:327-329`） |
| `error.details`（`RetryInfo` / `QuotaFailure`） | 结构化数组 | 只在 `raw_body`，无 typed 访问器 | (b) 可接受（P2） |
| 非 JSON 错误体 | — | 退化为 `HttpError`（保留 body） | 一致（`common.rs:56-63`） |
| 429 / 5xx 重试 | — | POST 非幂等不重试，GET 重试（`docs/gemini.md:437-443`） | 设计选择，非缺口 |

### 4.4 结构化输出

- 官方：`responseMimeType` 取 `text/plain|application/json|text/x.enum`（`docs/api-reference__generate-content__generate-content.md:180`）；
  `responseSchema`（OpenAPI 子集）与 `responseJsonSchema`（标准 JSON Schema）二选一；schema 子集不支持的部分会被忽略
  （`docs__structured-output--generateContent.md:963`、`:1060-1061`）。
  **但 SDK 3.6.1 已把 `responseMimeType` / `responseSchema` 标注为 deprecated，推荐 `responseFormat`**
  （`dist/genai.d.ts:7863`、`:7866`、`:7870`）—— 这是结构化输出路径上最值得盯的漂移。
- airx：两个 schema 字段都 typed（`generation.rs:35-40`），`.json_schema()` 只写 `application/json` +
  `responseSchema`（`request.rs:190-199`、`generation.rs:95-101`），没有 `text/x.enum` 快捷方式；
  `parse_as::<T>` 只做 serde 解析、不校验 schema（`crates/airx/src/gemini.rs:147-153`、
  `crates/airx/src/response.rs:77-86`，`docs/gemini.md:281-287`）。
- 判定：(c) 小缺口（`text/x.enum`、`responseJsonSchema` 快捷方式）+ (d) 上表所述的依赖风险，无行为错误。

### 4.5 工具载荷

- `functionCall.args` 是完整 JSON 对象（`docs/compatibility.md:125-126`、REST 参考 `:3906-3912`），
  airx `args: Value`（`content.rs:298-300`）✓，流式也不拼增量（Vertex 专属的 `partialArgs` 不适用）。
- `functionResponse.response` 官方 required（`:3925-3931`），airx 默认 `{}`（`content.rs:322`），可接受。
- `functionResponse.parts`（多模态结果）官方支持，并允许 `response` 里用 `{"$ref": "<displayName>"}` 引用
  （`:3933-3995`、`docs/function-calling--generateContent.md` 的 multimodal 段）。airx 未建模 → §0 第 2 条。
- 服务端工具（`googleSearch` / `codeExecution` / `urlContext` …）只能 `Tool::server(kind, json!({}))`
  （`tool.rs:39-47`），结果侧 `groundingMetadata` / `citationMetadata` / `urlContextMetadata` 是 `Value`
  （`response.rs:91-99`），`executableCode` / `codeExecutionResult` 是 typed。判定：(b) + (c)（P2）。

---

## 5. 业务接入易用性与代码清晰度

以下建议均保持"只做协议载荷、不做 agent 层"：不新增执行循环、不自动执行工具、不维护会话。

### 5.1 修 P0 的数据模型（必须先做，其余建议才有意义）

```rust
// 现状：签名藏在 functionCall 内部，且解析时丢失（content.rs:302-307 / :427-432）
// 目标：Part 级元数据统一由 Part 承载，data 成员保持纯净

pub struct PartMeta {              // 方案 A：显式结构（更清晰，但改动面大）
    pub thought_signature: Option<String>,
    pub part_metadata: Option<Value>,
}

impl Part {
    pub fn thought_signature(&self) -> Option<&str>;
    pub fn with_thought_signature(self, signature: impl Into<String>) -> Self;
    pub fn part_metadata(&self) -> Option<&Value>;
}

// 方案 B（最小改动）：保留各变体的 extensions，但
//   serialize: {"functionCall": {...}, "thoughtSignature": "..."}   // 提到兄弟层
//   deserialize: 兄弟层 thoughtSignature 写入 FunctionCallPart.thought_signature，
//                其余未知兄弟键写入该变体的 extensions（不再丢弃）
impl FunctionCallPart {
    pub fn thought_signature(&self) -> Option<&str>;   // 兼容现有字段访问
}
```

验收点：`{"functionCall":{"name":"f","args":{}},"thoughtSignature":"sig"}` 反序列化后再序列化语义等价；
`{"inlineData":{…},"videoMetadata":{…}}` 往返不丢 `videoMetadata`；新增 fixture 覆盖"turn1 模型返回 → turn2 原样回传"。

### 5.2 完成语义与可读访问器（业务最容易踩坑的地方）

```rust
pub enum FinishReason {                    // 覆盖度对齐 dist/genai.d.ts:6492-6540
    Stop, MaxTokens, Safety, Recitation, Blocklist, ProhibitedContent, Spii,
    MalformedFunctionCall, MissingThoughtSignature, NoImage, Continuation,
    Other, Unknown(String),
}

impl GenerateContentResponse {
    pub fn finish_reason_typed(&self) -> Option<FinishReason>;  // 保留现有 finish_reason() -> &str
    pub fn is_blocked(&self) -> bool;      // blockReason / SAFETY / BLOCKLIST / PROHIBITED_CONTENT / SPII / NO_IMAGE
    pub fn is_truncated(&self) -> bool;    // MAX_TOKENS
    pub fn requires_continuation(&self) -> Option<&str>;       // CONTINUATION → continuationToken
    pub fn block_message(&self) -> Option<&str>;               // PromptFeedback.blockReasonMessage
    pub fn candidate_count(&self) -> usize;
}
```

同时给 `GenerateContentStreamAccumulator`：`CONTINUATION` 不置 `Completed`（`accumulator.rs:133-135`），
避免长输出静默截断；`finish()` 里 `Failed` 死分支改为可达或删除（`:114`）。

### 5.3 构建器：让常见路径零 `Result`、零 JSON 拼接

```rust
impl GenerateContentRequest {
    pub fn prompt(text: impl Into<String>) -> Self;   // 单轮文本，无需 build()?（contents 天然非空）
    pub fn of_user(text: impl Into<String>) -> Self;  // 语义更明确的别名
}

impl GenerateContentRequestBuilder {
    pub fn image_bytes(mut self, mime: impl Into<String>, bytes: &[u8]) -> Self; // 内部 base64
    pub fn video_bytes(...) ;                          // 可选：需要 base64 依赖
    pub fn service_tier(mut self, tier: ServiceTier) -> Self;      // 新字段落地后
    pub fn labels(mut self, labels: impl IntoIterator<Item = (String, String)>) -> Self;
    pub fn safety_identifier(mut self, id: impl Into<String>) -> Self;  // labels.safety_identifier
}
```

理由：当前每个调用都要 `.build()?`（`request.rs:223`），且 base64 由调用方负责（`Part::inline_image`
接字符串，`content.rs:180-187`），业务接入摩擦最大的一步正好在这里。

### 5.4 工具声明：把"服务端工具"从 JSON 字符串升级为可发现 API

```rust
impl Tool {
    pub fn google_search() -> Self;                       // {"googleSearch":{}}
    pub fn code_execution() -> Self;
    pub fn url_context() -> Self;
    pub fn file_search(store: impl Into<String>) -> Self; // {"fileSearch":{"fileSearchStoreNames":[…]}}
    pub fn google_maps() -> Self;
}
impl ToolConfig {
    pub fn include_server_side_invocations(mut self, yes: bool) -> Self;
}
impl FunctionCallingConfig {
    pub fn auto() -> Self; pub fn any() -> Self; pub fn none() -> Self; pub fn validated() -> Self;
}
```

`Tool::server(kind, config)` 保留作为逃生通道（`tool.rs:39-47`）；不要新增任何"自动调用/自动重放"能力。

### 5.5 计 token 与 Embeddings 的形状

```rust
impl CountTokensRequest {
    pub fn of(request: &GenerateContentRequest) -> Self;   // 官方 generateContentRequest 形态
}
impl Gemini {
    pub fn count_tokens(&self, model: impl Into<ModelId>) -> CountTokensCall; // 语义归属比挂在 generate_content 下更清楚
}

impl EmbedContentResponse {
    pub fn embeddings(&self) -> &[Embedding];  // 兼容单数 embedding 与数组 embeddings
    pub fn values(&self) -> &[f32];            // 取第一条
}
impl EmbedContentRequest {
    pub fn model(mut self, model: impl Into<ModelId>) -> Self;  // 显式覆盖；端点默认自动填
}
```

现状 `gemini.generate_content(m).count_tokens(...)`（`crates/airx/src/gemini.rs:156-165`）把"计 token"
挂在生成调用下，和 `models().get/list` 的归属不一致；`docs/gemini.md:530` 也照抄了这种形状。

### 5.6 模型 API 命名一致性

`GeminiModels::get`（`crates/airx/src/gemini.rs:204`）与 `OpenAiModels::retrieve` /
`AnthropicModels::retrieve`（`crates/airx/src/openai.rs:357`、`anthropic.rs:211`）不一致；文档却统一写
`retrieve()`（`docs/compatibility.md:194`）。建议 Gemini 加 `retrieve` 别名或改文档（见 §6）。

---

## 6. 文档与代码不一致

| # | 文档说法 | 实际 | 判定 | 证据 |
| --- | --- | --- | --- | --- |
| 1 | "`thoughtSignature` 在 `TextPart` / `FunctionCallPart` 上" | `FunctionCallPart` 上的位置错误（官方是 Part 级）；且解析时被丢弃 | 与官方不符 + 掩盖 P0 | `docs/gemini.md:198`；`docs/compatibility.md:124`（"已建模"） |
| 2 | 模型列表 `client.models().list() / retrieve()` | Gemini 没有 `retrieve()`，只有 `get()` | 代码与文档不一致 | `docs/compatibility.md:194`、`docs/architecture.md:472` vs `crates/airx/src/gemini.rs:204` |
| 3 | `toolConfig`（AUTO / ANY / NONE） | 官方枚举还有 `VALIDATED`、`MODE_UNSPECIFIED`，airx 未支持 | 与官方不符 | `docs/protocol-support.md:80`、`docs/gemini.md:224` vs `dist/genai.d.ts:6660-6681` |
| 4 | "未覆盖：Vertex AI 专属参数、Live API" | 还有非 Vertex 的未 typed 字段：`serviceTier` / `store` / `labels` / `continuationToken`（请求体）、`logprobs` / `responseLogprobs` / `responseFormat` / `translationConfig` / `audioTranscriptionConfig` 等（generationConfig） | 低估缺口 | `docs/protocol-support.md:91`、`docs/compatibility.md:130`、`docs/architecture.md:589` vs `docs/api-reference__generate-content__generate-content.md:427/608/626/682/699/705/711` |
| 5 | 采样参数"仍在 `generationConfig` 中"（读起来像仍有效） | Gemini 4 / 3.6+ 忽略并回 deprecation 头，未来 400 | 与官方不符 | `docs/compatibility.md:157-160` vs `docs/text-generation--generateContent.md:534-542` |
| 6 | Part 表只列 9 种，未提 `videoMetadata` / `partMetadata` / `toolCall` / `toolResponse` | 这些 part 级字段只能 `Part::Other`，且已知变体会丢兄弟键 | 文档不全 + 隐藏数据丢失 | `docs/gemini.md:186-196`、`docs/protocol-support.md:83` vs `dist/genai.d.ts:14750-14860` |
| 7 | "批量请求会自动为每条 item 写入 `model`" | 单条 `embedContent` 官方 body 也要求 `model`，airx 未写 | 文档只讲了一半 | `docs/gemini.md:412-413` vs `docs/api-reference__embeddings__embed-content.md:30-34` |
| 8 | countTokens 请求字段 = `contents/systemInstruction/tools/generationConfig` | 官方还有 `generateContentRequest` 形态 | 文档不全 | `docs/gemini.md:362-364` vs `docs/api-reference__tokens__count-tokens.md:862-864` |
| 9 | 构建器方法表 | 漏了 `.contents(...)`（`request.rs:95`）与 `.extension(...)`（`request.rs:217`） | 文档不全 | `docs/gemini.md:169-182` |
| 10 | `promptFeedback` = `block_reason` + `safety_ratings` | 官方还有 `blockReasonMessage` | 与官方不完整（代码与文档一致） | `docs/gemini.md:310` vs `dist/genai.d.ts:7560-7568` |
| 11 | 流式说明页位于 `docs__streaming.md`（快照中只有 Interactions 版，含 `data: [DONE]`） | generateContent 无 `[DONE]`；airx 结论正确，但"证据页"不是 generateContent 版 | 快照口径需注意（非代码问题） | `docs__streaming.md:195`、`reference/gemini/docs/meta.json` 无 `streaming.md?generate-content=true` |
| 12 | `response_schema` 标注为"兼容性最好"（`docs/gemini.md:236`） | SDK 3.6.1 已把该字段标 deprecated，推荐 `responseFormat` | 与官方演进不符（会给业务"这是首选"的错觉） | `docs/gemini.md:236`、`:283` vs `dist/genai.d.ts:7866`、`:7870` |

---

## 7. 风险与注意事项

### 7.1 `generateContent` 已被官方称为 legacy（战略风险，不是缺口）

- 官方："The Interactions API is our new interface… As of June 2026, it is Generally Available and the
  recommended interface for all new projects. While it is now considered legacy, the original
  `generateContent` API remains fully supported."（`docs__interactions-overview.md:8-16`）
- 官方："new models and capabilities beyond the core mainline family, along with new agentic capabilities and
  tools, will launch on the Interactions API."（`docs__interactions-overview.md:35-39`）
- 迁移指南确认 generateContent "remains fully supported"，但推荐新项目用 Interactions
  （`docs__migrate-to-interactions.md:12-15`）。

对 airx 的含义：

1. **不建议现在实现 Interactions**：它的核心增量是服务端会话状态（`previous_interaction_id` / `store`）、
   可观测 step、`background=true`、WebSocket/RTC（`docs__interactions-overview.md:16-26`），与"不做 agent 层"
   的定位冲突面很大；且 `interactions.create` 的 SSE 事件模型、错误模型都不同（`docs__streaming.md:163-195`、
   `:331`）。
2. **但必须准备切换条件**：一旦出现"目标模型只在 Interactions 可用"或"generateContent 收不到新能力"，
   就需要一个最小 Interactions 编解码器（请求 + SSE 增量 + `[DONE]` + `interaction.completed`）。建议在
   `docs/protocol-support.md` 的 Gemini 段记录触发条件与决策，而不是等到必须迁移时才发现。
3. 快照本身仍以 generateContent 为主线（28 页 `?generate-content=true` 版本），本报告结论对当前 3.6.1
   有效；升级 SDK 时需重跑 `reference/gemini/update.sh` 并 diff `api/SUMMARY.md`。

### 7.2 模型路径与命名

- 模型必须在路径中；airx 已规范化裸名与 `models/` / `tunedModels/` / `publishers/` 前缀
  （`endpoint.rs:96-129`），并拒绝 `:` / 空段 / `..`。这条规则同时阻止了 URL 注入。
- 但 `publishers/...` 是 Vertex 形状，Gemini API 不会接受（属误用防护缺失，非 bug）；`docs/gemini.md:121`
  把它列为合法输入，容易误导。
- 官方 REST 同时要求 body 里的 `model`（§4.1），airx 只发路径。若接严格网关，出现 400 时第一件事是
  用 `.extension("model", json!("models/…"))` 补齐。

### 7.3 Schema 漂移

- 官方 `Part` 有 20 个 part 级字段、`Candidate` 12 个、`UsageMetadata` 12 个、`FinishReason` 24 个值，
  且新增速度很快（本快照已包含 `toolCall` / `toolResponse` / `mediaProcessing` / `trafficType` /
  `MODEL_ARMOR` 等）。airx 靠 `extensions` / `Part::Other` 抗漂移，但 **Part 兄弟键丢失**（§3.3）
  是这套策略的破口：一旦新字段作为兄弟键出现，已知变体会把它吃掉。
- 结构化输出正在换轨：SDK 3.6.1 把 `responseMimeType` / `responseSchema` 标为 deprecated，推荐
  `responseFormat`（`dist/genai.d.ts:7863-7871`）；REST 参考仍写旧字段（`:180`）。airx 现走旧字段，
  属"能跑但已在退役通道"。
- `EmbedContentResponse` 的"单数 vs 数组"是当前最典型的漂移案例（§2.6），建议按"两种都认"实现。
- `SafetySetting` 必填化、`ThinkingLevel` 大小写、`responseSchema` vs `responseJsonSchema` 这类细节
  都应以最新快照为准，不要以旧博客/旧 SDK 为准。

### 7.4 快照本身的读取注意

- `manifest.json` 的 enum 成员名提取失败（`other_types[].values` 全是 `"undefined: undefined"`），
  需回看 `dist/genai.d.ts` 对应行号（本报告已如此处理）。这是快照脚本的小缺陷，建议修
  `reference/gemini/scripts/extract-api.mjs` 或在 SUMMARY 中标注。
- `sdk/@google-genai@3.6.1/dist/http-routes.txt` 只是符号索引（`# Derived from dist/index.mjs`），
  不含函数体，不能用来推断 SDK 的客户端侧 reshape（例如 `embedding` → `embeddings`）。
- 文档站已迁到 `aistudio.google.com`（`reference/gemini/README.md:26-32`），旧 `ai.google.dev` 只是跳转。

---

## 8. 分级建议

### P0（正确性 bug，立即修）

**P0-1 `thoughtSignature` 层级与保留**

- 文件：`crates/airx-protocol/src/gemini/generate_content/content.rs:302-307`（字段位置）、
  `:359-386`（序列化包裹）、`:426-463`（`Part::inner` 丢弃兄弟键）。
- 动作：
  1. 序列化把 `thoughtSignature` 提到 Part 兄弟层（`{"functionCall":{…},"thoughtSignature":…}`）。
  2. 反序列化从 Part 兄弟层取签名；对其余未知兄弟键 `entry(key).or_insert(value)` 进该变体的
     `extensions`（不要丢）。
  3. 文档同步：`docs/gemini.md:198`、`docs/compatibility.md:124`。
- 验收点：新增 fixture `request_with_signature.json`，包含
  `{"functionCall":{…},"thoughtSignature":"sig"}` 与 `{"inlineData":{…},"videoMetadata":{…}}`；
  roundtrip 后语义等价；`cargo test -p airx-protocol gemini` 全绿（不新增依赖）。
- 风险提示：`FunctionCallPart.thought_signature` 若改为 Part 级，属于公开字段迁移；建议先加
  `Part::thought_signature()` 访问器并把字段标注 deprecated，保一个版本。

**P0-2 非 text 变体的 Part 兄弟键不再丢失**

- 与 P0-1 同源，可合并实现；验收点：`videoMetadata` / `partMetadata` / `mediaResolution` 往返不丢，
  且进 `Part::Other` 的未知 part 仍原样保留。

### P1（重要保真度与接入体验）

| 编号 | 内容 | 文件位置 | 验收点 |
| --- | --- | --- | --- |
| P1-1 | `FunctionResponse.parts` 多模态结果 + `$ref` 语义 | `content.rs:313-327` | `Part::function_response_parts(name, Vec<InlineData…>)` 序列化为官方形状；fixture 往返 |
| P1-2 | `embedContent` 响应兼容 `embeddings[]` + `truncated` + `usageMetadata` | `embeddings.rs:112-132` | 两种响应形状都能取到向量；`values()` 返回非空 |
| P1-3 | 请求体补 `model`（generateContent / countTokens / embedContent） | `endpoint.rs:135-147`、`count_tokens.rs:186-197`、`embeddings.rs:208-227` | 默认发出的 body 含 `model: "models/…"`；现有 fixture 更新；删掉 `BatchEmbedItem` 的特例注释 |
| P1-4 | `pageToken` 百分号编码（不再白名单拒绝） | `models.rs:224-234` | `pageToken="a+b/c=="` 能正常拼 query 并请求（mock 断言路径）；`list_all` 覆盖 |
| P1-5 | `FunctionCallingMode` 补 `Validated` / `ModeUnspecified` | `tool.rs:152-162` | 序列化 `VALIDATED`；反序列化 `VALIDATED` 成功；保留旧值 |
| P1-6 | `FunctionDeclaration` 补 `parametersJsonSchema` / `responseJsonSchema` / `behavior` | `tool.rs:51-95` | typed 字段 + builder；标准 JSON Schema 不再塞 `parameters` |
| P1-7 | 完成语义分类：`CONTINUATION` 不算完成；`is_blocked()` / `is_truncated()` | `response.rs:38-70`、`accumulator.rs:89-135` | `finishReason=CONTINUATION` 时 `finish()` 返回 `Incomplete`（或 `requires_continuation()` 有值）；`SAFETY` 时 `is_blocked()==true` |
| P1-8 | `PromptFeedback.blockReasonMessage` typed | `response.rs:137-150` | 反序列化可见；`block_message()` 访问器 |
| P1-9 | `ToolConfig.includeServerSideToolInvocations` typed | `tool.rs:98-117` | 序列化到 `toolConfig.includeServerSideToolInvocations` |
| P1-10 | 文档修正：采样参数在 Gemini 3.6+/4 失效、`VALIDATED`、`retrieve()` 命名、未覆盖字段清单 | `docs/compatibility.md:157-160`、`docs/protocol-support.md:80-91`、`docs/gemini.md:198/224`、`docs/architecture.md:472` | 与 §6 表格逐条对齐 |
| P1-11 | 结构化输出换轨准备：新增 `.json_schema_std(Value)`（写 `responseJsonSchema`）；文档标注 `responseSchema` 已被 SDK 标 deprecated | `request.rs:190-199`、`generation.rs:95-101`、`docs/gemini.md:236/283` | 两条路径各有测试；`docs/gemini.md` 说明何时选哪个 |

### P2（小众但成本低）

| 编号 | 内容 | 文件位置 |
| --- | --- | --- |
| P2-1 | `generationConfig` 补 `logprobs` / `responseLogprobs` / `translationConfig` / `audioTranscriptionConfig` / `enableAffectiveDialog` / `enableEnhancedCivicAnswers`；`speechConfig` / `imageConfig` 结构化 | `generation.rs:13-68` |
| P2-2 | 请求顶层补 `serviceTier` / `store` / `labels` / `continuationToken`（含 `Candidate.continuationToken`） | `request.rs:17-63`、`response.rs:72-112` |
| P2-3 | `CountTokensRequest` 支持 `generateContentRequest` 形态（`CountTokensRequest::of(&GenerateContentRequest)`） | `count_tokens.rs:22-140` |
| P2-4 | `Part` 补 `videoMetadata` / `partMetadata` / `mediaResolution` typed 字段；`toolCall` / `toolResponse` 至少给 `Other` 之外的读取器 | `content.rs:138-234` |
| P2-5 | `models.computeTokens` 端点；`Model.maxTemperature` / `thinking` typed | `models.rs:20-58` |
| P2-6 | SSE `[DONE]` 帧跳过而非报错；accumulator `Failed` 死分支清理 | `endpoint.rs:167-182`、`accumulator.rs:111-119` |
| P2-7 | 多候选流式：等所有候选终态再 `Completed`，或至少不丢后到内容 | `accumulator.rs:44-53`、`:89-100` |
| P2-8 | `inlineData` 增量合并规则（真机验证图片流式分片后再定） | `accumulator.rs:182-203` |
| P2-9 | Caches CRUD（`create/get/list/update/delete`），复用 `cachedContent` 字段 | 新文件 `gemini/caches.rs` |
| P2-10 | `Embedding.values` 精度（f64）与 `shape` 字段 | `embeddings.rs:112-121` |
| P2-11 | `SafetySetting` 的 `category` / `threshold` 改为 `Option`（官方均为 optional） | `generation.rs:170-191` |

### 明确不做（含理由）

| 项 | 理由 |
| --- | --- |
| Interactions API（本周期） | 核心价值是服务端会话状态 / step 可观测 / 后台任务（`docs__interactions-overview.md:16-26`），与"不做 agent 层"冲突；`generateContent` 官方仍"fully supported"。转为 §7.1 的触发条件观察项 |
| `live.*` / `interactions.connect`（WebSocket/RTC） | 非 HTTP 请求-响应模型，airx 的 Transport/Endpoint 抽象不覆盖；`SUMMARY.md:165-189`、`:150-153` |
| AGENTS / triggers / environments / credentials / voices / webhooks / decisions / authTokens | 托管 Agent 平台资源，属编排与状态管理 |
| `chat` / `chats`（SDK 会话封装） | SDK 在客户端维护 `history` 并自动 append（`SUMMARY.md:61-75`），属状态层；airx 明确由调用方维护 `contents`（`docs/gemini.md:16`） |
| `automaticFunctionCalling` / `automaticFunctionCallingHistory` | SDK 客户端自动执行函数并回填历史（`dist/genai.d.ts:1243-1255`、`:7349`、`:7415`），就是 agent 循环 |
| `automaticContinuation` 的客户端自动重发 | 多请求循环与计费放大（`dist/genai.d.ts:7373-7378`）；airx 只暴露 token 与判定（P1-7） |
| `tunings` / `fileSearchStores` / 图像视频 `predict*` / `batchGenerateContent` | 管理面、托管 RAG、长任务编排，超出请求工具包职责 |
| Vertex AI 专属字段（`routingConfig` / `audioTimestamp` / `streamFunctionCallArguments` / `SafetySetting.method` / `Tool.retrieval`） | SDK 注释明确 "not supported in Gemini API"（`dist/genai.d.ts:7846`、`:7872`、`:6653`、`:16420`、`:18227`） |

---

## 9. 不做清单（agent 相关，与理由）

airx 的定位是"请求工具箱"：只负责把请求编码对、把响应解出来。以下能力即使官方 SDK 内置，也不做：

1. **工具执行与函数调用循环**：`functionCall` 只作为响应载荷返回；`functionResponse` 只作为请求载荷发送。
   官方 SDK 的自动函数调用（含 `automaticFunctionCallingHistory`）属于循环编排，不做。
   依据：`docs/gemini.md:15`、`docs/compatibility.md:125-128`。
2. **会话 / 状态管理**：`chat` 封装的 `getHistory` / `sendMessage`（`SUMMARY.md:61-75`）不做；多轮历史由调用方
   传入 `contents`。Interactions 的 `previous_interaction_id` / `store` 同理不做（若未来做 Interactions 编解码，
   也只透传这两个字段，不实现状态语义）。
3. **服务端工具结果的加工**：`googleSearch` / `codeExecution` / `urlContext` / `fileSearch` 只声明与解析，
   不代理请求、不聚合引用、不重试。依据：`docs/gemini.md:451-453`。
4. **Agents / Triggers / Environments / Credentials / Webhooks / Voices / Decisions**：托管 Agent 平台的
   生命周期与控制面，与"不编排"直接冲突。
5. **Live / Realtime（WebSocket/RTC）**：需要长连接状态机与实时音频编解码，超出 HTTP 请求模型；
   需要时用 `Gemini::execute` 逃生通道之外的自建实现。
6. **向量库 / RAG 服务化**：`fileSearchStores` 是服务端索引与检索，属于"拿到结果之后做什么"的前置，
   且与 airx 的"协议 × 部署"分层不符。
7. **重试/续写的隐式多次请求**：`automaticContinuation` 类行为会把一次业务调用变成多次计费请求；
   airx 只在协议层暴露 `continuationToken` 与完成判定，由调用方决定是否续写。

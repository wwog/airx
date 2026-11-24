# OpenAI：官方行为 vs airx 差距分析

- 基准：openai@7.63.0（npm latest）。API 清单生成于 `2026-11-24T01:59:58Z`
  （`reference/openai/api/manifest.json`，90 resource / 155 带请求体方法 / 352 请求体类型），
  SDK 快照为 `reference/openai/sdk/openai@7.63.0/`。
- 文档快照：`2026-11-24T01:55:07Z`，278 页（`reference/openai/docs/`；下文引用其
  `api/reference/**` 与 `api/docs/guides/**` 原文）。
- 代码基准：`crates/airx-protocol/src/openai/**`、`crates/airx/src/openai.rs` 及
  `builder.rs` / `api.rs` / `stream.rs` / `response.rs` / `options.rs`（2026-11-24 工作区）。
- 分析范围：Chat Completions、Responses（含生命周期与 input_tokens）、Models、Files、
  Batches、Embeddings，以及这些请求会直接依赖的流式/错误/重试行为。
- 不在范围：Agent 运行时（Agents/Assistants/Threads/ChatKit）、`beta.agents`、MCP、
  Realtime、Vector Stores、Fine-tuning、Evals、Images、Audio、Admin/Org 接口。
  这些只在下文用于说明“为什么不做”。
- 可达性标记（贯穿全文）：`T` 已类型化；`E` 只能 `.extension(...)` 透传（wire 兼容但无类型）；
  `X` 不可达（同名字段已类型化，`extension` 会写出重复 JSON key，见 `docs/openai.md:526-528`；
  只能用 `OpenAi::execute`/自定义 Endpoint）；`M` 完全缺失。

## 0. 一页结论（Top 5–8）

| 差距 | 影响 | 优先级 | 建议动作 |
| --- | --- | --- | --- |
| Responses 函数工具与 `tool_choice` 复用了 Chat 的嵌套结构：`{"type":"function","function":{...}}`，官方是扁平的 `{type,name,parameters,strict}` / `{type,name}` | Responses 工具调用请求会被服务端拒绝或工具永不触发；`input_tokens` 计数同样受影响。属确定性协议错误 | **P0** | 新增 `ResponsesFunctionTool` 与 `ResponsesToolChoice`，替换 `ResponsesTool::Function(FunctionTool)`（`responses/tool.rs:15-33`）和 `ResponsesRequest.tool_choice: Option<ToolChoice>`（`request.rs:509`） |
| Embeddings 暴露了 `encoding_format("base64")`，但响应 `embedding` 固定 `Vec<f32>` | 用户一旦使用官方支持的 base64 压缩，整包反序列化失败 | **P0** | `Embedding.embedding` 改为 untagged `EmbeddingVector { Float(Vec<f32>), Base64(String) }` + 解码访问器（`embeddings.rs:62,173-179`） |
| Chat 消息无法表达 `developer` 角色（也没有 `name`、assistant `refusal`、`input_audio`/`file` 片段） | o-series/gpt-5 生产提示词大量使用 `developer`；目前只能改写成 `system` 或用 raw body | **P1** | 给 `ChatMessage` 增 `Developer` 变体与 `name`，`ContentPart` 增 `InputAudio`/`File`/`Other`（`chat/message.rs:60-87`） |
| Responses 输入回传没有任何转换 helper：`OutputItem` 不能直接作为 `InputItem` 复用 | `store:false`/ZDR 多轮必须手工搬运 message/function_call/reasoning；业务接入成本高且易漏字段 | **P1** | `impl From<OutputItem> for InputItem` + `ResponsesResponse::into_input()` + `InputItem::user()/function_call_output()` 构造器（`responses/request.rs:197-239`） |
| Chat 生产参数大面积未类型化：`penalties`、`logprobs/top_logprobs`、`n`、`seed`、`store`、`metadata`、`service_tier`、`safety_identifier`、`prompt_cache_key/options/retention` | 只能 `.extension`，无补全、无文档提示；explicit prompt caching（gpt-5.6+）在 Chat 的 content part 上完全不可达 | **P1** | 按“采样 / 可观测 / 缓存”三组补 `ChatCompletionRequestBuilder` 方法（`chat/request.rs:17-67`） |
| `InputTokensCountRequest::from_request` 静默丢 `previous_response_id`/`conversation`/`parallel_tool_calls`/`truncation`，且 `model` 被强制必填 | token 计数偏小且无告警；无法为“续接会话”做预算 | **P1** | 补齐 4 个字段并让 `from_request` 全量复制；`model` 改 `Option`（`responses/resources.rs:107-149`） |
| Chat 流式把 `refusal`/`system_fingerprint` 落进 `extensions`，`final_response()` 的 `.refusal` 永远为 `None` | 流式与非流式读同一字段结果不同，业务容易写出分支 bug | **P2** | `ChunkDelta` 增 `refusal`/`function_call`；`ChatCompletionChunk` 增 `system_fingerprint`/`service_tier`；accumulator 写回 typed 字段（`chunk.rs:11-75`、`accumulator.rs:38,177-231`） |
| 生命周期/资源细节：`responses.retrieve` 无 query、`responses.compact` 缺失、`models.delete` 缺失、Files 无 `order`/`expires_after`/`expires_at`、Batches 无 `output_expires_after`、Chat stored completions 4 个端点缺失 | 后台响应无法按 `include` 取加密推理、无法压缩长上下文、无法排序文件；均为低频但真实的接入阻塞点 | **P2** | 见 §1 与 §8，按端点小步补齐 |

## 1. 端点覆盖矩阵

官方口径来自 `api/manifest.json` + `sdk/.../resources/**` + `docs/api/reference/resources/**`。
airx 的入口均在 `crates/airx/src/openai.rs`。

| 官方 method / endpoint | airx 状态 | 备注 | 优先级 |
| --- | --- | --- | --- |
| `POST /chat/completions`（`chat.completions.create`） | 已实现 | `Chat::create`（`openai.rs:171`）与 `Chat::stream`（`openai.rs:206`）；请求见 `chat/request.rs:17` | — |
| `GET /chat/completions/{completion_id}` | 缺失 | 仅对 `store:true` 的补全有效；`Chat` 无入口 | P2 |
| `GET /chat/completions`（list，`after/limit/model/order/metadata`） | 缺失 | 同上；`store` 也未类型化 | P2 |
| `POST /chat/completions/{completion_id}`（update metadata） | 缺失 | 官方 SDK `completions.d.ts:83` | P2 |
| `DELETE /chat/completions/{completion_id}` | 缺失 | 官方 SDK `completions.d.ts:114` | P2 |
| `POST /responses` | 已实现 | `Responses::create`（`openai.rs:236`）/`stream`（`openai.rs:322`） | — |
| `GET /responses/{response_id}` | 已实现（参数不全） | 无 `include` / `include_obfuscation` / `starting_after` / `stream`（官方 `ResponseRetrieveParamsBase`，`responses.d.ts:10207-10235`） | P1 |
| `DELETE /responses/{response_id}` | 已实现 | 按文档解析 `{id,object,deleted}`（`resources.rs:29`）；注意 SDK 类型是 `void`，走网关时需容忍 204/空 body | P2（风险） |
| `POST /responses/{response_id}/cancel` | 已实现 | `openai.rs:268` | — |
| `POST /responses/compact` | 缺失 | 官方 `responses.d.ts:104`、`docs/.../responses/methods/compact.md` | P2 |
| `GET /responses/{response_id}/input_items` | 已实现 | `limit/after/order` 齐全（`resources.rs:79-91`） | — |
| `POST /responses/input_tokens` | 已实现（字段不全） | 官方 13 字段，airx 7 个（见 §2.2） | P1 |
| `GET /models` | 已实现 | `openai.rs:348`；官方无 query 参数 | — |
| `GET /models/{model}` | 已实现 | `openai.rs:357` | — |
| `DELETE /models/{model}` | 缺失 | 官方 `models.d.ts:23`；仅 fine-tuned 模型可删 | P2 |
| `POST /files`（multipart） | 已实现（字段不全） | 仅 `file` + `purpose`（`files.rs:198-207`）；缺 `expires_after` | P2 |
| `GET /files` | 已实现（字段不全） | 缺 `order`（`files.rs:118-128`，该结构无 `extensions` → `X`） | P2 |
| `GET /files/{file_id}` | 已实现 | — | — |
| `GET /files/{file_id}/content` | 已实现 | 返回 `Bytes`（`openai.rs:454`） | — |
| `DELETE /files/{file_id}` | 已实现 | — | — |
| `POST /batches` | 已实现（字段不全） | 缺 `output_expires_after`（可 `.extension` 透传，`batches.rs:98`） | P2 |
| `GET /batches` | 已实现 | `limit`/`after` 齐全（`batches.rs:216`） | — |
| `GET /batches/{batch_id}` | 已实现 | — | — |
| `POST /batches/{batch_id}/cancel` | 已实现 | `BatchEndpoint::cancel`（`batches.rs:324`） | — |
| `POST /embeddings` | 已实现（响应有 bug） | 请求 5/5 字段齐全；base64 响应解码失败（§0） | P0 |
| `POST /uploads` + `/uploads/{id}/parts` + `/complete` | 缺失 | 官方用于超大文件/分片；Batch 输入上限 200MB（`docs/api/docs/guides/batch.md:58`），airx 全量 `Bytes` 上传可覆盖 | Out/P2 |
| `POST /moderations` | 缺失 | 非 agent 能力、生产常用、实现成本低；当前只能用 `execute` | P2 |
| `POST /conversations`、`/conversations/{id}`、`/conversations/{id}/items` | 缺失 | airx 用 `previous_response_id` + `conversation` 透传（`request.rs:533`）即可接入服务端会话；CRUD 走 raw | Out/P2 |
| `POST /completions`（legacy） | 缺失 | 官方已由 Chat 取代 | Out |

统计（上述目标面共 25 个端点）：**已实现 19 / 25（76%）**，缺失 6 个
（chat retrieve/list/update/delete、responses.compact、models.delete）。
另有 3 个 auxiliary resource（uploads、moderations、conversations）全部缺失。

## 2. 请求参数与类型缺口

### 2.1 Chat Completions

官方 37 个 body 字段（2 必填 + 35 可选，`ChatCompletionCreateParamsBase`；文档同
`docs/api/reference/resources/chat.md:34-1446`）。airx 类型化 **16 / 37（43%）**，
其中 2 个已弃用字段也未类型化。缺失 21 个：

| 缺失字段 | 业务影响 | 可达性 | 建议 |
| --- | --- | --- | --- |
| `frequency_penalty` / `presence_penalty` | 去重/发散控制，写作与摘要类业务常用 | E | P1 补 builder |
| `n` | 一次多候选（排序/投票） | E | P1 |
| `logprobs` / `top_logprobs` | 置信度、分类打分、路由阈值 | E | P1（响应侧 `ChatChoice.logprobs` 已是 `Value`） |
| `seed` | 可复现实验/回归 | E | P1 |
| `store` / `metadata` | 请求打标、审计、stored completions 前置条件 | E | P1（但 stored completions 端点未实现） |
| `service_tier` | flex/priority 成本与时延档位 | E | P1 |
| `safety_identifier` | 官方已把它与 `prompt_cache_key` 作为 `user` 的替代（`chat.md:1427-1431`） | E | P1（同时文档应提示 `user` 被替代） |
| `prompt_cache_key` / `prompt_cache_options` / `prompt_cache_retention` | 长系统提示词的缓存命中与成本；`options` 仅 gpt-5.6+ 支持（`chat.md:977-999`） | E / **X** | P1；`prompt_cache_breakpoint` 需在 content part 上，airx 的 `ContentPart::Text` 无 `extensions`（`message.rs:31-42`）→ 当前完全不可达，只能 raw body |
| `logit_bias` | 禁用 token / 强制格式 | E | P2 |
| `prediction` | 预测输出降时延（Beta） | E | P2 |
| `web_search_options` | search-preview 模型的服务端检索 | E | P2 |
| `moderation` | 请求级内容审核结果（`ChatCompletionCreateParams.Moderation`） | E | P2 |
| `functions` / `function_call` | 已弃用，被 tools/tool_choice 取代 | E | P2 或不做 |
| `audio` / `modalities` | 音频输出 | E | Out（音频不封装） |

消息/片段缺口（`chat/message.rs`）：

- **`developer` 角色 `X`**：`ChatMessage` 是 `#[serde(tag="role")]` 的封闭 enum
  （`message.rs:58-87`），无 `Other` 变体；官方 `ChatCompletionDeveloperMessageParam`
  （`completions.d.ts:1241`，文档 `chat.md:5971`）无法表达。**P1**。
- `name`（user/assistant/system 均可带）缺失 → `E`? 否：`messages` 已类型化，
  想加 `name` 只能整体替换 messages 数组 → **X**。P2。
- assistant `refusal` / `audio` / `function_call` 缺失 → `X`（同一 enum）。P2。
- user content part 仅 `text` / `image_url`（`message.rs:31-42`）；官方还有
  `input_audio`、`file`（含 `file_id`/`file_data`/`filename`/`prompt_cache_breakpoint`，
  `completions.d.ts:965,1065`）。`ContentPart` 无 `Other` 变体 → **X**。P1（`file` 输入
  是生产常见能力）。
- `tool` 消息 `content` 仅 `String`（`message.rs:81-86`）；官方允许 text part 数组。
  P2。
- 工具调用：`ToolCall.function` 是必填（`tool.rs:95-106`），而官方
  `ChatCompletionMessageToolCall = FunctionToolCall | CustomToolCall`
  （`completions.d.ts:1388`）。**使用 custom tool 时响应解码会整体失败**（P1，见 §4.7）。

### 2.2 Responses

官方 36 个字段（`ResponseCreateParamsBase`；文档
`docs/api/reference/resources/responses/methods/create.md`）。airx 类型化
**23 / 36（64%）**，缺失 13 个：

| 缺失字段 | 业务影响 | 可达性 | 优先级 |
| --- | --- | --- | --- |
| `context_management`（`{type:"compaction", compact_threshold}`，create.md:58） | 长对话服务端压缩，直接决定长会话能否持续 | E | P1 |
| `prompt_cache_options` / `prompt_cache_retention` | 缓存成本（与 Chat 同族） | E | P1 |
| `stream_options.include_obfuscation` | 关闭混淆可省带宽（`create.md:5469`） | E（整字段未类型化） | P2 |
| `top_logprobs` | 置信度 | E | P2 |
| `user` | 终端用户标识 | E | P2 |
| `prompt`（可复用模板，`create.md:5260`） | 模板化提示词 | E | P2 |
| `image`（请求级图片选项） | 图片生成参数，属 images 域 | E | Out |
| `moderation` | 请求级审核 | E | P2 |
| `background_options`（`create.md:38`）、`access_programs`（:19）、`multi_agent`、`betas` | Agents/实验性能力 | E | Out |
| `tool_choice` 的 `allowed_tools` / `web_search` / `mcp` / `custom` 等变体 | 限定工具集、强制内置工具 | **X**（字段已类型化为 Chat `ToolChoice`，`request.rs:509`） | P1 |

输入 item 覆盖（`responses/request.rs:197-239`）：类型化 5 种
（message / function_call / function_call_output / reasoning / item_reference），
官方 `ResponseInputItem` 33 种（`responses.d.ts:3733`）。其余多为 agent 运行时
（shell / apply_patch / program / mcp / tool_search），可用 `InputItem::Other(Value)`
显式构造；但 `custom_tool_call`、`computer_call_output`、`mcp_approval_response`
这类非纯 agent 项没有构造器，只能手写 JSON。

`InputTokensCountRequest`（`resources.rs:107-131`）官方 13 字段，airx 7 个；
缺 `previous_response_id`、`conversation`、`parallel_tool_calls`、`truncation`、
`personality`、`betas`。其中前四项直接导致 `from_request`（:135-149）**静默少算**
token；`model` 在官方是可选的，airx 的 `build()` 强制必填（:236-240）。

### 2.3 其他资源

| 资源 | 官方字段 | airx | 差距 |
| --- | --- | --- | --- |
| Files create | `file`, `purpose`, `expires_after{anchor,seconds}` | 2 / 3 | `expires_after` 不可达（multipart 只写 `purpose`+`file`，`files.rs:198-207`）；实现前需按官方 SDK 的 multipart 嵌套编码核对（快照未包含该序列化代码） |
| Files list | `after`, `limit`, `order`, `purpose` | 3 / 4 | 缺 `order`，且结构无 `extensions` → **X**（`files.rs:118-128`） |
| Batch create | `completion_window`, `endpoint`, `input_file_id`, `metadata`, `output_expires_after` | 4 / 5 | `output_expires_after` 可 `.extension`（`batches.rs:98`） |
| Batch list | `after`, `limit` | 2 / 2 | — |
| Embeddings | `input`, `model`, `dimensions`, `encoding_format`, `user` | 5 / 5 | 字段全齐，但 base64 响应会崩（§0） |
| Chat completion update | `metadata` | 0 / 1 | 无端点 |
| Chat completion list | `after`, `limit`, `metadata`, `model`, `order` | 0 / 5 | 无端点 |

## 3. 响应字段与流式事件缺口

### 3.1 Chat 响应

已类型化：`id/object/created/model/choices/usage/system_fingerprint` + `extensions`
（`chat/response.rs:13-35`）；choice 含 `index/message/finish_reason/logprobs(Value)`。

未类型化（进 `extensions`，不丢字段但不可发现）：`metadata`、`moderation`、
`service_tier`（官方 `ChatCompletion` 显式字段，`completions.d.ts:152-212`）、
message 的 `audio`、`function_call`。

流式缺口（`chat/chunk.rs`）：

- `ChatCompletionChunk` 缺 `system_fingerprint` / `service_tier` / `moderation` /
  `obfuscation`（官方 `completions.d.ts` 的 chunk 显式列出）→ 全部落
  `chunk.extensions`，accumulator 把它们合并进顶层 `extensions`
  （`accumulator.rs:123-125`），而 `build()` 固定 `system_fingerprint: None`
  （:171）。**同一字段非流式有值、`final_response()` 为 `None`**。
- `ChunkDelta` 缺 `refusal` 与 `function_call`（官方 delta 有）→ 落
  `delta.extensions`，accumulator 只搬到 `message.extensions`
  （`accumulator.rs:225-227`）；`ChoiceState.refusal` 从定义到 `build()` 从未赋值
  （`accumulator.rs:38,149`）。流式拒答在 `.message.refusal` 永远读不到。

### 3.2 Responses 响应

已类型化：`id/object/created_at/status/model/output/usage/incomplete_details/error`
（`response.rs:37-67`）。未类型化（全部落 `extensions`）：API 还会回显
`instructions`、`temperature`、`top_p`、`truncation`、`previous_response_id`、
`conversation`、`background`、`max_output_tokens`、`reasoning`、`text`、
`tool_choice`、`tools`、`metadata`、`service_tier`、`prompt`、`safety_identifier`。
其中两个值得单独提出：

- **`prompt_cache_diagnostics`**（官方 `Response` 显式字段，取值
  `cache_hit` / `cache_miss` / `comparison_response_not_found` / `unavailable`）：
  缓存命中率是长提示词业务的直接成本指标，现在只能从 `extensions` 里取。
- **`tool_usage`**（服务端工具用量统计）+ `completed_at`：做计费/审计时需要。
- 官方 `Response` 自带 `output_text` 字段；airx 用 `output_text()` 方法重算
  （`response.rs:69-84`），语义一致（只拼 `output_text` 片段），不影响正确性。

`output` item：8 / 28 类型化（message、function_call、reasoning、web_search_call、
file_search_call、code_interpreter_call、image_generation_call、mcp_call，
`response.rs:88-107`）；官方 `ResponseOutputItem` 28 种（`responses.d.ts:5272`）。
未知项走 `Other(Value)`，往返不丢。

`output_text` 内的 `annotations` 只类型化 `url_citation`
（`response.rs:309-332`）；官方 4 种（file_citation / url_citation /
container_file_citation / file_path）。其余进 `Annotation::Other`。

`ResponsesUsage` 的 `input_tokens_details` / `output_tokens_details` 是 `Value`
（`response.rs:26-29`），`cached_tokens` / `reasoning_tokens` 需手工取值。

### 3.3 Responses 流式事件

官方 SDK union 59 个类型，对应 58 个 SSE `type` 字面量。airx
（`responses/event.rs:13-124`）：

- **18 个**有具名变体（另 `error` 单独处理，见 `endpoint.rs:88-97`）。
- **19 个**落入 `ServerToolCall` 前缀桶（web_search_call / file_search_call /
  code_interpreter_call / image_generation_call / mcp_call / mcp_list_tools）。
- **21 个**落 `Unknown(Value)`，其中非 agent 业务会碰到的：
  `response.reasoning_text.delta/done`（原始推理文本）、
  `response.output_text.annotation.added`（逐条引用标注）、
  `response.reasoning_summary_part.added/done`、
  `response.compaction.compacting`、`response.custom_tool_call_input.delta/done`、
  `response.audio.*`（4）、`response.shell_call_*`（5，agent 域）。
  未知事件原样保留，不失败，但 accumulator 全部忽略
  （`accumulator.rs:315`）。

`ResponsesStreamAccumulator` 从 delta 重建 item 时会丢信息：`OutputText.annotations`
固定为空（`accumulator.rs:93-97`）、item `status` 固定 `None`、reasoning 的 `status`
丢失。只有在 `response.completed.response.output` 为空（多数网关不会）时才暴露，
但仍建议按 delta 收集标注。

## 4. 行为差异

1. **缺省值**：`None` 不序列化，交给服务端默认（`temperature=1`、`top_p=1`、
   `parallel_tool_calls=true`、`store=false`、`reasoning_effort="medium"`、
   `verbosity="medium"`，见 `chat.md:1018,1216,1416,1433,1168,922`）。与官方 SDK 一致。
   差异在 `null`：`Option<T>` 无法表达显式 `null`（`docs/compatibility.md:5-13` 已声明），
   个别网关用 `null` 表达“关闭”时会失真。
2. **Chat 完成语义**：airx 要求 `[DONE]`，否则 `Incomplete`
   （`accumulator.rs:98-105`）；官方客户端把流正常结束视为结束。这是**更严格**的策略，
   但会与“省略 `[DONE]`”的代理/网关冲突（`docs/openai.md:594` 已提示）。
3. **Responses 完成语义**：只认 `response.completed`
   （`accumulator.rs:292-310`）；`response.incomplete`/`failed` 直接映射为
   `StreamError`，partial 响应无法取回（只能 `snapshot()`）。业务“被截断但想展示半截”
   需要额外 API。
4. **`Chat::stream` 自动注入 `stream_options.include_usage=true`**
   （`openai.rs:210-215`）。官方 SDK 是显式 opt-in；严格的兼容网关若拒绝未知字段会 400。
   目前可用 `.stream_options(StreamOptions{include_usage:false})` 关闭，但缺
   `include_obfuscation`（`chat/request.rs:78-82`）。
5. **错误映射**：`error.code` 为数字/字符串时转字符串，缺 `code` 时回退 `error.type`
   （`common/error.rs:30-34,71-76`）；非 JSON 错误体退化为 `HttpError`，message 丢失。
   Responses 的流内 `error` 事件单独构造 `ProviderError` 但没有 status
   （`responses/endpoint.rs:88-97`）。
6. **重试**：429 一律重试（含 `insufficient_quota` / 余额耗尽），与官方
   “重试配额类错误不会恢复访问”相悖（`docs/api/docs/guides/error-codes.md:28`）；
   `Retry-After` 只解析整数秒、忽略 HTTP-date，退避无 jitter
   （`airx-transport/src/retry.rs:217-229`）。POST 5xx 不重试是**有意设计**并有测试
   （`crates/airx/tests/vertical_slice.rs:140` `post_500_is_not_retried`），但与官方 SDK 对 5xx 的
   默认重试口径不同，接入时需按需打开。
7. **工具载荷**：Chat 的 `{"type":"function","function":{...}}` 正确
   （`tool.rs:23-33`）；Responses 错用同一结构（`responses/tool.rs:15-33`），
   `tool_choice` 也错用 Chat 结构（`request.rs:509` vs `responses.d.ts:12161-12169`）。
   Chat 的 custom tool call 响应会让 `ToolCall` 反序列化失败（`function` 必填）。
8. **结构化输出**：`parse_as` 只取第一个候选、不校验 schema、只支持非流式
   （`openai.rs:189-195,249-255`；`docs/openai.md:239-245`）。`TextFormat::JsonSchema`
   没有 `extensions`，未来 `text.format` 新字段只能 raw body。
9. **Embeddings base64**：见 §0，typed 参数导致硬失败。
10. **`stop` 恒为数组**（`chat/request.rs:42`）：服务端接受，等价；但请求快照与官方
    SDK 的 `string | string[]` 不同形。

## 5. 业务接入易用性与代码清晰度

以下建议均不引入 agent 循环/工具执行/编排，只在既有 builder + typed struct 习惯内扩展。

1. **Chat 消息构造器**（`chat/message.rs`）
   ```rust
   impl ChatMessage {
       pub fn developer(content: impl Into<String>) -> Self;      // 官方 developer 角色
       pub fn named(self, name: impl Into<String>) -> Self;        // 可选 name
   }
   ```
   验收：`serde_json::to_value(ChatMessage::developer("x"))` ==
   `{"role":"developer","content":"x"}`，并能反序列化回来。
2. **从响应直接续写历史**（`chat/message.rs` / `chat/response.rs`）
   ```rust
   impl ChatCompletionResponse {
       pub fn assistant_message(&self) -> Option<ChatMessage>; // 保留 content + tool_calls
       pub fn tool_calls(&self) -> &[ToolCall];
       pub fn finish_reason(&self) -> Option<&str>;
   }
   ```
   注意：assistant `refusal` 目前无法进入 `ChatMessage`；建议给
   `ChatMessage::Assistant` 增加 `refusal: Option<String>`（0.1 阶段可直接改）。
3. **Responses 工具与工具选择**（`responses/tool.rs`、`request.rs`）
   ```rust
   pub struct ResponsesFunctionTool {
       pub name: String,
       pub description: Option<String>,
       pub parameters: Option<serde_json::Value>,
       pub strict: Option<bool>,
       #[serde(flatten)] pub extensions: Extensions,
   }
   pub enum ResponsesToolChoice {
       Mode(ToolChoiceMode),                       // auto/none/required
       Function { name: String },                  // {type,name}
       AllowedTools { mode: String, tools: Vec<serde_json::Value> },
       Builtin(String),                            // {type:"web_search"} 等
   }
   ```
   计划中的 `ResponsesRequest.tool_choice: Option<ResponsesToolChoice>` 需要在
   `InputTokensCountRequest` 同步替换。
4. **Output → Input 回传**（`responses/request.rs`）
   ```rust
   impl From<OutputItem> for InputItem;                 // message/function_call/reasoning 直转
   impl ResponsesResponse { pub fn into_input(self) -> Vec<InputItem>; }
   impl InputItem {
       pub fn user(content: impl Into<String>) -> Self;
       pub fn function_call_output(call_id: impl Into<String>, output: impl Into<String>) -> Self;
   }
   ```
   这是 `store:false` 多轮的核心路径；不做循环、不做状态。
5. **生命周期选项**（`responses/resources.rs` / `openai.rs`）
   ```rust
   pub struct RetrieveResponseOptions { pub include: Vec<String>, pub starting_after: Option<u64>, pub stream: bool }
   impl Responses {
       pub async fn retrieve_with(&self, id: &str, opts: RetrieveResponseOptions) -> Result<ResponsesResponse, AirxError>;
       pub async fn compact(&self, req: CompactRequest) -> Result<ResponsesResponse, AirxError>;
   }
   ```
6. **token 计数**：`InputTokensCountRequest` 增 `previous_response_id`、`conversation`、
   `parallel_tool_calls`、`truncation`；`model` 改 `Option<ModelId>`；`from_request`
   逐字段复制（含 `extensions` 中的已建模字段），避免静默少算。
7. **Embeddings 向量类型**（`embeddings.rs`）
   ```rust
   #[serde(untagged)] pub enum EmbeddingVector { Float(Vec<f32>), Base64(String) }
   impl EmbeddingVector { pub fn to_f32(&self) -> Result<Vec<f32>, AirxError>; }
   ```
8. **流式收尾提供 partial**（`chat/accumulator.rs`、`responses/accumulator.rs`）
   ```rust
   pub fn finish_partial(self) -> (ChatCompletionResponse, StreamStatus);
   pub fn finish_partial(self) -> (ResponsesResponse, StreamStatus);
   ```
   保留 `finish()` 的严格语义，另外给“截断也要展示”的业务一个显式入口。
9. **`create` 与 `stream` 互斥校验**：`Chat::create` / `Responses::create` 收到
   `stream==Some(true)` 时返回 `ConfigurationError`（现在会把 SSE 当 JSON 解析，
   报一个与原因无关的 `Deserialization`）。
10. **prelude 补齐**（`crates/airx/src/prelude.rs:64-90`）：`ResponseFormat`、
    `JsonSchemaSpec`、`ResponsesTool`、`OutputItem`、`OutputContent`、
    `ChatCompletionChunk`、`StreamOptions`、`TextConfig` 目前都要从
    `airx_protocol::openai::...` 手写导入，与 “prelude 即用” 的定位不一致。
11. **类型化但不必强求**：`Files::upload` 可保留 `(filename,purpose,data)` 便捷签名，
    增加 `upload_with(UploadFileOptions { expires_after, .. })`；不必改成 builder 才能用。

## 6. 文档与代码不一致

| 位置 | 文档说法 | 代码事实 |
| --- | --- | --- |
| `docs/openai.md:301` | 输入 item “content 仅 String”，“多模态 content、`function_call`/`reasoning` 等输入 item”未类型化 | `InputContent` 已含 `input_text`/`input_image`/`input_file`（`responses/request.rs:49-79`），`InputItem` 已含 `FunctionCall`/`Reasoning`/`ItemReference`（:197-239）；同文件 `:325-327` 又写“已类型化”，自相矛盾 |
| `docs/openai.md:302` | 输出 `reasoning`、`file_search_call`、`code_interpreter_call` → `Other(Value)` | 均为具名变体（`responses/response.rs:88-107`） |
| `docs/openai.md:338` | `mcp_list_tools` / `mcp_approval_request` “仍落在 `Other`” | 解码器把二者归入 `ServerToolCall`（`responses/event.rs:115-124`）；`Other` 只发生在输出 item 未识别的类型上 |
| `docs/openai.md:8` | “本文覆盖 Chat Completions、Responses 与 Models” | 同文 §9 还覆盖 Files / Batches / Embeddings |
| `docs/openai.md:18-19` | 两行“边界之外”重复列 `Vector Store、Fine-tuning、Assistants / Threads` | 文案重复 |
| `docs/openai.md:611` / `docs/protocol-support.md:45` | `Responses::delete` 已实现 | 实现存在，但官方 SDK 类型为 `APIPromise<void>`（`responses.d.ts:75`），文档却描述 JSON body；走代理时可能遇到 204/空 body 而解析失败，需在文档标注 |
| `docs/architecture.md:303-312` | Responses 输入只列 `message` / `function_call_output`；输出只列 `message`/`function_call`/`web_search_call` | 与 `responses/request.rs:197-239`、`response.rs:88-107` 不符（同 §5.2 陈旧） |
| `docs/openai.md:162` | `.user_id(...)` 作为用户标识 | 官方已把 `user` 标记为被 `safety_identifier` / `prompt_cache_key` 取代（`chat.md:1427-1431`），airx 未提供这两个 typed 字段 |
| `docs/protocol-support.md:45` | 生命周期 retrieve/cancel/delete/input_items/input_tokens 已实现 | 正确，但未提 retrieve 无 query、input_tokens 字段不全 |

另注：`docs/openai.md:167` 明确“未列出的参数用 `.extension` 透传”，这条口径是对的；
但它掩盖了两类不可达场景（`X`）：已类型化字段的变体（`tool_choice`）与
`messages` 内部字段（`developer`、content part 的 `prompt_cache_breakpoint`）。

## 7. 风险与注意事项

1. **模型/网关漂移**：清单里已出现 `gpt-6-astra`、`gpt-5.6-*` 等模型别名
   （`manifest.json` 的 `ResponseCreateParamsBase.model`），`prompt_cache_options`
   仅 gpt-5.6+、`context_management` 依赖 compaction 能力。airx 不校验模型能力，
   把“参数不被模型支持”原样交给服务端 400，这是正确取舍；接入方需按模型门控。
2. **兼容网关**：DeepSeek 不支持 `json_schema`、部分网关忽略 `stream_options`/
   `developer`/`reasoning_content`、OpenCode Zen 接受 `deepseek_web_search` 声明但
   不保证执行（`docs/protocol-support.md:159-199` 已记录）。`Chat::stream` 的自动
   `include_usage` 是最容易踩的隐式行为。
3. **弃用参数**：`max_tokens`、`functions`/`function_call`、`user`
   （`chat.md:855,772,1427`）。airx 只类型化了 `max_tokens` 与 `user`，其余靠
   extension；文档应持续标注。
4. **schema 漂移**：快照是 2026-11-24 的一次切面；`manifest.json` 的 352 个请求体
   类型会随 SDK 版本变化。建议在 `reference/openai/update.sh` 刷新后对比
   `coverage` 与本文件 §2 的计数（Chat 37 / Responses 36 / input_tokens 13）。
5. **流式陷阱**：`[DONE]` 缺失、usage chunk 的 `choices` 为空、
   `obfuscation` 随机字段、DeepSeek 的 `reasoning_content`/`annotations` 只在末 chunk、
   Responses 的 `response.completed` 才带完整 usage。
6. **`extensions` 重复键**：对已类型化字段调用 `.extension()` 会输出重复 JSON key
   （`docs/openai.md:526-528`）。严格 JSON 网关可能 400；即使“后者覆盖”，也不是
   协议保证——这是 `X` 类缺口的根因。
7. **`Option<T>` 不保 `null`**：`docs/compatibility.md:7-13` 已声明；对
   `tool_choice: null`、`metadata: null` 这类“显式清除”语义要当心。
8. **超时/重试**：整体 `timeout` 会截断长流（`docs/openai.md:122`）；
   流式请求不重试；429 含配额错误会重试；退避无 jitter，多实例会同步重试。
9. **文件/批处理内存**：`files().upload` 把整个文件放 `Bytes` 并一次性编码 multipart
   （`files.rs:88-97,198-218`）；200MB 级 batch 输入会带来 2 倍内存峰值。

## 8. 分级建议

### P0（正确性，必须修）

1. **Responses 函数工具形状**
   - 位置：`crates/airx-protocol/src/openai/responses/tool.rs:15-33`、
     `request.rs:504-509`、`resources.rs:117-121`。
   - 动作：新增 `ResponsesFunctionTool`（扁平 name/parameters/strict/type）与
     `ResponsesToolChoice`；`ResponsesTool` 不再复用 `chat::FunctionTool`。
   - 验收：`serde_json::to_value(ResponsesTool::function("f", json!({}), None))` ==
     `{"type":"function","name":"f","parameters":{}}`；`ResponsesToolChoice::Function{name}`
     == `{"type":"function","name":"..."}`；新增单测覆盖 `tools`/`tool_choice` 编码
     （当前 `responses_protocol.rs` 完全没有工具用例）。

2. **Embeddings base64 解码**
   - 位置：`crates/airx-protocol/src/openai/embeddings.rs:62,173-179`。
   - 动作：`EmbeddingVector` untagged enum + `to_f32()`。
   - 验收：`{"embedding":"..."}` 能解码；`to_f32()` 返回 `base64` 解码错误类型；
     现有 float 用例不回归。

### P1（重要保真 / 易用性）

3. **Chat 消息模型**：`Developer` 角色、`name`、assistant `refusal`、
   `ContentPart::{InputAudio, File, Other}`（`chat/message.rs:31-87`）。
   验收：官方 `ChatCompletionMessageParam` 的 6 种角色可往返；`file` 片段可编码为
   `{"type":"file","file":{...}}`。
4. **Chat 生产参数**：penalties / logprobs+top_logprobs / n / seed / store / metadata /
   service_tier / safety_identifier / prompt_cache_key+options+retention
   （`chat/request.rs:17-104`）。验收：每个字段有 builder + 序列化单测；`None` 不外发。
5. **Responses 生命周期**：`retrieve_with`（include/starting_after/stream）、
   `compact`（新增 endpoint 于 `responses/resources.rs`，门面加在
   `openai.rs:220-329` 的 `Responses` impl）。
   验收：query 编码正确；`compact` 走 `POST /responses/compact`。
6. **Responses 回传**：`From<OutputItem> for InputItem`、`into_input()`、`InputItem`
   构造器（`responses/request.rs:197-239`）。验收：response fixture 的 output 转 input
   后 `to_value` 与官方 item 形状逐字段相等（忽略 id/status 时也应有明确文档说明）。
7. **token 计数**：补 4 字段 + `model: Option` + 修 `from_request`
   （`responses/resources.rs:107-149`）。验收：`from_request` 对含
   `previous_response_id`/`conversation` 的请求产生的 body 与原请求同名字段一致；
   缺 `model` 不报错（服务端定）。
8. **Custom tool 兼容**：让 `ToolCall` 的 `function` 可选并保留 `custom`，或在
   `ResponseMessage` 里用 `RawJson` 承接（`chat/tool.rs:95-106`、`message.rs:167-169`）。
   验收：含 `ChatCompletionMessageCustomToolCall` 的响应可解码且往返不丢 `custom`。
9. **流式字段对齐**：`ChunkDelta.refusal/function_call`、
   `ChatCompletionChunk.system_fingerprint/service_tier`、
   `StreamOptions.include_obfuscation`（`chat/chunk.rs`、`chat/request.rs:78-82`）。
   验收：流式 fixture 聚合后 `.message.refusal` 与 `.system_fingerprint` 与非流式一致。

### P2（细节 / 低频）

10. **资源细节**：Files `order` + `expires_after`/`expires_at`/`status_details`；
    Batches `output_expires_after`、`model`、`usage`；Models `shutdown_date` +
    `delete`（`files.rs:21-45,118-128,198-207`、`batches.rs:20-100`、`models.rs:21-36`）。
11. **Chat stored completions**：`retrieve/list/update/delete`（新增
    `ChatCompletionUpdate/ListParams` 端点）。前置：`store`/`metadata` typed。
12. **更多 Responses 字段**：`context_management`、`top_logprobs`、`user`、
    `prompt`、`stream_options`；响应侧补 `prompt_cache_diagnostics`、`tool_usage`、
    `completed_at`；输出 item 补 `custom_tool_call`、`compaction`；
    annotation 补 file_citation / container_file_citation / file_path。
13. **Moderations**：`POST /moderations` + `ModerationRequest/Response`（新文件
    `crates/airx-protocol/src/openai/moderations.rs`），不涉及 agent。
14. **重试策略**：429 按 `code` 区分（跳过 `insufficient_quota` /
    `credit_balance_exhausted` / `*_spend_limit_*`）、支持 HTTP-date 的
    `Retry-After`、加 jitter（`airx-transport/src/retry.rs:190-229`）。
15. **partial 收尾**：`finish_partial()`（两个 accumulator）。
16. **prelude 补齐**与 `create` 的 stream 校验。

## 9. 不做清单

| 不做 | 理由 |
| --- | --- |
| Agent 运行时（Agents、Assistants、Threads、ChatKit、`beta.agents.*`、runs/automations/sessions） | airx 是请求工具包：不做循环、不做状态机、不做工具执行。`tool_calls`/`function_call`/`response.output` 只作为协议载荷返回给调用方 |
| 工具执行与 MCP 客户端（`mcp` 工具族、`mcp_approval_*`、`mcp_list_tools` 交互） | 属于运行时编排；airx 只声明服务端工具并解析返回的调用记录（`ResponsesTool::web_search`、`Tool::server`） |
| Realtime / WebSocket / Live 会话 | 长连接会话协议，不是一次性请求；且与“请求编解码”定位不符 |
| Vector Stores / Files 的 RAG 编排、`file_search` 的本地检索 | 服务端 `file_search` 可作为工具声明透传，但索引/检索编排属应用或 agent 层 |
| Fine-tuning / Evals / Graders / Admin、Organization、Containers、Skills | 账号/运维面与训练面，不是业务请求路径；`models.delete` 仅因此保持 P2 |
| Images / Audio（`modalities:["audio"]`、`audio`、transcription、speech） | 独立的二进制/流式能力；airx 当前定位是文本/多模态输入+工具载荷；`ContentPart::InputAudio` 若要支持只做请求编码（P1 可选） |
| 内置 JSON Schema 校验 / 自动重试结构化输出 / 自动修复 tool 参数 | 需要模型或 schema 引擎参与；airx 只做 serde 与传输 |
| 跨 Provider 统一请求模型 | 明确的设计前提（`docs/openai.md:7-10`）；只保留各协议原生类型 |

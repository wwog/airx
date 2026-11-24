# Anthropic：官方行为 vs airx 差距分析

- **基准**：`@anthropic-ai/sdk@1.13.0`（npm `latest`，快照 `reference/anthropic/sdk/@anthropic-ai-sdk@1.13.0/`，`api/manifest.json` 记录 117 resource / 399 方法 / 466 条 `*Params` 声明）；官方文档快照 52 页，`docs/meta.json` 生成于 `2026-11-24T01:33:08Z`（`docs/meta.json` `generated_at`），索引源 `platform.claude.com/llms.txt`（`versions.env`）。
- **airx 侧基准**：`crates/airx-protocol/src/anthropic/**`（约 2.4k 行）+ `crates/airx/src/{anthropic,api,stream,response,options,builder}.rs`，文档 `docs/anthropic.md`、`docs/protocol-support.md`、`docs/compatibility.md`。
- **分析方式**：纯静态阅读 + 本地 JSON 快照脚本化对比；未运行 cargo / 测试。

## 分析范围 / 不在范围

**在范围**：Messages（create / countTokens / 流式聚合 / tools / thinking / 结构化输出 `output_config.format`）、Messages Batches、Models、Files，以及它们共享的 Header / 版本 / 错误 / 用量语义；门面易用性与文档一致性。

**不在范围**：`beta.*` 命名空间共 **286 / 399** 个方法（agents / sessions / environments / deployments / memoryStores / vaults / bossiness… `api/SUMMARY.md:16-861`）、GA 的组织管理与账号域（`organization.*`、`skills*`、`userProfiles*`，`api/SUMMARY.md:907-1233`）、Managed Agents / Skills / MCP connector 等托管侧能力。对其中少数「会直接影响业务请求体」的 beta 字段（`context_management`、`compaction`、`mcp_servers`、`fallbacks`、`output_format`）只在第 7 节提示，不作为 airx 待办。

---

## 0. 一页结论（Top 8）

| # | 差距 | 影响 | 优先级 | 建议动作 |
| --- | --- | --- | --- | --- |
| 1 | 默认 `anthropic-version: 2023-06-01`，而 airx 建模的 `output_config` / `thinking.display` / `adaptive` / `between_tools` / `speed` / `stop_reason=refusal` 全部是 `2026-10-21` 文档面 | 真实 API 上结构化输出、thinking 展示控制、effort 可能 400 或被忽略；所有示例默认走旧版本 | **P0** | `config.rs:16` 改默认值，测试 `anthropic_protocol.rs:40-43` 同步 |
| 2 | Files 列表分页用了 `after_id`/`before_id`，官方是 `page` 游标 + `next_page`；`FileList::next_cursor()` 恒返回 `None` | 翻页静默失效（不报错、只拿到第一页） | **P0** | 改 `ListFilesRequest`/`FileList`（`files.rs:47-76,125-136`），补 `ids` |
| 3 | `FileDeleteResponse.deleted` 是 airx 自造字段，官方 `DeletedFile` 只有 `{id, type}` | 业务判断 `deleted == true` 永远为 false，静默错 | **P0** | 删掉 `deleted`，`files.rs:79-92` |
| 4 | `system` 只接受 `String`；官方是 `string \| TextBlockParam[]`，且 `messages[]` 支持 `role:"system"` | 无法给 system 挂 `cache_control`（显式缓存断点的标准做法）、无法多块 system、无法插对话中 system | P1 | `SystemPrompt` 枚举 + `Role::System`（`request.rs:22-23`、`message.rs:12-17`） |
| 5 | 流式聚合丢字段：`message_delta.delta.stop_details`/`container` 被丢弃、`citations_delta` 不累积、块的 `extensions` 被重建为空 | refusal 分类、citations、服务器工具元数据在流式路径上丢失（非流式有） | P1 | `event.rs:174-182` 补字段；`accumulator.rs:36-40` 保留块原始 JSON |
| 6 | `count_tokens` 缺 `tool_choice` / `output_config` / `cache_control` / `speed` | 计数与真实请求不一致（工具选择、effort、缓存标记都会影响输入 token） | P1 | `count_tokens.rs:21-38` 对齐 11 个官方字段 |
| 7 | `ToolChoice` 缺 `none` 与 `disable_parallel_tool_use`；`Tool` 缺 `strict` / `cache_control` / `eager_input_streaming` / `defer_loading` | 无法「本次禁用工具」、无法强制串行、无法用 strict tool use（结构化输出的另一半） | P1 | `request.rs:213-224`、`request.rs:107-119` |
| 8 | 缺 `DELETE /v1/messages/batches/{id}`；`workspace_id` / `user_profile_id` 是 Header 参数，airx 无处安放（`extensions` 会写进 body） | 批量结果无法清理；多 workspace key 的请求无法通过正确通道选择 workspace | P1 | 新增 `Batches::delete`；`RequestOptions` 提供具名 Header 助手 |

> 一句话：**主线协议（messages 的文本/工具/thinking/结构化输出/流式）骨架是对的且方向正确（含 `output_config` 命名、`temperature` 已移除的判断），差距集中在「版本头 + Files 分页/删除 + system 与缓存的类型化 + 流式保真 + 少量端点/字段」**。

---

## 1. 端点覆盖矩阵

范围统计：GA 方法 **113** 个，其中属于 airx 目标面的 **18** 个（`files` 5 + `messages` 4 + `messages.batches` 6 + `models` 2 + `organization.retrieve` 1）；扣除 `organization.retrieve`，实际目标是 **15 个 HTTP 端点 + 2 个 SDK 便捷方法**。airx 覆盖 **14 / 15 HTTP**、便捷方法 1 完整 + 1 部分。

| 官方 method / endpoint | airx 状态 | 备注（airx 证据） | 优先级 |
| --- | --- | --- | --- |
| `messages.create` `POST /v1/messages` | 已实现（字段子集） | `endpoint.rs:43-74`；19 个官方字段中 10 个类型化（见 §2） | P0/P1 |
| `messages.countTokens` `POST /v1/messages/count_tokens` | 已实现（字段子集） | `count_tokens.rs:146-199`；11 个官方字段中 5 个类型化 | P1 |
| `messages.stream`（SDK 便捷方法，无独立端点） | 已实现 | `anthropic.rs:176-182` 置 `stream=true`；`stream.rs:166-183` `final_text/final_response` | — |
| `messages.parse`（SDK 便捷方法） | 部分实现 | `anthropic.rs:148-155` `parse_as` **不设置** `output_config.format`、不校验 schema；官方 `parse` 会注入 schema 并返回 `parsed_output`（`docs/build-with-claude/structured-outputs.md:199,243`） | P1 |
| `messages.batches.create` `POST .../batches` | 已实现 | `batches.rs:117-145`；`requests[].params` 复用 `MessagesRequest`，因此受 §2 字段子集限制 | P2 |
| `messages.batches.retrieve` `GET .../{id}` | 已实现 | `batches.rs:196-251` | — |
| `messages.batches.list` `GET .../batches` | 已实现（参数正确） | `batches.rs:102-114`：`limit`/`before_id`/`after_id` 与官方 `PageParams` 一致（`batches.d.ts:353`） | — |
| `messages.batches.delete` `DELETE .../{id}` | **缺失** | 全仓无 `DeleteBatch`（`anthropic.rs:326-381` 仅 create/list/retrieve/cancel/results） | P1 |
| `messages.batches.cancel` `POST .../cancel` | 已实现 | `batches.rs:208-251` | — |
| `messages.batches.results` `GET .../results` | 已实现（原始 JSONL） | `batches.rs:253-294` 返回 `Bytes`，无类型化行解析 | P2 |
| `models.retrieve` `GET /v1/models/{id}` | 已实现 | `models.rs:181-212` | — |
| `models.list` `GET /v1/models` | 已实现（缺过滤） | `models.rs:69-81` 缺官方 `lifecycle[]`（`models.d.ts:298-302`） | P2 |
| `files.upload` `POST /v1/files` | 已实现（缺参数） | `files.rs:186-230`；缺 `expires_in_seconds`（`files.d.ts:240-251`） | P2 |
| `files.list` `GET /v1/files` | **已实现但语义错误** | `files.rs:124-136` 用 `after_id/before_id`；官方是 `page` + `ids`（`files.d.ts:188-201`、`docs/api/files.md:157-167`） | **P0** |
| `files.retrieveMetadata` `GET /v1/files/{id}` | 已实现 | `files.rs:279-310` | — |
| `files.delete` `DELETE /v1/files/{id}` | **已实现但响应字段错误** | `files.rs:79-92` 的 `deleted` 字段官方不存在（`files.d.ts:116-119`、`docs/api/files/delete.md:36-46`） | **P0** |
| `files.download` `GET /v1/files/{id}/content` | 已实现 | `files.rs:345-384` 返回 `Bytes` | — |
| `organization.retrieve` `GET /v1/organizations/me` | 缺失（**不做**） | 账号域，见 §9 | Out |

**Beta 面**：`beta.messages.create` 有 28 个字段（GA 19），新增 `billing_policy` / `compaction` / `context_management` / `fallbacks` / `fallback_credit_token` / `mcp_servers` / `output_format` / `thread` / `betas`（`manifest.json` types → `MessageCreateParams` @ `resources/beta/messages/messages.d.ts`）。airx 未做 beta 命名空间是合理取舍；但 `context_management` / `compaction`（长对话自动压缩）是「不改协议也能显著影响业务体验」的字段，建议以「文档 + `extensions` 示例」而非新类型对待（见 §7）。

---

## 2. 请求参数与类型缺口

### 2.1 `messages.create`：19 → 10 类型化

官方 `MessageCreateParamsBase`（`resources/messages/messages.d.ts:5639`）：`max_tokens, messages, model, cache_control, container, diagnostics, inference_geo, metadata, output_config, service_tier, speed, stop_sequences, stream, system, thinking, tool_choice, tools, user_profile_id, workspace_id`。

airx `MessagesRequest`（`request.rs:14-45`）类型化 10 个：`model, max_tokens, messages, system, output_config, stop_sequences, tools, tool_choice, thinking, stream`。

| 官方字段 | 类别 | airx 位置 / 说明 | 优先级 |
| --- | --- | --- | --- |
| `system: string \| TextBlockParam[]` | **(c) 类型化但收窄** | `request.rs:22-23` 只有 `Option<String>`；数组形式**连 extensions 都救不了**（flatten 只会产生重复键 `system`） | P1 |
| `cache_control`（顶层，自动缓存） | (b) 仅 extensions | 作为 body 字段可 `.extension("cache_control", json!({"type":"ephemeral"}))`，wire 兼容；但无类型/无 `ttl` 校验 | P1 |
| `metadata.user_id` | (b) 仅 extensions | `metadata` 未建模（官方 `Metadata` 只有 `user_id`，`messages.d.ts` 内 `Metadata`） | P2 |
| `service_tier` | (b) 仅 extensions | 文档已说明（`docs/anthropic.md:423-426`） | P2 |
| `speed`（`standard`/`fast`） | (b) 仅 extensions | 需 `fast-mode-2026-02-01` beta 头，见 `messages.d.ts` `speed` 注释 | P2 |
| `container` | (b) 仅 extensions | 请求侧透传可行；响应侧读取见 §3 | P2 |
| `diagnostics.previous_message_id` | (b) 仅 extensions | 缓存诊断（`docs/build-with-claude/cache-diagnostics.md`），进阶排障用 | P2 |
| `inference_geo` | (b) 仅 extensions | 数据驻留 | P2 |
| `user_profile_id` | **(a) 无法表达** | 官方是 **Header** `anthropic-user-profile-id`（`docs/api/messages.md:26`），塞进 `extensions` 会写进 body | P2 |
| `workspace_id` | **(a) 无法表达** | 官方是 **Header** `anthropic-workspace-id`（`docs/api/overview.md:49`：未绑定 workspace 的 key **必填**）；`docs/anthropic.md:426` 声称走 extensions，**这是错的** | P1 |

> 计数口径：**7 个 body 字段 wire 兼容（extensions）、2 个 Header 参数无法表达、1 个字段（system）类型收窄到不能用数组**。

### 2.2 `messages.countTokens`：11 → 5 类型化

官方 `MessageCountTokensParams`（`messages.d.ts:5957`）：`messages, model, cache_control, output_config, speed, system, thinking, tool_choice, tools, user_profile_id, workspace_id`。
airx `CountTokensRequest`（`count_tokens.rs:21-38`）：`model, messages, system(String), tools, thinking` + `extensions`。

- 缺 `tool_choice`（P1）：官方 token 计数会把 tool_choice 计入；缺失会让「预留 context 预算」偏小。
- 缺 `output_config` / `cache_control` / `speed`（P1/P2）：同样影响计数一致性，也和 §4「结构化输出会额外注入 system prompt」相关（`structured-outputs.md:2855-2858`）。
- 缺 2 个 Header 参数：同 §2.1。

### 2.3 工具声明：`Tool`（10 → 3 类型化）

官方 `Tool`（`messages.d.ts:4416`）：`input_schema, name, allowed_callers, cache_control, defer_loading, description, eager_input_streaming, input_examples, strict, type`。
airx `ClientTool`（`request.rs:107-119`）：`name, description, input_schema` + `extensions`。

| 官方字段 | airx | 影响 | 优先级 |
| --- | --- | --- | --- |
| `strict: true` | (b) 仅 extensions | 结构化输出的「strict tool use」；无类型字段导致大量业务忽略它 | P1 |
| `cache_control`（工具级缓存断点） | (b) 仅 extensions | 大工具集场景的缓存实践（`tool-use-with-prompt-caching.md`） | P1 |
| `input_examples` | (b) 仅 extensions | 提升工具调用准确率 | P2 |
| `eager_input_streaming` | (b) 仅 extensions | 细粒度工具参数流（`fine-grained-tool-streaming.md`） | P2 |
| `defer_loading` / `allowed_callers` / `type:"custom"` | (b) 仅 extensions / (a) | 工具搜索与代码执行编排，偏 beta/高级 | P2 |
| `Tool::Other(Value)` 兜底 | 存在（`request.rs:103,206-208`） | 未知服务端工具可原样声明，设计良好 | — |

### 2.4 `tool_choice`：4 变体 → 3 变体

官方（`messages.d.ts:4510,4527,4540,4546`）：`auto{disable_parallel_tool_use?}` / `any{disable_parallel_tool_use?}` / `tool{name, disable_parallel_tool_use?}` / `none`。
airx `ToolChoice`（`request.rs:212-224`）：`Auto` / `Any` / `Tool{name}`，**无 `None`、无 `disable_parallel_tool_use`**。

- 缺 `none`（P1）：「本通道禁用工具」是常见业务开关（例如把同一 prompt 复用于纯文本模式）。
- 缺 `disable_parallel_tool_use`（P1）：强制串行工具调用，是控制副作用顺序的标准手段。
- 该枚举未标 `#[non_exhaustive]`，反序列化遇到 `"none"` 会直接失败（`request.rs:213`）。

### 2.5 请求侧内容块：`ContentBlockParam` 16 → 8

官方 union（`messages.d.ts:3572`）：text, image, document, search_result, thinking, redacted_thinking, tool_use, tool_result, server_tool_use, web_search_tool_result, web_fetch_tool_result, code_execution_tool_result, bash_code_execution_tool_result, text_editor_code_execution_tool_result, tool_search_tool_result, container_upload。

airx 类型化 8 个（`message.rs:31-50`），缺 **8** 个：`document`、`search_result`、`web_fetch_tool_result`、`code_execution_tool_result`、`bash_code_execution_tool_result`、`text_editor_code_execution_tool_result`、`tool_search_tool_result`、`container_upload`。

| 缺失块 | 实际可行性 | 业务影响 | 优先级 |
| --- | --- | --- | --- |
| `document`（PDF / 纯文本 / URL / file_id） | 只能 `ContentBlock::Other(json!({"type":"document",...}))` | **PDF/文档问答是高频需求**，目前无 typed 构造器、无 `source` 枚举、无 `title/context/citations` 字段，容易写错 | P1 |
| `search_result` | 同上 | 自定义 RAG 结果注入 | P2 |
| 各类 `*_tool_result`（web_fetch / code_execution / text_editor / tool_search） | 响应侧落 `Other` 保留，请求侧需手写 JSON | 多轮服务端工具续跑（`pause_turn`）需要原样回传 | P2 |
| `container_upload` | 手写 JSON | 代码执行容器 | P2 |

同时缺 3 个常用字段的类型化位置（都在 `extensions` 里，wire 兼容）：

- `TextBlockParam.cache_control` / `citations`（`messages.d.ts` `TextBlockParam @4192`）→ `TextBlock`（`message.rs:53-60`）无类型字段，**所有 prompt 缓存示例都必须手写 `extensions`**（P1）。
- `ImageBlockParam.transformations`（P2）。
- `ToolResultBlockParam.content` 是 `string | Array<...>`，airx 用 `Option<Value>`（`message.rs:92-94`）——可用但无 ergonomics（P2）。

### 2.6 其他资源

| 资源 | 官方字段 | airx | 缺口 | 优先级 |
| --- | --- | --- | --- | --- |
| `ModelListParams`（`models.d.ts:298`） | `limit, before_id, after_id, lifecycle[], workspace_id` | `models.rs:69-81`（3 字段） | 缺 `lifecycle` 过滤（`active/deprecated/retired`）与 `workspace_id` | P2 |
| `FileListParams`（`files.d.ts:188`） | `limit, page, ids[], workspace_id` | `files.rs:125-136`（`limit, after_id, before_id`） | **参数名全错**，`ids` 缺失 | **P0** |
| `FileUploadParams`（`files.d.ts:240`） | `file, expires_in_seconds, workspace_id` | `files.rs:95-122` | 缺 `expires_in_seconds`（3600–7776000 秒） | P2 |
| `BatchCreateParams`（`batches.d.ts:300`） | `requests[], user_profile_id, workspace_id` | `batches.rs:28-46` | Header 参数无处安放；`stream` 未被禁止进 batch body | P2 |

---

## 3. 响应字段、内容块与流式事件缺口

### 3.1 `Message` 响应：11 → 8

官方（`messages.d.ts:3761`）：`id, container, content, diagnostics, model, role, stop_details, stop_reason, stop_sequence, type, usage`。
airx `MessagesResponse`（`response.rs:12-38`）：`id, type, role, model, content, stop_reason, stop_sequence, usage` + `extensions`。

| 缺失 | 影响 | 优先级 |
| --- | --- | --- |
| `stop_details`（`RefusalStopDetails{category, explanation, type}`，`messages.d.ts:4068`） | 只进 `extensions`，无类型：refusal 时业务拿不到 `category`（cyber/bio/frontier_llm/reasoning_extraction/general_harms），只能手工取 JSON | P1 |
| `container`（`{id, expires_at, skills}`） | 代码执行/容器续跑需要 `container.id`（`handling-stop-reasons.md:1565`）；现在只能 `response.extensions["container"]["id"]` | P2 |
| `diagnostics`（`cache_miss_reason`） | 缓存排障（`cache-diagnostics.md`） | P2 |
| `stop_reason` 仍是 `Option<String>` | 无 `StopReason` 枚举、无 `is_tool_use()`/`is_refusal()`/`truncated()` 助手（官方 7 个取值：`messages.d.ts:4175`） | P1 |
| `role` 是 `Option<String>` | 类型弱化，应为字面量 | P2 |

### 3.2 `Usage`：10 → 4

官方（`messages.d.ts:4806`）：`cache_creation{ephemeral_1h_input_tokens, ephemeral_5m_input_tokens}, cache_creation_input_tokens, cache_read_input_tokens, inference_geo, input_tokens, output_tokens, output_tokens_details{thinking_tokens}, server_tool_use{web_fetch_requests, web_search_requests}, service_tier, speed`。
airx（`common.rs:86-102`）：`input_tokens, output_tokens, cache_creation_input_tokens, cache_read_input_tokens` + `extensions`。

- `output_tokens_details.thinking_tokens`（P1）：官方明确「thinking 按 output 计费，用该字段核对成本」（`extended-thinking.md:306`）。业务要做成本归因只能挖 `extensions`。
- `cache_creation` 分 5m/1h（P1）：1h 缓存与 5m 缓存单价不同（`prompt-caching.md:828-901`）。
- `server_tool_use`（P2）、`service_tier`/`speed`/`inference_geo`（P2）。

### 3.3 响应内容块：12 → 6 类型化

官方 `ContentBlock` union（`messages.d.ts:3571`）12 个成员；airx 命中 6 个（text / thinking / redacted_thinking / tool_use / server_tool_use / web_search_tool_result），其余 6 个落 `Other(Value)` 且**双向保真**（`message.rs:244-275` 未知 type → `Other`，序列化原样回写，`message.rs:234`）——设计上没问题，属于「可读写但不可类型化访问」。

字段级缺口：

| 块 | 官方字段 | airx | 优先级 |
| --- | --- | --- | --- |
| `TextBlock` | `citations[]` | 无类型字段（`message.rs:53-60`），落 `extensions` | P1（若用 citations） |
| `ToolUseBlock` | `caller`（**必填**）、`toolset_name` | 无类型字段（`message.rs:74-85`） | P2 |
| `ThinkingBlock` | `signature`（**必填**） | `Option<String>` + `skip_serializing_if`（`message.rs:105-114`）：手工构造时若漏填，会发出非法请求 | P1 |
| `ToolResultBlock` | `content: string \| blocks[]`、`is_error`、`toolset_name` | `content: Option<Value>`（`message.rs:92-94`） | P2 |

### 3.4 流式事件与增量

事件层覆盖完整：官方 6 类事件 + `ping` + `error` 全部有变体（`event.rs:14-33`），未知事件 `Unknown(Value)`（`event.rs:63`），端点过滤 `ping`、`error` 直接变 `Err`（`endpoint.rs:87-96`）。

但**增量与嵌套载荷有丢字段**：

| 位置 | 官方 | airx | 影响 | 优先级 |
| --- | --- | --- | --- | --- |
| `RawMessageDeltaEvent.delta` | `{container, stop_details, stop_reason, stop_sequence}`（`messages.d.ts:4015-4031`） | `MessageDelta` 只有 `stop_reason`/`stop_sequence` 且**无 `extensions` 兜底**（`event.rs:174-182`） | `stop_details`/`container` 在流式路径被**彻底丢弃**（非流式还在 extensions 里）→ 流式无法识别 refusal 细类、无法续跑容器 | P1 |
| `citations_delta` | 独立 delta 类型（`messages.d.ts:2387`） | `Delta::Other` 命中后**被静默忽略**（`accumulator.rs:168`） | 流式 texts 的 citations 拿不到 | P1 |
| `content_block_start` 的块字段 | 保留全部形状 | `BlockState::from_block` 只抽 `text`/`thinking`/`id+name+input`，重建时 `extensions: Extensions::new()`（`accumulator.rs:34-96`） | 块级 `citations`/`caller`/`toolset_name`/未知字段在聚合结果中丢失（`snapshot()` 与 `finish()` 都丢） | P1 |
| `MessageDeltaUsage.input_tokens: number \| null` | 允许 `null`（`messages.d.ts:3890`、`docs/api/messages.md:4318`） | `Usage.input_tokens: u32` + `#[serde(default)]`（`common.rs:88-89`）：缺字段 OK，**显式 `null` 会反序列化失败** | 一旦出现即整条流报 `Deserialization`，`message_stop` 前功尽弃 | P1 |
| `ContentBlockStop` | `{index}` | 不记录块结束状态（`accumulator.rs:176`） | 无功能影响，仅无法判断「块是否完整」 | P2 |

---

## 4. 行为差异（默认值、完成语义、错误映射、结构化输出、工具载荷、thinking）

1. **API 版本头（最严重）**。`config.rs:16` 默认 `2023-06-01`；官方当前版本是 `2026-10-21`（`docs/api/versioning.md:4,25`，`docs/api/overview.md:50`），且 `2026-10-21` 的 breaking 变更正好覆盖 airx 已建模的参数：`temperature/top_p/top_k` 移除、`document` 块限制、citations 限制（`versioning.md:35-39`）。`output_config`、`thinking.display`、`adaptive`/`between_tools`、`speed`、`stop_reason=refusal|model_context_window_exceeded` 都只在这个文档面里出现。测试把旧值锁死了（`crates/airx-protocol/tests/anthropic_protocol.rs:40-43`）。
2. **`stream` 与 `create` 的组合无保护**。`.stream(true)` 后调 `create()` 会把 SSE 响应体交给 JSON 解码，报 `Deserialization`（`endpoint.rs:61-73`），错误信息不指向「用错了入口」。`stream()` 会强制置位（`anthropic.rs:180`），但反向没有校验。
3. **`parse_as` 与官方 `parse` 语义不同**。官方 `messages.parse` 接受 schema/`output_format`、注入 `output_config.format`、校验并返回 `parsed_output`（`structured-outputs.md:199,243`）；airx 只是 `serde_json::from_str(response.text())`（`anthropic.rs:148-155`），文档也承认（`docs/anthropic.md:272,517`）。风险是名字暗示了「结构化输出保证」。
4. **结构化输出的不兼容约束不做前置校验**。官方：与 citations 同时使用返回 400、与 assistant prefill 不兼容（`structured-outputs.md:3069-3070`），切换 format 会使 prompt cache 失效并额外注入 system prompt（`structured-outputs.md:2855-2859`）。airx 全部交给服务端（`docs/anthropic.md` 已说明），可接受，但缺一个「本地快速失败」的开关。
5. **thinking 预算不校验**。官方硬规则：`budget_tokens ≥ 1024` 且 `< max_tokens`，且不能与 `max_tokens: 0` 预暖组合（`extended-thinking.md:281-287`）。airx 的 `ThinkingConfig::enabled(u32)` 无任何校验（`request.rs:318-323`），而 `max_tokens` 是必填项（`request.rs:367-371`）——两者本可在 `build()` 里互相校验，属于「低成本高收益」的本地检查。
6. **`max_tokens: 0`（缓存预暖）** 在 airx 可行（`u32` 允许 0，无下限校验），与官方一致（`messages.d.ts` `max_tokens` 注释；`prompt-caching.md:908-975`），但文档未提这个用法（`docs/anthropic.md` §9 只说「必填」）。
7. **重试语义与官方 SDK 不同**。官方 SDK「连接错误、速率限制、5xx 默认重试两次」（`docs/api/errors.md:57`）；airx 对 **POST 的 5xx/超时不重试**（`docs/anthropic.md:410-416`）。对 `messages.create` 这种幂等语义上安全、但官方认为可重试的接口，业务会感受到更多 529/500 穿透。这是有意的边界（避免重复计费），但应在文档中作为「与官方 SDK 的差异」显式列出（目前只在重试表里隐含）。
8. **错误映射**：`error.type → ProviderError.code`、`status`、`request-id` Header 都做了（`common.rs:32-59,75-82`）。两个缺口：
   - 错误 body 顶层的 `request_id` 字段（`docs/api/errors.md:76`）没有解析，`AnthropicErrorResponse` 无 `extensions` 兜底（`common.rs:12-19`）→ 网关吞掉 Header 时拿不到 request id（P2）。
   - `error.details.error_code`（如 `storage_quota_exceeded` / `cmek_key_disabled`，`docs/api/files.md:243-260`、`docs/api/files/upload.md`）未建模（P2）。
9. **流内 `error` 事件**：`endpoint.rs:89` 直接 `Err`，因此 `final_response()` 在 error 处提前返回，**拿不到已累积的部分内容**（`accumulator.apply(Error)` 分支只在直接喂事件时可达，`accumulator.rs:201-204`）。官方建议「流内错误不遵循 HTTP 语义」（`errors.md:63`），业务常需要「部分结果 + 错误」；建议提供 `apply` 后仍可 `snapshot()` 的路径（P2）。
10. **完成语义**：`message_stop → Completed`，EOF 无 `message_stop → Incomplete`（`accumulator.rs:198-230`），与官方「`message_stop` 是结束信号」（`streaming.md:328-332`）一致。注意官方警告 `message_delta.usage` 是**累计值**（`streaming.md:334`），airx 的合并策略是「`output_tokens` 覆盖、cache 字段有新值才覆盖」（`accumulator.rs:184-196`），语义正确；但**不更新 `input_tokens`**，若 `message_delta` 携带（schema 允许 `number|null`），聚合结果会漏。
11. **工具调用载荷**：`input_json_delta` 拼接后解析，`finish()` 严格模式、`snapshot()` 宽松模式（非法 JSON 变字符串）（`accumulator.rs:71-86`），与官方「partial_json 流式拼装」（`streaming.md` 工具用例）一致，设计优于「静默吞错」。
12. **`messages[]` 支持 `role:"system"`**（对话中途插入系统指令，`docs/api/messages/create.md:1098`、`working-with-messages.md:357`），airx `Role` 只有 `User`/`Assistant`（`message.rs:12-17`），反序列化 `system` 角色会失败。

---

## 5. 业务接入易用性与代码清晰度（建议的 API 形状）

原则：**只做「一个请求一个响应」的类型与助手**，把「多轮/循环/执行」留给业务。以下签名均可增量添加，不破坏现有调用。

### 5.1 prompt caching 一等公民（当前完全靠 `extensions`）

```rust
// airx-protocol/src/anthropic/messages/message.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControl { pub kind: CacheKind, pub ttl: Option<CacheTtl> }   // {"type":"ephemeral","ttl":"1h"}
impl TextBlock { pub fn cached(self, control: CacheControl) -> Self; }      // 写 cache_control 到 extensions
impl Tool      { pub fn cached(self, control: CacheControl) -> Self; }
impl MessagesRequestBuilder {
    pub fn cache_control(self, control: CacheControl) -> Self;              // 顶层自动缓存
    pub fn system_blocks(self, blocks: impl IntoIterator<Item = TextBlock>) -> Self;
}
```

价值：让 `system + tools + 前若干轮` 的断点写法从「手写 json」变成 3 行；同时把 §2.1 的 `system` 收窄问题一并解决。

### 5.2 `system` 与 `Role`

```rust
pub enum SystemPrompt { Text(String), Blocks(Vec<TextBlock>) }   // impl From<&str>/String，保持 .system("x") 可编译
pub enum Role { User, Assistant, System }                        // + Role::system() / Message::system(...)
```

### 5.3 响应用的「判定助手」而不是字符串比较

```rust
#[non_exhaustive]
pub enum StopReason { EndTurn, MaxTokens, StopSequence, ToolUse, PauseTurn, Refusal, ModelContextWindowExceeded, Other(String) }
impl MessagesResponse {
    pub fn stop_reason(&self) -> Option<StopReason>;
    pub fn is_tool_use(&self) -> bool;
    pub fn is_paused(&self) -> bool;                 // pause_turn：服务端工具续跑
    pub fn is_refusal(&self) -> bool;                // + stop_details()
    pub fn is_truncated(&self) -> bool;              // max_tokens | model_context_window_exceeded
    pub fn stop_details(&self) -> Option<&RefusalStopDetails>;
    pub fn thinking_tokens(&self) -> u32;            // usage.output_tokens_details.thinking_tokens
    pub fn container_id(&self) -> Option<&str>;
}
```

这是「不做 agent 层」的关键平衡点：airx 只告诉业务「为什么停了」，续跑由业务决定。

### 5.4 结构化输出：让名字和语义对齐

```rust
pub struct Parsed<T> { pub data: T, pub response: MessagesResponse }

impl Messages {
    /// 要求 request.output_config.format 已设置，否则返回 Configuration 错误（快速失败）。
    pub async fn parse_as<T: DeserializeOwned>(&self, request: MessagesRequest) -> Result<Parsed<T>, AirxError>;
    /// 由 schema 生成 request 的便捷构造（不引入 schema 校验库，只做 request 拼装）。
    pub fn structured(model: impl Into<ModelId>, max_tokens: u32, schema: Value) -> MessagesRequestBuilder;
}
```

保留现有 `parse_as` 语义（纯 serde）时，至少改名或文档首行加粗「不做 schema 注入」。

### 5.5 Files 分页与删除（P0 修复的形状）

```rust
pub struct ListFilesRequest { pub limit: Option<u32>, pub page: Option<String>, pub ids: Option<Vec<String>> }
pub struct FileList { pub data: Vec<FileMetadata>, pub next_page: Option<String>, pub extensions: Extensions }
impl FileList { pub fn next_page(&self) -> Option<&str>; }        // 替代恒 None 的 next_cursor()
pub struct DeletedFile { pub id: String, pub kind: Option<String> }  // 删除 deleted: bool
```

### 5.6 Batches 与结果类型化

```rust
impl Batches { pub async fn delete(&self, id: impl Into<String>) -> Result<DeletedBatch, AirxError>; }

#[derive(Deserialize)] pub struct BatchResultLine { pub custom_id: String, pub result: BatchResult }
#[derive(Deserialize)] #[serde(tag = "type", rename_all = "snake_case")]
pub enum BatchResult { Succeeded { message: MessagesResponse }, Errored { error: AnthropicErrorBody }, Canceled, Expired }
impl Batches { pub async fn results_typed(&self, id: impl Into<String>) -> Result<Vec<BatchResultLine>, AirxError>; }
```

保留现有 `results() -> Bytes` 作为逃生通道；`results_typed` 只是逐行 serde。

### 5.7 Header 参数的正确安放处

`workspace_id` / `user_profile_id` 是 Header，不该走 `extensions`。建议在门面显式命名（避免用户记 Header 名）：

```rust
impl RequestOptions {                     // 或新的 AnthropicRequestOptions
    pub fn workspace(self, workspace_id: impl Into<String>) -> Self;      // anthropic-workspace-id
    pub fn user_profile(self, profile_id: impl Into<String>) -> Self;     // anthropic-user-profile-id
}
```

现有 `RequestOptions::try_header`（`options.rs:37-50`）已能实现，成本极低，主要是「发现性」问题。

### 5.8 枚举稳定性

给 `ContentBlock`、`Delta`、`MessagesEvent`、`StopReason`、`Tool`、`ToolChoice` 加 `#[non_exhaustive]`：官方明确「会新增块类型 / 事件类型 / 枚举值」（`versioning.md:11-19`），现在新增变体是破坏性变更。

---

## 6. 文档与代码不一致

| # | 文档 | 位置 | 实际 / 官方 | 判定 |
| --- | --- | --- | --- | --- |
| 1 | `docs/anthropic.md` | `:118` `const BETA: &str = "context-management-2026-01-01";` | 官方 beta 名是 `context-management-2025-06-27`（`docs/api/beta-headers.md:34,47,60`）。照抄会得到 400 `invalid beta flag` | **错误示例** |
| 2 | `docs/anthropic.md` | `:423-426` 把 `workspace_id` 列为「走 extensions」 | `workspace_id` 是 Header `anthropic-workspace-id`（`docs/api/overview.md:49`、`docs/api/messages.md` 每个方法 Headers 段），写进 body 无效 | **错误** |
| 3 | `docs/anthropic.md` | `:426` 同段列了 `context_management` | GA `MessageCreateParams` **没有** 该字段（`messages.d.ts:5639`），它是 beta 字段 + beta 头；示例（`:428-430`）会在 GA 端点被拒或忽略 | **错误** |
| 4 | `docs/anthropic.md` | `:82`、`:99` 默认/示例 `api_version("2023-06-01")` | 与代码一致（`config.rs:16`），但与官方当前 `2026-10-21` 不一致，且示例在强化旧值 | 需同步（P0） |
| 5 | `docs/anthropic.md` | `:168` `.system(impl Into<String>)`「单个字符串，不是消息数组」 | 陈述准确，但未标注这是**相对官方的功能缺口**（官方 `string \| TextBlockParam[]`，`docs/api/messages/create.md:1332`） | 需补限制说明 |
| 6 | `docs/anthropic.md` | `:299-310`（事件表）把 `message_delta` 载荷写成 `stop_reason / stop_sequence / usage.output_tokens` | 官方还含 `container` / `stop_details`（`messages.d.ts:4015-4031`），airx **丢弃**这两个字段；文档未提示丢失 | 需补 |
| 7 | `docs/anthropic.md` | `:265,272,517` `parse_as` 只做 serde | 与代码一致（`anthropic.rs:148-155`）——但入口名与官方 `parse` 同名不同义，建议在 §5.1 顶部加「与官方 `messages.parse` 的区别」 | 需澄清 |
| 7b | `docs/protocol-support.md` | `:66-67` 同样把 `workspace_id` 归到 `extensions` 透传 | 与官方 Header 语义冲突，同 #2 | **错误** |
| 8 | `docs/protocol-support.md` | `:215` 对齐来源写 `anthropic-sdk-python`（`Messages.create` 签名、`message_create_params.py`） | 仓库实际基准是 **TypeScript SDK** `@anthropic-ai/sdk@1.13.0`（`api/SUMMARY.md:4`、`versions.env`）。Python 与 TS 在 beta/字段命名上并不总是一致 | **过期** |
| 9 | `docs/protocol-support.md` | `:6` 「对齐基准：2026-11-23」 | 快照 `docs/meta.json.generated_at = 2026-11-24T01:33:08Z` | 小误差 |
| 10 | `docs/protocol-support.md` | `:57` 内容块只列 text/image/tool_use/tool_result/thinking/redacted_thinking | `server_tool_use`、`web_search_tool_result` 也是类型化的（`message.rs:44-47`）——文档**低估**自身能力 | 需补 |
| 11 | `docs/protocol-support.md` | `:101-107` Files/Batches 表只写路径 | 未记录 Files 分页参数与官方不一致（§2.6），读者会以为已对齐 | 需补 |
| 12 | 测试 | `crates/airx/tests/resources.rs:339` mock 返回 `{"data":[],"has_more":false}` | 官方 List Files 响应是 `{data, next_page}`（`docs/api/files.md:237-239`）；测试把错误形状固化了，导致 P0 不会被发现 | 需改 |
| 13 | 测试 | `crates/airx-protocol/tests/anthropic_protocol.rs:40-43` 断言 `anthropic-version == 2023-06-01` | 与 P0-1 冲突，改默认值必须同步改测试 | 需改 |

---

## 7. 风险与注意事项

1. **版本头是「全局开关」**。`RequestOptions` 可以覆盖同名 Header（`docs/anthropic.md:82`），但默认值错了会让所有业务默认走旧语义。修默认值时要同时评估网关（DeepSeek Anthropic 兼容端点等）——它们未必接受 `2026-10-21`，因此**保留 `with_api_version` 覆盖能力**（`config.rs:68-72`）是必要的，且应在文档写清「兼容网关请显式固定版本」。
2. **`output_format` vs `output_config.format` 易混**。GA 是 `output_config.format`（`messages.d.ts:3946`），beta 的 `parse()` 侧有独立 `output_format`（`resources/beta/messages/messages.d.ts` 的 `parse` docstring）。airx 只做 GA 是正确选择，但文档/示例不要混用两个名字。
3. **prompt cache 的失效触发面很宽**：`thinking` 模式或 `budget_tokens`（`extended-thinking.md:340`）、`output_config.effort`（`prompt-caching.md:914`）、`output_config.format`（`structured-outputs.md:2859`）、工具集合（`structured-outputs.md:2851`）都会让缓存整体失效。把这些写进一处「缓存失效清单」比散落在各章更有用。
4. **thinking 与 effort 的耦合存在 400 陷阱**。例如 Haiku 5.5 在 `effort ≥ xhigh` 时 `thinking: disabled` 会 400（`thinking.md:499`）。airx 不校验是合理的（模型矩阵会变），但应给一行错误示例 → 指向官方 troubleshooting。
5. **`stop_reason` 必须被业务处理**：`pause_turn` 需要把 assistant content 原样回传（`handling-stop-reasons.md:1560-1565`），`refusal` 需要读 `stop_details`（`:20`），`model_context_window_exceeded` 应视为截断（`:21`）。airx 支持声明 `web_search`（`request.rs:201-203`），用户**一定会**遇到 `pause_turn`，因此 `is_paused()` 这类助手价值很高。
6. **模型示例认知漂移**：官方 `Model` 联合类型已到 `claude-opus-5-5` / `claude-sonnet-5-5`（`messages.d.ts:3945`），airx 文档示例仍用 `claude-sonnet-4-5`（`docs/anthropic.md:49,184,226,332,430`）。示例不会失效（`(string & {})` 兜底），但会误导新用户对大模型能力的预期。
7. **认证方式**：`x-api-key` 已被官方描述为「`Authorization` 的 legacy fallback，仍支持」（`docs/api/overview.md:47-48`）。airx 只发 `x-api-key`（`config.rs:118-121`）当前可用，但若要接 OAuth/WIF 短期 token（`Bearer`）就没有入口——属于 P2 的长期项。
8. **`extensions` 的双刃剑**：`BTreeMap<String, Value>` + `flatten`（`airx-core/src/json/value.rs:10`）能双向保真，但**把已建模字段塞进 extensions 会产出重复 JSON 键**（struct 字段 + flatten 各写一次），语义依赖解析器。文档应明确：extensions 只用于「未建模字段」，已建模字段用类型化入口（`system` / `cache_control` 就是典型受害者）。
9. **请求体大小限制**：Messages / countTokens 128 MB、Batch 256 MB、Files 500 MB（`docs/api/errors.md:66-72`），airx 不做本地限制，超限得到 413 `request_too_large`；本地提前拒绝（batch 请求条数/文件大小）可减少无效上传。
10. **schema 漂移的检测**：manifest 有 466 条 `*Params`，airx 只覆盖其中 8 条左右；建议加一个「快照 diff → 生成差异清单」的脚本（`reference/anthropic/scripts/` 已有解析脚本），把「新增字段」变成可审阅的 PR 而不是线上 400。

---

## 8. 分级建议

### P0（正确性 bug / 阻断常见生产用法）

| # | 动作 | 文件位置 | 验收点 |
| --- | --- | --- | --- |
| P0-1 | `DEFAULT_API_VERSION` 改 `2026-10-21`，文档同步，保留 `with_api_version` 覆盖 | `crates/airx-protocol/src/anthropic/config.rs:16`；`docs/anthropic.md:82,99` | 新测试断言默认头为 `2026-10-21`；`anthropic_protocol.rs:40-43` 更新；网关场景文档给出显式固定版本的例子 |
| P0-2 | Files 列表改为 `limit/page/ids`，响应读 `next_page`，`next_cursor()` → `next_page()` | `files.rs:47-76,124-136`；`docs/anthropic.md:420-434` 附近；`docs/protocol-support.md:101-107` | 单测：`GET /v1/files?page=page_1&limit=10`；`FileList{data,next_page:Some}` 能翻第二页；`resources.rs:339` mock 改为官方形状 |
| P0-3 | 删除 `FileDeleteResponse.deleted`，改为 `{id, type}` 并保留 `extensions` | `files.rs:79-92`；`anthropic.rs:298-305` 返回类型 | 单测：解析 `{"id":"file_1","type":"file_deleted"}` 无 `deleted` 字段；若为兼容保留，则标注 `#[deprecated]` 并附「官方不返回」注释 |

### P1（重要的保真度 / 易用性）

| # | 动作 | 文件位置 | 验收点 |
| --- | --- | --- | --- |
| P1-1 | `system` 支持 `string \| TextBlock[]`（`SystemPrompt` 枚举），`Role::System` | `request.rs:22-23,398-403`；`message.rs:12-17` | `.system("x")` 仍可编译；`.system_blocks([...])` 产出数组；`role:"system"` 消息可解析；`text()` 不受影响 |
| P1-2 | 流式保真：`MessageDelta` 增 `container`/`stop_details` + `extensions`；补 `citations_delta`；`BlockState` 保留原始块 JSON；`Usage.input_tokens` 改 `Option<u32>` | `event.rs:174-182,104-116`；`accumulator.rs:17-96,168,184-196`；`common.rs:88-89` | 单测：`message_delta` 带 `stop_details`/`container` 后聚合结果可读；`input_tokens:null` 不报错；`citations_delta` 进入文本块 citations |
| P1-3 | `Usage` 补 `output_tokens_details.thinking_tokens`、`cache_creation{5m,1h}`、`server_tool_use`、`service_tier`、`speed` | `common.rs:86-102` | `thinking_tokens()` 访问器；流式与聚合都能取到 |
| P1-4 | `count_tokens` 对齐 11 字段（至少补 `tool_choice`/`output_config`/`cache_control`） | `count_tokens.rs:21-38,49-56` | 序列化断言；文档 §6.1 字段表更新 |
| P1-5 | `ToolChoice::{None}` + `disable_parallel_tool_use`；`Tool::{strict, cached}` | `request.rs:212-224,107-119,180-209` | `{"type":"none"}` / `{"type":"auto","disable_parallel_tool_use":true}` / `{"strict":true}` 快照测试 |
| P1-6 | 新增 `DELETE {messages_path}/batches/{id}` 与 `Batches::delete` | `batches.rs`（新增 `DeleteBatchEndpoint`）；`anthropic.rs:326-381` | 路径断言 `DELETE /v1/messages/batches/msgbatch_1`；返回 `{id,type:"message_batch_deleted"}` |
| P1-7 | `RequestOptions::{workspace, user_profile}`；文档把 `workspace_id` 从 extensions 清单移走 | `options.rs`；`docs/anthropic.md:423-426` | 请求头断言 `anthropic-workspace-id`；文档不再建议 extensions |
| P1-8 | `document` 块类型化（`Base64/URL/FileId/PlainText/Content` source + `title/context/citations`） | `message.rs`（新增 `DocumentBlock`），`ContentBlock` 加变体 | 往返测试；`docs/anthropic.md:4.3` 表更新 |
| P1-9 | 响应助手：`stop_reason()` 枚举、`is_tool_use/is_paused/is_refusal/is_truncated`、`stop_details()` | `response.rs:40-62` | 单测覆盖 7 个 stop_reason；文档「如何处理 stop_reason」小节 |
| P1-10 | 修正 beta 名与 GA/beta 归属（`context-management-2025-06-27`；`context_management` 标注 beta-only） | `docs/anthropic.md:118,426-430` | 示例可复制不报错；§3.2 增加「beta 字段必须同时带 beta 头」 |
| P1-11 | `create()` 拒绝 `stream=true`（明确报 Config 错） | `anthropic.rs:136-138` | 单测：`.stream(true)` + `create()` → `Configuration` |
| P1-12 | 文档与测试不再把 Files 官方形状写错；`protocol-support.md` 对齐来源改为 TS SDK 1.13.0 | `docs/protocol-support.md:6,57,215`；`resources.rs:339` | 文档日期/来源与 `meta.json` 一致 |

### P2（细分场景）

- `metadata.user_id`、`service_tier`、`speed`、`inference_geo`、`diagnostics`、`container` 类型化（`request.rs`）。
- `ModelListParams.lifecycle[]` + `ModelInfo` 的 `max_tokens/max_input_tokens/lifecycle/capabilities`（`models.rs:20-36,69-81`）。
- `FileUploadParams.expires_in_seconds`（`files.rs:95-122`）；`FileMetadata.expires_at`（`files.rs:20-45`）。
- `batch.results` 类型化行解析（`batches.rs:253-294`）。
- 错误体 `request_id` / `error.details.error_code`（`common.rs:12-29`）。
- `ToolUseBlock.caller/toolset_name`（`message.rs:74-85`）；`ToolResultBlock` 的 `toolset_name`。
- `#[non_exhaustive]` 加到所有官方可扩展枚举（`message.rs:31`、`event.rs:14,104`、`request.rs:97,213`）。
- `extensions` 与已建模字段重复键的本地防护（`build()` 时拒绝保留键）。
- Bearer/OAuth 认证入口（`config.rs:117-132`）。

### 明确不做

见 §9。

---

## 9. 不做清单（agent 相关；说明为什么不做）

| 能力 | 为什么不做 | 业务需要时的替代路径 |
| --- | --- | --- |
| 工具执行循环 / tool_use → 执行 → tool_result 自动回传 | 这是 agent runtime，不是协议。airx 的职责边界是「一次请求 = 一个响应」（`docs/anthropic.md:15`）。循环一旦内建，就要接管超时、并发、权限、审计、重试语义，与「请求工具」定位冲突 | 业务自己 `response.tool_uses()` → 执行 → `Message::blocks(Role::User, [ToolResultBlock])` → 再发一次；`docs/anthropic.md:4.3` 已有示例 |
| `beta.messages.toolRunner` / `beta.messages.thread` | 官方这两个是 Agent SDK 形态（`api/SUMMARY.md:154-155`，无 HTTP 端点），本质是编排器 | 用 `stream()` + 自己的循环 |
| `pause_turn` 自动续跑 | 续跑需要「把 content 原样回传 + 保持 tools + 容器 id」，属于编排决策（要不要继续、继续几次、花多少钱） | airx 提供 `is_paused()` / `container_id()`，业务自行重建请求（`handling-stop-reasons.md:1560-1565`） |
| Managed Agents / Sessions / Environments / Deployments（beta，`api/SUMMARY.md:16-59,703-758`） | 托管 agent 平台，不是 LLM 请求协议 | 直接调官方 SDK 或 REST |
| Skills / Memory Stores / Vaults / MCP connector（`beta.skills`、`beta.memoryStores`、`beta.vaults`、`mcp_servers`） | 状态与凭据托管，超出「请求工具」；且全部 beta | 走 `execute(HttpRequest)` 逃生通道或官方 SDK |
| 组织/账号管理（`organization.*`、`apiKeys`、`rbacGroups`、`spendLimits`、`workspaces`，GA 37 个 resource 中的 34 个） | 控制面 API，与业务 LLM 调用无关 | 管理后台 / 官方 SDK |
| schema 编译期校验（JSON Schema → Rust 类型生成）、`transform_schema` 等价物 | 会引入重依赖与代码生成，违背「代码清晰」；且 airx 不做 schema 校验是显式设计（`docs/anthropic.md:17`） | 业务侧用 `schemars`/`serde_json::Value` 自管；airx 只负责把 `schema` 放进 `output_config.format` |
| 客户端 tokenizer / 本地计 token | 官方口径不公开，本地方案必然漂移 | `count_tokens`（补齐 §2.2 字段后即准） |
| 流式重试 / 事件重放 | SSE 事件不可重放；重试会在服务端产生二次计费 | 文档已明确（`docs/anthropic.md:8.5`）；业务在首事件前自行重试 |

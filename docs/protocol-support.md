# 协议支持

本文记录各协议已实现的能力与当前边界。实现位于 `airx-protocol`，全部为纯编解码，
不依赖传输层。

> **对齐基准：2026-11-23。**
> 官方文档站点在部分区域不可达，参数以官方 SDK 的生成类型 / 请求签名为准：
> Anthropic Python SDK `messages.create`、OpenAI Python SDK Chat/Responses 参数、
> Gemini `generateContent` REST 参考、DeepSeek JSON mode 指南。协议参数会随模型变化，接入新模型前请先核对
> [compatibility.md](compatibility.md) 与官方最新文档。

## OpenAI Chat Completions

| 能力 | 状态 |
| --- | --- |
| 请求：model / messages / temperature / top_p / stop / user | 已实现 |
| 请求：`max_completion_tokens`（推荐，含推理 token） | 已实现 |
| 请求：`max_tokens` | 已实现但**已弃用**；o-series 不兼容 |
| 请求：`reasoning_effort`（none/minimal/low/medium/high/xhigh/max） | 已实现 |
| 请求：`verbosity`（low/medium/high） | 已实现 |
| 请求：tools / tool_choice / parallel_tool_calls | 已实现 |
| 请求：response_format（text / json_object / json_schema） | 已实现 |
| 请求：stream / stream_options.include_usage | 已实现 |
| 消息：system / user（文本或多模态）/ assistant（文本 + tool_calls）/ tool | 已实现 |
| 响应：choices / usage / system_fingerprint / 未知字段保留 | 已实现 |
| 错误体：message / type / code / param → ProviderError | 已实现 |
| 流式：chunk / delta / tool_calls 增量 / `reasoning_content` / `[DONE]` / usage chunk | 已实现 |
| 聚合：`ChatStreamAccumulator`（合并 reasoning 到 `extensions.reasoning_content`；无 `[DONE]` 报 Incomplete） | 已实现 |
| 未覆盖参数 | 通过 `extensions` 透传 |

## OpenAI Responses

| 能力 | 状态 |
| --- | --- |
| 请求：model / input（文本或 item）/ instructions / max_output_tokens / temperature / top_p | 已实现 |
| 请求：`reasoning.effort` | 已实现 |
| 请求：`text.format`（text / json_object / json_schema） | 已实现 |
| 请求：tools / tool_choice / parallel_tool_calls | 已实现 |
| 输入 item：message / function_call_output | 已实现 |
| 输出 item：message（output_text / refusal）/ function_call | 已实现 |
| 未知 item / content / 事件 | `Other(Value)` / `Unknown(Value)` 原样保留 |
| 流式事件：created / output_item.added·done / output_text.delta·done / function_call_arguments.delta·done / completed / failed / incomplete / error | 已实现 |
| 流式未知事件 | `Unknown(Value)` |
| 聚合：`ResponsesStreamAccumulator`（无 completed 报 Incomplete，failed 报 Failed） | 已实现 |

## Anthropic Messages

| 能力 | 状态 |
| --- | --- |
| 请求：model / max_tokens（必填）/ messages / system / stop_sequences | 已实现 |
| **已移除**：temperature / top_p / top_k | 最新 Messages API 不再接受这些参数 |
| 请求：`output_config.effort`（low/medium/high/xhigh/max） | 已实现 |
| 请求：`output_config.format`（`json_schema` 结构化输出） | 已实现 |
| 请求：`thinking`（enabled + budget_tokens / disabled / between_tools / adaptive） | 已实现 |
| 请求：tools / tool_choice | 已实现 |
| 内容块：text / image / tool_use / tool_result / thinking / redacted_thinking | 已实现 |
| 未知内容块 | `Other(Value)` 原样保留 |
| 响应：content / stop_reason / usage（含缓存 token） | 已实现 |
| 错误体：type / message → ProviderError | 已实现 |
| 流式事件：message_start / content_block_start·delta·stop / message_delta / message_stop / ping / error | 已实现 |
| 流式未知事件 | `Unknown(Value)`；`ping` 在协议层过滤 |
| 聚合：`MessagesStreamAccumulator`（tool 参数增量拼接后解析） | 已实现 |

`output_config` / `thinking` 的 `display` 等新字段已建模；账号级参数
（`inference_geo` / `service_tier` / `speed` / `workspace_id` 等）暂通过 `extensions` 透传。

## Gemini generateContent

Gemini 的模型位于路径中（`/models/{model}:generateContent`），因此
`GenerateContentEndpoint` 持有模型，请求体只对应官方 JSON body。

| 能力 | 状态 |
| --- | --- |
| 请求：`contents`（role + parts）/ `systemInstruction` | 已实现 |
| 请求：`generationConfig`（temperature / topP / topK / candidateCount / maxOutputTokens / stopSequences / penalties / seed / responseModalities / mediaResolution / speechConfig / imageConfig） | 已实现 |
| 请求：结构化输出（`responseMimeType` + `responseSchema` / `responseJsonSchema`） | 已实现 |
| 请求：`thinkingConfig`（`thinkingBudget` / `includeThoughts` / `thinkingLevel`） | 已实现 |
| 请求：`tools`（`functionDeclarations`）/ `toolConfig`（AUTO / ANY / NONE + allowedFunctionNames） | 已实现 |
| 请求：服务端工具（`googleSearch` / `codeExecution` / `urlContext` 等） | `Tool::server(...)` 原样透传 |
| 请求：`safetySettings` / `cachedContent` | 已实现 |
| 内容片段：`text`（含 `thought` / `thoughtSignature`）/ `inlineData` / `fileData` / `functionCall` / `functionResponse` / `executableCode` / `codeExecutionResult` | 已实现 |
| 未知片段 / 未知字段 | `Part::Other(Value)` 与 `extensions` 原样保留 |
| 响应：`candidates`（content / finishReason / safetyRatings / citation / grounding / logprobs）/ `promptFeedback` / `usageMetadata` / `modelVersion` / `responseId` | 已实现 |
| 错误体：`error.code` / `message` / `status` / `details` → ProviderError | 已实现 |
| 流式：`alt=sse`，每个 data 为 `GenerateContentResponse` 增量 | 已实现 |
| 聚合：`GenerateContentStreamAccumulator`（文本按 thought 标记合并；`finishReason` 或 `blockReason` 判定完成） | 已实现 |
| 未覆盖：`countTokens`、模型列表、Vertex AI 专属参数 | 未封装；用 Raw 扩展字段 / 原生 Endpoint |

Gemini 流没有 `[DONE]` 哨兵。`finish()` 只有在收到候选的 `finishReason`
（或输入被拦截的 `promptFeedback.blockReason`）时才返回成功；EOF 提前发生返回
`Incomplete`。`FINISH_REASON_UNSPECIFIED` 不视为完成。

认证使用 `x-goog-api-key`（不把 key 放进 URL）；默认基地址
`https://generativelanguage.googleapis.com/v1beta`。模型可写 `gemini-2.5-flash`、
`models/gemini-2.5-flash`、`tunedModels/...`，端点会规范化路径。

## 兼容部署：DeepSeek

DeepSeek **不是独立协议**，而是 OpenAI / Anthropic 兼容部署。airx 不提供
`airx-protocol/deepseek` 模块：同一个协议客户端 + `base_url` 即可接入。

| 协议形态 | base_url | 说明 |
| --- | --- | --- |
| OpenAI Chat Completions | `https://api.deepseek.com` | `POST /chat/completions`；JSON mode 使用 `response_format={"type":"json_object"}` |
| Anthropic Messages | `https://api.deepseek.com/anthropic` | `POST /v1/messages`；`x-api-key` + `anthropic-version` |

DeepSeek JSON mode（依据官方指南）：

- `response_format` 设为 `{"type": "json_object"}`；不支持 `json_schema`。
- 系统或用户提示词**必须包含 “json”** 并给出期望输出示例。
- 解析失败的非 JSON 内容会被替换为空格，可能出现 content 为空但 `finish_reason=stop`。
- 用足够的 `max_tokens` 防止截断；空 content 应做重试/兜底。
- 示例见 [deepseek_json.rs](../crates/airx/examples/deepseek_json.rs)。

DeepSeek 的服务端工具（`deepseek_web_search` / `deepseek_read_url`）通过 OpenAI Chat 的
`tools` 数组声明（`Tool::server(...)`）；`thinking` / `reasoning_effort` 通过 Chat 请求字段
或 `extensions.thinking` 传入。Anthropic-Compatible 部署请使用 Anthropic Native API。

## 服务端联网工具（Provider 后台执行）

这类工具的网络请求由 **Provider 后台**发起：airx 只发送工具声明、解析返回的调用记录与引用，
不代理、不抓取、不发任何本地网络请求。

| Provider | 声明方式 | 结果解析 |
| --- | --- | --- |
| OpenAI Responses | `ResponsesTool::web_search()`（`type` 可为 `web_search` / `web_search_2025_08_26`；支持 `external_web_access`、`filters.allowed_domains`、`search_context_size`） | `OutputItem::WebSearchCall`；`OutputText.annotations` 中的 `Annotation::UrlCitation` |
| Anthropic | `Tool::web_search()`（`web_search_20260318` / `20260209` / `20250305`）；其他服务端工具用 `Tool::raw(...)` 声明 | `ContentBlock::ServerToolUse`、`ContentBlock::WebSearchToolResult`；未知块保留为 `Other(Value)` |
| DeepSeek | `Tool::server("deepseek_web_search")` / `Tool::server("deepseek_read_url")` | `ResponseMessage.annotations`、`ResponseMessage.server_tool_calls`；流式在最终 chunk 汇总；`usage.server_tool_use` |

注意区分两类工具：

- **服务端工具**（上表）：模型在 Provider 后台联网，返回结果是引用/搜索记录。
- **客户端工具**（function tool）：模型只返回 `tool_calls`，真正的 HTTP 请求由你的应用执行，
  再把结果作为 `tool` 消息回传。airx 不内置通用联网/抓取实现。

网关/代理是否透传服务端工具由网关决定；例如 OpenCode Zen 接受 `deepseek_web_search` 声明，
但模型侧不保证实际执行。接入前请用目标网关做一次真实调用验证。

## 流式生命周期

- `ProtocolStream::next()` 区分正常结束（`Ok(None)`）与错误；网络断开不会伪装成成功。
- SSE 层负责 LF/CRLF/CR、UTF-8 跨 chunk、BOM、多行 data；`[DONE]` 由各 Provider 解码器处理。
- 聚合器 `finish()` 的失败语义：
  - Chat：未收到 `[DONE]` → `Incomplete`
  - Responses：未收到 `response.completed` → `Incomplete`；`response.failed` / `error` → `Failed`
  - Messages：未收到 `message_stop` → `Incomplete`；`error` 事件 → `Failed`
  - Gemini：未收到 `finishReason` / `blockReason` → `Incomplete`；SSE 内错误对象 → `Err`

## 对齐来源

| Provider | 来源 |
| --- | --- |
| Anthropic | `anthropic-sdk-python`：`Messages.create` 签名与 `message_create_params.py`、`output_config_param.py`、`thinking_config_*` |
| OpenAI | `openai-python`：Chat Completions 参数与 docstring（`max_tokens` 弃用说明、`reasoning_effort` 取值、`verbosity`） |
| Gemini | Google AI for Developers：`generateContent` / `streamGenerateContent` REST 参考、`GenerationConfig`、`FunctionDeclaration` / `FunctionCallingConfig`、`GenerationConfig.thinkingConfig` |
| DeepSeek | DeepSeek API Docs：JSON mode 指南、Chat Completions 与 Anthropic 兼容端点 |

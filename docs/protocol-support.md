# 协议支持

本文记录各协议已实现的能力与当前边界。实现位于 `airx-protocol`，全部为纯编解码，
不依赖传输层。

> **对齐基准：2026-11-23。**
> 官方文档站点在部分区域不可达，参数以官方 SDK 的生成类型 / 请求签名为准：
> Anthropic Python SDK `messages.create`、OpenAI Python SDK Chat/Responses 参数、
> DeepSeek Chat Completions 文档。协议参数会随模型变化，接入新模型前请先核对
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

## DeepSeek Chat

| 能力 | 状态 |
| --- | --- |
| 当前模型 | 文档为 `deepseek-flash` / `deepseek-v4-pro`（旧别名以官方为准） |
| 结构与 OpenAI Chat 兼容的部分 | 复用 `openai::chat` 类型（明确限定，不是别名） |
| 独立配置 / 认证 / 错误映射（ProviderId = deepseek） | 已实现 |
| `thinking`：`{"type":"enabled"｜"disabled"}` | 通过 `extensions.thinking` 传入，策略会读取 |
| `reasoning_effort`：none/low/high/max | 复用 Chat 类型字段 |
| thinking 模式下 `temperature` 无效 | Strict 报错；BestEffort 移除并返回 Warning |
| thinking 模式下 `tool_choice=required/指定工具` 被拒绝 | Strict 报错；BestEffort 降级为 `auto` |
| `response_format` 仅支持 text / json_object | Strict 报错；BestEffort 降级为 json_object |
| 流式 | 复用 Chat chunk 解析与 `ChatStreamAccumulator` |

DeepSeek 的 `thinking` 与 `reasoning_effort` 都缺省时，thinking **默认开启**；
策略按此判断，避免“看起来设置了温度、实际被忽略”。

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

## 对齐来源

| Provider | 来源 |
| --- | --- |
| Anthropic | `anthropic-sdk-python`：`Messages.create` 签名与 `message_create_params.py`、`output_config_param.py`、`thinking_config_*` |
| OpenAI | `openai-python`：Chat Completions 参数与 docstring（`max_tokens` 弃用说明、`reasoning_effort` 取值、`verbosity`） |
| DeepSeek | DeepSeek API Docs：Chat Completions 请求参数、thinking / reasoning_effort、`response_format` 限制 |

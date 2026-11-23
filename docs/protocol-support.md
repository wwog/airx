# 协议支持

本文记录各协议已实现的能力与当前边界。实现位于 `airx-protocol`，全部为纯编解码，
不依赖传输层。

## OpenAI Chat Completions

| 能力 | 状态 |
| --- | --- |
| 请求：model / messages / temperature / top_p / max_tokens / stop / user | 已实现 |
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
| 请求：model / max_tokens / messages / system / temperature / top_p / top_k / stop_sequences | 已实现 |
| 请求：tools / tool_choice / thinking | 已实现 |
| 内容块：text / image / tool_use / tool_result / thinking / redacted_thinking | 已实现 |
| 未知内容块 | `Other(Value)` 原样保留 |
| 响应：content / stop_reason / usage（含缓存 token） | 已实现 |
| 错误体：type / message → ProviderError | 已实现 |
| 流式事件：message_start / content_block_start·delta·stop / message_delta / message_stop / ping / error | 已实现 |
| 流式未知事件 | `Unknown(Value)`；`ping` 在协议层过滤 |
| 聚合：`MessagesStreamAccumulator`（tool 参数增量拼接后解析） | 已实现 |

## DeepSeek Chat

| 能力 | 状态 |
| --- | --- |
| 结构与 OpenAI Chat 兼容的部分 | 复用 `openai::chat` 类型（明确限定，不是别名） |
| 独立配置 / 认证 / 错误映射（ProviderId = deepseek） | 已实现 |
| 兼容策略：reasoner 不支持 temperature / top_p | Strict 报错；BestEffort 移除并返回 Warning |
| 流式 | 复用 Chat chunk 解析与 `ChatStreamAccumulator` |

## 流式生命周期

- `ProtocolStream::next()` 区分正常结束（`Ok(None)`）与错误；网络断开不会伪装成成功。
- SSE 层负责 LF/CRLF/CR、UTF-8 跨 chunk、BOM、多行 data；`[DONE]` 由各 Provider 解码器处理。
- 聚合器 `finish()` 的失败语义：
  - Chat：未收到 `[DONE]` → `Incomplete`
  - Responses：未收到 `response.completed` → `Incomplete`；`response.failed` / `error` → `Failed`
  - Messages：未收到 `message_stop` → `Incomplete`；`error` 事件 → `Failed`

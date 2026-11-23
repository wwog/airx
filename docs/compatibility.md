# 兼容性与行为约定

本文记录跨 Provider 的显式约定，避免“看起来统一、实际有损”的隐式行为。

## 1. Null 与字段缺省

类型化模型使用 `Option<T>`，**无法区分 `null` 与字段缺省**：

- 请求：`None` 不序列化（符合各 API 的缺省语义）。
- 响应：`None` 不重新序列化；若原始响应显式给出 `null`，重新序列化后会变成缺省。
- 需要 null / 字节级保真时，使用 `RawJson`（`Box<RawValue>`）或直接保留原始响应体。

这是当前唯一的已知保真缺口，测试中以“语义等价（忽略 null）”断言。

## 2. 重试

默认**关闭**（`RetryPolicy::disabled()`）。启用后按幂等性决策：

| 结果 | 重试条件 |
| --- | --- |
| 连接失败（Connect） | 任意方法（请求未发出） |
| 超时 | 仅幂等方法：GET / HEAD / OPTIONS / TRACE / PUT / DELETE |
| 429 | 任意方法（含 POST），优先使用 `Retry-After`（秒） |
| 5xx | 仅幂等方法 |
| 取消（Cancelled） | 不重试 |
| 流式请求 | 不重试 |

退避为指数增长（`initial × 2^n`，上限 `max_backoff`）。WASM 目标暂不支持自动重试，启用会返回配置错误。

## 3. Header 优先级

1. 协议层写入（认证、`content-type`、Provider 版本 / Beta Header）。
2. Provider 配置的 `default_headers`（覆盖同名协议 Header）。
3. 执行级 `default_headers`（**不覆盖**已存在的 Header）。

敏感 Header（`authorization`、`x-api-key` 等）不会出现在日志中；错误结构只保留状态码、错误码、request id 与原始响应体。

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

## 5. DeepSeek 兼容部署与 JSON mode

DeepSeek 没有独立协议模块，接入方式是“同一协议 + base_url”：

```rust
// OpenAI-Compatible
Client::builder()
    .openai()
    .api_key(key)
    .base_url("https://api.deepseek.com")
    .build()?;

// Anthropic-Compatible
Client::builder()
    .anthropic()
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

Anthropic-Compatible 部署请使用 Native API（`client.anthropic()`）。`thinking` /
`reasoning_effort` 通过 Chat 字段或 `extensions.thinking` 传入；DeepSeek 的
`temperature` 在 thinking 模式下无效、`tool_choice=required` 在 thinking 模式下被拒绝，
这些由调用方按提示与官方文档处理，airx 不再内置 provider 专属策略。

## 6. Gemini 行为约定

- **模型在路径中**：`client.gemini()?.generate_content(model)` 决定
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
- **未封装**：`countTokens`、模型列表、Vertex AI 专属参数（可用 `extensions` 或原生
  `Endpoint` 逃生通道）。

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

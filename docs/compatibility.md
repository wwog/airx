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

| Provider | 默认值 | 端点拼接 |
| --- | --- | --- |
| OpenAI | `https://api.openai.com/v1` | `{base}/chat/completions`、`{base}/responses` |
| Anthropic | `https://api.anthropic.com` | `{base}/v1/messages` |
| DeepSeek | `https://api.deepseek.com` | `{base}/chat/completions` |

`base_url` 必须为绝对地址；末尾 `/` 会被去掉。使用网关时按上表保留或省略 `/v1`。

## 5. DeepSeek 兼容策略

- 默认 `Strict`：`deepseek-reasoner` 携带 `temperature` / `top_p` 直接报 `Compatibility` 错误。
- `BestEffort`：移除不支持的参数，并在 `Adapted.warnings` 中记录。

策略只存在于 `deepseek::compatibility`，不进入 OpenAI 类型。

## 6. Unified 兼容模式

- `Strict`（`Client::generate`）：不可表达能力直接报 `Compatibility` 错误。
- `BestEffort`（`Client::generate_best_effort`）：降级并在 `GenerateResponse.warnings` 返回原因。

映射规则摘要：

| 统一内容 | OpenAI Chat | Anthropic Messages |
| --- | --- | --- |
| Text | 文本 / content parts | text block |
| Image | `image_url` part | image block（`source.type = url`） |
| ToolCall | `tool_calls` | `tool_use` |
| ToolResult | `tool` 消息 | `tool_result` block |
| Thinking | 不可表达（Strict 报错） | 不可回传（缺 signature，Strict 报错） |
| Custom | 不可表达（Strict 报错） | `Other(Value)` 原样保留 |

ANTHROPIC 的 `max_tokens` 必填：未设置时统一层默认使用 4096。

## 7. 能力判定

`airx_unified::capabilities(provider, model)` 基于模型名给出已知模型的能力；
未知模型一律 `Unknown`。`Unknown` 不等于 `Unsupported`：调用方不应假定能力可用，
也不应把未知当作拒绝。

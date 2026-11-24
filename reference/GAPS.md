# 跨协议差距汇总（airx vs 官方快照）

三份逐协议报告：

- [openai/GAP.md](openai/GAP.md)
- [anthropic/GAP.md](anthropic/GAP.md)
- [gemini/GAP.md](gemini/GAP.md)

基准为 2026-11-24 拉取的官方文档与 npm SDK：`openai@7.63.0`、`@anthropic-ai/sdk@1.13.0`、
`@google/genai@3.6.1`。本页只做跨协议归纳与排序，细节、字段清单、验收点以三份报告为准。

定位不变：**airx 是协议请求整合工具，不是 agent 框架**。工具调用只作为请求/响应载荷，
不内建执行循环、会话状态、MCP、记忆或编排。

> **实施状态（2026-11-24）：GAPS 中的 P0 / P1 / P2 已全部落地。**
> 三份 GAP.md §8 的条目均已实现并通过测试；标记为 Out / 观察项 / 明确不做的不在此列。
> 仅两处有意偏差：Gemini `countTokens` 顶层不强制写 `model`（官方文档只列路径参数，
> 嵌套 `generateContentRequest.model` 会填）；Gemini `inlineData` 流式合并默认关闭
> （官方未规定分片规则，提供 `with_inline_data_merge(true)` 显式开启）。
> 共享的 `final_parsed::<T>()` / 部分结果收尾 / prelude 导出已补齐。

## 已核实的 P0（已修复）

| # | 协议 | 问题 | 影响 | 证据 |
| --- | --- | --- | --- | --- |
| 1 | OpenAI | Responses 函数工具与 `tool_choice` 复用了 Chat 的嵌套结构：`{"type":"function","function":{…}}`；官方是扁平 `{type,name,parameters,strict}` / `{type,name}` | 工具声明被拒或永不触发；`input_tokens` 同样算错 | `responses/tool.rs:15-33`、`responses/request.rs:509`；官方 `responses.d.ts` `FunctionTool` / `ToolChoiceFunction` |
| 2 | OpenAI | Embeddings 允许 `encoding_format=base64`，响应却固定 `Vec<f32>` | 使用官方支持的 base64 压缩时整包反序列化失败 | `embeddings.rs:62,173-179`；官方 SDK 默认 base64 并在客户端解码 |
| 3 | Anthropic | 默认 `anthropic-version: 2023-06-01` 过时；官方当前 `2026-10-21` | 已建模的 `output_config`、`thinking.display`、`refusal` 等走旧语义，可能 400/被忽略 | `config.rs:16`；快照 `docs/api/versioning.md`；测试 `anthropic_protocol.rs:40-43` 锁死旧值 |
| 4 | Anthropic | Files 列表分页用 `after_id/before_id`，官方是 `page` + `next_page`（另有 `ids`） | 翻页静默失效，只能拿第一页 | `files.rs:47-76,124-136`；快照 `docs/api/files.md` |
| 5 | Anthropic | 删除文件响应自造 `deleted: bool`，官方 `DeletedFile` 只有 `{id,type}` | 业务判断永远 false，静默错 | `files.rs:79-92`；快照 `docs/api/files/delete.md` |
| 6 | Gemini | `thoughtSignature` 建模在 `functionCall` 内部，官方在 Part 层；解析时 Part 级签名和其它兄弟键被丢弃 | Gemini 3/4 多轮函数调用丢签名（400 / `MISSING_THOUGHT_SIGNATURE`）；`partMetadata` 等往返消失 | `generate_content/content.rs:302-307,427-432`；`dist/genai.d.ts` Part 定义 |

修复顺序建议：1 → 3 → 6 → 2 → 4 → 5。1、3 影响最大面（工具调用与所有 Anthropic 请求）；
4、5 属于「不报错但结果错」，需要同时改掉固化错误形状的测试。

## P1：按共性归并

### 1. 请求参数类型化覆盖

| 协议 | 现状 | 主要缺口 |
| --- | --- | --- |
| OpenAI | Chat 16/37、Responses 23/36 | `developer` 角色、`name`、`refusal`、`audio/file` 片段；penalties、logprobs、`n`、`seed`、`store`、`metadata`、`service_tier`、`safety_identifier`、`prompt_cache_*` |
| Anthropic | `messages.create` 10/19 | `system` 只收 String（官方还支持块数组）、顶层 `cache_control`、`metadata`、`service_tier`、`speed`、`diagnostics`、`inference_geo`；`workspace_id`/`user_profile_id` 是 Header 而非 body |
| Gemini | `GenerateContentRequest` 7/12、`GenerationConfig` 16/26 | `serviceTier`、`store`、`labels`、`continuationToken`、`logprobs`、`translationConfig` 等只能 `extensions` |

原则：先补「会被业务高频设置且影响计费/结果」的字段；长尾继续用 `extensions`，但要防
「已建模字段 + extension 同名」产生重复键。

### 2. 内容/消息模型

- OpenAI：`developer` 角色、assistant `refusal`、`input_audio` / `file` 内容片段；
  Responses 输入 item 仅 5/33 类型化，输出 8/28。
- Anthropic：`system` 支持 `string | TextBlock[]`、`Role::System`、`document` 块；
  请求侧内容块 16 只覆盖 8。
- Gemini：`FunctionResponse.parts`（多模态工具结果）、Part 级 `thoughtSignature` /
  `partMetadata` / `videoMetadata` / `mediaResolution` 等兄弟键保留。

### 3. 工具与工具选择

- OpenAI：Responses 工具形状（P0）修完后，补 `allowed_callers`、`defer_loading` 等低频字段。
- Anthropic：`ToolChoice` 缺 `none` 与 `disable_parallel_tool_use`；`Tool` 缺 `strict` /
  `cache_control` / `input_examples` / `eager_input_streaming`。
- Gemini：`FunctionCallingMode` 缺 `VALIDATED` / `MODE_UNSPECIFIED`；`Tool` / `FunctionDeclaration`
  覆盖面偏小（1/13、4/7）。

### 4. 流式保真与完成语义

- OpenAI：流式 `refusal` / `system_fingerprint` 落进 `extensions`，`final_response()` 与
  非流式读取结果不一致；Responses 事件仍有 21 个落入 `Unknown`（多为新服务端工具）。
- Anthropic：`message_delta` 丢 `stop_details` / `container`；`citations_delta` 不累积；
  块 `extensions` 重建为空；`Usage.input_tokens` 遇 `null` 会使整条流失败；
  流内 `error` 事件导致拿不到已累积的部分结果。
- Gemini：任何非 `FINISH_REASON_UNSPECIFIED` 都算完成 → `CONTINUATION` 静默截断，
  `SAFETY` / `NO_IMAGE` / `MALFORMED_*` 以 `Ok` + 空文本返回。

共性建议：三协议统一提供「为什么停了」的判定助手（`is_truncated()` / `is_blocked()` /
`is_refusal()` / `is_tool_use()` / `requires_continuation()`），只报告状态，不自动续跑。

### 5. 易用性与代码清晰度（不引入 agent 层）

- **输出回传**：OpenAI Responses 缺 `OutputItem → InputItem` 转换与构造器，`store:false`
  多轮接入成本高；建议 `into_input()` / `From` 实现。
- **结构化输出**：三协议 `parse_as` 都只做 serde、不注入/校验 schema。建议统一命名与文档，
  并提供流式收尾后的 `final_parsed::<T>()`（仅 serde），避免业务自己拼
  `final_text()` + `from_str`。
- **Header 参数**：Anthropic `workspace_id` / `user_profile_id` 应通过具名
  `RequestOptions` 助手发送，不应写进 `extensions`。
- **枚举稳定性**：`ContentBlock` / `Delta` / `StreamEvent` / `ToolChoice` 等加
  `#[non_exhaustive]`，官方明确会加新变体。
- **计数一致性**：Anthropic `count_tokens` 缺 `tool_choice` / `output_config` / `cache_control`；
  OpenAI `InputTokensCountRequest::from_request` 静默丢 4 个字段 → 预算偏小。
- **分页与查询编码**：Gemini `pageToken` 白名单拒绝 `+ / =`（官方 token 是不透明 base64）；
  OpenAI Files 缺 `order`；Anthropic Files 分页（P0）。

### 6. 文档与代码不一致

三家都有，共性两类：

1. **文档把「扩展透传」说成「已支持」**：例如 Anthropic `workspace_id` 被写成走
   `extensions`（实际是 Header）；Gemini `thoughtSignature` 被文档描述成
   `FunctionCallPart` 字段（掩盖 P0）。
2. **文档自相矛盾/过期**：OpenAI `docs/openai.md` 一处说输入 item 未类型化、另一处说相反；
   Anthropic `protocol-support.md` 标注对齐 Python SDK，实际基准是 TS SDK；
   Gemini 文档仍把 `responseSchema` 称作最佳路径，SDK 已标 deprecated。

建议把「官方归属/版本/来源」作为文档生成或 CI 检查项，而不是手写记忆。

## 观察项（不是本轮缺口）

- **Gemini `generateContent` 被官方称为 legacy，新能力优先 Interactions API**：Interactions
  是状态化/会话型接口，当前不在 airx 范围；记录触发条件（新模型只在 Interactions 可用时再评估）。
- **Gemini `responseMimeType` / `responseSchema` → `responseFormat`**：SDK 已标 deprecated，
  REST 稳定后再类型化，当前保留 `json_schema` 并文档预警。
- **OpenAI `user` 参数级联弃用**：转向 `safety_identifier` / `prompt_cache_key`。
- **Anthropic 版本头**：修复默认值时保留 `with_api_version`，兼容网关未必接受新版本。

## 明确不做（agent 层）

三份报告一致排除：工具执行循环、`pause_turn` 自动续跑、托管 Agent/Sessions/Environments、
Skills/Memory/Vaults/MCP 托管、组织与账号管理、Live/Realtime 双向会话、schema 编译期代码生成、
本地 tokenizer。需要这些能力时走官方 SDK 或 airx 的 `execute(HttpRequest)` 逃生通道。

## 建议的实施批次

| 批次 | 内容 | 验收 |
| --- | --- | --- |
| B1 正确性 | OpenAI Responses 工具形状、Embeddings base64；Anthropic 默认版本、Files 分页/删除；Gemini Part 级签名 | 每个 P0 一个序列化/反序列化单测 + 修正固化错误形状的旧测试 |
| B2 高频缺口 | OpenAI `developer`/参数组/输出回传；Anthropic system 块、tools 字段、count_tokens；Gemini `VALIDATED`、完成语义、pageToken、FunctionResponse.parts | 请求快照测试 + 流式聚合测试 + 文档同步 |
| B3 收尾 | 统一 stop-reason 助手、`#[non_exhaustive]`、剩余 P2 端点、文档校正 | clippy/fmt/test/doc 全绿；三份 GAP 报告勾掉条目 |

B1 全部是协议正确性问题，建议单独一个 PR，避免和参数扩展混在一起。

**完成情况（2026-11-24）**：B1 / B2 / B3 均已落地；未提交的改动包含代码、测试与文档。
全量验证：`cargo fmt --check`、`cargo clippy --workspace --all-targets --all-features -D warnings`、
`cargo test --workspace --all-features`（24 个 test binary 全绿）、`cargo doc -D warnings`、
依赖方向检查、最小 feature / 单 Provider / wasm32 检查全部通过。

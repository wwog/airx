# airx 架构设计

> One Rust SDK. Multiple AI protocols. No feature loss.

本文是 airx 的**唯一架构基准文档**，作为后续实现、评审与扩展的默认依据；与代码冲突时以本文为准，修改架构需先修改本文。

文档组织顺序即设计推演顺序：**功能需求 → 设计原则 → 分层与包职责 → 接口契约 → 关键横切设计 → 交付与质量**。

---

## 目录

1. [产品定位与设计目标](#1-产品定位与设计目标)
2. [功能需求](#2-功能需求)
3. [核心架构决策](#3-核心架构决策)
4. [Workspace 与依赖关系](#4-workspace-与依赖关系)
5. [分层与包职责](#5-分层与包职责)
6. [关键接口契约](#6-关键接口契约)
7. [关键横切设计](#7-关键横切设计)
8. [对外 API 与 Feature](#8-对外-api-与-feature)
9. [质量保障](#9-质量保障)
10. [实施路线](#10-实施路线)
11. [当前状态](#11-当前状态)
12. [附录：决策速查与被否决方案](#12-附录决策速查与被否决方案)

---

## 1. 产品定位与设计目标

### 1.1 定位

airx 不是一个“统一的 LLM 请求库”，而是**多协议 AI API 基础设施库**。

它同时服务两类调用者：

- 需要**跨 Provider 快速开发**的应用 → 使用 Unified API。
- 需要**协议完整能力**的应用 → 使用 Native API，并在需要时下探到 Raw JSON 与自定义 Transport。

### 1.2 设计目标

| 编号 | 目标 | 说明 |
| --- | --- | --- |
| G1 | 协议完整性 | 完整覆盖 OpenAI Responses、Chat Completions、Anthropic Messages，不丢失任何原生能力 |
| G2 | 协议保真 | 统一抽象不得成为使用原生能力的阻碍；无法映射时必须保留或明确报错 |
| G3 | 高性能 | 减少不必要分配与 JSON 往返；支持流式解析与背压 |
| G4 | 函数式核心 | 转换、编解码、聚合均为纯函数；副作用集中在边界层 |
| G5 | 可扩展 | 新增 Provider 不改核心领域模型；第三方可扩展协议、Transport、JSON 策略 |
| G6 | 跨平台 | Native 与 `wasm32-unknown-unknown` 均可编译、可测试 |
| G7 | 长期可维护 | crate 边界清晰、单向依赖、公共 API 稳定、模块可独立测试 |

### 1.3 三条不可妥协的原则

1. **Native First**：原生协议永远拥有最高完整性，Unified 只是便利层。Provider 不依赖 Unified，Unified 也不作为 Native 的上游。
2. **No Silent Loss**：任何无法映射的参数、事件、内容块都必须能保留（Raw JSON / Unknown 变体 / 扩展字段）或明确报错（Strict 模式）。禁止静默丢弃 Thinking、Tool Call、多模态内容。
3. **Effects at the Edge**：协议处理、JSON 转换、事件聚合是纯函数；网络、时钟、重试等待、日志才是副作用。

### 1.4 非目标

- 不做运行时 Provider 插件市场（只保留编译期扩展点与动态注册的轻量接口）。
- 不内建 Prompt 编排、Agent 框架、RAG、向量库。
- 不承诺“一个请求结构体打穿所有 Provider”。
- 不为追求零拷贝而牺牲公共 API 的可读性（见 §7.1）。

---

## 2. 功能需求

需求编号在测试与 PR 中可直接引用，作为验收锚点。优先级：P0 = 首个可用版本必须完成，P1 = 稳定版，P2 = 后续演进。

### 2.1 协议完整性

| 编号 | 需求 | 验收要点 | 优先级 |
| --- | --- | --- | --- |
| FR-P1 | OpenAI Responses | Request / Response / Input·Output **Item 生命周期** / 语义事件（`response.output_item.added`、`response.output_text.delta`、`response.function_call_arguments.delta` 等）/ 工具调用 / Structured Outputs / 多模态 / 状态 | P0 |
| FR-P2 | OpenAI Chat Completions | Request / Response / `chunk`+`delta` 流模型 / `tool_calls` / usage / 多模态 | P0 |
| FR-P3 | Anthropic Messages | Request / Response / Content Block 全类型 / Thinking 与 Redacted Thinking / Tool Use 与 Tool Result / Prompt Caching / Structured Outputs / `message_start`…`message_stop` 事件 | P1 |
| FR-P4 | DeepSeek 兼容部署 | 通过 OpenAI / Anthropic 兼容端点接入，不新增协议模块；JSON mode 差异由示例与文档说明 | P1 |
| FR-P5 | 未知内容保留 | 未知字段、未知事件、未知 Content Block 均可原样保留并可重新序列化 | P0 |
| FR-P6 | 协议可扩展 | 第三方能新增协议模块，无需修改 `airx-core` | P2 |

Responses 与 Chat Completions 必须是两个独立协议模块。二者事件模型不同（语义事件 vs chunk/delta），不能共用 `StreamChunk`。

### 2.2 流式

| 编号 | 需求 | 验收要点 | 优先级 |
| --- | --- | --- | --- |
| FR-S1 | SSE 语法完整 | `data:` / `event:` / `id:` / `retry:`、多行 data、CRLF 与 LF、注释行、UTF-8 跨 buffer 边界、EOF、网络中断 | P0 |
| FR-S2 | 原生事件强类型 | 每个协议拥有独立 `Event` 枚举，不做 `String` 直通、不压成通用 chunk | P0 |
| FR-S3 | 事件聚合 | 独立 `StreamAccumulator`；`finish()` 必须区分 Completed / Failed / Incomplete / Cancelled | P0 |
| FR-S4 | 增量 JSON | Tool 参数 delta 可累积，支持“是否已完整”判断，不完整不提前报错 | P0 |
| FR-S5 | 取消与背压 | 用户提前 drop 流即停止读取，不在后台跑完；错误携带流状态 | P0 |
| FR-S6 | `[DONE]` 处理 | 属于协议约定，由 Provider Decoder 处理，不写入 SSE 语法层 | P0 |

### 2.3 JSON 保真

| 编号 | 需求 | 验收要点 | 优先级 |
| --- | --- | --- | --- |
| FR-J1 | 四种表示并存 | 强类型 struct / `serde_json::Value` / `RawValue` / `Bytes`，按场景选型 | P0 |
| FR-J2 | 未知字段双向保留 | 反序列化保留 + 重新序列化可写回，顺序不假设为协议语义 | P0 |
| FR-J3 | 字节级保真 | 签名、代理转发、特殊 Provider 场景可保留原始字节 | P1 |
| FR-J4 | 确定性序列化 | Canonical 模式，供测试、缓存 Key、签名使用（预留 RFC 8785 JCS） | P1 |
| FR-J5 | 格式化模式 | `Compact` / `Pretty` / `Preserve` / `Canonical` 四态 | P1 |
| FR-J6 | Provider quirks 隔离 | `max_tokens` / `max_completion_tokens` / `max_output_tokens`、`response_format` vs `text.format`、`tool_choice` 差异等，只存在于对应协议模块 | P0 |
| FR-J7 | Raw 逃生通道 | 用户可在请求与响应两个方向注入/取回原始 JSON，不必等 SDK 发版 | P1 |

必须区分的三个概念（不可混为一谈）：**JSON 语义保真**、**未知字段保留**、**JSON 字节级保真**。

### 2.4 Unified 层

| 编号 | 需求 | 验收要点 | 优先级 |
| --- | --- | --- | --- |
| FR-U1 | 统一模型 | 统一 Message / Content / Tool / GenerateRequest / Response / Stream | P1 |
| FR-U2 | 纯转换 Adapter | Native ⇄ Unified 双向转换无 IO，可单独测试 | P1 |
| FR-U3 | 能力模型 | `CapabilitySupport` 四态（Supported / Unsupported / ModelDependent / Unknown），不用 `bool` | P1 |
| FR-U4 | 兼容模式 | `Strict` 报错 / `BestEffort` 降级并返回 Warning | P1 |
| FR-U5 | Provider 扩展命名空间 | `ProviderExtensions` 按 Provider 分区，后续可收紧为强类型 | P2 |

### 2.5 传输与平台

| 编号 | 需求 | 验收要点 | 优先级 |
| --- | --- | --- | --- |
| FR-T1 | 可替换 Transport | 协议实现不出现 `reqwest::Client`；替换 Transport 不改协议代码 | P0 |
| FR-T2 | Native HTTP | Reqwest；TLS 后端可选（native-tls / rustls）；连接池、代理 | P0 |
| FR-T3 | WASM Fetch | `wasm32-unknown-unknown` 可用，不强制 Future 为 `Send` | P1 |
| FR-T4 | Mock Transport | 单元测试与 golden 测试可用 | P0 |
| FR-T5 | 执行策略 | 超时、取消、Middleware、幂等感知的重试 | P1 |
| FR-T6 | 流式响应体 | 字节流读取 + 背压，不整包缓冲 | P0 |
| FR-T7 | 配置可覆盖 | `base_url`、Header 可覆盖（兼容代理/网关/兼容 API）；敏感 Header 不落日志 | P0 |
| FR-T8 | HTTP 元数据保留 | status / headers / request_id 可被用户读取 | P0 |

### 2.6 错误与可观测

| 编号 | 需求 | 验收要点 | 优先级 |
| --- | --- | --- | --- |
| FR-O1 | 统一错误分类 + 原始信息 | 保留 provider、status、code、message、request_id、raw body；400/429/500 不被折叠成同一个错误 | P0 |
| FR-O2 | 重试判定信息 | Retryable、Retry-After、限流信息可读 | P1 |
| FR-O3 | 可观测 | `tracing` 结构化诊断；可选 metrics 钩子；可脱敏 | P2 |

### 2.7 工程质量

| 编号 | 需求 | 验收要点 | 优先级 |
| --- | --- | --- | --- |
| FR-Q1 | 单向依赖 | 无循环依赖，禁止反向依赖（§4.3） | P0 |
| FR-Q2 | 跨平台可验证 | Linux / macOS / Windows / wasm32 编译矩阵 | P1 |
| FR-Q3 | Golden fixtures | 真实协议样例驱动序列化、反序列化、事件、转换测试 | P0 |
| FR-Q4 | Feature 组合可控 | 默认 feature 少；不默认引入全部 Provider 与大型 HTTP 依赖 | P1 |
| FR-Q5 | API 稳定性 | 公共 API 有版本策略；MSRV 明确（1.85+，edition 2024） | P2 |

### 2.8 需求 → 分层归属（追溯矩阵）

| 需求域 | 主责 crate | 主要模块 | 协作方 |
| --- | --- | --- | --- |
| FR-P1 / P2 / P3 / P4 | `airx-protocol` | `openai::responses`、`openai::chat`、`anthropic::messages`；DeepSeek 复用兼容端点 | core（类型/JSON/SSE 语法）、airx（编排） |
| FR-P5 / FR-J2 / FR-J3 / FR-J7 | `airx-core` | `json::*`、`extension` | protocol（字段声明） |
| FR-P6 | `airx-protocol` | `extension`、`internal::codec` | core（数据契约） |
| FR-S1 / FR-S6 | `airx-core` | `stream::sse`、`stream::decoder` | — |
| FR-S2 | `airx-protocol` | `<provider>/<api>/event` | core（SSE 输出） |
| FR-S3 | `airx-protocol`（状态机） + `airx-core`（`StreamStatus`） | `accumulator` | airx（驱动） |
| FR-S4 | `airx-core` | `stream::json_accumulator` | protocol（tool 参数） |
| FR-S5 | `airx-transport`（读取与取消） + `airx`（生命周期） | `stream`、`retry` | core（状态） |
| FR-J1 / J4 / J5 | `airx-core` | `json::raw`、`json::canonical` | — |
| FR-J6 | `airx-protocol` | `internal::json_policy`、`<provider>::compatibility` | core（工具函数） |
| FR-U1 ~ U5 | `airx-unified` | `message`、`content`、`tool`、`capability`、`compatibility`、`adapter::*` | protocol（被转换方） |
| FR-T1 / T2 / T3 / T4 / T5 / T6 | `airx-transport` | `transport`、`native`、`wasm`、`retry`、`middleware`、`stream` | core（HTTP 数据结构） |
| FR-T7 / T8 | `airx-protocol`（Provider 配置与 Header） + `airx-transport`（响应元数据） | `config`、`response` | airx（Builder） |
| FR-O1 / O2 | `airx-core`（错误形状） + `airx-protocol`（错误体解析） + `airx-transport`（后端错误映射） | `error` | airx（re-export） |
| FR-O3 | `airx-transport` | `middleware`、`tracing` span | airx（钩子） |
| FR-Q1 ~ Q5 | 全部 | Workspace 根配置、`tests/`、`examples/` | CI |

---

## 3. 核心架构决策

### 3.1 决策表

| 设计问题 | 决策 | 理由 |
| --- | --- | --- |
| 整体架构 | **Protocol-First** | 协议差异是本质差异，抽象必须后置 |
| 核心设计 | **Functional Core / Imperative Shell** | 纯函数可测、可组合；副作用集中在传输层 |
| Provider 扩展 | **Adapter Architecture** | Unified ⇄ Native 单向转换，不让 Protocol 感知 Unified |
| crate 数量 | **5 个** | crate 边界对应稳定架构职责，模块边界对应功能 |
| Native API | 独立强类型模型，按 Provider × API 分模块 | 保证完整性 |
| Unified API | 可选、下游、纯转换 | 便利层不得成为能力上限 |
| JSON | Serde + `RawValue` + `Value` + `Bytes` | 四种保真需求不同 |
| Stream | SSE 解析一次 + 各协议独立事件 | 事件语义不可通用化 |
| 聚合 | 独立 Accumulator | 用户可选择增量/聚合/自管状态 |
| IO | 可替换 Transport，协议层零 HTTP 依赖 | 可测试、可 WASM、可代理 |
| WASM | 同一 crate，target-specific 实现 + feature | 不新增 crate，不污染抽象 |
| Async | 原生 `async fn` + 静态泛型分发 | 避免全量 `Box<dyn Future>` |
| 动态扩展 | 扩展字段命名空间 + Raw JSON | 新功能不必等发版 |
| 错误 | 统一分类 + 原始信息保留 | 可分类处理，也可排查 |
| 测试 | Golden Fixtures + Protocol/流式矩阵 | 主要风险是协议演进，不是业务逻辑 |

### 3.2 为什么是 5 个 crate

crate 边界 = **稳定的架构职责 + 独立版本/发布需要**；模块边界 = **功能划分**。

把 JSON、SSE、每个 Provider、Adapter、Retry 都拆成独立 crate，在职责划分上并没有错，但它们之间**没有独立的演进节奏**，硬拆会带来：

- 版本号与 feature 组合的矩阵爆炸（Cargo features 是 additive 的，跨 crate 合并极易产生意外依赖）。
- 公共 API 被迫暴露在细粒度 crate 上，后期合并是 breaking change。
- 纯内部模块被固化成一等公民，反而限制重构。

因此采取**默认不拆**的策略：JSON、Stream、Retry、各 Provider 协议、Adapter 全部保留为模块边界，只有在出现明确的独立发布或独立版本管理需求时才拆出 crate。

以此得到 5 个 crate，每个对应一个不会随功能增删而改变的架构职责：`airx-core`（基础）、`airx-protocol`（协议）、`airx-transport`（IO）、`airx-unified`（便利层）、`airx`（组合入口）。

### 3.3 分层模型

```text
┌──────────────────────────────────────────────────────────────┐
│ L4  airx            Facade / Client / Builder / Prelude      │  组合根
├──────────────────────────────────────────────────────────────┤
│ L3  airx-unified    统一模型 / Adapter / Capability（可选）   │  便利层
├───────────────────────────────┬──────────────────────────────┤
│ L2  airx-protocol             │  airx-transport              │  两个平行层
│     原生协议模型 + 纯编解码     │  HTTP / SSE IO / Retry       │
├───────────────────────────────┴──────────────────────────────┤
│ L1  airx-core       Error / Model / JSON / SSE / HTTP 数据结构 │  基础层
└──────────────────────────────────────────────────────────────┘
```

关键点：**Protocol 与 Transport 是兄弟层，互不依赖**。

一种直觉做法是让协议层直接依赖传输实现（`airx-protocol → airx-transport`）。airx 不采用，而让二者只共享 `airx-core` 中的**数据契约**（`HttpRequest` / `HttpResponse` / `RawEvent`）。原因：

- 编码是纯函数，可以被单独测试，也可以在无网络环境做 golden 校验。
- Transport 可整体替换（Reqwest / WASM Fetch / Mock / 自建代理），协议代码零改动。
- 消除“协议模块为了拿 HTTP 类型而拉入 reqwest”的隐式耦合。

### 3.4 明确不做的事

| 被否决做法 | 原因 |
| --- | --- |
| 单一 `Provider` trait + 统一 `CompletionRequest` / `CompletionResponse` | 会系统性丢失事件模型、工具语义、内容结构与状态管理 |
| 一个 `OpenAIRequest` 同时覆盖 Responses 与 Chat | 二者 item 生命周期与事件语义不同 |
| 通用 `StreamChunk` 承载所有流 | 事件信息被压扁，无法还原原生能力 |
| 把所有 payload 都转成 `serde_json::Value` | 丢失数字/字符串原始表示、字段顺序与字节保真 |
| 在 core 或统一模型里写 `if provider == DeepSeek` | Provider 差异必须收敛在各自协议模块 |
| 全量 `Box<dyn Future>`、自建 Monad / Effect System | Rust 的 `Result`/`Option`/Iterator/enum/所有权已足够 |
| 强制所有 Future `Send + Sync` | WASM 上不可行，会迫使跨平台设计妥协 |
| 发送前先转统一模型再转回原生模型 | 无谓往返且必然丢字段；原生请求直发 |
| 把 `[DONE]` 当作 SSE 通用语法 | 它是协议约定，属于 Provider Decoder |
| 默认对所有失败自动重试 | 非幂等请求会重复产生副作用 |

---

## 4. Workspace 与依赖关系

### 4.1 目录结构

当前仓库已按此结构建立骨架；标 `*` 的为规划中、尚未实现的模块。

```text
airx/
├── Cargo.toml                 # workspace 根：members / workspace.package / workspace.dependencies
├── Cargo.lock
├── rust-toolchain.toml        # 固定工具链通道（target 显式安装）
├── rustfmt.toml
├── clippy.toml*
├── deny.toml*                 # 依赖许可与安全审计（M9 接入）
├── README.md
│
├── crates/
│   ├── airx/                  # L4 门面
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── client.rs      # 组合 Protocol + Transport + Unified
│   │       ├── builder.rs     # 配置聚合
│   │       ├── config.rs      # 执行配置（timeout / retry / headers）
│   │       ├── provider/      # 各 Provider 的 typed endpoint 入口
│   │       ├── prelude.rs
│   │       └── error.rs       # 对外错误 re-export
│   │
│   ├── airx-core/             # L1 基础层
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs
│   │       ├── model.rs       # ProviderId / ModelId / Capability
│   │       ├── extension.rs   # RawExtension / Extensions
│   │       ├── json/
│   │       │   ├── mod.rs
│   │       │   ├── raw.rs
│   │       │   ├── value.rs
│   │       │   ├── canonical.rs*   # M9：RFC 8785 JCS
│   │       │   └── format.rs
│   │       ├── stream/
│   │       │   ├── mod.rs
│   │       │   ├── sse.rs
│   │       │   ├── decoder.rs
│   │       │   ├── buffer.rs
│   │       │   ├── status.rs
│   │       │   └── json_accumulator.rs
│   │       └── http.rs        # 仅数据结构，不含客户端
│   │
│   ├── airx-protocol/         # L2 原生协议层（纯函数）
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── extension.rs
│   │       ├── internal/
│   │       │   ├── codec.rs       # 共享编解码辅助
│   │       │   ├── json_policy.rs # Provider JSON 策略
│   │       │   └── serde_helpers.rs
│   │       ├── openai/
│   │       │   ├── mod.rs
│   │       │   ├── config.rs
│   │       │   ├── common/        # model / usage / error / tool
│   │       │   ├── responses/     # request / response / input / output / item / event / stream / endpoint
│   │       │   └── chat/          # request / response / message / chunk / tool / event / stream
│   │       ├── anthropic/
│   │       │   ├── mod.rs
│   │       │   ├── config.rs
│   │       │   ├── common/
│   │       │   └── messages/      # request / response / content / tool / thinking / event / stream / accumulator
│   │       └── google/            # 预留，首版不实现
│   │
│   ├── airx-transport/        # L2 副作用层
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── transport.rs   # Transport trait / BoxFuture（对象安全）
│   │       ├── native.rs      # Reqwest 实现 + 错误映射
│   │       ├── mock.rs        # 可脚本化 Mock（响应队列 + 请求记录）
│   │       ├── wasm.rs        # Fetch 实现（含 ReadableStream）
│   │       ├── response.rs    # HttpStreamResponse / ResponseMetadata
│   │       ├── stream.rs      # ByteStream
│   │       ├── retry.rs       # RetryPolicy + RetryTransport
│   │       └── middleware.rs  # 同步中间件链
│   │
│   └── airx-unified/          # L3 统一层（可选）
│       └── src/
│           ├── lib.rs
│           ├── client.rs      # 统一请求描述（不含 IO）
│           ├── request.rs
│           ├── response.rs
│           ├── message.rs
│           ├── content.rs
│           ├── tool.rs
│           ├── capability.rs
│           ├── compatibility.rs
│           ├── stream.rs
│           └── adapter/
│               ├── mod.rs
│               ├── openai.rs
│               └── anthropic.rs
│
├── tests/
│   ├── fixtures/
│   │   ├── openai/{responses,chat}/
│   │   └── anthropic/messages/
│   ├── protocol.rs
│   ├── serialization.rs
│   └── streaming.rs
│
├── examples/{native,unified,streaming}/
├── scripts/
│   └── check-deps.sh          # 依赖方向校验（本地与 CI 共用）
├── .github/workflows/ci.yml   # fmt / clippy / test / 依赖方向 / wasm
└── docs/
    ├── architecture.md        # 本文
    ├── protocol-support.md
    └── compatibility.md
```

### 4.2 依赖 DAG

```text
airx                     (组合根：唯一推荐给用户的入口)
 ├── airx-core
 ├── airx-protocol
 ├── airx-transport
 └── airx-unified
       ├── airx-protocol
       └── airx-core

airx-protocol
 └── airx-core

airx-transport
 └── airx-core
```

`airx-unified` **不依赖** `airx-transport`：它只做纯转换，真正发送由 `airx` 组合完成。这样 Native 与 Unified 两条调用路径共享同一套传输设施，且 Unified 可被完全移除而不影响 Native 路径。

### 4.3 依赖规则

必须成立：

- `airx-core` 不依赖任何其他 workspace crate，也不知道 OpenAI / Anthropic 的存在。
- `airx-protocol` 只依赖 `airx-core`；不依赖传输实现的 crate、不依赖 Unified。
- `airx-transport` 只依赖 `airx-core`；不知道什么是 Responses / Messages。
- `airx-unified` 依赖 `airx-core` + `airx-protocol`。
- `airx` 依赖以上全部，负责组装。

禁止出现：

```text
airx-core      -> airx-protocol        # 基础层反向依赖
airx-protocol  -> airx-transport       # 协议层绑定传输实现
airx-protocol  -> airx-unified         # 原生被统一层污染
airx-openai    -> airx-anthropic       # Provider 之间互依赖（同 crate 内也不允许跨协议引用）
```

CI 通过 `scripts/check-deps.sh`（基于 `cargo metadata`）校验依赖方向；许可与安全审计由 `deny.toml` 在 M9 接入。

---

## 5. 分层与包职责

统一约定每个 crate 必须写明**负责**、**不负责**、**模块归属**。判断标准：如果一段代码**只依赖 std / 通用库**且**没有 IO**，且**与某 Provider 语义无关**，它属于 core；一旦涉及某个协议字段语义，属于 protocol；一旦触发时间、网络、日志，属于 transport。

### 5.1 `airx-core`（L1，最稳定）

**负责**

- 统一错误类型与错误形状（`AirxError`、`ProviderError`、`TransportErrorKind`）。
- `ProviderId`、`ModelId`、`ApiVersion`、`ModelCapabilities`、`CapabilitySupport`。
- JSON 工具：`RawJson`、`RawExtension`、扩展字段容器、格式化模式、Canonical 序列化。
- SSE 语法解析与增量 JSON 累积。
- 通用 HTTP **数据结构**（Method / Url / HeaderMap / Body / Status），不含客户端。
- 协议无关的流状态（`StreamStatus`）与缓冲。
- 与协议无关的纯函数工具（字节/UTF-8 边界处理等）。

**不负责**

- 发送 HTTP、读取网络流、重试等待、访问时钟。
- 任何 Provider 的请求/响应类型。
- 统一对话语义。
- API Key 管理与认证。
- 记录日志（仅接受可选 `tracing` 事件，不做输出策略）。

**稳定度要求：最高。** 这里每一次破坏性变更都会传导到全部上层。

### 5.2 `airx-protocol`（L2，演进最快、最重要）

**负责**

- 每个 Provider × API 的完整模型：Request、Response、Input/Output Item、Content Block、Tool、Event、Usage、Error。
- 纯编码：`request -> HttpRequest`（method/path/headers/body + Provider 认证与版本 Header）。
- 纯解码：`Bytes/RawEvent -> Response/Event`。
- Provider 配置：`base_url`、认证方式、API Version、Beta Header、默认 Header。
- Provider 兼容策略：JSON quirks、字段替换、模型差异、参数裁剪。
- 各协议的 Stream 事件模型与 Accumulator 状态机。

**不负责**

- 任何网络 IO、重试、超时。
- Unified 类型与转换。
- 跨 Provider 的统一语义。
- 记录或决定日志内容。

**组织原则**

- 按 `protocol/api` 二级模块划分，`openai::responses` / `openai::chat` / `anthropic::messages` 互不共享类型。
- 共享仅限 `common/`（model、usage、error、tool 等**语义确实相同**的部分）与 `internal/`（纯辅助函数，不对外）。
- Provider 是「协议 × 部署配置」：DeepSeek 这类兼容服务通过 OpenAI 或 Anthropic 端点接入，
  只改 `base_url` / 认证 Header，不新增协议模块，也不在协议层写 provider 分支。

**关于 `ApiProtocol` trait**

不在 crate 层强制所有 API 实现统一 trait。Responses、Chat、Messages 的类型与流生命周期不同，强行抽象会牺牲类型表达。做法：

- 各 API 暴露具体类型与具体 `encode` / `decode` / `decode_event` 函数。
- 只有在确实具有共同语义的**编排**处（`airx` 中）使用泛型约束，见 §6.4。

### 5.3 `airx-transport`（L2，唯一 IO 层）

**负责**

- `Transport` trait 与请求执行。
- Native 实现（Reqwest）：连接池、TLS 选择、代理、超时。
- WASM 实现（Fetch / ReadableStream）。
- 流式响应体读取与背压。
- 重试策略执行（含幂等判定、`Retry-After` 解析、退避）。
- Middleware 链、取消传播、`tracing` span。
- `ResponseMetadata`（status / headers / request_id）。
- 把后端错误映射为 core 的错误形状。

**不负责**

- 任何协议知识（不知道 Responses / Messages 是什么）。
- 协议事件解析（只产出字节或 SSE 原始事件）。
- Provider 认证 Header 的构造（由 protocol 在编码阶段写入）。
- 决定重试是否“业务安全”——只依据方法与错误类别的中立规则，协议层可通过标记覆盖。

**平台约束**

- Trait 不默认要求 `Send + Sync`；约束施加在 native 具体实现上。
- Native/WASM 通过 `cfg(target_arch)` + feature 分离实现，不新增 crate。

### 5.4 `airx-unified`（L3，可选）

**负责**

- 统一 Message / Content / Tool / GenerateRequest / Response / Stream。
- Provider namespace 扩展（`extensions.openai` / `extensions.anthropic`）。
- Capability 判定与降级决策。
- `adapter/*`：Native ⇄ Unified 的**纯函数**转换。
- 兼容模式：`Strict` 报错 / `BestEffort` 降级 + Warning 列表。

**不负责**

- 网络、重试、超时（不依赖 `airx-transport`）。
- 协议细节实现（复用 protocol 的类型与字段，不重新定义协议）。
- 成为 Native 能力的子集或必经路径。

### 5.5 `airx`（L4，组合根）

**负责**

- `Client` / `Builder` / 配置合并 / `prelude`。
- 把 protocol 的编码结果交给 transport 执行，再把响应交回 protocol 解码。
- typed endpoint 入口：`client.openai().responses()`、`client.anthropic().messages()`；
  兼容部署（如 DeepSeek）通过对应协议的 `base_url` 接入。
- Unified 路径编排：Unified 请求 → adapter → native 请求 → transport → adapter → Unified 响应。
- Feature 汇总与平台选择（native / wasm）。
- 对外错误 re-export 与统一文档。

**不负责**

- 重新实现协议、JSON、SSE 逻辑。
- 存放业务语义（Builder 只做配置，见 §8.4）。

### 5.6 横切职责归属

| 关注点 | 归属 | 说明 |
| --- | --- | --- |
| 错误分类与形状 | `airx-core::error` | 上层只做映射，不新增并行错误体系 |
| 失败重试 | `airx-transport::retry` | 策略 + 执行；协议层只提供幂等提示 |
| JSON quirk | `airx-protocol::internal::json_policy` + 各 Provider `compatibility` | 绝不进 core |
| SSE 语法 | `airx-core::stream::sse` | 只认语法，不认 `[DONE]` |
| 事件语义 | `airx-protocol::<provider>::<api>::event` | — |
| 流状态 | `airx-core::stream::status` | 各协议复用 |
| 能力判定 | 类型在 `airx-core::model`，判定在 `airx-unified::capability` | protocol 只声明自身约束 |
| API Version / Beta Header | `airx-protocol::<provider>::config` | 不写死在通用代码 |
| base_url / 认证 | `airx-protocol::<provider>::config` | 必须可覆盖（代理、网关、兼容 API） |
| 超时 / 中间件 / TLS | `airx-transport` | 执行配置，与 Provider 配置分离 |
| 日志与脱敏 | `airx-transport` + `airx` | 敏感 Header 默认不输出 |
| Raw 透传 | `airx-core::json::raw` + protocol 扩展字段 | 双向可用 |

---

## 6. 关键接口契约

### 6.1 职责三分

```text
Protocol  描述“如何编码 / 如何解码”       纯函数，无 IO
Transport 执行“发送 / 读取”              唯一副作用点
Client    组合二者，管理生命周期与策略    位于 airx
```

不要让 Provider 实现一个同时包含 HTTP、Stream、JSON、Retry 的巨型 trait。

### 6.2 Protocol 契约

```rust
pub trait Endpoint {
    type Request;
    type Response;

    fn encode(&self, request: &Self::Request) -> Result<HttpRequest, AirxError>;
    fn decode(&self, response: HttpResponse) -> Result<Self::Response, AirxError>;
}
```

`encode` 产出的 `HttpRequest` 定义在 `airx-core::http`，因此 protocol 无需依赖 transport。
流式事件解码在 M4 以独立的 `StreamingEndpoint` trait 补充，而不是现在增加未使用的关联类型。

### 6.3 Transport 契约

```rust
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait Transport: Send + Sync {
    fn execute(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>>;
    fn execute_stream(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpStreamResponse, TransportError>>;
}
```

- **对象安全**：`Client` 需要在运行时持有可替换的传输实现，因此方法返回 `BoxFuture`。
  只有传输边界付出一次装箱，协议与解析仍是纯函数。
- **不强制 `Send`**：Native 下要求 `Send + Sync`；WASM 下按目标平台去掉该约束
  （`BoxFuture` 与 trait 均有 `cfg` 版本）。
- **固定错误形状**：实现只把后端错误映射为 `TransportError`，不引入第二套错误体系。
- 实现 `&T` 与 `Arc<T>` 的转发，Mock 与自定义 Transport 都可直接交给 Builder。
- 必须提供 Mock 实现，供单元测试与 golden 测试使用。

### 6.4 组合

```rust
pub struct ApiClient<P> {
    endpoint: P,
    executor: Arc<Executor>,   // 持有 Arc<dyn Transport> 与执行级 Header
}

impl<P: Endpoint> ApiClient<P> {
    pub async fn create(&self, request: &P::Request) -> Result<P::Response, AirxError>;
    pub async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, AirxError>; // 逃生通道
}
```

- 协议端点是**静态泛型**：类型信息不丢失，每个 API 保持自己的 Request / Response。
- 传输实现通过 `Arc<dyn Transport>` **动态替换**：Builder 可在 Reqwest / Mock / 自定义实现间切换，无需改协议代码。
- `ApiClient` 位于 `airx`（组合根），保证 protocol 与 transport 不互相依赖。

### 6.5 执行流水线

```text
Request
  │
  ▼
Authentication        protocol 配置注入（API Key / 版本 / Beta Header）
  │
  ▼
Request Middleware    transport（可选）
  │
  ▼
Retry Policy          幂等感知：连接失败 / 429 / 5xx / 超时 / 非幂等跳过
  │
  ▼
Transport             唯一 IO
  │
  ▼
Response Middleware   transport（可选）
  │
  ▼
Protocol Decoder      protocol（纯函数）
```

### 6.6 错误契约

```rust
pub enum AirxError {
    Transport(TransportError),      // 连接、TLS、超时、取消
    Http(HttpError),                // 非 2xx 且无法解析为 Provider 错误
    Serialization(SerializationError),
    Deserialization(DeserializationError),
    Provider(ProviderError),        // 可识别的 Provider 错误体
    Stream(StreamError),            // 语法、截断、状态非法
    Compatibility(CompatibilityError), // Strict 模式下的不可表达
    Configuration(ConfigurationError),
}

pub struct ProviderError {
    pub provider: ProviderId,
    pub status: Option<u16>,
    pub code: Option<String>,
    pub message: String,
    pub request_id: Option<String>,
    pub raw_body: Option<RawJson>,
}
```

依赖约束下的落地方式：错误**形状**定义在 `airx-core`（普通数据结构 + 可选 boxed source，不引用任何后端类型）；`airx-transport` 把 reqwest/gloo 错误映射进来；`airx-protocol` 把错误体解析成 `ProviderError`。上层提供 `is_retryable()`、`retry_after()`、`request_id()` 等判定，但**不把 400 / 429 / 500 折叠为同一错误**。

### 6.7 流生命周期契约

```rust
pub enum StreamStatus {
    Active,
    Completed,
    Failed,
    Incomplete,
    Cancelled,
}

impl StreamAccumulator {
    pub fn apply(&mut self, event: Event) -> Result<(), AirxError>;
    pub fn snapshot(&self) -> &StreamState;
    /// 必须区分正常完成 / 失败 / 不完整
    pub fn finish(self) -> Result<Response, AirxError>;
}
```

- 网络断开**不得**被当作成功完成。
- 用户 drop 流即取消读取，不在后台继续消费。
- 聚合逻辑不写在 HTTP Client 内部。

---

## 7. 关键横切设计

### 7.1 JSON

**四种表示，按场景选型，不用一种类型硬扛**

| 表示 | 用途 |
| --- | --- |
| 强类型 struct | 协议 Request / Response |
| `serde_json::Value` | 动态字段、未知结构探索 |
| `RawValue` / `RawJson` | 原样转发、字节级保真 |
| `Bytes` | 原始网络数据、日志/代理透传 |

**处理流水线**

```text
Typed Request → Compatibility Transform → Serialize → Transport
                                                        │
Raw Response ← Bytes ← Deserialize ← Normalize(可选) ←───┘
```

- 原生请求**直发**，不经过 Unified 中转。
- 默认公共 API 使用拥有所有权的类型；内部解析器按需借用；高性能场景另给 `*_ref` 接口。
- 未知字段用显式扩展字段（`Extensions`）或 `RawExtension`，不假设排序是协议语义，不用 `Value` 保证原始字节。

**格式化模式**：`Compact`（请求）、`Pretty`（日志）、`Preserve`（保真）、`Canonical`（确定性，签名/缓存 Key 场景预留 RFC 8785 JCS）。

**兼容差异纯函数化**：Provider 特有的 JSON/参数差异优先用纯函数或静态配置；只有需要用户自定义或运行时注册时才用 trait object。

### 7.2 Stream

三层职责，不可合并：

| 层级 | 职责 | 位置 |
| --- | --- | --- |
| SSE 语法 | 事件边界、字段、多行、CRLF、UTF-8 跨 buffer | `airx-core::stream::sse` |
| 协议事件 | 事件类型、增量语义、`[DONE]`、错误事件 | `airx-protocol::<provider>::<api>::event` |
| 网络读取 | 字节流、背压、取消 | `airx-transport::stream` |

```text
HTTP Body Stream → Transport Stream → SSE Decoder → Raw Event
  → Provider Decoder → Native Event → [可选] Accumulator → Response
```

要求：

- 每个协议独立的 Stream 类型（`ResponsesStream` / `AnthropicStream`），不统一成 `Stream<Item = Result<GenericChunk, Error>>`。
- 解析阶段即完成结构化，不把事件降级为 `String`。
- `JsonAccumulator` 接收增量字符串、保留原始片段、判断完整性、完成后一次性反序列化；**不对每个 delta 做完整解析**。
- 支持“只取文本”“只监听工具调用”“自管状态”四种消费姿势。

### 7.3 Capability / API Version / Model ID

三个概念必须分离：

| 概念 | 含义 | 归属 |
| --- | --- | --- |
| API Version | Provider 接口版本 | `airx-protocol::<provider>::config` |
| Model Capability | 模型功能能力 | 类型 `airx-core::model`，判定 `airx-unified::capability` |
| Model ID | 模型标识 | `airx-core::model` |

```rust
pub enum CapabilitySupport {
    Supported,
    Unsupported,
    ModelDependent,
    Unknown,
}
```

不用 `bool`：能力取决于模型、API 版本与参数组合。需要支持用户自定义模型能力、模型别名与模型专属参数（Model Registry 预留）。

### 7.4 Unified 映射

- 统一 `Content` 含 `Text / Image / Audio / Video / File / ToolCall / ToolResult / Refusal / Thinking / Custom`。
- 转换遵循 `CompatibilityMode`：`Strict` 不可表达即报错；`BestEffort` 允许降级但必须返回结构化 Warning。
- 扩展走 `ProviderExtensions` 命名空间，后续可收紧为强类型。
- **任何情况下不得静默丢弃** Thinking、Tool Call、多模态内容。

### 7.5 配置分层

| 配置类型 | 内容 | 归属 |
| --- | --- | --- |
| Provider 配置 | api_key、base_url、API version、默认/Beta Header、组织/项目 | `airx-protocol::<provider>::config` |
| 执行配置 | transport、timeout、retry_policy、default_headers | `airx::config` / `airx-transport` |

`base_url` 必须可覆盖：官方 API、OpenAI Compatible、企业代理、自建服务、第三方 Gateway 都是真实场景。不允许把官方域名硬编码进通用路径构造。

---

## 8. 对外 API 与 Feature

### 8.1 Native API

```rust
use airx::prelude::*;

let client = Client::builder()
    .openai()
    .api_key("sk-xxx")
    .build()?;

let response = client
    .openai()?                 // Client 可能未配置该 Provider
    .chat()
    .create(
        ChatCompletionRequest::builder()
            .model("gpt-4o-mini")
            .user("Hello")
            .build()?,
    )
    .await?;
```

```rust
let mut stream = client.openai().responses().stream(request).await?;

while let Some(event) = stream.next().await {
    match event? {
        ResponsesEvent::OutputTextDelta(e) => print!("{}", e.delta),
        ResponsesEvent::Completed(_) => break,
        _ => {}
    }
}
```

`.openai()` / `.anthropic()` 返回**静态类型**的 API Client，不做字符串查找 Provider；
兼容部署通过 `base_url` 指定。

### 8.2 Unified API

```rust
let client = Client::builder().api_key("xxx").build()?;

let response = client
    .generate(
        GenerateRequest::builder()
            .model("provider/model")
            .user("Hello")
            .build()?,
    )
    .await?;
```

Unified 的 Provider 选择可通过 Model ID、显式 Provider 或 Registry 完成；Native 路径不依赖该 Registry。

### 8.3 Feature 设计

| Feature | 内容 |
| --- | --- |
| `native` | Native HTTP（reqwest，TLS 后端可再细分） |
| `wasm` | 浏览器 Fetch |
| `openai` | OpenAI Responses + Chat |
| `anthropic` | Anthropic Messages |
| `all-providers` | 全部 Provider |
| `unified` | 统一层 |
| `stream` | 流式能力（core / transport / protocol 联动） |
| `json` | JSON 扩展工具 |

约定：

- 默认 feature 尽量少：`default = ["native", "openai"]` 量级，不默认引入全部 Provider。
- Cargo features 是 additive：同一依赖被多路径引入会合并，需特别关注 TLS 选择与 native/wasm 的目标依赖。
- 没有流式能力的协议不被 `stream` 强行牵连，按模块导出。

### 8.4 Builder 只做配置

```rust
let client = Client::builder()
    .transport(transport)
    .timeout(Duration::from_secs(60))
    .retry_policy(RetryPolicy::default())
    .default_headers(headers)
    .build()?;
```

Builder 不做业务决策、不做请求改写、不做隐式重试。

---

## 9. 质量保障

### 9.1 测试矩阵

| 维度 | 覆盖点 |
| --- | --- |
| Serialization | 请求序列化、响应反序列化、字段缺省与 `null`、未知字段保留、Raw JSON |
| Streaming | SSE 分片边界、UTF-8 跨 buffer、多行 data、tool 参数 delta、异常中断、正常完成、`[DONE]` |
| Protocol | Responses、Chat Completions、Messages、兼容部署 JSON mode |
| Compatibility | JSON Schema、tool_choice、thinking、多模态、Provider 专属字段 |
| Conversion | Native → Unified（含 Strict 报错与 BestEffort Warning） |
| Platform | Linux / macOS / Windows / `wasm32-unknown-unknown` |
| Transport | Mock 执行、超时、重试退避、取消、错误映射 |

### 9.2 Golden Fixtures

```text
tests/fixtures/
├── openai/{responses,chat}/{request,response}.json + events/
└── anthropic/messages/{request,response}.json + events/
```

协议模型每次变更都跑：反序列化 → 序列化 → 字段兼容性 → 流事件 → Native→Unified 转换 → WASM 编译。

不要只比较 JSON 字符串：结构等价与“明确要求字节保真”的用例分开断言。

### 9.3 编译与依赖治理

- `rustfmt.toml` 统一格式；lint 统一在根 `[workspace.lints]` 配置并由各 crate 继承，不分散到 `clippy.toml`。
- 依赖方向由 `scripts/check-deps.sh` 校验；`deny.toml` 在 M9 接入许可白名单、重复依赖与安全公告检查。
- CI 矩阵：`cargo test`、`cargo check --target wasm32-unknown-unknown`、feature 组合抽样（最小 / 默认 / all-providers / unified+stream）。
- MSRV 与 edition 2024 保持一致（Rust 1.85+），实际发布前按 WASM 目标与 TLS 方案复核依赖版本。

---

## 10. 实施路线

围绕两个约束排序：**依赖只能自底向上**，**功能必须纵向切通**。任何一层都不等“全量完成”才接线。

### 10.1 里程碑与内部顺序

| 里程碑 | 前置 | 实现顺序 | 出口标准 |
| --- | --- | --- | --- |
| **M0 Workspace 接线** | — | 根 `[workspace.package]` / `[workspace.dependencies]` / feature 空表 → 各 crate 声明允许依赖 → fmt/clippy/CI 最小集 | 全 workspace `cargo check` 通过；`cargo metadata` 可校验依赖方向 |
| **M1 core 契约层** | M0 | `error` → `model` → `http`（数据结构）→ `json::{raw,value,format}` → `stream::{buffer,sse,decoder,status,json_accumulator}` | SSE 在任意字节切分、CRLF/LF、UTF-8 跨块下测试通过；无 IO、无 async、无时钟 |
| **M2 transport 执行层** | M1 | `transport` trait → `mock` → `native`（reqwest）→ `stream`（字节流/背压）→ 错误映射 + `ResponseMetadata` → timeout/cancel → `retry`/`middleware` | Mock 覆盖全部执行分支；一条流式读取测试通过；trait 形状满足非 `Send` 平台 |
| **M3 第一条端到端竖切** | M1、M2 | 最小 `Client`/`Builder` → `openai::chat` 最小子集（config、request、response、message）→ 编解码接到 transport → example | Mock 驱动的一次 `create` 全链路通过。**Protocol / Transport / Client 三契约在此冻结** |
| **M4 OpenAI Chat 完整 + 流式** | M3 | 非流式补全（tool、structured output、多模态）→ `chunk`/事件模型 → 接通 SSE → Accumulator | fixtures 全绿；`finish()` 四态断言；tool 参数增量 JSON 测试 |
| **M5 OpenAI Responses** | M4 | `input`/`item` → `response`/`output` → 非流式 create → 语义事件 → Accumulator → 工具/结构化/多模态 | fixtures 全绿；事件类型逐条对齐官方事件名 |
| **M6 Anthropic Messages** | M5 | config/common → content block/message → request/response → tool use/thinking → 事件 + Accumulator → caching/structured outputs | fixtures 全绿；**新增 Provider 未修改 core 与 transport**（架构验证点） |
| **M7 兼容部署（DeepSeek）** | M6 | OpenAI-Compatible / Anthropic-Compatible base_url → JSON mode 示例 → 文档 | 同一协议客户端可直接接入，无 provider 专属协议模块 |
| **M8 Unified** | ≥2 个 native 协议完成 | 统一类型 → `capability` → `adapter::openai` → `adapter::anthropic` → `compatibility`（Strict/BestEffort + Warning）→ Unified 编排 | 双向转换矩阵 + 不可表达能力的报错/告警断言；**新增统一层未修改 protocol** |
| **M9 生产化** | M8 | WASM Fetch → retry 策略细化 → middleware/tracing → Canonical JSON 与 Raw 透传 API → `deny`/clippy/MSRV/feature 矩阵 → 文档与示例 → API 冻结评审 | 跨平台 CI 全绿；公共 API 冻结候选 |

为什么这个顺序：

- **M1 先做 `error`**：所有函数签名都要用到它，先定形状可避免后期批量改签名。
- **M2 先做 `mock` 再做 `native`**：Mock 是 M3 之后全部测试的基础设施，晚做会被迫用真实网络写测试。
- **M3 只做“薄竖切”**：在写几百个类型之前先验证组合契约；契约错误此时修复成本最低。
- **先 Chat 后 Responses**：Chat 的非流式与 `chunk/delta` 模型更简单，用来把解码与流式基础设施跑通；Responses 的 item 生命周期与语义事件放在基础稳固之后。
- **兼容部署早于 Unified**：DeepSeek 通过 base_url 验证“新增部署不改协议”，成本低；而 Unified 需要多个协议作为抽象样本。
- **WASM、Canonical JSON、可观测放到 M9**：在此之前没有真实消费者，提前实现容易做成无人使用的抽象。

### 10.2 排序原则

1. **依赖自底向上，功能纵向切透**：每完成一层就接一条端到端链路，不做“全量类型后再接线”。
2. **能力进入方式固定**：先落 fixture（取自真实响应/官方文档），再写类型，再写断言，最后接 UI 入口。
3. **用新增 Provider 检验架构**：M6/M7 若需要改动 core 或 transport，停下来修边界，而不是加 `if`。
4. **稳定度与冻结节奏**：`airx-core` 在 M2 结束后冻结公共 API；`airx-protocol` 允许持续演进直到 M9；`airx-unified` 始终可演进。
5. **每个里程碑结束**：`cargo test` + `cargo check --target wasm32-unknown-unknown` + fmt/clippy 全绿，不允许跨里程碑带债。

### 10.3 明确暂缓

| 项目 | 暂缓原因 |
| --- | --- |
| `google` 协议模块 | 无首批消费者，目录预留即可 |
| 派生宏 crate | 手写 builder/serde 已足够，宏会提前锁死字段形态 |
| WebSocket / 双向流 Transport | 当前协议均为 HTTP + SSE |
| 运行时 Provider Registry | Native 路径使用静态类型；Unified 先用 Model ID 推断 |
| `ApiProtocol` 的 trait object 化 | 静态泛型已覆盖；动态分发在确有需要时再适配 |
| Canonical JSON（JCS） | 等出现签名/缓存 Key 的真实需求 |
| 多语言绑定（FFI/WASM 包） | 待 Rust API 冻结后再考虑 |

---

## 11. 当前状态

**M0–M9 已全部完成**（边界见下方“已知缺口”）。

M0：

- 5 个 crate 接入 workspace，`edition = "2024"`、`resolver = "3"`、`rust-version = "1.85"`。
- 根 `Cargo.toml` 配置 `[workspace.dependencies]`、`[workspace.lints]`（`unsafe_code = forbid` 等）与 release profile。
- 依赖方向由 `scripts/check-deps.sh` 强制；`.github/workflows/ci.yml` 覆盖 fmt、clippy、test、依赖方向、最小 feature 与 wasm32 编译。

M1（`airx-core`，纯函数、无 IO、无 async、无时钟）：

| 模块 | 已实现 |
| --- | --- |
| `error` | `AirxError` 八类变体；`TransportError` / `HttpError` / `ProviderError` / `StreamError` 等保留 status、code、request id、原始 body 与 source 链；`is_retryable()` / `http_status()` 中立判定；WASM 下放宽 `Send + Sync` |
| `model` | `ProviderId` / `ModelId` / `ApiVersion` 三个独立字符串新类型；`CapabilitySupport` 四态；`ModelCapabilities` 默认 `Unknown` |
| `http` | `HttpRequest` / `HttpResponse` 数据结构与 `http` 类型 re-export，不含客户端 |
| `json` | `RawJson`（`Box<RawValue>`）与 `parse_raw` / `raw_from_value`、`RawExtension`、`Extensions`（`BTreeMap`）、`JsonMode` 四态；`Canonical` 显式返回未实现错误 |
| `stream::buffer` | 字节 → 行：LF / CRLF / 单 CR、跨 chunk `CR` 等待、UTF-8 整行校验、BOM 跳过 |
| `stream::sse` | 字段语法（`event` / `data` / `id` / `retry` / 注释 / 无冒号行）与 `RawEvent` |
| `stream::decoder` | 事件装配：多行 data、空 data 不分发、`last_event_id`、`finish()` 区分完成与截断 |
| `stream::json_accumulator` | 增量结构判定（`Empty` / `Incomplete` / `Complete` / `Malformed`）、只扫描增量、不完整时给出明确状态 |
| `stream::status` | `StreamStatus` 五态，`Incomplete` 与 `Failed` 分离 |

M2（`airx-transport`，唯一 IO 层）：

| 模块 | 已实现 |
| --- | --- |
| `transport` | 对象安全 `Transport`（`BoxFuture`）、`&T` / `Arc<T>` 转发、Native 要求 `Send + Sync`、WASM 去掉该约束 |
| `mock` | 响应队列、流式响应、错误注入、请求记录；克隆共享状态 |
| `native` | Reqwest 实现、超时、请求映射、错误分类（connect / timeout / body / TLS / other）并保留 source |
| `stream` / `response` | `ByteStream`、`HttpStreamResponse`、`ResponseMetadata` 与常见 request id Header |
| `retry` | `RetryPolicy`（默认关闭、指数退避、上限）、`Retry-After`、幂等感知：Connect 任意方法 / Timeout 与 5xx 仅幂等方法 / 429 任意方法；流式不重试 |
| `middleware` | 同步中间件链（请求正序、响应逆序），流式响应不触发 `on_response` |

M3（端到端竖切，契约冻结）：

| 契约 | 实现 |
| --- | --- |
| Protocol | `airx_protocol::Endpoint`（`encode` / `decode`，纯函数）；`openai::chat` 的 Request / Message / Response / Endpoint / 错误映射 / `OpenAiConfig`（Debug 脱敏、base_url 校验） |
| Transport | 固定错误形状 + `BoxFuture` + Mock/Reqwest 双实现 |
| Client | `Client` / `ClientBuilder` / `OpenAiBuilder` / typed `chat().create()`；执行级 Header 不覆盖协议 Header；`openai()` 返回 `Result` 以表达“未配置该 Provider” |
| 端到端 | Mock 驱动的 8 个集成用例：全链路、Provider 错误、Header 优先级、默认不重试、429 重试、base_url 覆盖、未配置 Provider、扩展字段透传 |

M4（OpenAI Chat 完整化与流式）：

| 能力 | 实现 |
| --- | --- |
| 请求 | tools / tool_choice / parallel_tool_calls / response_format（json_schema）/ stream / stream_options |
| 消息 | 多模态 user parts、assistant tool_calls、tool 结果 |
| 事件 | `ChatStreamEvent::{Chunk, Done}`、chunk/delta/tool_calls 增量、usage chunk |
| 聚合 | `ChatStreamAccumulator`：合并文本与工具参数，未收到 `[DONE]` 报 `Incomplete` |
| 契约 | `StreamingEndpoint` + `ProtocolStream`：字节流 → SSE → 协议事件，非 2xx 先读完 body 再映射错误 |

M5（OpenAI Responses）：

| 能力 | 实现 |
| --- | --- |
| 请求 | input（文本 / item）、instructions、max_output_tokens、tools、stream |
| 响应 | OutputItem（message / function_call）、`Other(Value)` 保留未知 item / content |
| 事件 | created / output_item.added·done / output_text.delta·done / function_call_arguments.delta·done / completed / failed / incomplete / error / `Unknown(Value)` |
| 聚合 | `ResponsesStreamAccumulator`：完成后优先使用服务端完整响应，否则合成累积 item |

M6（Anthropic Messages）：

| 能力 | 实现 |
| --- | --- |
| 请求 | system、max_tokens、temperature / top_p / top_k / stop_sequences、tools / tool_choice、thinking |
| 内容块 | text / image / tool_use / tool_result / thinking / redacted_thinking / `Other(Value)` |
| 事件 | message_start / content_block_start·delta·stop / message_delta / message_stop / ping / error / `Unknown(Value)` |
| 聚合 | `MessagesStreamAccumulator`：tool 参数增量拼接后解析，`message_stop` 前终止报 `Incomplete` |

M7（兼容部署：DeepSeek）：

- 不再提供独立 DeepSeek 协议模块；DeepSeek 通过 OpenAI-Compatible（`https://api.deepseek.com`）
  或 Anthropic-Compatible（`https://api.deepseek.com/anthropic`）端点接入，只改 `base_url`。
- JSON mode：`response_format=json_object`，提示词必须包含 “json” 与示例；空 content/截断需兜底。
- 新增 `deepseek_json` 示例，同时展示两种协议形态。
- Unified 的 `ProviderId::deepseek` 仅作标签，实际走 OpenAI Chat；Anthropic-Compatible 用 Native API。

M8（Unified）：

- 统一 `Content` / `Message` / `GenerateRequest` / `GenerateResponse` / `TokenUsage`。
- 纯函数 Adapter：OpenAI、Anthropic；`Custom` 在 Anthropic 侧以 `Other(Value)` 保留。
  DeepSeek 通过 OpenAI-Compatible 协议进入 Unified，`ProviderId::deepseek` 仅作为标签。
- `capabilities(provider, model)`：已知模型给表，未知模型一律 `Unknown`。
- `Client::generate`（Strict）/ `generate_best_effort`（降级 + `GenerateResponse.warnings`）。
- Provider 选择：显式 `.provider(...)`，否则按模型名推断。

M9（生产化）：

| 项 | 实现 |
| --- | --- |
| WASM | `WasmTransport`：Fetch 请求、Header 转换、`ReadableStream` 流式；`wasm32` 编译检查通过 |
| 可观测 | `tracing` 请求 / 响应 / 流建立 / 重试事件；不记录敏感 Header |
| CI | fmt / clippy / test / rustdoc（`-D warnings`）/ 依赖方向 / 最小 feature / wasm32（含 unified） |
| 文档 | `protocol-support.md`、`compatibility.md`、README 快速开始与 Feature 表 |
| 示例 | `chat`、`chat_stream`、`unified` |

测试总计 **180** 个，`cargo test --workspace --features airx/anthropic,airx/unified` 全绿。
覆盖 SSE 任意字节切分、UTF-8 跨 chunk、CRLF/CR、BOM、截断流、增量 JSON、三个 Provider 的
流式事件与聚合、错误映射、重试幂等分支、Header 优先级、Unified 的 Strict/BestEffort。

**已知缺口（有意保留，且不会静默降级）**：

1. `JsonMode::Canonical`（RFC 8785 JCS）未实现，返回明确错误；不做“近似规范化”。
2. WASM 传输只做了编译检查，浏览器运行时（CORS、受限 Header、流读取）尚未纳入 CI。
3. 中间件为同步实现；异步中间件（如 Token 刷新）请自定义 `Transport`。
4. 流式请求不重试；首事件前重试留待后续。
5. `Option<T>` 不区分 `null` 与缺省；字节 / null 级保真需使用 `RawJson`（见 `docs/compatibility.md`）。

**接口对齐（2026-11-23）**：按官方 SDK / API 文档更新协议模型——Anthropic 移除
`temperature` / `top_p` / `top_k`，新增 `output_config`（`effort` / `format`）与四种
`thinking` 形态；OpenAI `max_tokens` 弃用，新增 `max_completion_tokens` /
`reasoning_effort` / `verbosity`，Responses 增加 `reasoning.effort` 与 `text.format`；
DeepSeek 通过 OpenAI / Anthropic 兼容端点接入，不设独立协议；JSON mode 差异记录在
`docs/protocol-support.md` 与 `docs/compatibility.md`。

服务端联网工具（`web_search` / `web_fetch` / `deepseek_web_search`）按 Provider 建模：
工具声明与结果（`web_search_call`、`server_tool_use`、`web_search_tool_result`、
`annotations`）进协议层，联网请求由 Provider 后台执行，airx 不代发。


---

## 12. 附录：决策速查与被否决方案

### 12.1 速查

| 问题 | 结论 |
| --- | --- |
| 整体架构 | Protocol-First |
| crate 数量 | 5（core / protocol / transport / unified / airx） |
| 核心设计 | Functional Core / Imperative Shell |
| Provider 扩展 | Adapter，单向 |
| Native API | 独立强类型模型，按 provider × api 分模块 |
| Unified API | 可选、下游、纯转换、不依赖 transport |
| JSON | Serde + RawValue + Value + Bytes，四种表示 |
| Stream | SSE 解析一次，事件各自建模，聚合独立 |
| IO | 可替换 Transport，协议零 HTTP 依赖 |
| WASM | 同 crate + target-specific 实现 + feature |
| Async | 原生 async + 静态泛型，不强制 Send |
| 错误 | 统一分类 + 原始信息保留 |
| 测试 | Golden Fixtures + 协议/流式矩阵 |
| 演进 | 小 Workspace、强模块边界、协议保真、纯函数、可替换副作用 |

### 12.2 长期竞争力预留

1. **API Versioning**：API 版本、Beta Header、Feature Header 由各协议 Client 管理，不散落在通用代码。
2. **Model Registry**：支持用户自定义能力、别名与模型专属参数；模型名不决定能力。
3. **Raw Request / Raw Response**：Provider 新增字段而 SDK 未支持时，用户仍可通过 Raw JSON 与原生接口使用新能力，无需等待发版。

### 12.3 设计哲学

> Small Workspace, Strong Module Boundaries, Native Protocol Fidelity, Functional Core, Replaceable Effects.

一句话：5 个 crate、单向依赖、原生协议优先、纯函数转换、副作用集中在 transport。

### 12.4 参考

- [OpenAI Responses API](https://developers.openai.com/api/reference/resources/responses)
- [OpenAI Chat Completions API](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create)
- [OpenAI Responses Streaming Events](https://developers.openai.com/api/reference/resources/responses/streaming-events)
- [Anthropic API Documentation](https://docs.anthropic.com/)

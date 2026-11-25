# airx-core

`airx` 的基础层：错误分类、模型标识、JSON 保真、SSE 解码与 HTTP 数据结构。
不依赖 `airx` 的其他 crate，可单独使用。

[文档](https://docs.rs/airx-core) · [仓库](https://github.com/wwog/airx)

## 内容

- `AirxError`：`Transport` / `Http` / `Provider` / `Stream` / `Deserialization` /
  `Configuration` 分类，保留状态码、错误码与原始响应体。
- `JsonMode` / `RawJson`：compact / pretty / 原样转发（`Box<RawValue>`），
  `Extensions`（`BTreeMap<String, Value>`）用于未知字段双向保留。
- `SseDecoder` / `LineBuffer` / `RawEvent`：SSE 语法解析（LF/CRLF/CR、跨 chunk UTF-8）。
- `JsonAccumulator`：工具参数等增量 JSON 的结构完整性判断（不做部分反序列化）。
- `ModelId` / `ProviderId` / `ApiVersion`：轻量标识类型。
- `HttpRequest` / `HttpResponse` / `HeaderMap` 等传输无关结构。

## 安装

```toml
[dependencies]
airx-core = "0.1"
```

```rust
use airx_core::stream::SseDecoder;

let mut decoder = SseDecoder::new();
decoder.push(b"data: {\"ok\":true}\n\n");
let event = decoder.next_event()?.expect("event");
assert_eq!(event.data, "{\"ok\":true}");
# Ok::<(), airx_core::AirxError>(())
```

## Feature

- `tracing`：额外提供 `log::TracingLogger` 适配器，把日志转发到 `tracing`
  （scope 映射为 `airx::net` 等 target）。日志类型本身不受 feature 控制，始终可用。

## 日志

`log` 模块只定义类型与过滤规则，不产生 IO：

```rust
use std::sync::Arc;
use airx_core::log::{LogConfig, LogHandle, LogLevel, Logger, Record, Scope};

struct Stderr;

impl Logger for Stderr {
    fn log(&self, record: &Record<'_>) {
        eprintln!("[{}][{}] {}", record.scope, record.level, record.message);
    }
}

let log = LogHandle::new(
    Arc::new(Stderr),
    LogConfig::new()
        .scope(Scope::NET, LogLevel::Info)
        .scope(Scope::CALL, LogLevel::Info),
);

assert!(log.enabled(Scope::NET, LogLevel::Warn));
assert!(!log.enabled(Scope::RETRY, LogLevel::Warn));
```

配置某级别表示输出该级别及更严重的级别；未配置的 scope 默认 `Off`。

辅助设施（纯函数，供上层日志点复用）：

- `Field::debug(key, value)`：以 `Debug` 形态记录没有 `Display` 的结构（如流式事件）。
- `redact_headers(&HeaderMap)`：内置 Header 脱敏；`LogConfig::header_redactor` 可完全替换。
- `body_preview(content_type, body, limit)`：文本截断预览 / 二进制摘要。
- `LogConfig::body_limit(usize)`：`Debug` 级别 body 预览上限，默认 4 KiB。

## License

MIT OR Apache-2.0

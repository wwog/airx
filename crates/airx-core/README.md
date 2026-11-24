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

- `tracing`：启用可选日志（默认关闭）。

## License

MIT OR Apache-2.0

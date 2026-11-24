# airx-transport

`airx` 的传输层：把编码好的 `HttpRequest` 交给 Reqwest（native）或浏览器 Fetch
（wasm），返回响应或字节流；处理超时、取消、重试与中间件。

[文档](https://docs.rs/airx-transport) · [仓库](https://github.com/wwog/airx)

## 安装

```toml
[dependencies]
airx-transport = "0.1"          # 默认 native + rustls
```

裁剪：

```toml
airx-transport = { version = "0.1", default-features = false, features = ["native", "native-tls"] }
```

## Feature

| Feature | 作用 | 默认 |
| --- | --- | --- |
| `native` | Reqwest 传输（连接池、超时、TLS） | ✅ |
| `rustls-tls` | Reqwest 使用 rustls | ✅ |
| `native-tls` | Reqwest 使用系统 TLS（与 rustls 二选一） | — |
| `compression` | gzip / brotli / deflate | — |
| `wasm` | 浏览器 Fetch + `ReadableStream` | — |

WASM 目标不自动重试；native 默认对幂等请求重试 2 次，支持 429 / 5xx /
`Retry-After`（秒或 HTTP-date）与 ±20% jitter。

## 使用

```rust
use airx_core::http::HttpRequest;
use airx_transport::Transport;

let transport = airx_transport::ReqwestTransport::new()?;
let request = HttpRequest::get("https://example.com".parse().unwrap());
let response = transport.execute(request).await?;
println!("{}", response.status);
# Ok::<(), airx_core::AirxError>(())
```

## License

MIT OR Apache-2.0

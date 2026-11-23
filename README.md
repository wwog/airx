# airx

多协议 AI API 基础设施库（Rust）。One Rust SDK. Multiple AI protocols. No feature loss.

## 结构

| Crate | 职责 |
| --- | --- |
| `airx-core` | 基础层：错误分类、模型与能力标识、JSON 保真、SSE 语法、HTTP 数据结构 |
| `airx-protocol` | 原生协议：OpenAI / Anthropic / DeepSeek 的强类型模型与纯编解码 |
| `airx-transport` | 传输层：HTTP 执行、流式读取、超时取消、重试、中间件 |
| `airx-unified` | 统一层（可选）：跨 Provider 的统一模型与 Adapter |
| `airx` | 门面：Client / Builder / typed endpoint / prelude |

依赖单向：`airx → {core, protocol, transport, unified}`，`protocol/transport → core`，`unified → {core, protocol}`。
完整设计与实现顺序见 [docs/architecture.md](docs/architecture.md)。

## 开发

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/check-deps.sh
cargo check -p airx --target wasm32-unknown-unknown \
  --no-default-features --features wasm,openai,stream
```

## 状态

M0（Workspace 接线）与 M1（`airx-core` 契约层）已完成，60 个测试通过。
下一步 M2：`airx-transport` 的 `Transport` trait、Mock、native、流式读取与重试。

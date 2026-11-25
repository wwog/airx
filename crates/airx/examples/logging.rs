//! 按 scope 开启日志：业务层注入自己的 `Logger` 实现。
//!
//! ```bash
//! cargo run -p airx --example logging
//! ```
//!
//! 这里用一个内存收集器演示，实际业务可以换成 channel、文件或 `tracing`。

use std::sync::{Arc, Mutex};

use airx::prelude::*;

/// 把日志写进内存集合的业务实现。
///
/// `Logger::log` 必须同步、快速、不阻塞；需要异步落盘时投递到 channel 后立即返回。
#[derive(Default)]
struct MemoryLogger {
    lines: Mutex<Vec<String>>,
}

impl MemoryLogger {
    fn snapshot(&self) -> Vec<String> {
        self.lines.lock().expect("lock").clone()
    }
}

impl Logger for MemoryLogger {
    fn log(&self, record: &Record<'_>) {
        let fields = record
            .fields
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        self.lines.lock().expect("lock").push(format!(
            "[{}][{}] {} {}",
            record.scope, record.level, record.message, fields
        ));
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let api_key = std::env::var("OPENCODE_API_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .expect("需要 OPENCODE_API_KEY 或 OPENAI_API_KEY（参考 .env.example）");
    let base_url = std::env::var("OPENCODE_BASE_URL")
        .or_else(|_| std::env::var("OPENAI_BASE_URL"))
        .unwrap_or_else(|_| "https://api.openai.com/v1".to_owned());

    // 只打开 net 与 call：等价于 `{net: Info, call: Info}`。
    let config = LogConfig::new()
        .scope(Scope::NET, LogLevel::Info)
        .scope(Scope::CALL, LogLevel::Info);

    let logger = Arc::new(MemoryLogger::default());
    let client = Client::builder()
        .logger(logger.clone())
        .log_config(config)
        .openai()
        .api_key(api_key)
        .base_url(base_url)
        .build()?;

    let request = ChatCompletionRequest::builder()
        .model("gpt-4o-mini")
        .user("用一句话解释 Rust 所有权")
        .build()?;

    match client.chat().create(request).await {
        Ok(response) => println!("{}", response.first_content().unwrap_or_default()),
        Err(error) => println!("请求失败（日志里应有 error / warn）：{error}"),
    }

    println!("\n=== 捕获到的日志 ===");
    for line in logger.snapshot() {
        println!("{line}");
    }

    Ok(())
}

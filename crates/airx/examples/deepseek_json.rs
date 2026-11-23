//! DeepSeek 接入示例：DeepSeek 没有独立协议，只是 OpenAI / Anthropic 兼容部署。
//!
//! - OpenAI-Compatible：`base_url = https://api.deepseek.com` + JSON mode。
//! - Anthropic-Compatible：`base_url = https://api.deepseek.com/anthropic`。
//!
//! ```bash
//! DEEPSEEK_API_KEY=sk-... cargo run -p airx --example deepseek_json
//! ```

use airx::prelude::*;
use airx_protocol::openai::chat::ResponseFormat;
use serde_json::Value;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

/// DeepSeek JSON mode 的提示词要求：必须出现 "json" 并给出示例。
const JSON_PROMPT: &str = "阅读下面的文本，以 json 返回，字段必须是 question 与 answer。\
示例：{\"question\":\"天空为什么是蓝色？\",\"answer\":\"瑞利散射。\"}\n\
文本：为什么 Rust 的借用检查器能在编译期发现悬垂引用？";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let api_key =
        std::env::var("DEEPSEEK_API_KEY").expect("DEEPSEEK_API_KEY 未设置（参考 .env.example）");
    let model = env_or("DEEPSEEK_MODEL", "deepseek-flash");
    let openai_base = env_or("DEEPSEEK_OPENAI_BASE_URL", "https://api.deepseek.com");
    let anthropic_base = env_or(
        "DEEPSEEK_ANTHROPIC_BASE_URL",
        "https://api.deepseek.com/anthropic",
    );

    // 1) OpenAI-Compatible + JSON mode
    println!("=== OpenAI-Compatible · JSON mode ===");
    let client = Client::builder()
        .openai()
        .api_key(&api_key)
        .base_url(&openai_base)
        .build()?;

    let request = ChatCompletionRequest::builder()
        .model(model.clone())
        .system("你只输出 json，不要解释。")
        .user(JSON_PROMPT)
        .response_format(ResponseFormat::JsonObject)
        // JSON mode 可能被 max_tokens 截断，官方建议留足预算。
        .max_tokens(512)
        .build()?;

    let response = client.openai()?.chat().create(request).await?;
    let text = response.first_content().unwrap_or_default();
    println!(
        "finish_reason = {:?}",
        response
            .choices
            .first()
            .and_then(|choice| choice.finish_reason.clone())
    );

    if text.trim().is_empty() {
        // 官方文档：JSON 解析失败的内容会被替换为空格，可能表现为空 content。
        eprintln!("content 为空：检查提示词是否包含 json 示例，或调大 max_tokens 后重试");
    } else {
        let value: Value = serde_json::from_str(text)?;
        println!("parsed = {value}");
    }

    if std::env::args().any(|arg| arg == "--openai-only") {
        println!("\n（--openai-only：跳过 Anthropic-Compatible 调用）");
        return Ok(());
    }

    // 2) Anthropic-Compatible Messages（同一模型，换协议入口）
    println!("\n=== Anthropic-Compatible · Messages ===");
    let client = Client::builder()
        .anthropic()
        .api_key(&api_key)
        .base_url(&anthropic_base)
        .build()?;

    let response = client
        .anthropic()?
        .messages()
        .create(
            MessagesRequest::builder()
                .model(model)
                .max_tokens(512)
                .system("你只输出 json，不要解释。")
                .user(JSON_PROMPT)
                .build()?,
        )
        .await?;

    let text = response.text();
    println!("stop_reason = {:?}", response.stop_reason);
    match serde_json::from_str::<Value>(&text) {
        Ok(value) => println!("parsed = {value}"),
        Err(error) => println!("未返回严格 JSON（{error}）：{text}"),
    }

    Ok(())
}

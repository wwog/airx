//! DeepSeek 接入示例：DeepSeek 没有独立协议，只是 OpenAI / Anthropic 兼容部署。
//!
//! - OpenAI-Compatible：`base_url = https://api.deepseek.com` + JSON mode。
//! - Anthropic-Compatible：`base_url = https://api.deepseek.com/anthropic`。
//!
//! ```bash
//! # 只跑 OpenAI-Compatible（默认 feature）
//! DEEPSEEK_API_KEY=sk-... cargo run -p airx --example deepseek_json
//!
//! # 同时跑 Anthropic-Compatible
//! DEEPSEEK_API_KEY=sk-... cargo run -p airx --features anthropic --example deepseek_json
//! ```

use airx::prelude::*;
#[cfg(feature = "openai")]
use airx_protocol::openai::chat::ResponseFormat;
use serde_json::Value;

/// 两种协议共用的系统提示。
const SYSTEM: &str = "你只输出 json，不要解释。";

/// DeepSeek JSON mode 的提示词要求：必须出现 "json" 并给出示例。
const JSON_PROMPT: &str = "阅读下面的文本，以 json 返回，字段必须是 question 与 answer。\
示例：{\"question\":\"天空为什么是蓝色？\",\"answer\":\"瑞利散射。\"}\n\
文本：为什么 Rust 的借用检查器能在编译期发现悬垂引用？";

/// JSON mode 可能被截断，官方建议留足预算；两种协议共用同一预算。
const MAX_TOKENS: u32 = 512;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

/// 两种协议共用的结果处理。
fn report(label: &str, detail: &str, text: &str) {
    println!("\n=== {label} ===");
    println!("{detail}");
    if text.trim().is_empty() {
        // 官方文档：JSON 解析失败的内容会被替换为空格，可能表现为空 content。
        eprintln!("content 为空：检查提示词是否包含 json 示例，或调大 MAX_TOKENS 后重试");
        return;
    }
    match serde_json::from_str::<Value>(text) {
        Ok(value) => println!("parsed = {value}"),
        Err(error) => println!("未返回严格 JSON（{error}）：{text}"),
    }
}

/// OpenAI-Compatible：Chat Completions + JSON mode。
#[cfg(feature = "openai")]
async fn via_openai(
    api_key: &str,
    model: &str,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    // 只有兼容部署才需要 base_url；官方 OpenAI 直接 Client::openai().api_key(...)
    // 就使用默认地址与默认路径。
    let openai = Client::openai()
        .api_key(api_key)
        .base_url(env_or(
            "DEEPSEEK_OPENAI_BASE_URL",
            "https://api.deepseek.com",
        ))
        .build()?;

    let response = openai
        .chat()
        .create(
            ChatCompletionRequest::builder()
                .model(model)
                .system(SYSTEM)
                .user(JSON_PROMPT)
                .response_format(ResponseFormat::JsonObject)
                .max_tokens(MAX_TOKENS)
                .build()?,
        )
        .await?;

    let finish_reason = response
        .choices
        .first()
        .and_then(|choice| choice.finish_reason.clone());
    Ok((
        format!("finish_reason = {finish_reason:?}"),
        response.first_content().unwrap_or_default().to_owned(),
    ))
}

/// Anthropic-Compatible：Messages（同一模型，换协议入口）。
#[cfg(feature = "anthropic")]
async fn via_anthropic(
    api_key: &str,
    model: &str,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    // 官方 Anthropic 同样不需要 base_url。
    let anthropic = Client::anthropic()
        .api_key(api_key)
        .base_url(env_or(
            "DEEPSEEK_ANTHROPIC_BASE_URL",
            "https://api.deepseek.com/anthropic",
        ))
        .build()?;

    let response = anthropic
        .messages()
        .create(
            MessagesRequest::builder()
                .model(model)
                .max_tokens(MAX_TOKENS)
                .system(SYSTEM)
                .user(JSON_PROMPT)
                .build()?,
        )
        .await?;

    Ok((
        format!("stop_reason = {:?}", response.stop_reason),
        response.text(),
    ))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let api_key =
        std::env::var("DEEPSEEK_API_KEY").expect("DEEPSEEK_API_KEY 未设置（参考 .env.example）");
    let model = env_or("DEEPSEEK_MODEL", "deepseek-flash");

    #[cfg(feature = "openai")]
    {
        let (detail, text) = via_openai(&api_key, &model).await?;
        report("OpenAI-Compatible · JSON mode", &detail, &text);
    }

    #[cfg(feature = "anthropic")]
    {
        let (detail, text) = via_anthropic(&api_key, &model).await?;
        report("Anthropic-Compatible · Messages", &detail, &text);
    }

    #[cfg(not(feature = "anthropic"))]
    eprintln!(
        "\n未启用 anthropic feature，跳过 Anthropic-Compatible 示例；\
         使用 `--features anthropic` 启用。"
    );

    Ok(())
}

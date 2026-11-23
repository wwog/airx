//! Gemini `generateContent` 示例。
//!
//! ```bash
//! GEMINI_API_KEY=... cargo run -p airx --features gemini --example gemini
//! GEMINI_MODEL=gemini-2.5-pro GEMINI_API_KEY=... cargo run -p airx --features gemini --example gemini
//! ```

use airx::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let api_key = std::env::var("GEMINI_API_KEY")?;
    let model = std::env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.5-flash".to_owned());

    let mut builder = Client::builder().gemini().api_key(api_key);
    if let Ok(base_url) = std::env::var("GEMINI_BASE_URL") {
        builder = builder.base_url(base_url);
    }
    let client = builder.build()?;

    let request = GenerateContentRequest::builder()
        .system("You are a concise assistant.")
        .user("用一句话解释 Rust 的所有权。")
        .max_output_tokens(256)
        .thinking(GeminiThinkingConfig::dynamic())
        .build()?;

    let response = client
        .gemini()?
        .generate_content(model.as_str())
        .create(request)
        .await?;

    if !response.thought_text().is_empty() {
        eprintln!("[thinking] {}", response.thought_text());
    }
    println!("{}", response.text());
    if let Some(usage) = &response.usage_metadata {
        println!(
            "tokens: prompt={} candidates={} total={}",
            usage.prompt_token_count, usage.candidates_token_count, usage.total_token_count
        );
    }

    Ok(())
}

//! Gemini 流式 `streamGenerateContent` 示例。
//!
//! ```bash
//! GEMINI_API_KEY=... cargo run -p airx --features gemini,stream --example gemini_stream
//! ```

use airx::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let api_key = std::env::var("GEMINI_API_KEY")?;
    let model = std::env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.5-flash".to_owned());

    let client = Client::builder().gemini().api_key(api_key).build()?;

    let request = GenerateContentRequest::builder()
        .user("用三句话解释 Rust 的所有权。")
        .build()?;

    let mut stream = client
        .gemini()?
        .generate_content(model.as_str())
        .stream(request)
        .await?;
    let mut accumulator = GenerateContentStreamAccumulator::new();

    while let Some(event) = stream.next().await? {
        let text = event.text();
        if !text.is_empty() {
            print!("{text}");
            use std::io::Write;
            std::io::stdout().flush()?;
        }
        accumulator.apply(event)?;
    }

    let response = accumulator.finish()?;
    println!("\n---\n{}", response.text());
    if let Some(usage) = &response.usage_metadata {
        println!("total tokens: {}", usage.total_token_count);
    }

    Ok(())
}

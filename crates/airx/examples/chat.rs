//! OpenAI Chat Completions 示例。
//!
//! ```bash
//! OPENAI_API_KEY=sk-... cargo run -p airx --example chat
//! ```

use airx::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPENAI_API_KEY")?;

    let client = Client::openai().api_key(api_key).build()?;

    let request = ChatCompletionRequest::builder()
        .model("gpt-4o-mini")
        .system("You are a concise assistant.")
        .user("Say hello in Chinese.")
        .max_completion_tokens(64)
        .build()?;

    let response = client.chat().create(request).await?;

    println!("{}", response.first_content().unwrap_or("<no content>"));
    if let Some(usage) = &response.usage {
        println!(
            "tokens: prompt={} completion={} total={}",
            usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
        );
    }

    Ok(())
}

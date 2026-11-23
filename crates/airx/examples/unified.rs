//! Unified API 示例：同一请求可路由到不同 Provider。
//!
//! ```bash
//! OPENAI_API_KEY=sk-... cargo run -p airx --example unified
//! ```

use airx::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPENAI_API_KEY")?;

    let client = Client::builder().openai().api_key(api_key).build()?;

    let response = client
        .generate(
            GenerateRequest::builder()
                .model("gpt-4o-mini")
                .instructions("You are concise.")
                .user("Say hello in Chinese.")
                .max_tokens(64)
                .build()?,
        )
        .await?;

    println!("provider: {}", response.provider);
    println!("text: {}", response.text());
    if let Some(usage) = response.usage {
        println!("tokens: {} total", usage.total_tokens);
    }
    for warning in response.warnings {
        eprintln!("warning: {}", warning.message);
    }

    Ok(())
}

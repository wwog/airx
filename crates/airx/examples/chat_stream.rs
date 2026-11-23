//! 流式 Chat 示例。
//!
//! ```bash
//! OPENAI_API_KEY=sk-... cargo run -p airx --example chat_stream
//! ```

use airx::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPENAI_API_KEY")?;

    let client = Client::builder().openai().api_key(api_key).build()?;

    let request = ChatCompletionRequest::builder()
        .model("gpt-4o-mini")
        .user("用一句话解释 Rust 的所有权。")
        .build()?;

    let mut stream = client.openai()?.chat().stream(request).await?;
    let mut accumulator = ChatStreamAccumulator::new();

    while let Some(event) = stream.next().await? {
        if let ChatStreamEvent::Chunk(chunk) = &event {
            for choice in &chunk.choices {
                if let Some(text) = &choice.delta.content {
                    print!("{text}");
                }
            }
        }
        accumulator.apply(event)?;
    }

    let response = accumulator.finish()?;
    println!("\n---\n{}", response.first_content().unwrap_or_default());

    Ok(())
}

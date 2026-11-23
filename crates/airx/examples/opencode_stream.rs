//! 流式生成：SSE → 事件 → 聚合器。
//!
//! ```bash
//! cargo run -p airx --example opencode_stream
//! ```

mod common;

use std::time::Instant;

use airx::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = common::load();
    let client = common::client(&env);

    for model in &env.models {
        println!("\n=== {model} · 流式 ===");

        let request = ChatCompletionRequest::builder()
            .model(model.clone())
            .system("你是图形生成器，只输出可解析的 SVG 源码。")
            .user(common::POSTER_PROMPT)
            .max_tokens(2048)
            .build()?;

        let started = Instant::now();
        let mut stream = client.openai()?.chat().stream(request).await?;
        let mut accumulator = ChatStreamAccumulator::new();
        let mut chunks = 0usize;
        let mut first_text = None;
        let mut reasoning_chars = 0usize;

        while let Some(event) = stream.next().await? {
            if let ChatStreamEvent::Chunk(chunk) = &event {
                chunks += 1;
                for choice in &chunk.choices {
                    reasoning_chars += choice
                        .delta
                        .reasoning_content
                        .as_deref()
                        .map(str::len)
                        .unwrap_or(0);
                    if first_text.is_none()
                        && choice
                            .delta
                            .content
                            .as_deref()
                            .is_some_and(|text| !text.is_empty())
                    {
                        first_text = Some(started.elapsed());
                    }
                }
            }
            accumulator.apply(event)?;
        }

        let response = accumulator.finish()?;
        let text = response.first_content().unwrap_or_default();
        let reasoning = response
            .choices
            .first()
            .and_then(|choice| choice.message.extensions.get("reasoning_content"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");

        println!(
            "chunks={chunks} first_text={:?} total={:?} chars={} reasoning_chars={}",
            first_text,
            started.elapsed(),
            text.chars().count(),
            reasoning.chars().count(),
        );
        let _ = reasoning_chars;

        let path = common::write_svg(model, text)?;
        println!("saved  = {path}");

        assert!(chunks > 0, "{model} 没有收到任何 chunk");
        assert!(first_text.is_some(), "{model} 流中没有文本增量");
        assert!(text.contains("<svg"), "{model} 聚合结果不是 SVG");
        assert!(
            !reasoning.is_empty(),
            "{model} 的流式 reasoning 没有进入聚合结果"
        );
    }

    println!("\n全部模型流式测试通过");
    Ok(())
}

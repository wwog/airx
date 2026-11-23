//! 非流式文本生成：让模型输出一张“文字海报” SVG。
//!
//! ```bash
//! cargo run -p airx --example opencode_text
//! ```

mod common;

use airx::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = common::load();
    let client = common::client(&env);

    println!("base_url = {}", env.base_url);
    println!("models   = {:?}", env.models);

    for model in &env.models {
        println!("\n=== {model} · 非流式 ===");

        let request = ChatCompletionRequest::builder()
            .model(model.clone())
            .system("你是图形生成器，只输出可解析的 SVG 源码。")
            .user(common::POSTER_PROMPT)
            .max_completion_tokens(2048)
            .build()?;

        let response = client.chat().create(request).await?;
        let text = response.first_content().unwrap_or_default();
        let reasoning = common::reasoning_of(&response);
        let usage = response.usage.as_ref();

        println!(
            "finish={:?} chars={} reasoning_chars={} tokens={:?}",
            response
                .choices
                .first()
                .and_then(|choice| choice.finish_reason.clone()),
            text.chars().count(),
            reasoning.chars().count(),
            usage.map(|usage| (
                usage.prompt_tokens,
                usage.completion_tokens,
                usage.total_tokens
            )),
        );

        let path = common::write_svg(model, text)?;
        println!("saved  = {path}");

        assert!(
            text.contains("<svg"),
            "{model} 没有返回 SVG（前 120 字符：{}）",
            text.chars().take(120).collect::<String>()
        );
        assert!(text.contains("AIRX"), "{model} 的 SVG 中没有目标文字 AIRX");
    }

    println!("\n全部模型非流式测试通过");
    Ok(())
}

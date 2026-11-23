//! 全功能：结构化输出与工具调用。
//!
//! ```bash
//! cargo run -p airx --example opencode_full
//! ```

mod common;

use airx::prelude::*;
use airx_core::json::Extensions;
use airx_protocol::openai::chat::{JsonSchemaSpec, ResponseFormat};
use serde_json::{Value, json};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = common::load();
    let client = common::client(&env);
    let mut failures = Vec::new();

    for model in &env.models {
        println!("\n=== {model} · 全功能 ===");

        if let Err(error) = structured_output(&client, model).await {
            failures.push(format!("{model} · 结构化输出: {error}"));
        }
        if let Err(error) = tool_calling(&client, model).await {
            failures.push(format!("{model} · 工具调用: {error}"));
        }
    }

    if failures.is_empty() {
        println!("\n全部模型全功能测试通过");
        Ok(())
    } else {
        for failure in &failures {
            eprintln!("FAILED: {failure}");
        }
        Err(std::io::Error::other(failures.join("; ")).into())
    }
}

/// 结构化输出：JSON Schema → 本地合成一张文字 SVG。
async fn structured_output(client: &OpenAi, model: &str) -> Result<(), Box<dyn std::error::Error>> {
    let request = ChatCompletionRequest::builder()
        .model(model)
        .user("为一张纯文字海报生成 JSON：title 一行，lines 两行；中文或英文均可。")
        .response_format(ResponseFormat::JsonSchema {
            json_schema: JsonSchemaSpec {
                name: "poster".to_owned(),
                description: Some("text poster".to_owned()),
                schema: json!({
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "lines": {"type": "array", "items": {"type": "string"}, "minItems": 2, "maxItems": 2}
                    },
                    "required": ["title", "lines"],
                    "additionalProperties": false
                }),
                strict: Some(true),
                extensions: Extensions::new(),
            },
        })
        .max_completion_tokens(1024)
        .build()?;

    let response = client.chat().create(request).await?;
    let text = response.first_content().unwrap_or_default();
    let poster: Value = serde_json::from_str(text)
        .map_err(|error| format!("模型没有返回合法 JSON: {error}; 原文: {text}"))?;

    let title = poster["title"].as_str().unwrap_or("AIRX");
    let lines = poster["lines"]
        .as_array()
        .map(|lines| {
            lines
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut body = String::new();
    for (index, line) in lines.iter().enumerate() {
        body.push_str(&format!(
            r##"<text x="600" y="{}" fill="#d7e3ff" font-size="48" text-anchor="middle" font-family="sans-serif">{}</text>"##,
            360 + index * 70,
            common::xml_escape(line)
        ));
    }
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630"><rect width="1200" height="630" fill="#0b1020"/><text x="600" y="220" fill="#ffffff" font-size="110" font-weight="bold" text-anchor="middle" font-family="sans-serif">{}</text>{}</svg>"##,
        common::xml_escape(title),
        body
    );

    let path = format!("target/opencode-{}-poster.svg", common::slug(model));
    std::fs::write(&path, &svg)?;
    println!("structured: title={title:?} lines={lines:?} saved={path}");
    Ok(())
}

/// 工具调用：先让模型调用工具，再回传结果获取最终回答。
async fn tool_calling(client: &OpenAi, model: &str) -> Result<(), Box<dyn std::error::Error>> {
    let user_prompt = "查询新加坡当前天气，然后用一句话回答。必须调用工具。";
    let tools = [Tool::function(
        "get_weather",
        json!({
            "type": "object",
            "properties": {"city": {"type": "string"}},
            "required": ["city"]
        }),
        Some("获取城市当前天气".to_owned()),
    )];

    let first = ChatCompletionRequest::builder()
        .model(model)
        .user(user_prompt)
        .tools(tools)
        .tool_choice(ToolChoice::auto())
        .max_completion_tokens(512)
        .build()?;

    let response = client.chat().create(first).await?;
    let calls = response
        .choices
        .first()
        .and_then(|choice| choice.message.tool_calls.clone())
        .unwrap_or_default();

    if calls.is_empty() {
        println!(
            "tools: 模型未发起工具调用，直接回答: {:?}",
            response.first_content()
        );
        return Ok(());
    }

    println!(
        "tools: 模型调用 {} 次，第一个参数 = {}",
        calls.len(),
        calls[0].function.arguments
    );

    let second = ChatCompletionRequest::builder()
        .model(model)
        .user(user_prompt)
        .message(ChatMessage::assistant_tool_calls(calls.clone()))
        .message(ChatMessage::tool_result(
            calls[0].id.clone(),
            "新加坡：31°C，湿度 78%，多云",
        ))
        .tools([Tool::function(
            "get_weather",
            json!({
                "type": "object",
                "properties": {"city": {"type": "string"}},
                "required": ["city"]
            }),
            Some("获取城市当前天气".to_owned()),
        )])
        .max_completion_tokens(512)
        .build()?;

    let final_response = client.chat().create(second).await?;
    let text = final_response.first_content().unwrap_or_default();
    println!("tools: 最终回答 = {text:?}");
    if text.is_empty() {
        return Err("工具结果回传后没有最终回答".into());
    }
    Ok(())
}

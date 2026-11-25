//! OpenCode Zen 网关示例的共享工具。
//!
//! 所有配置来自 `.env`（dotenvy 载入）：
//! - `OPENCODE_BASE_URL`
//! - `OPENCODE_API_KEY`
//! - `OPENCODE_MODELS`（逗号分隔）

#![allow(dead_code)]

use std::time::Duration;

use airx::http::{HeaderMap, HeaderName, HeaderValue};
use airx::prelude::*;

/// 让模型输出“文字海报”的提示词（纯 SVG，不依赖外部资源）。
pub const POSTER_PROMPT: &str = "请只输出一个 SVG 图片代码，不要 Markdown 代码块，不要解释。\
尺寸 1200x630，深色背景，居中显示两行大字：第一行 AIRX，第二行 OPENCODE；\
再显示一行小字：protocol-first · no feature loss。\
只使用 <text>、<rect> 等基础元素，不引用外部图片或字体。";

/// 示例环境。
pub struct Env {
    /// 网关基地址。
    pub base_url: String,
    /// API Key。
    pub api_key: String,
    /// 待测试模型列表。
    pub models: Vec<String>,
}

/// 从 `.env` 与进程环境加载配置。
pub fn load() -> Env {
    dotenvy::dotenv().ok();

    let base_url = std::env::var("OPENCODE_BASE_URL")
        .unwrap_or_else(|_| "https://opencode.ai/zen/go/v1".to_owned());
    let api_key =
        std::env::var("OPENCODE_API_KEY").expect("OPENCODE_API_KEY 未设置（参考 .env.example）");
    let models = std::env::var("OPENCODE_MODELS")
        .unwrap_or_else(|_| "hy4-flash-free,deepseek-v4.1-flash".to_owned())
        .split(',')
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_owned)
        .collect();

    Env {
        base_url,
        api_key,
        models,
    }
}

/// 构造到网关的 OpenAI 客户端。
pub fn client(env: &Env) -> OpenAi {
    Client::builder()
        .timeout(Duration::from_secs(180))
        .default_headers(headers())
        .openai()
        .api_key(env.api_key.clone())
        .base_url(env.base_url.clone())
        .build()
        .expect("构建 Client 失败")
}

/// 网关要求的会话 Header。
pub fn headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    let session = format!("airx-example-{}", std::process::id());
    headers.insert(
        HeaderName::from_static("x-opencode-session"),
        HeaderValue::from_str(&session).expect("session header"),
    );
    headers
}

/// 文件名安全的模型标识。
pub fn slug(model: &str) -> String {
    model
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

/// 从模型输出中提取 SVG（容忍 Markdown 代码块或前后解释）。
pub fn extract_svg(text: &str) -> String {
    let trimmed = text.trim();
    let start = trimmed.find("<svg").unwrap_or(0);
    let end = trimmed
        .rfind("</svg>")
        .map(|index| index + "</svg>".len())
        .unwrap_or(trimmed.len());
    trimmed[start..end].trim().to_owned()
}

/// 非流式响应中的 reasoning 文本。
pub fn reasoning_of(response: &ChatCompletionResponse) -> &str {
    response
        .choices
        .first()
        .and_then(|choice| choice.message.extensions.get("reasoning_content"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

/// 把模型输出写入 `target/`，返回 SVG 路径。
pub fn write_svg(model: &str, text: &str) -> std::io::Result<String> {
    let svg = extract_svg(text);
    let path = format!("target/opencode-{}.svg", slug(model));
    let raw_path = format!("target/opencode-{}.txt", slug(model));
    std::fs::write(&path, &svg)?;
    std::fs::write(raw_path, text)?;
    Ok(path)
}

/// 最小 XML 转义。
pub fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

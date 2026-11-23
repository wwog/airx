//! 重试：注入一次 429，验证 RetryTransport 的真实重试路径。
//!
//! ```bash
//! cargo run -p airx --example opencode_retry
//! ```

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use airx::prelude::*;
use airx_core::error::TransportError;
use airx_core::http::{HeaderMap, HeaderValue, HttpRequest, HttpResponse, StatusCode};
use airx_transport::{BoxFuture, ReqwestTransport, Transport};

/// 前 `fail_first` 次调用返回 429，之后透传到真实传输。
struct FlakyTransport {
    inner: ReqwestTransport,
    attempts: Arc<AtomicUsize>,
    fail_first: usize,
}

impl Transport for FlakyTransport {
    fn execute(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst);
        let fail_fast = attempt < self.fail_first;

        Box::pin(async move {
            if fail_fast {
                let mut headers = HeaderMap::new();
                headers.insert("retry-after", HeaderValue::from_static("0"));
                return Ok(HttpResponse::new(
                    StatusCode::TOO_MANY_REQUESTS,
                    headers,
                    br#"{"error":{"message":"injected 429","type":"rate_limit_error"}}"#.as_slice(),
                ));
            }
            self.inner.execute(request).await
        })
    }

    fn execute_stream(
        &self,
        request: HttpRequest,
    ) -> BoxFuture<'_, Result<airx_transport::HttpStreamResponse, TransportError>> {
        self.inner.execute_stream(request)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = common::load();

    for model in &env.models {
        println!("\n=== {model} · 重试 ===");

        let attempts = Arc::new(AtomicUsize::new(0));
        let inner = ReqwestTransport::builder()
            .timeout(Duration::from_secs(180))
            .build()?;
        let flaky = FlakyTransport {
            inner,
            attempts: Arc::clone(&attempts),
            fail_first: 1,
        };

        let client: OpenAi = Client::builder()
            .transport(flaky)
            .retry_policy(
                RetryPolicy::standard()
                    .with_max_retries(2)
                    .with_initial_backoff(Duration::from_millis(10)),
            )
            .default_headers(common::headers())
            .openai()
            .api_key(env.api_key.clone())
            .base_url(env.base_url.clone())
            .build()?;

        let request = ChatCompletionRequest::builder()
            .model(model.clone())
            .user("只回复两个字：重试成功")
            .max_completion_tokens(256)
            .build()?;

        let response = client.chat().create(request).await?;
        let text = response
            .first_content()
            .unwrap_or_default()
            .trim()
            .to_owned();
        let total_attempts = attempts.load(Ordering::SeqCst);

        println!("attempts={total_attempts} text={text:?}");
        assert_eq!(
            total_attempts, 2,
            "{model} 期望 1 次注入失败 + 1 次成功，实际 {total_attempts} 次"
        );
        assert!(!text.is_empty(), "{model} 重试后仍然没有内容");
    }

    println!("\n全部模型重试测试通过（429 + Retry-After，POST 也可重试）");
    Ok(())
}

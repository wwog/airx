//! 重试策略与执行。
//!
//! 设计原则：native 默认 2 次重试（对齐官方 SDK），且按幂等性区分；
//! WASM 暂不支持自动重试，默认关闭。
//!
//! | 情况 | 是否重试 |
//! | --- | --- |
//! | 连接失败（请求未发出） | 任意方法 |
//! | 超时 | 仅幂等方法 |
//! | 429 | 任意方法（请求被拒绝，未执行） |
//! | 5xx | 仅幂等方法 |
//! | 取消 | 不重试 |
//!
//! 流式请求不重试：一旦开始消费事件，重放会破坏语义。
//! “首个事件前重试”留到 M9。
//!
//! [`RetryPolicy`] 是纯数据，所有目标平台可用；执行器 [`RetryTransport`]
//! 依赖 native 定时器，仅在非 WASM 目标提供。

use std::time::Duration;

/// 重试策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct RetryPolicy {
    /// 首次失败后的最大重试次数（不含首次请求）。
    pub max_retries: u32,
    /// 第一次重试前的等待时间，之后按 2 的幂退避。
    pub initial_backoff: Duration,
    /// 单次等待上限。
    pub max_backoff: Duration,
}

impl RetryPolicy {
    /// 禁用重试（逃生开关）。
    pub const fn disabled() -> Self {
        Self {
            max_retries: 0,
            initial_backoff: Duration::from_millis(200),
            max_backoff: Duration::from_secs(8),
        }
    }

    /// 推荐基线：最多 2 次重试，500ms 起、8s 上限。
    pub const fn standard() -> Self {
        Self {
            max_retries: 2,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(8),
        }
    }

    /// 是否启用。
    pub fn is_enabled(&self) -> bool {
        self.max_retries > 0
    }

    /// 设置最大重试次数。
    #[must_use]
    pub const fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// 设置首次退避。
    #[must_use]
    pub const fn with_initial_backoff(mut self, initial_backoff: Duration) -> Self {
        self.initial_backoff = initial_backoff;
        self
    }

    /// 设置退避上限。
    #[must_use]
    pub const fn with_max_backoff(mut self, max_backoff: Duration) -> Self {
        self.max_backoff = max_backoff;
        self
    }

    /// 第 `retry_index` 次重试前的等待时间（0 表示第一次重试）。
    pub fn backoff(&self, retry_index: u32) -> Duration {
        let factor = 1u32.checked_shl(retry_index).unwrap_or(u32::MAX);
        self.initial_backoff
            .saturating_mul(factor)
            .min(self.max_backoff)
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            Self::disabled()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self::standard()
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::time::Duration;

    use airx_core::error::{TransportError, TransportErrorKind};
    use airx_core::http::{HttpRequest, HttpResponse, Method, StatusCode};

    use crate::response::HttpStreamResponse;
    use crate::transport::{BoxFuture, Transport};

    use super::RetryPolicy;

    /// 应用重试策略的传输包装。
    #[derive(Debug, Clone)]
    pub struct RetryTransport<T> {
        inner: T,
        policy: RetryPolicy,
    }

    impl<T> RetryTransport<T> {
        /// 包装传输实现。
        pub fn new(inner: T, policy: RetryPolicy) -> Self {
            Self { inner, policy }
        }

        /// 当前策略。
        pub fn policy(&self) -> RetryPolicy {
            self.policy
        }
    }

    impl<T: Transport> Transport for RetryTransport<T> {
        fn execute(
            &self,
            request: HttpRequest,
        ) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
            Box::pin(async move {
                let mut retries = 0;

                loop {
                    match self.inner.execute(request.clone()).await {
                        Ok(response) => {
                            if retries < self.policy.max_retries
                                && should_retry_status(&request.method, response.status)
                            {
                                let delay = retry_delay(&self.policy, retries, Some(&response));
                                tracing::debug!(
                                    attempt = retries + 1,
                                    delay_ms = delay.as_millis() as u64,
                                    status = response.status.as_u16(),
                                    "airx: retrying after status"
                                );
                                tokio::time::sleep(delay).await;
                                retries += 1;
                                continue;
                            }
                            return Ok(response);
                        }
                        Err(error) => {
                            if retries < self.policy.max_retries
                                && should_retry_error(&request.method, &error)
                            {
                                let delay = retry_delay(&self.policy, retries, None);
                                tracing::debug!(
                                    attempt = retries + 1,
                                    delay_ms = delay.as_millis() as u64,
                                    error = %error,
                                    "airx: retrying after transport error"
                                );
                                tokio::time::sleep(delay).await;
                                retries += 1;
                                continue;
                            }
                            return Err(error);
                        }
                    }
                }
            })
        }

        fn execute_stream(
            &self,
            request: HttpRequest,
        ) -> BoxFuture<'_, Result<HttpStreamResponse, TransportError>> {
            // 流式请求不重试：已消费的事件无法重放。
            self.inner.execute_stream(request)
        }
    }

    fn should_retry_status(method: &Method, status: StatusCode) -> bool {
        if status == StatusCode::TOO_MANY_REQUESTS {
            return true;
        }
        status.is_server_error() && is_idempotent(method)
    }

    fn should_retry_error(method: &Method, error: &TransportError) -> bool {
        match error.kind {
            TransportErrorKind::Connect => true,
            TransportErrorKind::Timeout => is_idempotent(method),
            _ => false,
        }
    }

    pub(super) fn is_idempotent(method: &Method) -> bool {
        matches!(
            method,
            &Method::GET
                | &Method::HEAD
                | &Method::OPTIONS
                | &Method::TRACE
                | &Method::PUT
                | &Method::DELETE
        )
    }

    fn retry_delay(
        policy: &RetryPolicy,
        retry_index: u32,
        response: Option<&HttpResponse>,
    ) -> Duration {
        if let Some(seconds) = response
            .and_then(|response| response.headers.get("retry-after"))
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
        {
            return Duration::from_secs(seconds).min(policy.max_backoff);
        }
        policy.backoff(retry_index)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::RetryTransport;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_and_caps() {
        let policy = RetryPolicy::standard()
            .with_initial_backoff(Duration::from_millis(100))
            .with_max_backoff(Duration::from_millis(250));

        assert_eq!(policy.backoff(0), Duration::from_millis(100));
        assert_eq!(policy.backoff(1), Duration::from_millis(200));
        assert_eq!(policy.backoff(2), Duration::from_millis(250));
        assert_eq!(policy.backoff(64), Duration::from_millis(250));
    }

    #[test]
    fn default_matches_platform_policy() {
        assert!(RetryPolicy::standard().is_enabled());
        assert!(!RetryPolicy::disabled().is_enabled());

        #[cfg(not(target_arch = "wasm32"))]
        assert!(RetryPolicy::default().is_enabled());
        #[cfg(target_arch = "wasm32")]
        assert!(!RetryPolicy::default().is_enabled());
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod native_tests {
    use super::*;
    use crate::mock::MockTransport;
    use crate::transport::Transport;
    use airx_core::error::TransportErrorKind;
    use airx_core::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri};
    use std::sync::Arc;
    use std::time::Duration;

    fn get() -> airx_core::http::HttpRequest {
        airx_core::http::HttpRequest::get("https://example.com/v1".parse::<Uri>().expect("uri"))
    }

    fn post() -> airx_core::http::HttpRequest {
        airx_core::http::HttpRequest::post("https://example.com/v1".parse::<Uri>().expect("uri"))
    }

    fn fast_policy(max_retries: u32) -> RetryPolicy {
        RetryPolicy::disabled()
            .with_max_retries(max_retries)
            .with_initial_backoff(Duration::from_millis(1))
            .with_max_backoff(Duration::from_millis(5))
    }

    #[test]
    fn idempotency_rules() {
        assert!(native::is_idempotent(&Method::GET));
        assert!(native::is_idempotent(&Method::PUT));
        assert!(!native::is_idempotent(&Method::POST));
        assert!(!native::is_idempotent(&Method::PATCH));
    }

    #[tokio::test]
    async fn retries_5xx_for_idempotent_requests() {
        let mock = MockTransport::new();
        mock.push_json(500, "{}");
        mock.push_json(200, "{}");
        let transport = RetryTransport::new(mock.clone(), fast_policy(1));

        let response = transport.execute(get()).await.expect("response");
        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(mock.request_count(), 2);
    }

    #[tokio::test]
    async fn does_not_retry_5xx_for_post() {
        let mock = MockTransport::new();
        mock.push_json(500, "{}");
        let transport = RetryTransport::new(mock.clone(), fast_policy(2));

        let response = transport.execute(post()).await.expect("response");
        assert_eq!(response.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(mock.request_count(), 1);
    }

    #[tokio::test]
    async fn retries_429_for_any_method_and_honors_retry_after() {
        let mock = MockTransport::new();
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", HeaderValue::from_static("0"));
        mock.push_response(429, headers, "{}");
        mock.push_json(200, "{}");

        let transport = RetryTransport::new(mock.clone(), fast_policy(1));
        let response = transport.execute(post()).await.expect("response");

        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(mock.request_count(), 2);
    }

    #[tokio::test]
    async fn retries_timeout_only_for_idempotent_requests() {
        let mock = MockTransport::new();
        mock.push_error(TransportErrorKind::Timeout, "slow");
        mock.push_json(200, "{}");
        let transport = RetryTransport::new(mock.clone(), fast_policy(1));

        assert_eq!(
            transport.execute(get()).await.expect("ok").status,
            StatusCode::OK
        );
        assert_eq!(mock.request_count(), 2);

        let mock = MockTransport::new();
        mock.push_error(TransportErrorKind::Timeout, "slow");
        let transport = RetryTransport::new(mock.clone(), fast_policy(1));
        let error = transport.execute(post()).await.expect_err("timeout");
        assert_eq!(error.kind, TransportErrorKind::Timeout);
        assert_eq!(mock.request_count(), 1);
    }

    #[tokio::test]
    async fn exhausted_retries_return_last_outcome() {
        let mock = MockTransport::new();
        mock.push_json(500, "{}");
        mock.push_json(500, "{}");
        let transport = RetryTransport::new(mock.clone(), fast_policy(1));

        let response = transport.execute(get()).await.expect("response");
        assert_eq!(response.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(mock.request_count(), 2);
    }

    #[tokio::test]
    async fn can_be_shared_through_arc() {
        let mock = MockTransport::new();
        mock.push_json(200, "{}");
        let shared = Arc::new(RetryTransport::new(mock.clone(), fast_policy(0)));

        let response = shared.execute(get()).await.expect("response");
        assert_eq!(response.status, StatusCode::OK);
    }
}

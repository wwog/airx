//! 重试策略与执行。
//!
//! 设计原则：native 默认 2 次重试（对齐官方 SDK），且按幂等性区分；
//! WASM 暂不支持自动重试，默认关闭。
//!
//! | 情况 | 是否重试 |
//! | --- | --- |
//! | 连接失败（请求未发出） | 任意方法 |
//! | 超时 | 仅幂等方法 |
//! | 429（非配额类） | 任意方法（请求被拒绝，未执行） |
//! | 429 配额类（`insufficient_quota` / 余额耗尽 / `*_spend_limit_*`） | 不重试（重试不会恢复访问） |
//! | 5xx | 仅幂等方法 |
//! | 取消 | 不重试 |
//!
//! `Retry-After` 同时支持整数秒与 HTTP-date；指数退避默认带 ±20% jitter，
//! 避免多实例同步重试。
//!
//! 流式请求不重试：一旦开始消费事件，重放会破坏语义。
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
    /// 是否为指数退避加入 ±20% jitter（`Retry-After` 不受影响）。
    pub jitter: bool,
}

impl RetryPolicy {
    /// 禁用重试（逃生开关）。
    pub const fn disabled() -> Self {
        Self {
            max_retries: 0,
            initial_backoff: Duration::from_millis(200),
            max_backoff: Duration::from_secs(8),
            jitter: true,
        }
    }

    /// 推荐基线：最多 2 次重试，500ms 起、8s 上限、带 jitter。
    pub const fn standard() -> Self {
        Self {
            max_retries: 2,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(8),
            jitter: true,
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

    /// 开关 jitter。
    #[must_use]
    pub const fn with_jitter(mut self, jitter: bool) -> Self {
        self.jitter = jitter;
        self
    }

    /// 第 `retry_index` 次重试前的等待时间（0 表示第一次重试）。
    ///
    /// 返回未加 jitter 的确定性退避；运行时等待见 `jittered_backoff`。
    pub fn backoff(&self, retry_index: u32) -> Duration {
        let factor = 1u32.checked_shl(retry_index).unwrap_or(u32::MAX);
        self.initial_backoff
            .saturating_mul(factor)
            .min(self.max_backoff)
    }
}

/// 为指数退避加入 ±20% jitter。
///
/// `seed` 由调用方提供（运行时使用当前时间），便于测试确定性。
#[cfg_attr(target_arch = "wasm32", allow(dead_code))] // WASM 不自动重试
pub(crate) fn jittered_backoff(policy: &RetryPolicy, retry_index: u32, seed: u64) -> Duration {
    let base = policy.backoff(retry_index);
    if !policy.jitter {
        return base;
    }
    let millis = base.as_millis() as u64;
    let span = millis / 5;
    if span == 0 {
        return base;
    }
    let offset = seed % (span * 2 + 1);
    Duration::from_millis(millis - span + offset).min(policy.max_backoff)
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
                                && should_retry_response(&request.method, &response)
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

    fn should_retry_response(method: &Method, response: &HttpResponse) -> bool {
        if response.status == StatusCode::TOO_MANY_REQUESTS {
            // 配额类 429 重试不会恢复访问，直接返回给调用方。
            return !is_quota_exhausted(&response.body);
        }
        response.status.is_server_error() && is_idempotent(method)
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

    /// 429 是否属于“配额耗尽”类（重试无意义）。
    pub(super) fn is_quota_exhausted(body: &[u8]) -> bool {
        let text = String::from_utf8_lossy(body);
        for field in ["code", "type"] {
            if let Some(value) = json_field_value(&text, field) {
                if is_quota_code(value) {
                    return true;
                }
            }
        }
        false
    }

    fn is_quota_code(code: &str) -> bool {
        code == "insufficient_quota"
            || code == "credit_balance_exhausted"
            || code.contains("_spend_limit_")
            || code.ends_with("_spend_limit")
    }

    /// 从 JSON 文本中取第一个 `"field": "value"` 的字符串值。
    fn json_field_value<'a>(text: &'a str, field: &str) -> Option<&'a str> {
        let needle = format!("\"{field}\"");
        let mut rest = text;
        while let Some(position) = rest.find(&needle) {
            let after_key = &rest[position + needle.len()..];
            if let Some(after_colon) = after_key.trim_start().strip_prefix(':') {
                if let Some(after_quote) = after_colon.trim_start().strip_prefix('"') {
                    if let Some(end) = after_quote.find('"') {
                        return Some(&after_quote[..end]);
                    }
                }
            }
            rest = &rest[position + needle.len()..];
        }
        None
    }

    fn retry_delay(
        policy: &RetryPolicy,
        retry_index: u32,
        response: Option<&HttpResponse>,
    ) -> Duration {
        if let Some(duration) = response
            .and_then(|response| response.headers.get("retry-after"))
            .and_then(|value| value.to_str().ok())
            .and_then(parse_retry_after)
        {
            return duration.min(policy.max_backoff);
        }
        super::jittered_backoff(policy, retry_index, jitter_seed())
    }

    /// 解析 `Retry-After`：整数秒或 HTTP-date（IMF-fixdate）。
    pub(super) fn parse_retry_after(value: &str) -> Option<Duration> {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        if value.bytes().all(|byte| byte.is_ascii_digit()) {
            return value.parse::<u64>().ok().map(Duration::from_secs);
        }
        let target = parse_http_date(value)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs();
        Some(Duration::from_secs(target.saturating_sub(now)))
    }

    /// 解析 IMF-fixdate（`Sun, 06 Nov 1994 08:49:37 GMT`）。
    pub(super) fn parse_http_date(value: &str) -> Option<u64> {
        let mut parts = value.split_whitespace();
        let _weekday = parts.next()?;
        let day: u32 = parts.next()?.parse().ok()?;
        let month = month_number(parts.next()?)?;
        let year: i64 = parts.next()?.parse().ok()?;
        // HTTP-date 只有四位年份；拒绝异常值，避免后续日历运算溢出。
        if !(1970..=9999).contains(&year) {
            return None;
        }
        let time = parts.next()?;
        let zone = parts.next()?;
        if !zone.eq_ignore_ascii_case("GMT") {
            return None;
        }

        let mut clock = time.split(':');
        let hour: u64 = clock.next()?.parse().ok()?;
        let minute: u64 = clock.next()?.parse().ok()?;
        let second: u64 = clock.next()?.parse().ok()?;
        if hour > 23 || minute > 59 || second > 60 {
            return None;
        }

        let days = days_from_civil(year, month, day);
        let days = u64::try_from(days).ok()?;
        days.checked_mul(86_400)?
            .checked_add(hour * 3_600 + minute * 60 + second)
    }

    fn month_number(name: &str) -> Option<u32> {
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        MONTHS
            .iter()
            .position(|month| month.eq_ignore_ascii_case(name))
            .map(|index| index as u32 + 1)
    }

    /// 公历日期 → 1970-01-01 起的天数（Howard Hinnant 算法）。
    fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
        let max_day = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
                if leap { 29 } else { 28 }
            }
            _ => return -1,
        };
        if day == 0 || day > max_day {
            return -1;
        }
        let year = if month <= 2 { year - 1 } else { year };
        let era = if year >= 0 { year } else { year - 399 } / 400;
        let year_of_era = year - era * 400;
        let month = i64::from(month);
        let day_of_year =
            (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }

    fn jitter_seed() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs() ^ u64::from(elapsed.subsec_nanos()))
            .unwrap_or_default()
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
    fn jitter_stays_within_twenty_percent_and_caps() {
        let policy = RetryPolicy::standard()
            .with_initial_backoff(Duration::from_millis(1000))
            .with_max_backoff(Duration::from_millis(5000));

        for seed in 0..500u64 {
            let delay = jittered_backoff(&policy, 0, seed);
            assert!(delay >= Duration::from_millis(800), "{delay:?}");
            assert!(delay <= Duration::from_millis(1200), "{delay:?}");
        }

        // 上限生效。
        for seed in 0..100u64 {
            let delay = jittered_backoff(&policy, 10, seed);
            assert!(delay <= Duration::from_millis(5000));
        }

        // 关闭 jitter 后完全确定。
        let deterministic = policy.with_jitter(false);
        assert_eq!(
            jittered_backoff(&deterministic, 1, 12345),
            deterministic.backoff(1)
        );
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

    #[tokio::test]
    async fn quota_429_is_not_retried() {
        for code in [
            "insufficient_quota",
            "credit_balance_exhausted",
            "project_spend_limit_exceeded",
        ] {
            let mock = MockTransport::new();
            mock.push_json(
                429,
                format!(
                    r#"{{"error":{{"message":"quota","type":"insufficient_quota","code":"{code}"}}}}"#
                ),
            );
            mock.push_json(200, "{}");
            let transport = RetryTransport::new(mock.clone(), fast_policy(2));

            let response = transport.execute(post()).await.expect("response");
            assert_eq!(response.status, StatusCode::TOO_MANY_REQUESTS, "{code}");
            assert_eq!(mock.request_count(), 1, "{code}");
        }
    }

    #[tokio::test]
    async fn plain_429_still_retries() {
        let mock = MockTransport::new();
        mock.push_json(
            429,
            r#"{"error":{"message":"slow down","type":"rate_limit_error","code":"rate_limit_exceeded"}}"#,
        );
        mock.push_json(200, "{}");
        let transport = RetryTransport::new(mock.clone(), fast_policy(1));

        let response = transport.execute(post()).await.expect("response");
        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(mock.request_count(), 2);
    }

    #[test]
    fn retry_after_supports_http_dates() {
        assert_eq!(
            native::parse_retry_after("3").expect("seconds"),
            Duration::from_secs(3)
        );
        assert_eq!(
            native::parse_retry_after("Thu, 01 Jan 1970 00:00:10 GMT").expect("past date"),
            Duration::from_secs(0)
        );

        // 远未来日期：应得到一个很大的正值。
        let far_future =
            native::parse_retry_after("Fri, 01 Jan 2100 00:00:00 GMT").expect("future date");
        assert!(far_future > Duration::from_secs(1_000_000_000));

        assert!(native::parse_retry_after("not a date").is_none());
        assert!(native::parse_retry_after("Fri, 01 Jan 2100 00:00:00 PST").is_none());
    }

    #[test]
    fn http_date_parsing_handles_epoch_anchors() {
        assert_eq!(
            native::parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT"),
            Some(0)
        );
        assert_eq!(
            native::parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT"),
            Some(784_111_777)
        );
        assert_eq!(
            native::parse_http_date("Sun, 31 Feb 1994 08:49:37 GMT"),
            None
        );
    }

    #[test]
    fn absurd_http_dates_are_rejected_without_overflow() {
        assert_eq!(
            native::parse_http_date("Fri, 01 Jan 1000000000000 00:00:00 GMT"),
            None
        );
        assert_eq!(
            native::parse_http_date("Fri, 01 Jan 9999 00:00:00 GMT"),
            Some(253_370_764_800)
        );
        assert_eq!(
            native::parse_http_date("Fri, 01 Jan 1969 00:00:00 GMT"),
            None
        );
    }

    #[test]
    fn quota_body_classification() {
        assert!(native::is_quota_exhausted(
            br#"{"error":{"code":"insufficient_quota"}}"#
        ));
        assert!(native::is_quota_exhausted(
            br#"{"error":{"type":"credit_balance_exhausted"}}"#
        ));
        assert!(native::is_quota_exhausted(
            br#"{"error":{"code":"team_spend_limit_reached"}}"#
        ));
        assert!(!native::is_quota_exhausted(b"{}"));
        assert!(!native::is_quota_exhausted(
            br#"{"error":{"code":"rate_limit_exceeded"}}"#
        ));
    }
}

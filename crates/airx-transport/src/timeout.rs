//! 单次请求超时。

use std::future::Future;
use std::time::Duration;

use airx_core::error::{TransportError, TransportErrorKind};

/// 在指定时间内等待 future。
///
/// Native 使用 `tokio::time`；WASM 暂不支持单次超时（返回明确错误）。
#[cfg(not(target_arch = "wasm32"))]
pub async fn with_timeout<F: Future>(
    duration: Duration,
    future: F,
) -> Result<F::Output, TransportError> {
    match tokio::time::timeout(duration, future).await {
        Ok(output) => Ok(output),
        Err(_) => Err(TransportError::new(
            TransportErrorKind::Timeout,
            format!("request timed out after {duration:?}"),
        )),
    }
}

/// 在指定时间内等待 future（WASM：暂不支持）。
#[cfg(target_arch = "wasm32")]
pub async fn with_timeout<F: Future>(
    _duration: Duration,
    _future: F,
) -> Result<F::Output, TransportError> {
    Err(TransportError::new(
        TransportErrorKind::Other,
        "WASM 目标暂不支持单次请求超时",
    ))
}

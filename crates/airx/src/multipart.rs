//! 门面层的 multipart boundary 生成。

use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 生成进程内唯一的 multipart boundary。
pub(crate) fn boundary() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    #[cfg(target_arch = "wasm32")]
    let nanos = 0u128;

    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("airx-{}-{}-{}", std::process::id(), nanos, counter)
}

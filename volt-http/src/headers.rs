//! Helpers for reading response headers.

use std::time::{Duration, SystemTime};

use wreq::header::{HeaderMap, RETRY_AFTER};

/// Returns the wait the `Retry-After` header asks for, given in seconds or as an HTTP date. A date
/// already past gives zero, and a missing or unreadable header gives `None`.
pub fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if let Ok(seconds) = raw.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let deadline = httpdate::parse_http_date(raw).ok()?;
    Some(
        deadline
            .duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO),
    )
}

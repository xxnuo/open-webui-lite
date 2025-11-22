use chrono::Utc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Returns current timestamp in seconds (Unix epoch)
#[allow(dead_code)]
pub fn current_timestamp_seconds() -> i64 {
    Utc::now().timestamp()
}

/// Returns current timestamp in milliseconds
#[allow(dead_code)]
pub fn current_timestamp_millis() -> i64 {
    Utc::now().timestamp_millis()
}

/// Returns current timestamp in nanoseconds (compatible with Python backend's time.time_ns())
pub fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos() as i64
}

/// Returns current timestamp in nanoseconds (alias for current_timestamp)
pub fn current_timestamp_nanos() -> i64 {
    current_timestamp()
}

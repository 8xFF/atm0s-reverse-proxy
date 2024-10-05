use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> u64 {
    // Get the current time
    let now = SystemTime::now();

    // Calculate the duration since the UNIX epoch
    let duration = now.duration_since(UNIX_EPOCH).expect("Time went backwards");

    // Return the total milliseconds
    duration.as_millis() as u64
}

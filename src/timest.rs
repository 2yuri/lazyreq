use std::time::{SystemTime, UNIX_EPOCH};

pub fn get_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn is_older_than(timestamp: u64) -> bool {
    let current_timestamp = get_timestamp();
    current_timestamp > timestamp
}

pub fn add_minutes(timestamp: u64, minutes: u64) -> u64 {
    timestamp + (minutes * 60)
}

pub fn add_seconds(timestamp: u64, seconds: u64) -> u64 {
    timestamp + seconds
}

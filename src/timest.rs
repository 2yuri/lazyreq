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

pub fn add_seconds(timestamp: u64, seconds: u64) -> u64 {
    timestamp + seconds
}

/// Unix timestamp → `YYYY-MM-DD HH:MM:SS` (UTC), without pulling in chrono.
pub fn format_timestamp(timestamp: u64) -> String {
    let days = (timestamp / 86_400) as i64;
    let secs = timestamp % 86_400;

    // Howard Hinnant's civil_from_days algorithm.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe as i64 + era * 400 + if month <= 2 { 1 } else { 0 };

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year,
        month,
        day,
        secs / 3_600,
        (secs % 3_600) / 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_timestamps() {
        assert_eq!(format_timestamp(0), "1970-01-01 00:00:00");
        assert_eq!(format_timestamp(951_827_696), "2000-02-29 12:34:56"); // leap day
        assert_eq!(format_timestamp(1_783_987_601), "2026-07-14 00:06:41");
        assert_eq!(format_timestamp(4_102_444_799), "2099-12-31 23:59:59");
    }

    #[test]
    fn expiry_helpers() {
        assert_eq!(add_seconds(100, 30), 130);
        assert!(is_older_than(get_timestamp() - 10));
        assert!(!is_older_than(get_timestamp() + 10));
    }
}

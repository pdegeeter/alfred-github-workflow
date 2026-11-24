//! Timestamps from the GitHub API (`2026-11-24T16:54:22Z`) and the relative
//! ages shown in subtitles. The clock is always passed in.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch of `now`.
pub fn unix(now: SystemTime) -> i64 {
    match now.duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    }
}

/// Parse a UTC ISO 8601 timestamp as GitHub returns it
/// (`YYYY-MM-DDTHH:MM:SSZ`, optional fractional seconds) into Unix seconds.
pub fn parse_iso8601(value: &str) -> Option<i64> {
    let value = value.strip_suffix('Z')?;
    let (date, time) = value.split_once('T')?;
    let time = time.split('.').next()?;

    let mut date_parts = date.splitn(3, '-').map(str::parse::<i64>);
    let year = date_parts.next()?.ok()?;
    let month = date_parts.next()?.ok()?;
    let day = date_parts.next()?.ok()?;

    let mut time_parts = time.splitn(3, ':').map(str::parse::<i64>);
    let hour = time_parts.next()?.ok()?;
    let minute = time_parts.next()?.ok()?;
    let second = time_parts.next()?.ok()?;

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }

    let days = days_from_civil(year, month, day);

    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146_097 + day_of_era - 719_468
}

/// A compact age such as `just now`, `5m ago`, `3h ago`, `2d ago`,
/// `4mo ago` or `1y ago`.
pub fn ago(then: i64, now: i64) -> String {
    let seconds = (now - then).max(0);

    let (value, unit) = match seconds {
        0..=59 => return "just now".to_owned(),
        60..=3_599 => (seconds / 60, "m"),
        3_600..=86_399 => (seconds / 3_600, "h"),
        86_400..=2_591_999 => (seconds / 86_400, "d"),
        2_592_000..=31_535_999 => (seconds / 2_592_000, "mo"),
        _ => (seconds / 31_536_000, "y"),
    };

    format!("{value}{unit} ago")
}

/// Age of an ISO 8601 timestamp, or an empty string when it does not parse.
pub fn ago_iso(value: &str, now: i64) -> String {
    parse_iso8601(value)
        .map(|then| ago(then, now))
        .unwrap_or_default()
}

/// A duration such as `45s`, `3m 12s` or `1h 05m`.
pub fn duration(seconds: i64) -> String {
    let seconds = seconds.max(0);

    if seconds < 60 {
        return format!("{seconds}s");
    }

    if seconds < 3_600 {
        return format!("{}m {:02}s", seconds / 60, seconds % 60);
    }

    format!("{}h {:02}m", seconds / 3_600, (seconds % 3_600) / 60)
}

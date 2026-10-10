//! Unix-time formatting for human output. Always UTC, so output does not
//! depend on the machine's time zone.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `2026-09-26 14:03:11 UTC`
pub fn utc(ts: i64) -> String {
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

/// `2026-09-26`
pub fn utc_date(ts: i64) -> String {
    utc(ts)[..10].to_owned()
}

/// `2026-09-26 14:03 UTC`
pub fn utc_minute(ts: i64) -> String {
    let full = utc(ts);
    format!("{} UTC", &full[..16])
}

/// `3 minutes ago`; `just now` if the clock is behind the timestamp.
pub fn ago(ts: i64, now: i64) -> String {
    let d = now - ts;
    if d < 0 {
        return "just now".into();
    }
    let (n, unit) = match d {
        0..60 => (d, "second"),
        60..3_600 => (d / 60, "minute"),
        3_600..86_400 => (d / 3_600, "hour"),
        86_400..2_592_000 => (d / 86_400, "day"),
        2_592_000..31_536_000 => (d / 2_592_000, "month"),
        _ => (d / 31_536_000, "year"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

/// Unix seconds from `1700000000` or a UTC date `2024-01-31` (its first
/// second).
pub fn parse_unix_or_date(s: &str) -> Option<i64> {
    let s = s.trim();
    if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
        return s.parse().ok();
    }
    let mut parts = s.splitn(3, '-');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    if y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return None;
    }
    let (y, m, d): (i64, u32, u32) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    if !(1..=12).contains(&m) || d == 0 || d > days_in_month(y, m) {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400)
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// (year, month, day) to days since 1970-01-01. Howard Hinnant's algorithm.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
/// Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_utc() {
        assert_eq!(utc(0), "1970-01-01 00:00:00 UTC");
        // Bitcoin genesis block.
        assert_eq!(utc(1_231_006_505), "2009-01-03 18:15:05 UTC");
        assert_eq!(utc(951_782_400), "2000-02-29 00:00:00 UTC");
    }

    #[test]
    fn formats_dates_and_minutes() {
        assert_eq!(utc_date(1_790_380_800), "2026-09-26");
        assert_eq!(utc_minute(1_790_481_600), "2026-09-27 04:00 UTC");
    }

    #[test]
    fn parses_dates_and_unix_times() {
        assert_eq!(parse_unix_or_date("1700000000"), Some(1_700_000_000));
        assert_eq!(parse_unix_or_date("1970-01-01"), Some(0));
        assert_eq!(parse_unix_or_date("2024-02-29"), Some(1_709_164_800));
        assert_eq!(parse_unix_or_date("2023-02-29"), None);
        assert_eq!(parse_unix_or_date("2024-1-01"), None);
        assert_eq!(parse_unix_or_date("yesterday"), None);
    }

    #[test]
    fn formats_relative_time() {
        assert_eq!(ago(100, 100), "0 seconds ago");
        assert_eq!(ago(0, 1), "1 second ago");
        assert_eq!(ago(0, 180), "3 minutes ago");
        assert_eq!(ago(0, 7_200), "2 hours ago");
        assert_eq!(ago(0, 86_400), "1 day ago");
        assert_eq!(ago(10, 0), "just now");
    }
}

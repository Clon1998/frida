//! Built-in `{{$name}}` generators (PLAN.md §5.3). Each occurrence gets a fresh value.

use std::time::{SystemTime, UNIX_EPOCH};

/// Names (without `$`) of all dynamic variables, for docs and autocompletion.
pub const DYNAMIC_VARIABLES: &[&str] = &[
    "uuid",
    "randomUUID",
    "timestamp",
    "isoTimestamp",
    "randomInt",
    "randomEmail",
    "randomAlphaNumeric",
];

const ALPHANUMERIC: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// A fresh value for `$name` (pass the name without `$`), or `None` for unknown names.
pub fn generate(name: &str) -> Option<String> {
    Some(match name {
        "uuid" | "randomUUID" => uuid::Uuid::new_v4().to_string(),
        "timestamp" => unix_seconds().to_string(),
        "isoTimestamp" => iso_timestamp(unix_millis()),
        // Postman and Bruno use 0..=1000.
        "randomInt" => (random_u32() % 1001).to_string(),
        "randomEmail" => format!("{}@example.com", random_alphanumeric(10).to_lowercase()),
        "randomAlphaNumeric" => random_alphanumeric(1),
        _ => return None,
    })
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn unix_seconds() -> u128 {
    unix_millis() / 1000
}

fn random_u32() -> u32 {
    getrandom::u32().unwrap_or(4) // chosen by fair dice roll, if the OS RNG ever fails
}

fn random_alphanumeric(len: usize) -> String {
    (0..len)
        .map(|_| ALPHANUMERIC[random_u32() as usize % ALPHANUMERIC.len()] as char)
        .collect()
}

/// Formats milliseconds since the Unix epoch as `2026-10-15T20:07:13.306Z` (UTC).
///
/// Written by hand to avoid a date library for one format: days since 1970 are converted to a
/// calendar date with Howard Hinnant's well-known `civil_from_days` algorithm.
pub fn iso_timestamp(unix_millis: u128) -> String {
    let millis = (unix_millis % 1000) as u32;
    let total_seconds = (unix_millis / 1000) as i64;
    let days = total_seconds.div_euclid(86_400);
    let seconds_of_day = total_seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        seconds_of_day / 3600,
        (seconds_of_day % 3600) / 60,
        seconds_of_day % 60
    )
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_timestamps_match_known_dates() {
        assert_eq!(iso_timestamp(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_timestamp(951_782_400_000), "2000-02-29T00:00:00.000Z");
        assert_eq!(iso_timestamp(1_791_986_833_306), "2026-10-14T14:07:13.306Z");
    }

    #[test]
    fn every_dynamic_variable_generates_a_value() {
        for name in DYNAMIC_VARIABLES {
            assert!(generate(name).is_some_and(|v| !v.is_empty()), "{name}");
        }
        assert_eq!(generate("nope"), None);
    }

    #[test]
    fn values_have_the_expected_shape() {
        assert_eq!(generate("uuid").unwrap().len(), 36);
        assert!(generate("randomInt").unwrap().parse::<u32>().unwrap() <= 1000);
        assert!(generate("randomEmail").unwrap().ends_with("@example.com"));
        assert_eq!(generate("randomAlphaNumeric").unwrap().len(), 1);
        assert_ne!(generate("uuid"), generate("uuid"));
    }
}

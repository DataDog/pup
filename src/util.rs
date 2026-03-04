use anyhow::{bail, Result};
use chrono::Utc;
use regex::Regex;

/// Parses a relative duration string like "1h", "30m", "7d" into milliseconds.
///
/// Supported formats:
///   - Short: "30s", "10m", "1h", "7d", "1w"
///   - Long: "5min", "5minutes", "2hours", "3days", "1week"
///   - With spaces: "5 minutes", "2 hours"
///   - Leading minus is stripped: "-5m" → same as "5m"
fn parse_relative_duration_millis(input: &str) -> Result<i64> {
    let stripped = input.trim_start_matches('-').trim();

    let re = Regex::new(
        r"(?i)^(\d+)\s*(s|sec|secs|second|seconds|m|min|mins|minute|minutes|h|hr|hrs|hour|hours|d|day|days|w|week|weeks)$",
    )
    .unwrap();

    if let Some(caps) = re.captures(stripped) {
        let num: i64 = caps[1].parse()?;
        let unit = caps[2].to_lowercase();
        let seconds = match unit.as_str() {
            "s" | "sec" | "secs" | "second" | "seconds" => num,
            "m" | "min" | "mins" | "minute" | "minutes" => num * 60,
            "h" | "hr" | "hrs" | "hour" | "hours" => num * 3600,
            "d" | "day" | "days" => num * 86400,
            "w" | "week" | "weeks" => num * 7 * 86400,
            _ => bail!("unknown time unit: {}", unit),
        };
        return Ok(seconds * 1000);
    }

    bail!("unable to parse duration: {input:?}")
}

/// Parses a time string into Unix milliseconds.
///
/// Supported formats:
///   - "now" (case-insensitive)
///   - Relative short: "1h", "30m", "7d", "5s", "1w"
///   - Relative long: "5min", "5mins", "5minute", "5minutes", "2hr", "2hours", "3days", "1week"
///   - With spaces: "5 minutes", "2 hours"
///   - With leading minus: "-5m", "-2h"
///   - Unix timestamp (all digits, assumed milliseconds)
///   - RFC3339: "2024-01-01T00:00:00Z"
///
/// All relative times are interpreted as "ago from now".
/// Returns second-aligned milliseconds (Unix seconds * 1000) to match Go behavior.
pub fn parse_time_to_unix_millis(input: &str) -> Result<i64> {
    let input = input.trim();

    if input.eq_ignore_ascii_case("now") {
        return Ok(now_millis());
    }

    // Unix timestamp (all digits)
    if !input.is_empty() && input.chars().all(|c| c.is_ascii_digit()) {
        return Ok(input.parse()?);
    }

    // RFC3339 timestamp
    if input.contains('T') {
        let dt = chrono::DateTime::parse_from_rfc3339(input)?;
        return Ok(dt.timestamp() * 1000);
    }

    // Relative time
    let millis = parse_relative_duration_millis(input).map_err(|_| {
        anyhow::anyhow!(
            "unable to parse time: {input:?}\n\
             Expected: now, 1h, 30m, 7d, 5minutes, RFC3339, or Unix timestamp"
        )
    })?;
    Ok(now_millis() - millis)
}

/// Convenience: parse to Unix seconds.
pub fn parse_time_to_unix(input: &str) -> Result<i64> {
    Ok(parse_time_to_unix_millis(input)? / 1000)
}

/// Parses a human-readable duration string into milliseconds.
///
/// Unlike `parse_time_to_unix_millis`, this does **not** subtract from the
/// current time — it returns the raw duration value in milliseconds.
///
/// Supported formats:
///   - "now" → 0
///   - Short: "30s", "10m", "1h", "7d", "1w"
///   - Long: "5min", "5minutes", "2hours", "3days", "1week"
///   - With spaces: "5 minutes", "2 hours"
///   - With leading minus (stripped): "-5m" → same as "5m"
pub fn parse_duration_to_millis(input: &str) -> Result<i64> {
    let input = input.trim();

    if input.eq_ignore_ascii_case("now") {
        return Ok(0);
    }

    parse_relative_duration_millis(input).map_err(|_| {
        anyhow::anyhow!(
            "unable to parse duration: {input:?}\n\
             Expected: now, 1h, 30m, 7d, 5minutes, etc."
        )
    })
}

pub fn now_millis() -> i64 {
    Utc::now().timestamp() * 1000
}

/// Read a JSON file and deserialize into the specified type.
/// Used by create/update commands that accept `--file` input.
pub fn read_json_file<T: serde::de::DeserializeOwned>(path: &str) -> Result<T> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read file {path:?}: {e}"))?;
    serde_json::from_str(&contents)
        .map_err(|e| anyhow::anyhow!("failed to parse JSON from {path:?}: {e}"))
}

/// Parses a UUID string, returning a descriptive error if invalid.
pub fn parse_uuid(id: &str, label: &str) -> anyhow::Result<uuid::Uuid> {
    uuid::Uuid::parse_str(id).map_err(|e| anyhow::anyhow!("invalid {label} UUID '{id}': {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_now() {
        let ms = parse_time_to_unix_millis("now").unwrap();
        let diff = (Utc::now().timestamp() * 1000 - ms).abs();
        assert!(diff < 2000, "now should be within 2s: diff={diff}ms");
    }

    #[test]
    fn test_now_case_insensitive() {
        assert!(parse_time_to_unix_millis("NOW").is_ok());
        assert!(parse_time_to_unix_millis("Now").is_ok());
    }

    #[test]
    fn test_relative_short() {
        let ms = parse_time_to_unix_millis("1h").unwrap();
        let expected = (Utc::now().timestamp() - 3600) * 1000;
        assert!((ms - expected).abs() < 2000);
    }

    #[test]
    fn test_relative_long() {
        let ms = parse_time_to_unix_millis("5minutes").unwrap();
        let expected = (Utc::now().timestamp() - 300) * 1000;
        assert!((ms - expected).abs() < 2000);
    }

    #[test]
    fn test_relative_with_spaces() {
        let ms = parse_time_to_unix_millis("5 minutes").unwrap();
        let expected = (Utc::now().timestamp() - 300) * 1000;
        assert!((ms - expected).abs() < 2000);
    }

    #[test]
    fn test_relative_with_minus() {
        let ms = parse_time_to_unix_millis("-30m").unwrap();
        let expected = (Utc::now().timestamp() - 1800) * 1000;
        assert!((ms - expected).abs() < 2000);
    }

    #[test]
    fn test_unix_timestamp() {
        let ms = parse_time_to_unix_millis("1700000000000").unwrap();
        assert_eq!(ms, 1700000000000);
    }

    #[test]
    fn test_rfc3339() {
        let ms = parse_time_to_unix_millis("2024-01-01T00:00:00Z").unwrap();
        assert_eq!(ms, 1704067200000);
    }

    #[test]
    fn test_invalid() {
        assert!(parse_time_to_unix_millis("invalid").is_err());
        assert!(parse_time_to_unix_millis("").is_err());
    }

    #[test]
    fn test_parse_time_to_unix_returns_seconds() {
        let secs = parse_time_to_unix("1700000000000").unwrap();
        assert_eq!(secs, 1700000000);
    }

    #[test]
    fn test_parse_time_to_unix_now() {
        let secs = parse_time_to_unix("now").unwrap();
        let expected = Utc::now().timestamp();
        assert!((secs - expected).abs() < 2);
    }

    #[test]
    fn test_relative_days() {
        let ms = parse_time_to_unix_millis("7d").unwrap();
        let expected = (Utc::now().timestamp() - 7 * 86400) * 1000;
        assert!((ms - expected).abs() < 2000);
    }

    #[test]
    fn test_relative_weeks() {
        let ms = parse_time_to_unix_millis("1w").unwrap();
        let expected = (Utc::now().timestamp() - 7 * 86400) * 1000;
        assert!((ms - expected).abs() < 2000);
    }

    #[test]
    fn test_relative_seconds() {
        let ms = parse_time_to_unix_millis("30s").unwrap();
        let expected = (Utc::now().timestamp() - 30) * 1000;
        assert!((ms - expected).abs() < 2000);
    }

    #[test]
    fn test_relative_long_hours() {
        let ms = parse_time_to_unix_millis("2hours").unwrap();
        let expected = (Utc::now().timestamp() - 7200) * 1000;
        assert!((ms - expected).abs() < 2000);
    }

    #[test]
    fn test_relative_long_days() {
        let ms = parse_time_to_unix_millis("3days").unwrap();
        let expected = (Utc::now().timestamp() - 3 * 86400) * 1000;
        assert!((ms - expected).abs() < 2000);
    }

    #[test]
    fn test_relative_long_weeks() {
        let ms = parse_time_to_unix_millis("1week").unwrap();
        let expected = (Utc::now().timestamp() - 7 * 86400) * 1000;
        assert!((ms - expected).abs() < 2000);
    }

    #[test]
    fn test_read_json_file_missing() {
        let result: Result<serde_json::Value> = read_json_file("/tmp/__pup_nonexistent__.json");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("failed to read"));
    }

    #[test]
    fn test_read_json_file_invalid_json() {
        let path = std::env::temp_dir().join("__pup_test_invalid__.json");
        std::fs::write(&path, "not json").unwrap();
        let result: Result<serde_json::Value> = read_json_file(path.to_str().unwrap());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("failed to parse"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_read_json_file_valid() {
        let path = std::env::temp_dir().join("__pup_test_valid__.json");
        std::fs::write(&path, r#"{"name": "test"}"#).unwrap();
        let result: Result<serde_json::Value> = read_json_file(path.to_str().unwrap());
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["name"], "test");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_duration_10m() {
        assert_eq!(parse_duration_to_millis("10m").unwrap(), 600_000);
    }

    #[test]
    fn test_duration_1h() {
        assert_eq!(parse_duration_to_millis("1h").unwrap(), 3_600_000);
    }

    #[test]
    fn test_duration_30s() {
        assert_eq!(parse_duration_to_millis("30s").unwrap(), 30_000);
    }

    #[test]
    fn test_duration_7d() {
        assert_eq!(parse_duration_to_millis("7d").unwrap(), 604_800_000);
    }

    #[test]
    fn test_duration_5minutes() {
        assert_eq!(parse_duration_to_millis("5minutes").unwrap(), 300_000);
    }

    #[test]
    fn test_duration_now() {
        assert_eq!(parse_duration_to_millis("now").unwrap(), 0);
    }
}

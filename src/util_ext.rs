//! Hand-written pup command utilities: time/duration parsing, the JSON diff
//! helpers, compute-string parsing, and percent-encoding. None of this is used by
//! generated command modules (only `util::read_json_file` is) -- kept here as pup-
//! owned code the generator never touches.

use std::io::Read;

use anyhow::{bail, Result};
use chrono::Utc;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::formatter::{self, Metadata};

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
///   - Unix timestamp in seconds (10 digits or fewer) or milliseconds
///   - RFC3339: "2024-01-01T00:00:00Z"
///
/// All relative times are interpreted as "ago from now".
/// Returns second-aligned milliseconds (Unix seconds * 1000) to match Go behavior.
pub fn parse_time_to_unix_millis(input: &str) -> Result<i64> {
    let input = input.trim();

    if input.eq_ignore_ascii_case("now") {
        return Ok(now_millis());
    }

    // Unix timestamp (all digits). Treat 10-digit values as seconds to match
    // CLI examples and common shell usage like `date +%s`; preserve longer
    // values as milliseconds.
    if !input.is_empty() && input.chars().all(|c| c.is_ascii_digit()) {
        let ts: i64 = input.parse()?;
        return if input.len() <= 10 {
            Ok(ts * 1000)
        } else {
            Ok(ts)
        };
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

/// Parses a time string into a `chrono::DateTime<Utc>`.
///
/// Returns an `anyhow` error if the input cannot be parsed or if the resulting
/// timestamp falls outside chrono's representable range.
pub fn parse_time_to_datetime(input: &str) -> Result<chrono::DateTime<Utc>> {
    let ms = parse_time_to_unix_millis(input)?;
    chrono::DateTime::from_timestamp_millis(ms)
        .ok_or_else(|| anyhow::anyhow!("timestamp out of valid range: {input:?}"))
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

/// Read an entire reader into a `String`, attaching `err_context` on I/O failure.
///
/// Shared by commands that accept stdin input (e.g. `format::read_input` and
/// `events::resolve_message`) so the read-and-contextualize pattern lives in one
/// place instead of being duplicated per command.
pub fn read_to_string(mut reader: impl Read, err_context: &str) -> Result<String> {
    let mut buf = String::new();
    reader
        .read_to_string(&mut buf)
        .map_err(|e| anyhow::anyhow!("{err_context}: {e}"))?;
    Ok(buf)
}

// ---- JSON diff helpers ----

/// Read-only server-managed fields that are stripped before diffing a monitor.
pub const READONLY_MONITOR_FIELDS: &[&str] = &[
    "id",
    "created",
    "created_at",
    "modified",
    "deleted",
    "overall_state",
    "overall_state_modified",
    "creator",
    "org_id",
    "matching_downtimes",
];

/// The type of change in a [`DiffEntry`].
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    /// Field is present in the candidate but not in the live monitor.
    Added,
    /// Field is present in the live monitor but not in the candidate.
    Removed,
    /// Field is present in both but with different values.
    Modified,
}

/// A single change record produced by [`diff_json`].
#[derive(Debug, Serialize, PartialEq)]
pub struct DiffEntry {
    /// Dot-notation path to the changed field (e.g. `"options.thresholds.critical"`).
    pub path: String,
    /// Type of change.
    pub change: ChangeKind,
    /// Value in `before` (live). `None` for [`ChangeKind::Added`] entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<Value>,
    /// Value in `after` (candidate). `None` for [`ChangeKind::Removed`] entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<Value>,
}

/// Read-only server-managed fields that are stripped before diffing a workflow.
pub const READONLY_WORKFLOW_FIELDS: &[&str] = &[
    "data.id",
    "data.type",
    "data.attributes.action_id",
    "data.attributes.actionId",
    "data.attributes.created_at",
    "data.attributes.createdAt",
    "data.attributes.created_by",
    "data.attributes.createdBy",
    "data.attributes.modified_at",
    "data.attributes.modifiedAt",
    "data.attributes.updated_at",
    "data.attributes.updatedAt",
    "data.attributes.updated_by",
    "data.attributes.updatedBy",
];

/// Options for diffing a live resource against a candidate JSON value.
pub struct ResourceDiffOptions<'a> {
    pub command: &'a str,
    pub update_command: &'a str,
    pub resource_kind: &'a str,
    pub resource_id: &'a str,
    pub readonly_paths: &'a [&'a str],
    pub only: &'a [String],
    pub ignore: &'a [String],
    pub live_root: Option<&'a str>,
    pub candidate_root: Option<&'a str>,
    pub removed_entries_next_action: Option<&'a str>,
    pub no_changes_message: Option<String>,
}

impl<'a> ResourceDiffOptions<'a> {
    pub fn new(
        command: &'a str,
        update_command: &'a str,
        resource_kind: &'a str,
        resource_id: &'a str,
    ) -> Self {
        Self {
            command,
            update_command,
            resource_kind,
            resource_id,
            readonly_paths: &[],
            only: &[],
            ignore: &[],
            live_root: None,
            candidate_root: None,
            removed_entries_next_action: None,
            no_changes_message: None,
        }
    }
}

/// Remove `readonly` keys at dotted paths and recursively drop `null`-valued
/// keys so that absent fields and explicit `null`s compare equal.
pub fn normalize_for_diff(v: &mut Value, readonly: &[&str]) {
    for key in readonly {
        remove_path(v, key);
    }
    drop_null_object_fields(v);
}

fn remove_path(v: &mut Value, path: &str) {
    let parts: Vec<&str> = path.split('.').filter(|part| !part.is_empty()).collect();
    remove_path_parts(v, &parts);
}

fn remove_path_parts(v: &mut Value, parts: &[&str]) {
    if parts.is_empty() {
        return;
    }

    if let Value::Object(map) = v {
        if parts.len() == 1 {
            map.remove(parts[0]);
        } else if let Some(child) = map.get_mut(parts[0]) {
            remove_path_parts(child, &parts[1..]);
        }
    }
}

fn drop_null_object_fields(v: &mut Value) {
    if let Value::Object(map) = v {
        let keys: Vec<String> = map.keys().cloned().collect();
        for key in keys {
            if map[&key].is_null() {
                map.remove(&key);
            } else {
                drop_null_object_fields(map.get_mut(&key).unwrap());
            }
        }
    }
}

/// Diff a live resource and candidate using common pup resource-diff rules.
pub fn diff_resource_values(
    live: &Value,
    candidate: &Value,
    options: &ResourceDiffOptions<'_>,
) -> Result<Vec<DiffEntry>> {
    let mut live = select_diff_root(live, options.live_root, "live")?;
    let mut candidate = select_diff_root(candidate, options.candidate_root, "candidate")?;
    normalize_for_diff(&mut live, options.readonly_paths);
    normalize_for_diff(&mut candidate, options.readonly_paths);
    Ok(scope_diff(
        diff_json(&live, &candidate),
        options.only,
        options.ignore,
    ))
}

fn select_diff_root(value: &Value, root: Option<&str>, label: &str) -> Result<Value> {
    let Some(path) = root else {
        return Ok(value.clone());
    };

    let mut current = value;
    for part in path.split('.').filter(|part| !part.is_empty()) {
        current = current
            .get(part)
            .ok_or_else(|| anyhow::anyhow!("{label} JSON does not contain diff root '{path}'"))?;
    }
    Ok(current.clone())
}

/// Diff and print a live resource against a candidate JSON value.
pub fn format_resource_diff(
    cfg: &crate::config::Config,
    live: &Value,
    candidate: &Value,
    options: &ResourceDiffOptions<'_>,
) -> Result<()> {
    let entries = diff_resource_values(live, candidate, options)?;
    let has_removed = entries.iter().any(|e| e.change == ChangeKind::Removed);
    let next_action = if entries.is_empty() {
        None
    } else if has_removed {
        options
            .removed_entries_next_action
            .map(ToString::to_string)
            .or_else(|| {
                Some(format!(
                    "review changes, then run `{}`",
                    options.update_command
                ))
            })
    } else {
        Some(format!(
            "review changes, then run `{}`",
            options.update_command
        ))
    };
    let meta = Metadata {
        count: Some(entries.len()),
        truncated: false,
        command: Some(options.command.to_string()),
        next_action,
    };
    formatter::format_and_print(
        &entries,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&meta),
        cfg.jq.as_deref(),
    )?;
    if entries.is_empty() && !cfg.agent_mode {
        let message = options.no_changes_message.clone().unwrap_or_else(|| {
            format!(
                "No changes - {} {} is in sync.",
                options.resource_kind, options.resource_id
            )
        });
        eprintln!("{message}");
    }
    Ok(())
}

/// Recursively compare `before` and `after` as `serde_json::Value`s, building
/// dot-notation change records. Objects recurse; scalars and arrays compare as
/// whole values. Returns entries sorted by path for deterministic output.
pub fn diff_json(before: &Value, after: &Value) -> Vec<DiffEntry> {
    let mut entries = Vec::new();
    diff_json_inner(before, after, String::new(), &mut entries);
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    entries
}

fn diff_json_inner(before: &Value, after: &Value, prefix: String, out: &mut Vec<DiffEntry>) {
    match (before, after) {
        (Value::Object(b_map), Value::Object(a_map)) => {
            // Union of all keys across both objects
            let mut keys: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
            for k in b_map.keys() {
                keys.insert(k.as_str());
            }
            for k in a_map.keys() {
                keys.insert(k.as_str());
            }
            for key in keys {
                let child_path = if prefix.is_empty() {
                    key.to_string()
                } else {
                    format!("{prefix}.{key}")
                };
                match (b_map.get(key), a_map.get(key)) {
                    (Some(b_val), Some(a_val)) => {
                        diff_json_inner(b_val, a_val, child_path, out);
                    }
                    (None, Some(a_val)) => out.push(DiffEntry {
                        path: child_path,
                        change: ChangeKind::Added,
                        before: None,
                        after: Some(a_val.clone()),
                    }),
                    (Some(b_val), None) => out.push(DiffEntry {
                        path: child_path,
                        change: ChangeKind::Removed,
                        before: Some(b_val.clone()),
                        after: None,
                    }),
                    (None, None) => unreachable!(),
                }
            }
        }
        _ => {
            if before != after {
                out.push(DiffEntry {
                    path: prefix,
                    change: ChangeKind::Modified,
                    before: Some(before.clone()),
                    after: Some(after.clone()),
                });
            }
        }
    }
}

/// Filter a list of [`DiffEntry`]s by path prefix.
///
/// - If `only` is non-empty, keep only entries whose path equals or starts
///   with one of the `only` values (prefix match: `"options.thresholds"` matches
///   `"options.thresholds"` and `"options.thresholds.critical"`).
/// - Drop entries whose path equals or starts with any value in `ignore`.
/// - Empty slices are no-ops.
pub fn scope_diff(entries: Vec<DiffEntry>, only: &[String], ignore: &[String]) -> Vec<DiffEntry> {
    // Returns true when `path` equals `filter` exactly or is a sub-path of it
    // (i.e. starts with `filter.`). The dot-suffix check prevents "option" from
    // accidentally matching "options.thresholds".
    fn path_matches(path: &str, filter: &str) -> bool {
        path == filter
            || (path.starts_with(filter) && path.as_bytes().get(filter.len()) == Some(&b'.'))
    }

    entries
        .into_iter()
        .filter(|e| {
            if !only.is_empty() && !only.iter().any(|f| path_matches(&e.path, f)) {
                return false;
            }
            if !ignore.is_empty() && ignore.iter().any(|f| path_matches(&e.path, f)) {
                return false;
            }
            true
        })
        .collect()
}

/// Parses a UUID string, returning a descriptive error if invalid.
pub fn parse_uuid(id: &str, label: &str) -> anyhow::Result<uuid::Uuid> {
    uuid::Uuid::parse_str(id).map_err(|e| anyhow::anyhow!("invalid {label} UUID '{id}': {e}"))
}

/// Parse a compute string like "count", "avg(@duration)", "percentile(@duration, 99)"
/// into a (function_name, Option<metric>) pair as raw strings.
pub(crate) fn parse_compute_raw(input: &str) -> Result<(String, Option<String>)> {
    let input = input.trim();
    if input.is_empty() {
        bail!("--compute is required");
    }

    // Simple aggregations without a metric
    if input == "count" {
        return Ok(("count".into(), None));
    }

    // func(@field) pattern
    if let Some(paren) = input.find('(') {
        let func = &input[..paren];
        let rest = input[paren + 1..].trim_end_matches(')').trim();

        // Handle percentile(@field, N)
        if func == "percentile" {
            let parts: Vec<&str> = rest.splitn(2, ',').collect();
            if parts.len() != 2 {
                bail!("percentile requires field and value: percentile(@duration, 99)");
            }
            let metric = parts[0].trim().to_string();
            let pct: u32 = parts[1]
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid percentile value: {}", parts[1].trim()))?;
            let agg_name = match pct {
                75 => "pc75",
                90 => "pc90",
                95 => "pc95",
                98 => "pc98",
                99 => "pc99",
                _ => bail!("unsupported percentile: {pct} (supported: 75, 90, 95, 98, 99)"),
            };
            return Ok((agg_name.into(), Some(metric)));
        }

        let metric = rest.to_string();
        let agg_name = match func {
            "avg" | "sum" | "min" | "max" | "median" | "cardinality" => func.to_string(),
            "count" => bail!("count does not accept a field argument; use just 'count'"),
            _ => bail!("unknown aggregation function: {func}"),
        };
        return Ok((agg_name, Some(metric)));
    }

    bail!(
        "invalid --compute format: {input:?}\n\
         Expected: count, avg(@duration), sum(@duration), percentile(@duration, 99), etc."
    )
}

/// Percent-encode a string for safe use in URL paths and query values.
///
/// Unreserved characters (RFC 3986 §2.3) — `A-Z a-z 0-9 - _ . ~` — are
/// passed through; every other byte is encoded as `%XX`.  Spaces become
/// `%20`, not `+`.
///
/// The implementation is intentionally hand-rolled rather than pulling in the
/// `percent-encoding` crate: that crate is only a transitive dependency (via
/// `url`/`reqwest`) and its `AsciiSet` API requires more boilerplate than this
/// 8-line loop for the single RFC 3986 unreserved-character set we need.
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
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
    fn test_unix_timestamp_seconds_are_expanded_to_millis() {
        let ms = parse_time_to_unix_millis("1707048000").unwrap();
        assert_eq!(ms, 1707048000000);
    }

    #[test]
    fn test_unix_timestamp_milliseconds_are_preserved() {
        let ms = parse_time_to_unix_millis("1707048000000").unwrap();
        assert_eq!(ms, 1707048000000);
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
    fn test_parse_time_to_datetime_relative() {
        let dt = parse_time_to_datetime("1h").unwrap();
        let expected = Utc::now().timestamp() - 3600;
        assert!((dt.timestamp() - expected).abs() < 2);
    }

    #[test]
    fn test_parse_time_to_datetime_long_form() {
        let dt = parse_time_to_datetime("2hours").unwrap();
        let expected = Utc::now().timestamp() - 7200;
        assert!((dt.timestamp() - expected).abs() < 2);
    }

    #[test]
    fn test_parse_time_to_datetime_unix_millis() {
        let dt = parse_time_to_datetime("1700000000000").unwrap();
        assert_eq!(dt.timestamp_millis(), 1700000000000);
    }

    #[test]
    fn test_parse_time_to_datetime_rfc3339() {
        let dt = parse_time_to_datetime("2024-01-01T00:00:00Z").unwrap();
        assert_eq!(dt.timestamp_millis(), 1704067200000);
    }

    #[test]
    fn test_parse_time_to_datetime_invalid_input() {
        let err = parse_time_to_datetime("not-a-time").unwrap_err();
        assert!(err.to_string().contains("unable to parse time"));
    }

    #[test]
    fn test_parse_time_to_datetime_empty() {
        assert!(parse_time_to_datetime("").is_err());
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
    fn test_percent_encode_unreserved_passthrough() {
        // RFC 3986 §2.3 unreserved characters must not be encoded.
        assert_eq!(percent_encode("abc"), "abc");
        assert_eq!(percent_encode("foo.bar"), "foo.bar");
        assert_eq!(percent_encode("foo-bar_baz"), "foo-bar_baz");
        assert_eq!(percent_encode("UPPER"), "UPPER");
        assert_eq!(percent_encode("123"), "123");
        assert_eq!(percent_encode("foo~bar"), "foo~bar"); // tilde is unreserved
    }

    #[test]
    fn test_percent_encode_special_chars() {
        assert_eq!(percent_encode("env:prod"), "env%3Aprod");
        assert_eq!(percent_encode("k8s cluster"), "k8s%20cluster");
        assert_eq!(percent_encode("a&b=c"), "a%26b%3Dc");
        assert_eq!(percent_encode("path/value"), "path%2Fvalue");
    }

    #[test]
    fn test_percent_encode_empty() {
        assert_eq!(percent_encode(""), "");
    }

    #[test]
    fn test_percent_encode_multibyte_utf8() {
        // Multi-byte UTF-8 characters must be encoded byte-by-byte per RFC 3986.
        // "é" is 0xC3 0xA9 in UTF-8.
        assert_eq!(percent_encode("café"), "caf%C3%A9");
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

    #[test]
    fn test_parse_compute_count() {
        let (aggregation, metric) = parse_compute_raw("count").unwrap();
        assert_eq!(aggregation, "count");
        assert!(metric.is_none());
    }

    #[test]
    fn test_parse_compute_avg() {
        let (aggregation, metric) = parse_compute_raw("avg(@duration)").unwrap();
        assert_eq!(aggregation, "avg");
        assert_eq!(metric.unwrap(), "@duration");
    }

    #[test]
    fn test_parse_compute_sum() {
        let (aggregation, metric) = parse_compute_raw("sum(@duration)").unwrap();
        assert_eq!(aggregation, "sum");
        assert_eq!(metric.unwrap(), "@duration");
    }

    #[test]
    fn test_parse_compute_min() {
        let (aggregation, metric) = parse_compute_raw("min(@duration)").unwrap();
        assert_eq!(aggregation, "min");
        assert_eq!(metric.unwrap(), "@duration");
    }

    #[test]
    fn test_parse_compute_max() {
        let (aggregation, metric) = parse_compute_raw("max(@duration)").unwrap();
        assert_eq!(aggregation, "max");
        assert_eq!(metric.unwrap(), "@duration");
    }

    #[test]
    fn test_parse_compute_median() {
        let (aggregation, metric) = parse_compute_raw("median(@duration)").unwrap();
        assert_eq!(aggregation, "median");
        assert_eq!(metric.unwrap(), "@duration");
    }

    #[test]
    fn test_parse_compute_cardinality() {
        let (aggregation, metric) = parse_compute_raw("cardinality(@usr.id)").unwrap();
        assert_eq!(aggregation, "cardinality");
        assert_eq!(metric.unwrap(), "@usr.id");
    }

    #[test]
    fn test_parse_compute_percentile_99() {
        let (aggregation, metric) = parse_compute_raw("percentile(@duration, 99)").unwrap();
        assert_eq!(aggregation, "pc99");
        assert_eq!(metric.unwrap(), "@duration");
    }

    #[test]
    fn test_parse_compute_percentile_95() {
        let (aggregation, metric) = parse_compute_raw("percentile(@duration, 95)").unwrap();
        assert_eq!(aggregation, "pc95");
        assert_eq!(metric.unwrap(), "@duration");
    }

    #[test]
    fn test_parse_compute_percentile_90() {
        let (aggregation, metric) = parse_compute_raw("percentile(@duration, 90)").unwrap();
        assert_eq!(aggregation, "pc90");
        assert_eq!(metric.unwrap(), "@duration");
    }

    #[test]
    fn test_parse_compute_empty() {
        assert!(parse_compute_raw("").is_err());
    }

    #[test]
    fn test_parse_compute_invalid() {
        assert!(parse_compute_raw("invalid").is_err());
    }

    #[test]
    fn test_parse_compute_unknown_function() {
        assert!(parse_compute_raw("foo(@bar)").is_err());
    }

    #[test]
    fn test_parse_compute_unsupported_percentile() {
        assert!(parse_compute_raw("percentile(@duration, 42)").is_err());
    }

    #[test]
    fn test_parse_compute_percentile_missing_value() {
        assert!(parse_compute_raw("percentile(@duration)").is_err());
    }

    #[test]
    fn test_parse_compute_rejects_invalid_count_metric() {
        let err = parse_compute_raw("count(@duration)").unwrap_err();
        assert!(err.to_string().contains("does not accept a field"));
    }

    // ---- diff_json ----

    #[test]
    fn test_diff_json_identical() {
        let v: Value = serde_json::json!({"name": "cpu", "query": "avg:system.cpu.user{*} > 90"});
        assert!(diff_json(&v, &v).is_empty());
    }

    #[test]
    fn test_diff_json_modified_scalar() {
        let before = serde_json::json!({"options": {"thresholds": {"critical": 90}}});
        let after = serde_json::json!({"options": {"thresholds": {"critical": 95}}});
        let entries = diff_json(&before, &after);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "options.thresholds.critical");
        assert_eq!(entries[0].change, ChangeKind::Modified);
        assert_eq!(entries[0].before, Some(serde_json::json!(90)));
        assert_eq!(entries[0].after, Some(serde_json::json!(95)));
    }

    #[test]
    fn test_diff_json_added_field() {
        let before = serde_json::json!({"name": "cpu"});
        let after = serde_json::json!({"name": "cpu", "message": "alert!"});
        let entries = diff_json(&before, &after);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "message");
        assert_eq!(entries[0].change, ChangeKind::Added);
        assert_eq!(entries[0].before, None);
        assert_eq!(entries[0].after, Some(serde_json::json!("alert!")));
    }

    #[test]
    fn test_diff_json_removed_field() {
        let before = serde_json::json!({"name": "cpu", "message": "alert!"});
        let after = serde_json::json!({"name": "cpu"});
        let entries = diff_json(&before, &after);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "message");
        assert_eq!(entries[0].change, ChangeKind::Removed);
        assert_eq!(entries[0].before, Some(serde_json::json!("alert!")));
        assert_eq!(entries[0].after, None);
    }

    #[test]
    fn test_diff_json_sorted_paths() {
        let before = serde_json::json!({"z": 1, "a": 1, "m": 1});
        let after = serde_json::json!({"z": 2, "a": 2, "m": 2});
        let entries = diff_json(&before, &after);
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["a", "m", "z"]);
    }

    #[test]
    fn test_diff_json_array_compared_whole() {
        let before = serde_json::json!({"tags": ["env:prod", "team:backend"]});
        let after = serde_json::json!({"tags": ["team:backend", "env:prod"]});
        let entries = diff_json(&before, &after);
        // Arrays are compared as whole values; reordered tags → modified
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "tags");
        assert_eq!(entries[0].change, ChangeKind::Modified);
    }

    // ---- normalize_for_diff ----

    #[test]
    fn test_normalize_strips_readonly_fields() {
        let mut v = serde_json::json!({
            "id": 12345,
            "name": "cpu",
            "created": "2024-01-01",
            "overall_state": "OK",
            "org_id": 999,
        });
        normalize_for_diff(&mut v, READONLY_MONITOR_FIELDS);
        let obj = v.as_object().unwrap();
        assert!(!obj.contains_key("id"));
        assert!(!obj.contains_key("created"));
        assert!(!obj.contains_key("overall_state"));
        assert!(!obj.contains_key("org_id"));
        assert!(obj.contains_key("name"));
    }

    #[test]
    fn test_normalize_strips_nested_readonly_fields() {
        let mut v = serde_json::json!({
            "data": {
                "id": "wf-1",
                "type": "workflows",
                "attributes": {
                    "action_id": "wf-1",
                    "name": "Deploy",
                    "updatedAt": "2024-01-02T00:00:00Z"
                }
            }
        });
        normalize_for_diff(&mut v, READONLY_WORKFLOW_FIELDS);
        assert!(v.pointer("/data/id").is_none());
        assert!(v.pointer("/data/type").is_none());
        assert!(v.pointer("/data/attributes/action_id").is_none());
        assert!(v.pointer("/data/attributes/updatedAt").is_none());
        assert_eq!(
            v.pointer("/data/attributes/name"),
            Some(&serde_json::json!("Deploy"))
        );
    }

    #[test]
    fn test_normalize_drops_null_values() {
        let mut v = serde_json::json!({"name": "cpu", "message": null, "priority": null});
        normalize_for_diff(&mut v, &[]);
        let obj = v.as_object().unwrap();
        assert!(!obj.contains_key("message"));
        assert!(!obj.contains_key("priority"));
        assert!(obj.contains_key("name"));
    }

    #[test]
    fn test_normalize_absent_and_null_compare_equal() {
        // After normalizing, absent and null fields are both gone → no diff entry
        let mut live = serde_json::json!({"name": "cpu", "priority": null});
        let mut candidate = serde_json::json!({"name": "cpu"});
        normalize_for_diff(&mut live, &[]);
        normalize_for_diff(&mut candidate, &[]);
        assert!(diff_json(&live, &candidate).is_empty());
    }

    #[test]
    fn test_diff_resource_values_uses_roots_and_filters() {
        let live = serde_json::json!({
            "data": {
                "attributes": {
                    "name": "Old",
                    "spec": {"threshold": 1},
                    "updatedAt": "2024-01-01T00:00:00Z"
                }
            }
        });
        let candidate = serde_json::json!({
            "data": {
                "attributes": {
                    "name": "New",
                    "spec": {"threshold": 2}
                }
            }
        });
        let only = vec!["spec".to_string()];
        let readonly = ["updatedAt"];
        let mut options = ResourceDiffOptions::new("test diff", "test update", "thing", "id");
        options.readonly_paths = &readonly;
        options.only = &only;
        options.live_root = Some("data.attributes");
        options.candidate_root = Some("data.attributes");

        let entries = diff_resource_values(&live, &candidate, &options).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "spec.threshold");
        assert_eq!(entries[0].change, ChangeKind::Modified);
    }

    // ---- scope_diff ----

    // Shorthand for building a Modified DiffEntry in scope_diff tests.
    fn modified(path: &str) -> DiffEntry {
        DiffEntry {
            path: path.into(),
            change: ChangeKind::Modified,
            before: Some(serde_json::json!("a")),
            after: Some(serde_json::json!("b")),
        }
    }

    #[test]
    fn test_scope_diff_empty_filters_noop() {
        let entries = vec![modified("name"), modified("options.thresholds.critical")];
        assert_eq!(scope_diff(entries, &[], &[]).len(), 2);
    }

    #[test]
    fn test_scope_diff_only_prefix() {
        let entries = vec![
            modified("name"),
            modified("options.thresholds.critical"),
            modified("options.thresholds.warning"),
        ];
        let only = vec!["options.thresholds".to_string()];
        let result = scope_diff(entries, &only, &[]);
        assert_eq!(result.len(), 2);
        assert!(result
            .iter()
            .all(|e| e.path.starts_with("options.thresholds")));
    }

    #[test]
    fn test_scope_diff_only_exact_match() {
        let entries = vec![modified("options"), modified("name")];
        let only = vec!["options".to_string()];
        let result = scope_diff(entries, &only, &[]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, "options");
    }

    #[test]
    fn test_scope_diff_only_partial_prefix_does_not_match() {
        // "option" (no trailing dot) must NOT match "options.thresholds.critical".
        // The path_matches helper uses a dot-boundary check to prevent this.
        let entries = vec![modified("options.thresholds.critical")];
        let only = vec!["option".to_string()];
        assert!(scope_diff(entries, &only, &[]).is_empty());
    }

    #[test]
    fn test_scope_diff_ignore() {
        let entries = vec![modified("message"), modified("name")];
        let ignore = vec!["message".to_string()];
        let result = scope_diff(entries, &[], &ignore);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, "name");
    }

    #[test]
    fn test_scope_diff_ignore_prefix() {
        let entries = vec![modified("options.thresholds.critical"), modified("name")];
        let ignore = vec!["options".to_string()];
        let result = scope_diff(entries, &[], &ignore);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, "name");
    }

    #[test]
    fn test_scope_diff_only_and_ignore_combined() {
        let entries = vec![
            modified("options.thresholds.critical"),
            modified("options.thresholds.warning"),
            modified("name"),
        ];
        let only = vec!["options.thresholds".to_string()];
        let ignore = vec!["options.thresholds.warning".to_string()];
        let result = scope_diff(entries, &only, &ignore);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, "options.thresholds.critical");
    }
}

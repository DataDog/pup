//! Datadog `X-RateLimit-*` response header capture and readable 429 reporting.
//!
//! Typed SDK commands discard response headers; middleware in `client.rs` captures
//! rate-limit metadata so 429 errors can name the throttling rule.

use std::cell::Cell;
use std::sync::Mutex;

use anyhow::Result;
use reqwest::header::HeaderMap;

use crate::config::OutputFormat;
use crate::formatter;

thread_local! {
    static VERBOSE_ENABLED: Cell<bool> = const { Cell::new(false) };
}

/// Process exit code for HTTP 429 rate-limit failures.
///
/// On Unix the value is truncated to eight bits (`429 % 256 == 173`); check stderr
/// for the HTTP status and rate-limit rule when scripting.
pub const EXIT_RATE_LIMITED: i32 = 429;

static LAST_CAPTURED: Mutex<Option<RateLimitInfo>> = Mutex::new(None);

/// Enable or disable verbose rate-limit reporting for the current thread.
pub fn set_verbose(enabled: bool) {
    VERBOSE_ENABLED.with(|v| v.set(enabled));
}

/// Returns true when `--verbose` was passed for this invocation.
pub fn verbose_enabled() -> bool {
    VERBOSE_ENABLED.with(|v| v.get())
}

/// Parsed Datadog rate-limit response headers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RateLimitInfo {
    pub name: Option<String>,
    pub limit: Option<String>,
    pub remaining: Option<String>,
    pub reset: Option<String>,
    pub period: Option<String>,
}

impl RateLimitInfo {
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.limit.is_none()
            && self.remaining.is_none()
            && self.reset.is_none()
            && self.period.is_none()
    }

    /// Human-readable, indented detail lines for stderr.
    pub fn format_lines(&self) -> String {
        let mut lines = vec!["Rate limit details:".to_string()];
        if let Some(name) = &self.name {
            lines.push(format!("  rule: {name}"));
        }
        if let Some(limit) = &self.limit {
            lines.push(format!("  limit: {limit}"));
        }
        if let Some(remaining) = &self.remaining {
            lines.push(format!("  remaining: {remaining}"));
        }
        if let Some(period) = &self.period {
            lines.push(format!("  period: {period}"));
        }
        if let Some(reset) = &self.reset {
            lines.push(format!("  reset: {reset}"));
        }
        lines.join("\n")
    }

    pub fn to_json_value(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(name) = &self.name {
            map.insert("name".into(), name.clone().into());
        }
        if let Some(limit) = &self.limit {
            map.insert("limit".into(), limit.clone().into());
        }
        if let Some(remaining) = &self.remaining {
            map.insert("remaining".into(), remaining.clone().into());
        }
        if let Some(period) = &self.period {
            map.insert("period".into(), period.clone().into());
        }
        if let Some(reset) = &self.reset {
            map.insert("reset".into(), reset.clone().into());
        }
        serde_json::Value::Object(map)
    }
}

/// Extract Datadog `X-RateLimit-*` headers from an HTTP response.
pub fn extract_from_headers(headers: &HeaderMap) -> Option<RateLimitInfo> {
    let mut info = RateLimitInfo::default();
    for (name, value) in headers {
        let name_lc = name.as_str().to_ascii_lowercase();
        if !name_lc.starts_with("x-ratelimit-") {
            continue;
        }
        let Ok(v) = value.to_str() else {
            continue;
        };
        match name_lc.as_str() {
            "x-ratelimit-name" => info.name = Some(v.to_string()),
            "x-ratelimit-limit" => info.limit = Some(v.to_string()),
            "x-ratelimit-remaining" => info.remaining = Some(v.to_string()),
            "x-ratelimit-reset" => info.reset = Some(v.to_string()),
            "x-ratelimit-period" => info.period = Some(v.to_string()),
            _ => {}
        }
    }
    if info.is_empty() {
        None
    } else {
        Some(info)
    }
}

/// Remember rate-limit headers from the most recent SDK HTTP response.
pub fn store_last(info: Option<RateLimitInfo>) {
    if let Ok(mut guard) = LAST_CAPTURED.lock() {
        *guard = info;
    }
}

/// Take rate-limit headers captured by SDK middleware (clears the store).
pub fn take_last_captured() -> Option<RateLimitInfo> {
    LAST_CAPTURED.lock().ok().and_then(|mut guard| guard.take())
}

/// Peek at captured rate-limit headers without clearing the store.
pub fn peek_last_captured() -> Option<RateLimitInfo> {
    LAST_CAPTURED.lock().ok().and_then(|guard| guard.clone())
}

/// When `--verbose` is set, print captured rate-limit headers to stderr using
/// the same output format as the command payload.
pub fn eprint_verbose_response(format: &OutputFormat, agent_mode: bool) -> Result<()> {
    let Some(info) = peek_last_captured() else {
        return Ok(());
    };
    if info.is_empty() {
        return Ok(());
    }
    formatter::eprint_formatted(&info.to_json_value(), format, agent_mode)
}

/// Returns true when `err` represents an HTTP 429 rate-limit failure.
pub fn is_rate_limited(err: &anyhow::Error) -> bool {
    if let Some(http_err) = err.downcast_ref::<crate::raw_client::HttpError>() {
        return http_err.status == 429;
    }

    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("429")
        && (msg.contains("too many requests")
            || msg.contains("rate limit")
            || msg.contains("status code 429"))
}

/// Format a CLI error and choose an exit code (429 for rate limits, 1 otherwise).
pub fn cli_error(err: &anyhow::Error) -> (String, i32) {
    let mut msg = format!("{err:#}");

    if let Some(http_err) = err.downcast_ref::<crate::raw_client::HttpError>() {
        if http_err.status == 429 {
            if let Some(ref info) = http_err.rate_limit {
                if !info.is_empty() {
                    msg.push('\n');
                    msg.push_str(&info.format_lines());
                }
            }
            msg.push_str("\nHint: rate limited — wait and retry");
            return (msg, EXIT_RATE_LIMITED);
        }
    }

    if is_rate_limited(err) {
        if let Some(info) = take_last_captured() {
            if !info.is_empty() {
                msg.push('\n');
                msg.push_str(&info.format_lines());
            }
        }
        msg.push_str("\nHint: rate limited — wait and retry");
        return (msg, EXIT_RATE_LIMITED);
    }

    (msg, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_map(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (k, v) in pairs {
            map.insert(
                reqwest::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                reqwest::header::HeaderValue::from_str(v).unwrap(),
            );
        }
        map
    }

    #[test]
    fn test_extract_from_headers_all_fields() {
        let headers = header_map(&[
            ("x-ratelimit-name", "get_all_monitors"),
            ("x-ratelimit-limit", "1000"),
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", "1700000000"),
            ("x-ratelimit-period", "60"),
        ]);
        let info = extract_from_headers(&headers).expect("expected rate limit info");
        assert_eq!(info.name.as_deref(), Some("get_all_monitors"));
        assert_eq!(info.limit.as_deref(), Some("1000"));
        assert_eq!(info.remaining.as_deref(), Some("0"));
        assert_eq!(info.reset.as_deref(), Some("1700000000"));
        assert_eq!(info.period.as_deref(), Some("60"));
    }

    #[test]
    fn test_extract_from_headers_case_insensitive() {
        let headers = header_map(&[
            ("X-RateLimit-Name", "logs_public_search_api"),
            ("X-RateLimit-Limit", "10"),
        ]);
        let info = extract_from_headers(&headers).expect("expected rate limit info");
        assert_eq!(info.name.as_deref(), Some("logs_public_search_api"));
        assert_eq!(info.limit.as_deref(), Some("10"));
    }

    #[test]
    fn test_extract_from_headers_empty_when_absent() {
        assert!(extract_from_headers(&HeaderMap::new()).is_none());
    }

    #[test]
    fn test_format_lines_readable() {
        let info = RateLimitInfo {
            name: Some("get_all_monitors".into()),
            limit: Some("1000".into()),
            remaining: Some("0".into()),
            ..Default::default()
        };
        let text = info.format_lines();
        assert!(text.contains("rule: get_all_monitors"));
        assert!(text.contains("limit: 1000"));
        assert!(text.contains("remaining: 0"));
    }

    #[test]
    fn test_is_rate_limited_sdk_error_string() {
        let err = anyhow::anyhow!(
            "failed to list monitors: error in response: status code 429 Too Many Requests"
        );
        assert!(is_rate_limited(&err));
    }

    #[test]
    fn test_is_rate_limited_http_error() {
        let err = anyhow::Error::from(crate::raw_client::HttpError {
            status: 429,
            method: "GET".into(),
            url: "https://api.datadoghq.com/api/v1/monitor".into(),
            body: "Too Many Requests".into(),
            rate_limit: None,
        });
        assert!(is_rate_limited(&err));
    }

    #[test]
    fn test_is_rate_limited_false_for_other_errors() {
        let err = anyhow::anyhow!("failed to list monitors: status code 403 Forbidden");
        assert!(!is_rate_limited(&err));
    }

    #[test]
    fn test_cli_error_rate_limited_with_captured_headers() {
        store_last(Some(RateLimitInfo {
            name: Some("slo_get_all".into()),
            limit: Some("1000".into()),
            remaining: Some("0".into()),
            ..Default::default()
        }));
        let err = anyhow::anyhow!("failed to list slos: status code 429 Too Many Requests");
        let (msg, code) = cli_error(&err);
        assert_eq!(code, EXIT_RATE_LIMITED);
        assert!(msg.contains("rule: slo_get_all"));
        assert!(msg.contains("Hint: rate limited"));
    }

    #[test]
    fn test_cli_error_non_rate_limit_exits_one() {
        let err = anyhow::anyhow!("failed to get monitor: status code 404 Not Found");
        let (msg, code) = cli_error(&err);
        assert_eq!(code, 1);
        assert!(msg.contains("404"));
    }

    #[test]
    fn test_eprint_verbose_response_json() {
        set_verbose(true);
        store_last(Some(RateLimitInfo {
            name: Some("get_all_monitors".into()),
            limit: Some("1000".into()),
            remaining: Some("999".into()),
            ..Default::default()
        }));
        let rendered = formatter::format_value_to_string(
            &RateLimitInfo {
                name: Some("get_all_monitors".into()),
                limit: Some("1000".into()),
                remaining: Some("999".into()),
                ..Default::default()
            }
            .to_json_value(),
            &OutputFormat::Json,
            false,
        )
        .expect("json format");
        assert!(rendered.contains("\"limit\": \"1000\""));
        assert!(rendered.contains("\"name\": \"get_all_monitors\""));
        set_verbose(false);
    }

    #[test]
    fn test_eprint_verbose_response_table() {
        let rendered = formatter::format_value_to_string(
            &RateLimitInfo {
                name: Some("logs_public_search_api".into()),
                limit: Some("10".into()),
                remaining: Some("7".into()),
                ..Default::default()
            }
            .to_json_value(),
            &OutputFormat::Table,
            false,
        )
        .expect("table format");
        assert!(rendered.contains("logs_public_search_api"));
        assert!(rendered.contains("10"));
    }
}

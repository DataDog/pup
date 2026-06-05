use anyhow::{bail, Result};
use serde_json::Value;
use std::io::Read;

use crate::config::Config;
use crate::useragent;

/// Parse `key=value` into (key, value). Splits on the first `=` only.
fn parse_kv(s: &str) -> Result<(String, String)> {
    let pos = s
        .find('=')
        .ok_or_else(|| anyhow::anyhow!("expected KEY=VALUE, got {s:?}"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

/// Parse `key:value` into (key, value). Splits on the first `:` only.
fn parse_header_str(s: &str) -> Result<(String, String)> {
    let pos = s
        .find(':')
        .ok_or_else(|| anyhow::anyhow!("expected KEY:VALUE, got {s:?}"))?;
    Ok((
        s[..pos].trim().to_string(),
        s[pos + 1..].trim_start().to_string(),
    ))
}

/// Coerce a string to a typed JSON value.
/// Parses as `null`, `true`, `false`, integer, float, or falls back to string.
fn coerce_to_json(s: &str) -> Value {
    match s {
        "null" => Value::Null,
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => {
            if let Ok(n) = s.parse::<i64>() {
                Value::Number(n.into())
            } else if let Ok(f) = s.parse::<f64>() {
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .unwrap_or_else(|| Value::String(s.to_string()))
            } else {
                Value::String(s.to_string())
            }
        }
    }
}

/// Render a JSON value as a plain string suitable for a query parameter.
fn value_to_query_param(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

/// Normalize a caller-supplied endpoint to an absolute path under `/api`.
///
/// - `v2/monitors`       → `/api/v2/monitors`
/// - `/api/v2/monitors`  → `/api/v2/monitors`
/// - `/v2/monitors`      → `/api/v2/monitors`
fn normalize_path(endpoint: &str) -> String {
    if endpoint.starts_with("/api/") {
        endpoint.to_string()
    } else if endpoint.starts_with('/') {
        format!("/api{}", endpoint)
    } else {
        format!("/api/{}", endpoint)
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    cfg: &Config,
    endpoint: &str,
    method: &str,
    headers: &[String],
    fields: &[String],
    raw_fields: &[String],
    input: Option<&str>,
    include: bool,
    silent: bool,
    verbose: bool,
) -> Result<()> {
    let method_upper = method.to_uppercase();

    // Full URLs pass through; relative paths get the API base prepended.
    let is_absolute = endpoint.starts_with("http://") || endpoint.starts_with("https://");
    let url = if is_absolute {
        endpoint.to_string()
    } else {
        format!("{}{}", cfg.api_base_url(), normalize_path(endpoint))
    };

    // Path used for per-endpoint auth routing. For relative endpoints this is the
    // normalized API path; for absolute URLs we use the URL's path component so the
    // OAuth-exclusion table (client::requires_api_key_fallback) still applies.
    let auth_path = if is_absolute {
        reqwest::Url::parse(&url)
            .map(|u| u.path().to_string())
            .unwrap_or_else(|_| normalize_path(endpoint))
    } else {
        normalize_path(endpoint)
    };

    // POST, PUT, PATCH carry a body; GET/HEAD/DELETE use query params.
    let is_body_method = matches!(method_upper.as_str(), "POST" | "PUT" | "PATCH");

    let typed_fields = fields
        .iter()
        .map(|f| parse_kv(f).map(|(k, v)| (k, coerce_to_json(&v))))
        .collect::<Result<Vec<_>>>()?;

    let string_fields = raw_fields
        .iter()
        .map(|f| parse_kv(f))
        .collect::<Result<Vec<_>>>()?;

    // Resolve request body bytes.
    let body: Option<Vec<u8>> = if let Some(path) = input {
        if path == "-" {
            let mut buf = Vec::new();
            std::io::stdin().read_to_end(&mut buf)?;
            Some(buf)
        } else {
            Some(
                std::fs::read(path)
                    .map_err(|e| anyhow::anyhow!("failed to read --input {path:?}: {e}"))?,
            )
        }
    } else if is_body_method && (!typed_fields.is_empty() || !string_fields.is_empty()) {
        let mut obj = serde_json::Map::new();
        for (k, v) in &typed_fields {
            obj.insert(k.clone(), v.clone());
        }
        for (k, v) in &string_fields {
            obj.insert(k.clone(), Value::String(v.clone()));
        }
        Some(serde_json::to_vec(&obj)?)
    } else {
        None
    };

    let client = reqwest::Client::new();
    let method_val = reqwest::Method::from_bytes(method_upper.as_bytes())
        .map_err(|_| anyhow::anyhow!("unsupported HTTP method: {method}"))?;
    let mut req = client.request(method_val, &url);

    // Reuse the shared auth handler so `pup api` (and extensions that shell out to
    // it) get the same OAuth-vs-API-key routing as the typed clients, including the
    // per-endpoint OAuth-exclusion fallback.
    req = crate::client::apply_auth(req, cfg, &method_upper, &auth_path)?;

    req = req
        .header("User-Agent", useragent::get())
        .header("Accept", "application/json");

    for h in headers {
        let (k, v) = parse_header_str(h)?;
        req = req.header(k, v);
    }

    // For GET/HEAD/DELETE, fields become query parameters.
    if !is_body_method && (!typed_fields.is_empty() || !string_fields.is_empty()) {
        let params: Vec<(String, String)> = typed_fields
            .iter()
            .map(|(k, v)| (k.clone(), value_to_query_param(v)))
            .chain(string_fields.iter().map(|(k, v)| (k.clone(), v.clone())))
            .collect();
        req = req.query(&params);
    }

    if let Some(b) = body {
        req = req.header("Content-Type", "application/json").body(b);
    }

    if verbose {
        eprintln!("> {} {}", method_upper, url);
    }

    let resp = req.send().await?;
    let status = resp.status();
    let resp_headers = resp.headers().clone();

    if include || verbose {
        println!(
            "HTTP/1.1 {} {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or("")
        );
        for (name, value) in &resp_headers {
            if let Ok(v_str) = value.to_str() {
                println!("{}: {}", name, v_str);
            }
        }
        println!();
    }

    let body_bytes = resp.bytes().await?;

    if !status.is_success() {
        let text = String::from_utf8_lossy(&body_bytes);
        bail!("HTTP {} {}: {}", status.as_u16(), url, text);
    }

    if !silent && !body_bytes.is_empty() {
        if let Ok(json) = serde_json::from_slice::<Value>(&body_bytes) {
            // Render through the shared formatter so `--output`/agent mode are
            // honored, matching every other pup command.
            crate::formatter::format_and_print(&json, &cfg.output_format, cfg.agent_mode, None)?;
        } else {
            print!("{}", String::from_utf8_lossy(&body_bytes));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    use super::*;

    #[test]
    fn test_normalize_path_v2_prefix() {
        assert_eq!(normalize_path("v2/monitors"), "/api/v2/monitors");
    }

    #[test]
    fn test_normalize_path_already_api() {
        assert_eq!(normalize_path("/api/v2/monitors"), "/api/v2/monitors");
    }

    #[test]
    fn test_normalize_path_slash_prefix() {
        assert_eq!(normalize_path("/v2/monitors"), "/api/v2/monitors");
    }

    #[test]
    fn test_normalize_path_no_prefix() {
        assert_eq!(normalize_path("monitors"), "/api/monitors");
    }

    #[test]
    fn test_parse_kv_basic() {
        let (k, v) = parse_kv("key=value").unwrap();
        assert_eq!(k, "key");
        assert_eq!(v, "value");
    }

    #[test]
    fn test_parse_kv_equals_in_value() {
        let (k, v) = parse_kv("key=val=ue").unwrap();
        assert_eq!(k, "key");
        assert_eq!(v, "val=ue");
    }

    #[test]
    fn test_parse_kv_no_equals() {
        assert!(parse_kv("noequals").is_err());
    }

    #[test]
    fn test_parse_header_str_basic() {
        let (k, v) = parse_header_str("Content-Type: application/json").unwrap();
        assert_eq!(k, "Content-Type");
        assert_eq!(v, "application/json");
    }

    #[test]
    fn test_parse_header_no_colon() {
        assert!(parse_header_str("nocolon").is_err());
    }

    #[test]
    fn test_coerce_null() {
        assert_eq!(coerce_to_json("null"), Value::Null);
    }

    #[test]
    fn test_coerce_bool_true() {
        assert_eq!(coerce_to_json("true"), Value::Bool(true));
    }

    #[test]
    fn test_coerce_bool_false() {
        assert_eq!(coerce_to_json("false"), Value::Bool(false));
    }

    #[test]
    fn test_coerce_int() {
        assert_eq!(coerce_to_json("42"), Value::Number(42.into()));
    }

    #[test]
    fn test_coerce_negative_int() {
        assert_eq!(coerce_to_json("-5"), Value::Number((-5i64).into()));
    }

    #[test]
    fn test_coerce_string_fallback() {
        assert_eq!(coerce_to_json("hello"), Value::String("hello".to_string()));
    }

    #[test]
    fn test_value_to_query_param_string() {
        assert_eq!(value_to_query_param(&Value::String("prod".into())), "prod");
    }

    #[test]
    fn test_value_to_query_param_null() {
        assert_eq!(value_to_query_param(&Value::Null), "null");
    }

    #[test]
    fn test_value_to_query_param_number() {
        assert_eq!(value_to_query_param(&Value::Number(42.into())), "42");
    }

    #[test]
    fn test_value_to_query_param_bool() {
        assert_eq!(value_to_query_param(&Value::Bool(true)), "true");
    }

    #[tokio::test]
    async fn test_api_get_success() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", "/api/v2/monitors")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":1,"name":"Test"}]"#)
            .create_async()
            .await;

        let result = super::run(
            &cfg,
            "v2/monitors",
            "GET",
            &[],
            &[],
            &[],
            None,
            false,
            false,
            false,
        )
        .await;
        assert!(result.is_ok(), "api GET failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_api_get_absolute_path() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", "/api/v2/monitors")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create_async()
            .await;

        let result = super::run(
            &cfg,
            "/api/v2/monitors",
            "GET",
            &[],
            &[],
            &[],
            None,
            false,
            false,
            false,
        )
        .await;
        assert!(
            result.is_ok(),
            "api GET absolute path failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_api_post_with_fields() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("POST", "/api/v2/tags/hosts/myhost")
            .match_query(mockito::Matcher::Any)
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"tags":[]}"#)
            .create_async()
            .await;

        let result = super::run(
            &cfg,
            "v2/tags/hosts/myhost",
            "POST",
            &[],
            &["host=myhost".to_string()],
            &["source=web".to_string()],
            None,
            false,
            false,
            false,
        )
        .await;
        assert!(
            result.is_ok(),
            "api POST with fields failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_api_raw_error_response() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", "/api/v2/monitors")
            .match_query(mockito::Matcher::Any)
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Forbidden"]}"#)
            .create_async()
            .await;

        let result = super::run(
            &cfg,
            "v2/monitors",
            "GET",
            &[],
            &[],
            &[],
            None,
            false,
            false,
            false,
        )
        .await;
        assert!(result.is_err(), "api GET should fail on 403");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_api_silent_flag() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", "/api/v2/monitors")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":1}]"#)
            .create_async()
            .await;

        let result = super::run(
            &cfg,
            "v2/monitors",
            "GET",
            &[],
            &[],
            &[],
            None,
            false,
            true, // silent
            false,
        )
        .await;
        assert!(result.is_ok(), "api GET silent failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_api_bad_method() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let result = super::run(
            &cfg,
            "v2/monitors",
            "INVALID METHOD WITH SPACES",
            &[],
            &[],
            &[],
            None,
            false,
            false,
            false,
        )
        .await;
        assert!(result.is_err(), "expected error for invalid HTTP method");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_api_bad_field_format() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let result = super::run(
            &cfg,
            "v2/monitors",
            "GET",
            &[],
            &["notakeyvalue".to_string()], // missing '='
            &[],
            None,
            false,
            false,
            false,
        )
        .await;
        assert!(result.is_err(), "expected error for malformed field");
        cleanup_env();
    }

    /// `pup api -o table` must render through the shared formatter without error,
    /// proving the output now honors cfg.output_format instead of always JSON.
    #[tokio::test]
    async fn test_api_table_output() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        cfg.output_format = crate::config::OutputFormat::Table;
        let _mock = server
            .mock("GET", "/api/v2/monitors")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":1,"name":"Test"}]"#)
            .create_async()
            .await;

        let result = super::run(
            &cfg,
            "v2/monitors",
            "GET",
            &[],
            &[],
            &[],
            None,
            false,
            false,
            false,
        )
        .await;
        assert!(result.is_ok(), "api GET table failed: {:?}", result.err());
        cleanup_env();
    }

    /// OAuth-excluded endpoints (e.g. GET /api/v2/api_keys) must use API-key auth
    /// even when a bearer token is present. This exercises the reuse of
    /// client::apply_auth's per-endpoint fallback table.
    #[tokio::test]
    async fn test_api_oauth_excluded_uses_api_keys() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        // Both a bearer token AND API keys are configured; the excluded endpoint
        // must prefer the API keys.
        cfg.access_token = Some("bearer-token".into());
        let _mock = server
            .mock("GET", "/api/v2/api_keys")
            .match_query(mockito::Matcher::Any)
            .match_header("DD-API-KEY", "test-api-key")
            .match_header("DD-APPLICATION-KEY", "test-app-key")
            .match_header("authorization", mockito::Matcher::Missing)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        let result = super::run(
            &cfg,
            "v2/api_keys",
            "GET",
            &[],
            &[],
            &[],
            None,
            false,
            true,
            false,
        )
        .await;
        assert!(
            result.is_ok(),
            "expected API-key auth on OAuth-excluded endpoint: {:?}",
            result.err()
        );
        cleanup_env();
    }

    /// An absolute http(s):// endpoint must still consult the OAuth-exclusion
    /// table via its URL path — exercising the `is_absolute` auth_path branch.
    #[tokio::test]
    async fn test_api_absolute_url_oauth_excluded_uses_api_keys() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        cfg.access_token = Some("bearer-token".into());
        let _mock = server
            .mock("GET", "/api/v2/api_keys")
            .match_query(mockito::Matcher::Any)
            .match_header("DD-API-KEY", "test-api-key")
            .match_header("authorization", mockito::Matcher::Missing)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        // Pass the fully-qualified URL, not a relative path.
        let absolute = format!("{}/api/v2/api_keys", server.url());
        let result = super::run(
            &cfg,
            &absolute,
            "GET",
            &[],
            &[],
            &[],
            None,
            false,
            true,
            false,
        )
        .await;
        assert!(
            result.is_ok(),
            "expected API-key auth on absolute OAuth-excluded URL: {:?}",
            result.err()
        );
        cleanup_env();
    }
}

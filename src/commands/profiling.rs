//! Datadog Continuous Profiler commands.
//!
//! These wrap a small, pup-CLI-scoped API surface built specifically for
//! agentic callers like pup (PROF-15503). It is not part of the official
//! datadog-api-client-rust SDK, so raw HTTP is used throughout, following
//! the pattern established in `llm_obs.rs` for non-SDK endpoints.

use anyhow::Result;
use serde_json::json;

use crate::config::Config;
use crate::formatter::{self, Metadata};
use crate::raw_client;
use crate::util_ext;

const BASE: &str = "/api/unstable/profiling/pup";

/// Parses `--header` values of the form `Name: Value` or `Name=Value` into
/// `(name, value)` pairs, e.g. for routing requests to a specific test
/// environment ahead of general availability.
pub fn parse_extra_headers(headers: &[String]) -> Result<Vec<(String, String)>> {
    headers
        .iter()
        .map(|h| {
            let (name, value) =
                h.split_once(':')
                    .or_else(|| h.split_once('='))
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "invalid --header '{h}': expected 'Name: Value' or 'Name=Value'"
                        )
                    })?;
            let name = name.trim();
            let value = value.trim();
            if name.is_empty() {
                anyhow::bail!("invalid --header '{h}': header name cannot be empty");
            }
            Ok((name.to_string(), value.to_string()))
        })
        .collect()
}

fn filter_json(query: &str, from: &str, to: &str) -> Result<serde_json::Value> {
    let from_dt = util_ext::parse_time_to_datetime(from)?;
    let to_dt = util_ext::parse_time_to_datetime(to)?;
    Ok(json!({
        "from": from_dt.to_rfc3339(),
        "to": to_dt.to_rfc3339(),
        "query": query,
    }))
}

// ---- Profiles ----

#[allow(clippy::too_many_arguments)]
pub async fn profiles_list(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    limit: i32,
    sort_order: String,
    sort_field: String,
    extra_headers: &[(&str, &str)],
) -> Result<()> {
    let body = json!({
        "filter": filter_json(&query, &from, &to)?,
        "sort": { "order": sort_order, "field": sort_field },
        "limit": limit,
    });
    let resp = raw_client::raw_post_with_headers(
        cfg,
        &format!("{BASE}/profiles/list"),
        body,
        extra_headers,
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to list profile events: {e:?}"))?;
    let count = resp.get("data").and_then(|d| d.as_array()).map(|a| a.len());
    let meta = Metadata {
        count,
        truncated: count == Some(limit as usize),
        command: Some("pup profiling profiles list".to_string()),
        next_action: None,
    };
    formatter::format_and_print(
        &resp,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&meta),
        cfg.jq.as_deref(),
    )
}

pub async fn profiles_download(
    cfg: &Config,
    profile_id: String,
    event_id: String,
    file_name: Option<String>,
    output_file: String,
    extra_headers: &[(&str, &str)],
) -> Result<()> {
    let mut query: Vec<(&str, &str)> = vec![("eventId", event_id.as_str())];
    if let Some(ref v) = file_name {
        query.push(("fileName", v.as_str()));
    }
    let path = format!("{BASE}/profiles/{profile_id}/download");
    let resp = raw_client::raw_request(cfg, "GET", &path, &query, None, None, "*/*", extra_headers)
        .await
        .map_err(|e| anyhow::anyhow!("failed to download profile: {e:?}"))?;
    std::fs::write(&output_file, &resp.bytes)
        .map_err(|e| anyhow::anyhow!("failed to write to '{output_file}': {e}"))?;
    eprintln!(
        "Wrote {} bytes ({}) to '{output_file}'.",
        resp.bytes.len(),
        if resp.content_type.is_empty() {
            "unknown content type"
        } else {
            &resp.content_type
        },
    );
    Ok(())
}

// ---- Services ----

pub async fn services_list(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    extra_headers: &[(&str, &str)],
) -> Result<()> {
    let body = json!({ "filter": filter_json(&query, &from, &to)? });
    let resp =
        raw_client::raw_post_with_headers(cfg, &format!("{BASE}/services"), body, extra_headers)
            .await
            .map_err(|e| anyhow::anyhow!("failed to list profiled services: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Profile types ----

#[allow(clippy::too_many_arguments)]
pub async fn profile_types_list(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    trace_id: Option<String>,
    span_id: Option<String>,
    extra_headers: &[(&str, &str)],
) -> Result<()> {
    let mut body = json!({ "filter": filter_json(&query, &from, &to)? });
    if let Some(trace_id) = trace_id {
        body["traceContext"] = json!({
            "traceId": trace_id,
            "spanId": span_id,
            "timeHint": null,
        });
    }
    let resp = raw_client::raw_post_with_headers(
        cfg,
        &format!("{BASE}/profile-types"),
        body,
        extra_headers,
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to list profile types: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Explore ----

#[allow(clippy::too_many_arguments)]
pub async fn explore_flamegraph(
    cfg: &Config,
    profile_type: String,
    query: String,
    from: String,
    to: String,
    trace_id: Option<String>,
    span_id: Option<String>,
    profile_id: Option<String>,
    event_id: Option<String>,
    attribute: Option<String>,
    percent_cutoff: f64,
    limit_top_stacktraces: i32,
    max_stack_trace_size: i32,
    frame_regex_filter: Option<String>,
    endpoint_regex_filter: Option<String>,
    attribute_values_regex_filter: Option<String>,
    frame_format: String,
    frame_grouping: String,
    bypass_kind_truncation: bool,
    extra_headers: &[(&str, &str)],
) -> Result<()> {
    if trace_id.is_none() && profile_id.is_none() && query.trim().is_empty() {
        anyhow::bail!(
            "one of --query, --trace-id, or --profile-id is required to scope the flame graph"
        );
    }
    if profile_id.is_some() != event_id.is_some() {
        anyhow::bail!("--profile-id and --event-id must be used together");
    }

    let mut body = json!({
        "filter": filter_json(&query, &from, &to)?,
        "profileType": profile_type,
        "attribute": attribute,
        "percentCutoff": percent_cutoff,
        "limitTopStacktraces": limit_top_stacktraces,
        "maxStackTraceSize": max_stack_trace_size,
        "frameRegexFilter": frame_regex_filter,
        "endpointRegexFilter": endpoint_regex_filter,
        "attributeValuesRegexFilter": attribute_values_regex_filter,
        "frameFormat": if frame_format == "full" { "FULL" } else { "SIMPLE_STRING" },
        "frameGrouping": if frame_grouping == "line" { "LINE" } else { "METHOD" },
        "bypassKindTruncation": bypass_kind_truncation,
    });
    if let Some(trace_id) = trace_id {
        body["traceContext"] = json!({
            "traceId": trace_id,
            "spanId": span_id,
            "timeHint": null,
        });
    }
    if let Some(profile_id) = profile_id {
        body["profileContext"] = json!({
            "profileId": profile_id,
            "eventId": event_id,
        });
    }

    let resp = raw_client::raw_post_with_headers(
        cfg,
        &format!("{BASE}/explore/flamegraph"),
        body,
        extra_headers,
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to explore flame graph: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {
    use crate::config::{Config, OutputFormat};
    use crate::test_support::*;

    fn no_auth_config() -> Config {
        Config {
            api_key: None,
            app_key: None,
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        }
    }

    // ---- header parsing ----

    #[test]
    fn test_parse_extra_headers_ok() {
        let parsed = super::parse_extra_headers(&[
            "test-drive-hummer-aurora: 1".to_string(),
            "test-drive-service-abc123=1".to_string(),
        ])
        .expect("should parse");
        assert_eq!(
            parsed,
            vec![
                ("test-drive-hummer-aurora".to_string(), "1".to_string()),
                ("test-drive-service-abc123".to_string(), "1".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_extra_headers_empty() {
        let parsed = super::parse_extra_headers(&[]).expect("should parse");
        assert!(parsed.is_empty());
    }

    #[test]
    fn test_parse_extra_headers_missing_separator() {
        let result = super::parse_extra_headers(&["no-separator-here".to_string()]);
        assert!(
            result.is_err(),
            "expected error but got ok: {:?}",
            result.ok()
        );
    }

    #[test]
    fn test_parse_extra_headers_empty_name() {
        let result = super::parse_extra_headers(&[": value".to_string()]);
        assert!(
            result.is_err(),
            "expected error but got ok: {:?}",
            result.ok()
        );
    }

    // ---- profiles list ----

    #[tokio::test]
    async fn test_profiling_profiles_list_ok() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"data":[{"id":"evt-1","type":"profile","attributes":{"start":"2026-08-14T00:00:00Z"}}]}"#;
        let _mock = server
            .mock("POST", "/api/unstable/profiling/pup/profiles/list")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let result = super::profiles_list(
            &cfg,
            "service:my-service".into(),
            "1h".into(),
            "now".into(),
            100,
            "asc".into(),
            "start".into(),
            &[],
        )
        .await;
        assert!(result.is_ok(), "profiles_list failed: {:?}", result.err());

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_profiles_list_omits_org_and_user() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock("POST", "/api/unstable/profiling/pup/profiles/list")
            .match_body(mockito::Matcher::AllOf(vec![mockito::Matcher::Regex(
                r#""limit":100"#.into(),
            )]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        let result = super::profiles_list(
            &cfg,
            "".into(),
            "1h".into(),
            "now".into(),
            100,
            "asc".into(),
            "start".into(),
            &[],
        )
        .await;
        assert!(result.is_ok(), "profiles_list failed: {:?}", result.err());

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_profiles_list_sends_extra_headers() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock("POST", "/api/unstable/profiling/pup/profiles/list")
            .match_header("test-drive-hummer-aurora", "1")
            .match_header("test-drive-service-abc123", "1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        let result = super::profiles_list(
            &cfg,
            "".into(),
            "1h".into(),
            "now".into(),
            100,
            "asc".into(),
            "start".into(),
            &[
                ("test-drive-hummer-aurora", "1"),
                ("test-drive-service-abc123", "1"),
            ],
        )
        .await;
        assert!(result.is_ok(), "profiles_list failed: {:?}", result.err());

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_profiles_list_forbidden() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["The pup API is not enabled for this organization."]}"#)
            .create_async()
            .await;

        let result = super::profiles_list(
            &cfg,
            "".into(),
            "1h".into(),
            "now".into(),
            100,
            "asc".into(),
            "start".into(),
            &[],
        )
        .await;
        assert!(
            result.is_err(),
            "expected error but got ok: {:?}",
            result.ok()
        );

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_profiles_list_no_auth() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let cfg = no_auth_config();
        let result = super::profiles_list(
            &cfg,
            "".into(),
            "1h".into(),
            "now".into(),
            100,
            "asc".into(),
            "start".into(),
            &[],
        )
        .await;
        assert!(result.is_err(), "should fail without auth");
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    // ---- profiles download ----

    #[tokio::test]
    async fn test_profiling_profiles_download_ok() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let jfr_bytes: &[u8] = b"FLR\x00fake-jfr-bytes";
        let _mock = server
            .mock(
                "GET",
                "/api/unstable/profiling/pup/profiles/prof-123/download",
            )
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("eventId".into(), "evt-id-456".into()),
                mockito::Matcher::UrlEncoded("fileName".into(), "profile.jfr".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/octet-stream")
            .with_body(jfr_bytes)
            .create_async()
            .await;

        let out = std::env::temp_dir().join("pup_test_profiling_download.jfr");
        let out_str = out.to_str().unwrap().to_string();
        let result = super::profiles_download(
            &cfg,
            "prof-123".into(),
            "evt-id-456".into(),
            Some("profile.jfr".into()),
            out_str,
            &[],
        )
        .await;
        assert!(
            result.is_ok(),
            "profiles_download failed: {:?}",
            result.err()
        );
        let written = std::fs::read(&out).expect("expected output file to exist");
        assert_eq!(written, jfr_bytes);
        let _ = std::fs::remove_file(&out);

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_profiles_download_sends_extra_headers() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let jfr_bytes: &[u8] = b"FLR\x00fake-jfr-bytes";
        let _mock = server
            .mock(
                "GET",
                "/api/unstable/profiling/pup/profiles/prof-123/download",
            )
            .match_header("test-drive-hummer-aurora", "1")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/octet-stream")
            .with_body(jfr_bytes)
            .create_async()
            .await;

        let out = std::env::temp_dir().join("pup_test_profiling_download_headers.jfr");
        let out_str = out.to_str().unwrap().to_string();
        let result = super::profiles_download(
            &cfg,
            "prof-123".into(),
            "evt-id-456".into(),
            None,
            out_str,
            &[("test-drive-hummer-aurora", "1")],
        )
        .await;
        assert!(
            result.is_ok(),
            "profiles_download failed: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(&out);

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_profiles_download_zip_no_file_name() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let zip_bytes: &[u8] = b"PK\x03\x04fake-zip-bytes";
        let _mock = server
            .mock(
                "GET",
                "/api/unstable/profiling/pup/profiles/prof-123/download",
            )
            .match_query(mockito::Matcher::UrlEncoded(
                "eventId".into(),
                "evt-id-456".into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/octet-stream")
            .with_body(zip_bytes)
            .create_async()
            .await;

        let out = std::env::temp_dir().join("pup_test_profiling_download.zip");
        let out_str = out.to_str().unwrap().to_string();
        let result = super::profiles_download(
            &cfg,
            "prof-123".into(),
            "evt-id-456".into(),
            None,
            out_str,
            &[],
        )
        .await;
        assert!(
            result.is_ok(),
            "profiles_download failed: {:?}",
            result.err()
        );
        let written = std::fs::read(&out).expect("expected output file to exist");
        assert_eq!(written, zip_bytes);
        let _ = std::fs::remove_file(&out);

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_profiles_download_not_found() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(404)
            .with_body("not found")
            .create_async()
            .await;

        let out = std::env::temp_dir().join("pup_test_profiling_download_404.jfr");
        let result = super::profiles_download(
            &cfg,
            "missing".into(),
            "evt-id-456".into(),
            None,
            out.to_str().unwrap().to_string(),
            &[],
        )
        .await;
        assert!(
            result.is_err(),
            "expected error but got ok: {:?}",
            result.ok()
        );
        assert!(!out.exists(), "output file should not be written on error");

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_profiles_download_write_failure() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_any(&mut server, "GET", "ignored").await;
        // A path inside a nonexistent directory can never be written to.
        let bad_path = std::env::temp_dir()
            .join("pup_test_profiling_no_such_dir")
            .join("out.jfr")
            .to_str()
            .unwrap()
            .to_string();
        let result = super::profiles_download(
            &cfg,
            "prof-123".into(),
            "evt-id-456".into(),
            None,
            bad_path,
            &[],
        )
        .await;
        assert!(result.is_err(), "expected write failure");

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    // ---- services ----

    #[tokio::test]
    async fn test_profiling_services_list_ok() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"data":[{"id":"my-service","type":"profiledService","attributes":{"service":"my-service","family":["java"]}}]}"#;
        let _mock = mock_any(&mut server, "POST", body).await;

        let result =
            super::services_list(&cfg, "env:prod".into(), "1h".into(), "now".into(), &[]).await;
        assert!(result.is_ok(), "services_list failed: {:?}", result.err());

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_services_list_empty() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_any(&mut server, "POST", r#"{"data":[]}"#).await;

        let result = super::services_list(&cfg, "".into(), "1h".into(), "now".into(), &[]).await;
        assert!(result.is_ok(), "services_list failed: {:?}", result.err());

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_services_list_error() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(403)
            .with_body(r#"{"errors":["forbidden"]}"#)
            .create_async()
            .await;

        let result = super::services_list(&cfg, "".into(), "1h".into(), "now".into(), &[]).await;
        assert!(
            result.is_err(),
            "expected error but got ok: {:?}",
            result.ok()
        );

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    // ---- profile-types ----

    #[tokio::test]
    async fn test_profiling_profile_types_list_ok() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"data":[{"id":"cpu-time","type":"profileType","attributes":{"family":"java","profileType":"cpu-time","unit":"nanoseconds","availableAttributes":["line"]}}]}"#;
        let _mock = mock_any(&mut server, "POST", body).await;

        let result = super::profile_types_list(
            &cfg,
            "service:my-service".into(),
            "1h".into(),
            "now".into(),
            None,
            None,
            &[],
        )
        .await;
        assert!(
            result.is_ok(),
            "profile_types_list failed: {:?}",
            result.err()
        );

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_profile_types_list_empty_state() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"data":[],"meta":{"emptyStateReason":{"reason":"NO_DATA","description":"no profiles found"}}}"#;
        let _mock = mock_any(&mut server, "POST", body).await;

        let result =
            super::profile_types_list(&cfg, "".into(), "1h".into(), "now".into(), None, None, &[])
                .await;
        assert!(
            result.is_ok(),
            "profile_types_list failed: {:?}",
            result.err()
        );

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_profile_types_list_with_trace_context() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock("POST", "/api/unstable/profiling/pup/profile-types")
            .match_body(mockito::Matcher::AllOf(vec![
                mockito::Matcher::Regex(r#""traceId":"trace-abc""#.into()),
                mockito::Matcher::Regex(r#""spanId":"span-123""#.into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        let result = super::profile_types_list(
            &cfg,
            "".into(),
            "1h".into(),
            "now".into(),
            Some("trace-abc".into()),
            Some("span-123".into()),
            &[],
        )
        .await;
        assert!(
            result.is_ok(),
            "profile_types_list failed: {:?}",
            result.err()
        );

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_profile_types_list_error() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(500)
            .with_body(r#"{"errors":["server error"]}"#)
            .create_async()
            .await;

        let result =
            super::profile_types_list(&cfg, "".into(), "1h".into(), "now".into(), None, None, &[])
                .await;
        assert!(
            result.is_err(),
            "expected error but got ok: {:?}",
            result.ok()
        );

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    // ---- explore flamegraph ----

    #[allow(clippy::type_complexity)]
    type FlamegraphArgs = (
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        f64,
        i32,
        i32,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
        bool,
    );

    fn flamegraph_args() -> FlamegraphArgs {
        (
            "cpu-time".into(),
            "service:my-service".into(),
            "1h".into(),
            "now".into(),
            None,
            None,
            None,
            None,
            None,
            0.0,
            0,
            0,
            None,
            None,
            None,
            "simple-string".into(),
            "method".into(),
            false,
        )
    }

    #[tokio::test]
    async fn test_profiling_explore_flamegraph_ok() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"sortedStacktracesWithValues":[],"totalMatchingValue":{"value":0.0,"unit":"nanoseconds","isPerMinute":false},"totalValue":{"value":0.0,"unit":"nanoseconds","isPerMinute":false},"duration":0.0,"topEndpoints":{},"topAttributeValues":{},"visualizationLink":{"title":"","url":""}}"#;
        let _mock = mock_any(&mut server, "POST", body).await;

        let (
            profile_type,
            query,
            from,
            to,
            trace_id,
            span_id,
            profile_id,
            event_id,
            attribute,
            percent_cutoff,
            limit_top_stacktraces,
            max_stack_trace_size,
            frame_regex_filter,
            endpoint_regex_filter,
            attribute_values_regex_filter,
            frame_format,
            frame_grouping,
            bypass_kind_truncation,
        ) = flamegraph_args();
        let result = super::explore_flamegraph(
            &cfg,
            profile_type,
            query,
            from,
            to,
            trace_id,
            span_id,
            profile_id,
            event_id,
            attribute,
            percent_cutoff,
            limit_top_stacktraces,
            max_stack_trace_size,
            frame_regex_filter,
            endpoint_regex_filter,
            attribute_values_regex_filter,
            frame_format,
            frame_grouping,
            bypass_kind_truncation,
            &[],
        )
        .await;
        assert!(
            result.is_ok(),
            "explore_flamegraph failed: {:?}",
            result.err()
        );

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_explore_flamegraph_requires_scope() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let cfg = test_config("http://unused.local");

        let (
            profile_type,
            _query,
            from,
            to,
            trace_id,
            span_id,
            profile_id,
            event_id,
            attribute,
            percent_cutoff,
            limit_top_stacktraces,
            max_stack_trace_size,
            frame_regex_filter,
            endpoint_regex_filter,
            attribute_values_regex_filter,
            frame_format,
            frame_grouping,
            bypass_kind_truncation,
        ) = flamegraph_args();
        let result = super::explore_flamegraph(
            &cfg,
            profile_type,
            "".into(),
            from,
            to,
            trace_id,
            span_id,
            profile_id,
            event_id,
            attribute,
            percent_cutoff,
            limit_top_stacktraces,
            max_stack_trace_size,
            frame_regex_filter,
            endpoint_regex_filter,
            attribute_values_regex_filter,
            frame_format,
            frame_grouping,
            bypass_kind_truncation,
            &[],
        )
        .await;
        assert!(result.is_err(), "expected a scope-validation error");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("one of --query, --trace-id, or --profile-id"));

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_explore_flamegraph_frame_format_full() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock("POST", "/api/unstable/profiling/pup/explore/flamegraph")
            .match_body(mockito::Matcher::Regex(r#""frameFormat":"FULL""#.into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"sortedStacktracesWithValues":[],"totalMatchingValue":{"value":0.0,"unit":"nanoseconds","isPerMinute":false},"totalValue":{"value":0.0,"unit":"nanoseconds","isPerMinute":false},"duration":0.0,"topEndpoints":{},"topAttributeValues":{},"visualizationLink":{"title":"","url":""}}"#,
            )
            .create_async()
            .await;

        let (
            profile_type,
            query,
            from,
            to,
            trace_id,
            span_id,
            profile_id,
            event_id,
            attribute,
            percent_cutoff,
            limit_top_stacktraces,
            max_stack_trace_size,
            frame_regex_filter,
            endpoint_regex_filter,
            attribute_values_regex_filter,
            _frame_format,
            frame_grouping,
            bypass_kind_truncation,
        ) = flamegraph_args();
        let result = super::explore_flamegraph(
            &cfg,
            profile_type,
            query,
            from,
            to,
            trace_id,
            span_id,
            profile_id,
            event_id,
            attribute,
            percent_cutoff,
            limit_top_stacktraces,
            max_stack_trace_size,
            frame_regex_filter,
            endpoint_regex_filter,
            attribute_values_regex_filter,
            "full".into(),
            frame_grouping,
            bypass_kind_truncation,
            &[],
        )
        .await;
        assert!(
            result.is_ok(),
            "explore_flamegraph failed: {:?}",
            result.err()
        );

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_explore_flamegraph_tolerates_null_optional_fields() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"sortedStacktracesWithValues":[],"totalMatchingValue":{"value":0.0,"unit":"nanoseconds","isPerMinute":false},"totalValue":{"value":0.0,"unit":"nanoseconds","isPerMinute":false},"duration":0.0,"topEndpoints":{},"topAttributeValues":{},"visualizationLink":{"title":"","url":""},"message":null,"emptyStateReason":null,"featureUpgrade":null,"availableProfileTypes":null,"availableProfileTypeDetails":null}"#;
        let _mock = mock_any(&mut server, "POST", body).await;

        let (
            profile_type,
            query,
            from,
            to,
            trace_id,
            span_id,
            profile_id,
            event_id,
            attribute,
            percent_cutoff,
            limit_top_stacktraces,
            max_stack_trace_size,
            frame_regex_filter,
            endpoint_regex_filter,
            attribute_values_regex_filter,
            frame_format,
            frame_grouping,
            bypass_kind_truncation,
        ) = flamegraph_args();
        let result = super::explore_flamegraph(
            &cfg,
            profile_type,
            query,
            from,
            to,
            trace_id,
            span_id,
            profile_id,
            event_id,
            attribute,
            percent_cutoff,
            limit_top_stacktraces,
            max_stack_trace_size,
            frame_regex_filter,
            endpoint_regex_filter,
            attribute_values_regex_filter,
            frame_format,
            frame_grouping,
            bypass_kind_truncation,
            &[],
        )
        .await;
        assert!(
            result.is_ok(),
            "should tolerate null optional response fields: {:?}",
            result.err()
        );

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_explore_flamegraph_validation_error() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(400)
            .with_body(r#"{"errors":["profileType is required"]}"#)
            .create_async()
            .await;

        let (
            profile_type,
            query,
            from,
            to,
            trace_id,
            span_id,
            profile_id,
            event_id,
            attribute,
            percent_cutoff,
            limit_top_stacktraces,
            max_stack_trace_size,
            frame_regex_filter,
            endpoint_regex_filter,
            attribute_values_regex_filter,
            frame_format,
            frame_grouping,
            bypass_kind_truncation,
        ) = flamegraph_args();
        let result = super::explore_flamegraph(
            &cfg,
            profile_type,
            query,
            from,
            to,
            trace_id,
            span_id,
            profile_id,
            event_id,
            attribute,
            percent_cutoff,
            limit_top_stacktraces,
            max_stack_trace_size,
            frame_regex_filter,
            endpoint_regex_filter,
            attribute_values_regex_filter,
            frame_format,
            frame_grouping,
            bypass_kind_truncation,
            &[],
        )
        .await;
        assert!(
            result.is_err(),
            "expected error but got ok: {:?}",
            result.ok()
        );

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_profiling_explore_flamegraph_no_auth() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let cfg = no_auth_config();

        let (
            profile_type,
            query,
            from,
            to,
            trace_id,
            span_id,
            profile_id,
            event_id,
            attribute,
            percent_cutoff,
            limit_top_stacktraces,
            max_stack_trace_size,
            frame_regex_filter,
            endpoint_regex_filter,
            attribute_values_regex_filter,
            frame_format,
            frame_grouping,
            bypass_kind_truncation,
        ) = flamegraph_args();
        let result = super::explore_flamegraph(
            &cfg,
            profile_type,
            query,
            from,
            to,
            trace_id,
            span_id,
            profile_id,
            event_id,
            attribute,
            percent_cutoff,
            limit_top_stacktraces,
            max_stack_trace_size,
            frame_regex_filter,
            endpoint_regex_filter,
            attribute_values_regex_filter,
            frame_format,
            frame_grouping,
            bypass_kind_truncation,
            &[],
        )
        .await;
        assert!(result.is_err(), "should fail without auth");

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }
}

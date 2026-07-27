use anyhow::{bail, Result};

use crate::config::Config;
use crate::formatter;
use crate::raw_client;
use crate::util_ext;

fn parse_sort(sort: &str) -> Result<&'static str> {
    match sort {
        "asc" | "timestamp" => Ok("asc"),
        "desc" | "-timestamp" => Ok("desc"),
        _ => bail!("invalid --sort value: {sort:?}\nExpected: asc, desc, timestamp, or -timestamp"),
    }
}

fn build_search_body(
    query: String,
    from_ms: i64,
    to_ms: i64,
    limit: i32,
    sort: &str,
) -> Result<serde_json::Value> {
    if limit <= 0 {
        bail!("--limit must be a positive integer");
    }

    Ok(serde_json::json!({
        "list": {
            "indexes": ["databasequery"],
            "limit": limit,
            "search": { "query": query },
            "sorts": [
                { "time": { "order": parse_sort(sort)? } }
            ],
            "time": {
                "from": from_ms,
                "to": to_ms
            }
        }
    }))
}

pub async fn samples_search(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    limit: i32,
    sort: String,
) -> Result<()> {
    cfg.validate_auth()?;

    let from_ms = util_ext::parse_time_to_unix_millis(&from)?;
    let to_ms = util_ext::parse_time_to_unix_millis(&to)?;
    let body = build_search_body(query, from_ms, to_ms, limit, &sort)?;

    let resp = raw_client::raw_post(cfg, "/api/v1/logs-analytics/list?type=databasequery", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search DBM query samples: {e:?}"))?;

    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    use super::*;

    #[test]
    fn test_parse_sort_ascending_values() {
        assert_eq!(parse_sort("asc").unwrap(), "asc");
        assert_eq!(parse_sort("timestamp").unwrap(), "asc");
    }

    #[test]
    fn test_parse_sort_descending_values() {
        assert_eq!(parse_sort("desc").unwrap(), "desc");
        assert_eq!(parse_sort("-timestamp").unwrap(), "desc");
    }

    #[test]
    fn test_parse_sort_invalid() {
        assert!(parse_sort("invalid").is_err());
    }

    #[test]
    fn test_build_search_body() {
        let body = build_search_body(
            "service:db".into(),
            1_700_000_000_000,
            1_700_000_060_000,
            10,
            "desc",
        )
        .unwrap();

        assert_eq!(
            body["list"]["indexes"],
            serde_json::json!(["databasequery"])
        );
        assert_eq!(body["list"]["limit"], 10);
        assert_eq!(body["list"]["search"]["query"], "service:db");
        assert_eq!(
            body["list"]["sorts"],
            serde_json::json!([{ "time": { "order": "desc" } }])
        );
        assert_eq!(body["list"]["time"]["from"], 1_700_000_000_000_i64);
        assert_eq!(body["list"]["time"]["to"], 1_700_000_060_000_i64);
    }

    #[test]
    fn test_build_search_body_rejects_zero_limit() {
        let err = build_search_body(
            "service:db".into(),
            1_700_000_000_000,
            1_700_000_060_000,
            0,
            "desc",
        )
        .unwrap_err();

        assert_eq!(err.to_string(), "--limit must be a positive integer");
    }

    #[test]
    fn test_build_search_body_rejects_negative_limit() {
        let err = build_search_body(
            "service:db".into(),
            1_700_000_000_000,
            1_700_000_060_000,
            -1,
            "desc",
        )
        .unwrap_err();

        assert_eq!(err.to_string(), "--limit must be a positive integer");
    }

    #[tokio::test]
    async fn test_dbm_samples_search_uses_documented_payload() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock("POST", "/api/v1/logs-analytics/list")
            .match_query(mockito::Matcher::UrlEncoded(
                "type".into(),
                "databasequery".into(),
            ))
            .match_body(mockito::Matcher::Regex(
                r#""list":\{"indexes":\["databasequery"\]"#.to_string(),
            ))
            .match_body(mockito::Matcher::Regex(
                r#""query":"service:db""#.to_string(),
            ))
            .match_body(mockito::Matcher::Regex(
                r#""sorts":\[\{"time":\{"order":"asc"\}\}\]"#.to_string(),
            ))
            .match_body(mockito::Matcher::Regex(r#""from":\d{13}"#.to_string()))
            .match_body(mockito::Matcher::Regex(r#""to":\d{13}"#.to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        let result = super::samples_search(
            &cfg,
            "service:db".into(),
            "1h".into(),
            "now".into(),
            10,
            "asc".into(),
        )
        .await;

        assert!(
            result.is_ok(),
            "dbm samples search failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_dbm_samples_search_accepts_oauth_bearer_token() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        // Simulate OAuth-only auth: bearer token configured, no API/APP keys.
        cfg.api_key = None;
        cfg.app_key = None;
        cfg.access_token = Some("oauth-bearer-token".into());
        std::env::remove_var("DD_API_KEY");
        std::env::remove_var("DD_APP_KEY");

        let _mock = server
            .mock("POST", "/api/v1/logs-analytics/list")
            .match_query(mockito::Matcher::UrlEncoded(
                "type".into(),
                "databasequery".into(),
            ))
            .match_header("Authorization", "Bearer oauth-bearer-token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        let result = super::samples_search(
            &cfg,
            "service:db".into(),
            "1h".into(),
            "now".into(),
            10,
            "asc".into(),
        )
        .await;

        assert!(
            result.is_ok(),
            "dbm samples search with OAuth bearer failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_dbm_samples_search_rejects_when_no_auth_configured() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        cfg.api_key = None;
        cfg.app_key = None;
        cfg.access_token = None;
        std::env::remove_var("DD_API_KEY");
        std::env::remove_var("DD_APP_KEY");

        let result = super::samples_search(
            &cfg,
            "service:db".into(),
            "1h".into(),
            "now".into(),
            10,
            "asc".into(),
        )
        .await;

        assert!(result.is_err(), "expected auth error when no credentials");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("authentication"),
            "expected auth error message, got: {err}"
        );
        cleanup_env();
    }
}

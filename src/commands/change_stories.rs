use anyhow::Result;

use crate::config::Config;
use crate::formatter;
use crate::raw_client;
use crate::util_ext;

#[allow(clippy::too_many_arguments)]
pub async fn list(
    cfg: &Config,
    service: String,
    env: Option<String>,
    from: String,
    to: String,
    story_types: Vec<String>,
    filter_tags: Option<String>,
    token_limit: Option<i64>,
) -> Result<()> {
    let from_ms = util_ext::parse_time_to_unix_millis(&from)
        .map_err(|e| anyhow::anyhow!("invalid --from: {e}"))?;
    let to_ms = util_ext::parse_time_to_unix_millis(&to)
        .map_err(|e| anyhow::anyhow!("invalid --to: {e}"))?;
    let from_ms_str = from_ms.to_string();
    let to_ms_str = to_ms.to_string();

    let mut query: Vec<(&str, String)> = vec![
        ("service_name", service.clone()),
        ("start_ts", from_ms_str),
        ("end_ts", to_ms_str),
    ];
    if let Some(e) = &env {
        if !e.is_empty() {
            query.push(("env", e.clone()));
        }
    }
    if let Some(ft) = &filter_tags {
        if !ft.is_empty() {
            query.push(("filter_tags", ft.clone()));
        }
    }
    for st in &story_types {
        query.push(("story_types", st.clone()));
    }
    if let Some(tl) = token_limit {
        query.push(("token_limit", tl.to_string()));
    }

    let q_refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let data = raw_client::raw_get(cfg, "/api/unstable/change-stories/cli", &q_refs).await?;

    let count = data
        .get("stories")
        .and_then(|v| v.as_array())
        .map(|a| a.len());
    let truncated = data
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let meta = formatter::Metadata {
        count,
        truncated,
        command: Some("change-stories list".into()),
        next_action: None,
    };

    formatter::format_and_print(
        &data,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&meta),
        cfg.jq.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    const ISO_FROM: &str = "2024-01-15T00:00:00Z";
    const ISO_TO: &str = "2024-01-15T01:00:00Z";

    #[tokio::test]
    async fn test_list_happy_path() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/unstable/change-stories/cli")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("service_name".into(), "web".into()),
                mockito::Matcher::Regex("start_ts=".into()),
                mockito::Matcher::Regex("end_ts=".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"stories":[],"truncated":false}"#)
            .create_async()
            .await;

        let result = super::list(
            &cfg,
            "web".into(),
            None,
            ISO_FROM.into(),
            ISO_TO.into(),
            vec![],
            None,
            None,
        )
        .await;
        assert!(result.is_ok(), "list failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_list_with_all_filters() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/unstable/change-stories/cli")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("service_name".into(), "api".into()),
                mockito::Matcher::UrlEncoded("env".into(), "prod".into()),
                mockito::Matcher::UrlEncoded("filter_tags".into(), "version:1.2.3".into()),
                mockito::Matcher::UrlEncoded("token_limit".into(), "4000".into()),
                mockito::Matcher::UrlEncoded("story_types".into(), "deployment".into()),
                mockito::Matcher::Regex("start_ts=".into()),
                mockito::Matcher::Regex("end_ts=".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"stories":[{"id":"s1"}],"truncated":true}"#)
            .create_async()
            .await;

        let result = super::list(
            &cfg,
            "api".into(),
            Some("prod".into()),
            ISO_FROM.into(),
            ISO_TO.into(),
            vec!["deployment".into()],
            Some("version:1.2.3".into()),
            Some(4000),
        )
        .await;
        assert!(result.is_ok(), "list failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_list_repeats_story_types() {
        // The gorilla-schema decoder on the server expects `story_types` to be
        // repeated once per value; assert reqwest emits the same key twice
        // rather than a comma-joined value.
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/unstable/change-stories/cli")
            .match_query(mockito::Matcher::Regex(
                r"story_types=deployment.*story_types=feature_flag".into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"stories":[]}"#)
            .create_async()
            .await;

        let result = super::list(
            &cfg,
            "api".into(),
            None,
            ISO_FROM.into(),
            ISO_TO.into(),
            vec!["deployment".into(), "feature_flag".into()],
            None,
            None,
        )
        .await;
        assert!(result.is_ok(), "list failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_list_agent_envelope() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        cfg.agent_mode = true;

        let mock = server
            .mock("GET", "/api/unstable/change-stories/cli")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"stories":[{"id":"s1"},{"id":"s2"}],"truncated":false}"#)
            .create_async()
            .await;

        let result = super::list(
            &cfg,
            "web".into(),
            None,
            ISO_FROM.into(),
            ISO_TO.into(),
            vec![],
            None,
            None,
        )
        .await;
        assert!(
            result.is_ok(),
            "list in agent mode failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_list_accepts_relative_from() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/unstable/change-stories/cli")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("service_name".into(), "web".into()),
                mockito::Matcher::Regex("start_ts=".into()),
                mockito::Matcher::Regex("end_ts=".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"stories":[],"truncated":false}"#)
            .create_async()
            .await;

        let result = super::list(
            &cfg,
            "web".into(),
            None,
            "1h".into(),
            "now".into(),
            vec![],
            None,
            None,
        )
        .await;
        assert!(
            result.is_ok(),
            "relative --from should be accepted: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_list_http_error() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        server
            .mock("GET", "/api/unstable/change-stories/cli")
            .match_query(mockito::Matcher::Any)
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["bad start_ts"]}"#)
            .create_async()
            .await;

        let result = super::list(
            &cfg,
            "web".into(),
            None,
            ISO_FROM.into(),
            ISO_TO.into(),
            vec![],
            None,
            None,
        )
        .await;
        assert!(result.is_err(), "expected error on HTTP 400");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_list_rejects_invalid_from() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Should fail at parse, before any HTTP call.
        let result = super::list(
            &cfg,
            "web".into(),
            None,
            "not-a-timestamp".into(),
            ISO_TO.into(),
            vec![],
            None,
            None,
        )
        .await;
        assert!(result.is_err(), "expected error on invalid --from");
        assert!(
            result.unwrap_err().to_string().contains("--from"),
            "error should name --from flag"
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_list_rejects_invalid_to() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Should fail at parse, before any HTTP call.
        let result = super::list(
            &cfg,
            "web".into(),
            None,
            ISO_FROM.into(),
            "not-a-timestamp".into(),
            vec![],
            None,
            None,
        )
        .await;
        assert!(result.is_err(), "expected error on invalid --to");
        assert!(
            result.unwrap_err().to_string().contains("--to"),
            "error should name --to flag"
        );
        cleanup_env();
    }
}

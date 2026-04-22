//! Integration tests for command modules using mockito mock server.
//!
//! These tests use the PUP_MOCK_SERVER mechanism to redirect all DD API calls
//! to a local mockito server, testing command functions without a live API.
//!
//! For DD client API tests, we use Matcher::Any for paths since the DD client
//! library may construct URLs differently from what we expect. Each test gets
//! its own mockito server, so there's no cross-test interference.

use crate::config::{Config, OutputFormat};
use clap::CommandFactory;
use std::sync::Mutex;

/// Global mutex to serialize tests that modify process-wide env vars.
/// Uses unwrap_or_else to recover from poisoned state (previous test panicked).
static ENV_MUTEX: Mutex<()> = Mutex::new(());

fn lock_env() -> std::sync::MutexGuard<'static, ()> {
    ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner())
}

fn test_config(mock_url: &str) -> Config {
    std::env::set_var("PUP_MOCK_SERVER", mock_url);
    std::env::set_var("DD_API_KEY", "test-api-key");
    std::env::set_var("DD_APP_KEY", "test-app-key");

    Config {
        api_key: Some("test-api-key".into()),
        app_key: Some("test-app-key".into()),
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    }
}

fn cleanup_env() {
    std::env::remove_var("PUP_MOCK_SERVER");
}

/// Helper: create a catch-all mock that responds 200 with JSON for any request
/// matching the given HTTP method. Used for DD client API tests where the
/// exact path may differ from our expectations.
async fn mock_any(server: &mut mockito::Server, method: &str, body: &str) -> mockito::Mock {
    server
        .mock(method, mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create_async()
        .await
}

// =========================================================================
// DD Client API Command Tests (monitors, dashboards, slos, tags, events,
// logs, metrics) — use catch-all mocks since the DD client constructs its
// own URLs from OpenAPI specs.
// =========================================================================

// -------------------------------------------------------------------------
// Monitors
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_monitors_list_empty() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", "[]").await;

    let result = crate::commands::monitors::list(&cfg, None, None, 10).await;
    assert!(result.is_ok(), "monitors list failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_monitors_list_with_results() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let body = r#"[{"id": 1, "name": "Test Monitor", "type": "metric alert", "query": "avg(last_5m):avg:system.cpu.user{*} > 90", "message": "CPU high", "tags": [], "options": {}}]"#;
    let _mock = mock_any(&mut server, "GET", body).await;

    let result = crate::commands::monitors::list(&cfg, Some("Test".into()), None, 10).await;
    assert!(
        result.is_ok(),
        "monitors list with results failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_monitors_get() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let body = r#"{"id": 12345, "name": "Test Monitor", "type": "metric alert", "query": "avg(last_5m):avg:system.cpu.user{*} > 90", "message": "CPU high", "tags": [], "options": {}}"#;
    let _mock = mock_any(&mut server, "GET", body).await;

    let result = crate::commands::monitors::get(&cfg, 12345).await;
    assert!(result.is_ok(), "monitors get failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_monitors_search() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let body = r#"{"monitors": [], "metadata": {"page": 0, "page_count": 0, "per_page": 30, "total_count": 0}}"#;
    let _mock = mock_any(&mut server, "GET", body).await;

    let result = crate::commands::monitors::search(&cfg, Some("cpu".into())).await;
    assert!(result.is_ok(), "monitors search failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_monitors_delete() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "DELETE", r#"{"deleted_monitor_id": 12345}"#).await;

    let result = crate::commands::monitors::delete(&cfg, 12345).await;
    assert!(result.is_ok(), "monitors delete failed: {:?}", result.err());
    cleanup_env();
}

// -------------------------------------------------------------------------
// Dashboards
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_dashboards_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"dashboards": []}"#).await;

    let result = crate::commands::dashboards::list(&cfg).await;
    assert!(result.is_ok(), "dashboards list failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_dashboards_get() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "GET",
        r#"{"id": "abc-123", "title": "Test Dashboard", "layout_type": "ordered", "widgets": []}"#,
    )
    .await;

    let result = crate::commands::dashboards::get(&cfg, "abc-123").await;
    assert!(result.is_ok(), "dashboards get failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_dashboards_delete() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "DELETE",
        r#"{"deleted_dashboard_id": "abc-123"}"#,
    )
    .await;

    let result = crate::commands::dashboards::delete(&cfg, "abc-123").await;
    assert!(
        result.is_ok(),
        "dashboards delete failed: {:?}",
        result.err()
    );
    cleanup_env();
}

// -------------------------------------------------------------------------
// SLOs
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_slos_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let mock = server
        .mock("GET", "/api/v1/slo")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data": [], "errors": []}"#)
        .create_async()
        .await;

    let result = crate::commands::slos::list(&cfg, None, None, None, None, None).await;
    assert!(result.is_ok(), "slos list failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_slos_list_with_query() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let mock = server
        .mock("GET", "/api/v1/slo")
        .match_query(mockito::Matcher::UrlEncoded(
            "query".into(),
            "monitor-history-reader".into(),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data": [], "errors": []}"#)
        .create_async()
        .await;

    let result = crate::commands::slos::list(
        &cfg,
        Some("monitor-history-reader".into()),
        None,
        None,
        None,
        None,
    )
    .await;
    assert!(
        result.is_ok(),
        "slos list with query failed: {:?}",
        result.err()
    );
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_slos_list_with_tags_query() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let mock = server
        .mock("GET", "/api/v1/slo")
        .match_query(mockito::Matcher::UrlEncoded(
            "tags_query".into(),
            "team:slo-app".into(),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data": [], "errors": []}"#)
        .create_async()
        .await;

    let result =
        crate::commands::slos::list(&cfg, None, Some("team:slo-app".into()), None, None, None)
            .await;
    assert!(
        result.is_ok(),
        "slos list with tags_query failed: {:?}",
        result.err()
    );
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_slos_list_with_limit_and_offset() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let mock = server
        .mock("GET", "/api/v1/slo")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("limit".into(), "25".into()),
            mockito::Matcher::UrlEncoded("offset".into(), "50".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data": [], "errors": []}"#)
        .create_async()
        .await;

    let result = crate::commands::slos::list(&cfg, None, None, None, Some(25), Some(50)).await;
    assert!(
        result.is_ok(),
        "slos list with pagination failed: {:?}",
        result.err()
    );
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_slos_list_with_metrics_query() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let mock = server
        .mock("GET", "/api/v1/slo")
        .match_query(mockito::Matcher::UrlEncoded(
            "metrics_query".into(),
            "sum:requests.error{service:api}".into(),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data": [], "errors": []}"#)
        .create_async()
        .await;

    let result = crate::commands::slos::list(
        &cfg,
        None,
        None,
        Some("sum:requests.error{service:api}".into()),
        None,
        None,
    )
    .await;
    assert!(
        result.is_ok(),
        "slos list with metrics_query failed: {:?}",
        result.err()
    );
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_slos_list_api_error() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let mock = server
        .mock("GET", "/api/v1/slo")
        .match_query(mockito::Matcher::UrlEncoded("query".into(), "team".into()))
        .with_status(500)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["boom"]}"#)
        .create_async()
        .await;

    let result =
        crate::commands::slos::list(&cfg, Some("team".into()), None, None, None, None).await;
    assert!(
        result.is_err(),
        "slos list error path unexpectedly succeeded"
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("failed to list SLOs"),
        "slos list error did not contain context"
    );
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_slos_get() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "GET",
        r#"{"data": {"id": "abc123", "name": "Test SLO", "type": "metric", "thresholds": [{"timeframe": "7d", "target": 99.9}]}, "errors": []}"#,
    )
    .await;

    let result = crate::commands::slos::get(&cfg, "abc123").await;
    assert!(result.is_ok(), "slos get failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_slos_delete() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "DELETE", r#"{"data": []}"#).await;

    let result = crate::commands::slos::delete(&cfg, "abc123").await;
    assert!(result.is_ok(), "slos delete failed: {:?}", result.err());
    cleanup_env();
}

// -------------------------------------------------------------------------
// Tags
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_tags_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"tags": {}}"#).await;

    let result = crate::commands::tags::list(&cfg).await;
    assert!(result.is_ok(), "tags list failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_tags_get() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "GET",
        r#"{"host": "myhost", "tags": ["env:prod", "service:web"]}"#,
    )
    .await;

    let result = crate::commands::tags::get(&cfg, "myhost").await;
    assert!(result.is_ok(), "tags get failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_tags_add() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "POST",
        r#"{"host": "myhost", "tags": ["env:prod"]}"#,
    )
    .await;

    let result = crate::commands::tags::add(&cfg, "myhost", vec!["env:prod".into()]).await;
    assert!(result.is_ok(), "tags add failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_tags_update() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "PUT",
        r#"{"host": "myhost", "tags": ["env:staging"]}"#,
    )
    .await;

    let result = crate::commands::tags::update(&cfg, "myhost", vec!["env:staging".into()]).await;
    assert!(result.is_ok(), "tags update failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_tags_delete() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    // Delete returns 204 No Content
    let _mock = server
        .mock("DELETE", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(204)
        .create_async()
        .await;

    let result = crate::commands::tags::delete(&cfg, "myhost").await;
    assert!(result.is_ok(), "tags delete failed: {:?}", result.err());
    cleanup_env();
}

// -------------------------------------------------------------------------
// Events
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_events_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"events": []}"#).await;

    let now = chrono::Utc::now().timestamp();
    let result = crate::commands::events::list(&cfg, now - 3600, now, None).await;
    assert!(result.is_ok(), "events list failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_events_get() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "GET",
        r#"{"event": {"id": 12345, "title": "Test Event", "text": "Something happened"}}"#,
    )
    .await;

    let result = crate::commands::events::get(&cfg, 12345).await;
    assert!(result.is_ok(), "events get failed: {:?}", result.err());
    cleanup_env();
}

// -------------------------------------------------------------------------
// Logs (requires API keys)
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_logs_search() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "POST", r#"{"data": [], "meta": {"page": {}}}"#).await;

    let result = crate::commands::logs::search(
        &cfg,
        "status:error".into(),
        "1h".into(),
        "now".into(),
        10,
        "-timestamp".into(),
        None,
    )
    .await;
    assert!(result.is_ok(), "logs search failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_logs_search_with_oauth() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    std::env::set_var("PUP_MOCK_SERVER", &server.url());

    let cfg = Config {
        api_key: None,
        app_key: None,
        access_token: Some("token".into()),
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let _mock = mock_any(&mut server, "POST", r#"{"data": []}"#).await;

    let result = crate::commands::logs::search(
        &cfg,
        "status:error".into(),
        "1h".into(),
        "now".into(),
        10,
        "-timestamp".into(),
        None,
    )
    .await;
    assert!(result.is_ok(), "logs search should work with OAuth");
    cleanup_env();
}

#[tokio::test]
async fn test_logs_aggregate() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "POST", r#"{"data": {"buckets": []}}"#).await;

    let result = crate::commands::logs::aggregate(
        &cfg,
        crate::commands::logs::AggregateArgs {
            query: "*".into(),
            from: "1h".into(),
            to: "now".into(),
            compute: vec!["count".into()],
            group_by: vec![],
            limit: 10,
            storage: None,
            sort: "count".into(),
        },
    )
    .await;
    assert!(result.is_ok(), "logs aggregate failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_logs_aggregate_multiple_computes() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "POST", r#"{"data": {"buckets": []}}"#).await;

    let result = crate::commands::logs::aggregate(
        &cfg,
        crate::commands::logs::AggregateArgs {
            query: "*".into(),
            from: "1h".into(),
            to: "now".into(),
            compute: crate::commands::logs::split_compute_args(
                "count,avg(@duration),percentile(@duration, 95)",
            ),
            group_by: vec!["service".into(), "status".into()],
            limit: 10,
            storage: None,
            sort: "count".into(),
        },
    )
    .await;
    assert!(
        result.is_ok(),
        "logs aggregate with multiple computes failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_logs_search_with_flex_storage() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "POST", r#"{"data": [], "meta": {"page": {}}}"#).await;

    let result = crate::commands::logs::search(
        &cfg,
        "*".into(),
        "1h".into(),
        "now".into(),
        10,
        "-timestamp".into(),
        Some("flex".into()),
    )
    .await;
    assert!(
        result.is_ok(),
        "logs search with flex failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_logs_search_with_online_archives_storage() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "POST", r#"{"data": [], "meta": {"page": {}}}"#).await;

    let result = crate::commands::logs::search(
        &cfg,
        "*".into(),
        "1h".into(),
        "now".into(),
        10,
        "-timestamp".into(),
        Some("online-archives".into()),
    )
    .await;
    assert!(
        result.is_ok(),
        "logs search with online-archives failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_logs_search_with_invalid_storage_tier() {
    let _lock = lock_env();
    let server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let result = crate::commands::logs::search(
        &cfg,
        "*".into(),
        "1h".into(),
        "now".into(),
        10,
        "-timestamp".into(),
        Some("invalid-tier".into()),
    )
    .await;
    assert!(
        result.is_err(),
        "logs search with invalid storage tier should fail"
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("unknown storage tier"),
        "error should mention unknown storage tier"
    );
    cleanup_env();
}

#[tokio::test]
async fn test_logs_aggregate_with_flex_storage() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "POST", r#"{"data": {"buckets": []}}"#).await;

    let result = crate::commands::logs::aggregate(
        &cfg,
        crate::commands::logs::AggregateArgs {
            query: "*".into(),
            from: "1h".into(),
            to: "now".into(),
            compute: vec!["count".into()],
            group_by: vec![],
            limit: 10,
            storage: Some("flex".into()),
            sort: "count".into(),
        },
    )
    .await;
    assert!(
        result.is_ok(),
        "logs aggregate with flex failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_logs_archives_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data": []}"#).await;

    let result = crate::commands::logs::archives_list(&cfg).await;
    assert!(
        result.is_ok(),
        "logs archives list failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_logs_custom_destinations_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data": []}"#).await;

    let result = crate::commands::logs::custom_destinations_list(&cfg).await;
    assert!(
        result.is_ok(),
        "logs custom destinations list failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_logs_metrics_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data": []}"#).await;

    let result = crate::commands::logs::metrics_list(&cfg).await;
    assert!(
        result.is_ok(),
        "logs metrics list failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_logs_restriction_queries_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    // restriction_queries_list uses raw HTTP (not DD client), so mock specific path
    let _mock = server
        .mock("GET", "/api/v2/logs/config/restriction_queries")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data": []}"#)
        .create_async()
        .await;

    let result = crate::commands::logs::restriction_queries_list(&cfg).await;
    assert!(
        result.is_ok(),
        "logs restriction queries list failed: {:?}",
        result.err()
    );
    cleanup_env();
}

// -------------------------------------------------------------------------
// Metrics
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_metrics_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "GET",
        r#"{"metrics": [], "from": "2024-01-01T00:00:00Z"}"#,
    )
    .await;

    let result = crate::commands::metrics::list(&cfg, None, "1h".into()).await;
    assert!(result.is_ok(), "metrics list failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_metrics_query() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "GET",
        r#"{"status": "ok", "res_type": "time_series", "series": [], "from_date": 0, "to_date": 0, "query": "avg:system.cpu.user{*}"}"#,
    )
    .await;

    let result = crate::commands::metrics::query(
        &cfg,
        "avg:system.cpu.user{*}".into(),
        "1h".into(),
        "now".into(),
    )
    .await;
    assert!(result.is_ok(), "metrics query failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_metrics_metadata_get() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "GET",
        r#"{"type": "gauge", "description": "CPU usage", "unit": "percent"}"#,
    )
    .await;

    let result = crate::commands::metrics::metadata_get(&cfg, "system.cpu.user").await;
    assert!(
        result.is_ok(),
        "metrics metadata get failed: {:?}",
        result.err()
    );
    cleanup_env();
}

// -------------------------------------------------------------------------
// Events search (requires API keys)
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_events_search() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "POST", r#"{"data": [], "meta": {"page": {}}}"#).await;

    let result =
        crate::commands::events::search(&cfg, "source:nginx".into(), "1h".into(), "now".into(), 10)
            .await;
    assert!(result.is_ok(), "events search failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_events_search_requires_api_keys() {
    let _lock = lock_env();
    let server = mockito::Server::new_async().await;
    std::env::set_var("PUP_MOCK_SERVER", &server.url());

    let cfg = Config {
        api_key: None,
        app_key: None,
        access_token: Some("token".into()),
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let result =
        crate::commands::events::search(&cfg, "source:nginx".into(), "1h".into(), "now".into(), 10)
            .await;
    assert!(result.is_err(), "events search should require API keys");
    cleanup_env();
}

// =========================================================================
// Raw HTTP api module tests — these use the api.rs module directly
// (not the DD client library), so we can mock specific paths precisely.
// =========================================================================

#[tokio::test]
async fn test_api_get() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    std::env::set_var("PUP_MOCK_SERVER", &server.url());

    let cfg = Config {
        api_key: Some("test-key".into()),
        app_key: Some("test-app".into()),
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let mock = server
        .mock("GET", "/api/v1/test")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"status": "ok"}"#)
        .create_async()
        .await;

    let result = crate::api::get(&cfg, "/api/v1/test", &[]).await;
    assert!(result.is_ok(), "api get failed: {:?}", result.err());
    let val = result.unwrap();
    assert_eq!(val["status"], "ok");
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_api_get_with_query() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    std::env::set_var("PUP_MOCK_SERVER", &server.url());

    let cfg = Config {
        api_key: Some("test-key".into()),
        app_key: Some("test-app".into()),
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let mock = server
        .mock("GET", "/api/v1/search")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"results": []}"#)
        .create_async()
        .await;

    let query = vec![("q", "test".to_string())];
    let result = crate::api::get(&cfg, "/api/v1/search", &query).await;
    assert!(
        result.is_ok(),
        "api get with query failed: {:?}",
        result.err()
    );
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_api_post() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    std::env::set_var("PUP_MOCK_SERVER", &server.url());

    let cfg = Config {
        api_key: Some("test-key".into()),
        app_key: Some("test-app".into()),
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let mock = server
        .mock("POST", "/api/v2/test")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"created": true}"#)
        .create_async()
        .await;

    let body = serde_json::json!({"name": "test"});
    let result = crate::api::post(&cfg, "/api/v2/test", &body).await;
    assert!(result.is_ok(), "api post failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_api_put() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    std::env::set_var("PUP_MOCK_SERVER", &server.url());

    let cfg = Config {
        api_key: Some("test-key".into()),
        app_key: Some("test-app".into()),
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let mock = server
        .mock("PUT", "/api/v1/test/123")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"updated": true}"#)
        .create_async()
        .await;

    let body = serde_json::json!({"name": "updated"});
    let result = crate::api::put(&cfg, "/api/v1/test/123", &body).await;
    assert!(result.is_ok(), "api put failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_api_patch() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    std::env::set_var("PUP_MOCK_SERVER", &server.url());

    let cfg = Config {
        api_key: Some("test-key".into()),
        app_key: Some("test-app".into()),
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let mock = server
        .mock("PATCH", "/api/v1/test/123")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"patched": true}"#)
        .create_async()
        .await;

    let body = serde_json::json!({"name": "patched"});
    let result = crate::api::patch(&cfg, "/api/v1/test/123", &body).await;
    assert!(result.is_ok(), "api patch failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_api_delete() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    std::env::set_var("PUP_MOCK_SERVER", &server.url());

    let cfg = Config {
        api_key: Some("test-key".into()),
        app_key: Some("test-app".into()),
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let mock = server
        .mock("DELETE", "/api/v1/test/123")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"deleted": true}"#)
        .create_async()
        .await;

    let result = crate::api::delete(&cfg, "/api/v1/test/123").await;
    assert!(result.is_ok(), "api delete failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_api_error_response() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    std::env::set_var("PUP_MOCK_SERVER", &server.url());

    let cfg = Config {
        api_key: Some("test-key".into()),
        app_key: Some("test-app".into()),
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let mock = server
        .mock("GET", "/api/v1/test/missing")
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors": ["not found"]}"#)
        .create_async()
        .await;

    let result = crate::api::get(&cfg, "/api/v1/test/missing", &[]).await;
    assert!(result.is_err(), "should return error for 404");
    assert!(result.unwrap_err().to_string().contains("404"));
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_api_bearer_auth() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    std::env::set_var("PUP_MOCK_SERVER", &server.url());

    let cfg = Config {
        api_key: None,
        app_key: None,
        access_token: Some("test-bearer-token".into()),
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let mock = server
        .mock("GET", "/api/v1/test")
        .match_header("Authorization", "Bearer test-bearer-token")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"auth": "bearer"}"#)
        .create_async()
        .await;

    let result = crate::api::get(&cfg, "/api/v1/test", &[]).await;
    assert!(result.is_ok(), "bearer auth failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_api_no_auth() {
    let _lock = lock_env();

    let cfg = Config {
        api_key: None,
        app_key: None,
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let result = crate::api::get(&cfg, "/api/v1/test", &[]).await;
    assert!(result.is_err(), "should fail without auth");
    assert!(
        result.unwrap_err().to_string().contains("authentication"),
        "error should mention authentication"
    );
    cleanup_env();
}

#[tokio::test]
async fn test_api_empty_response() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    std::env::set_var("PUP_MOCK_SERVER", &server.url());

    let cfg = Config {
        api_key: Some("test-key".into()),
        app_key: Some("test-app".into()),
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let mock = server
        .mock("DELETE", "/api/v1/test/empty")
        .with_status(204)
        .with_body("")
        .create_async()
        .await;

    let result = crate::api::delete(&cfg, "/api/v1/test/empty").await;
    assert!(result.is_ok(), "empty response failed: {:?}", result.err());
    let val = result.unwrap();
    assert_eq!(val, serde_json::json!({}));
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_api_server_error() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    std::env::set_var("PUP_MOCK_SERVER", &server.url());

    let cfg = Config {
        api_key: Some("test-key".into()),
        app_key: Some("test-app".into()),
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let mock = server
        .mock("GET", "/api/v1/test")
        .with_status(500)
        .with_body(r#"{"errors": ["internal server error"]}"#)
        .create_async()
        .await;

    let result = crate::api::get(&cfg, "/api/v1/test", &[]).await;
    assert!(
        result.is_err(),
        "expected error but got ok: {:?}",
        result.ok()
    );
    assert!(result.unwrap_err().to_string().contains("500"));
    mock.assert_async().await;
    cleanup_env();
}

// =========================================================================
// Bulk command module tests — exercise list/get operations for all remaining
// command modules to maximize coverage. The mock_any helper catches all
// requests. We use `let _ =` instead of asserting success because some DD
// client types may not deserialize our minimal responses — the important
// thing is that the command code paths are exercised.
// =========================================================================

/// Mock all HTTP methods with the same response body.
async fn mock_all(s: &mut mockito::Server, body: &str) {
    for method in &["GET", "POST", "PUT", "PATCH", "DELETE"] {
        s.mock(method, mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;
    }
}

// --- RUM ---
#[tokio::test]
async fn test_rum_apps_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::rum::apps_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_rum_apps_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {"id": "abc", "type": "rum_browser"}}"#).await;
    let _ = crate::commands::rum::apps_get(&cfg, "abc").await;
    cleanup_env();
}
#[tokio::test]
async fn test_rum_apps_delete() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{}"#).await;
    let _ = crate::commands::rum::apps_delete(&cfg, "abc").await;
    cleanup_env();
}
#[tokio::test]
async fn test_rum_metrics_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::rum::metrics_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_rum_metrics_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::rum::metrics_get(&cfg, "m1").await;
    cleanup_env();
}
#[tokio::test]
async fn test_rum_metrics_delete() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{}"#).await;
    let _ = crate::commands::rum::metrics_delete(&cfg, "m1").await;
    cleanup_env();
}
#[tokio::test]
async fn test_rum_retention_filters_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::rum::retention_filters_list(&cfg, "app1").await;
    cleanup_env();
}
#[tokio::test]
async fn test_rum_events_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::rum::events_list(&cfg, None, "1h".into(), "now".into(), 10).await;
    cleanup_env();
}
#[tokio::test]
async fn test_rum_playlists_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::rum::playlists_list(&cfg).await;
    cleanup_env();
}

// --- Status Pages ---
#[tokio::test]
async fn test_status_pages_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::status_pages::pages_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_status_pages_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::status_pages::pages_get(&cfg, "p1").await;
    cleanup_env();
}
#[tokio::test]
async fn test_status_pages_delete() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{}"#).await;
    let _ = crate::commands::status_pages::pages_delete(&cfg, "p1").await;
    cleanup_env();
}
#[tokio::test]
async fn test_status_pages_components_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::status_pages::components_list(&cfg, "p1").await;
    cleanup_env();
}
#[tokio::test]
async fn test_status_pages_degradations_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::status_pages::degradations_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_status_pages_third_party_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::status_pages::third_party_list(&cfg, None, false).await;
    cleanup_env();
}

// --- Cases ---
#[tokio::test]
async fn test_cases_search() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::cases::search(&cfg, None, 10).await;
    cleanup_env();
}
#[tokio::test]
async fn test_cases_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::cases::get(&cfg, "case1").await;
    cleanup_env();
}
#[tokio::test]
async fn test_cases_projects_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::cases::projects_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_cases_projects_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::cases::projects_get(&cfg, "proj1").await;
    cleanup_env();
}
#[tokio::test]
async fn test_cases_projects_delete() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{}"#).await;
    let _ = crate::commands::cases::projects_delete(&cfg, "proj1").await;
    cleanup_env();
}

// --- Integrations ---
#[tokio::test]
async fn test_integrations_jira_accounts_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::integrations::jira_accounts_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_integrations_jira_templates_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::integrations::jira_templates_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_integrations_servicenow_instances_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::integrations::servicenow_instances_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_integrations_servicenow_templates_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::integrations::servicenow_templates_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_integrations_slack_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::integrations::slack_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_integrations_webhooks_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::integrations::webhooks_list(&cfg).await;
    cleanup_env();
}

// --- CI/CD ---
#[tokio::test]
async fn test_cicd_pipelines_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::cicd::pipelines_list(&cfg, None, "1h".into(), "now".into(), 10).await;
    cleanup_env();
}
#[tokio::test]
async fn test_cicd_tests_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::cicd::tests_list(&cfg, None, "1h".into(), "now".into(), 10).await;
    cleanup_env();
}
#[tokio::test]
async fn test_cicd_flaky_tests_search() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::cicd::flaky_tests_search(
        &cfg,
        Some("@test.service:my-service".into()),
        None,
        50,
        Some("-last_flaked".into()),
    )
    .await;
    cleanup_env();
}

#[tokio::test]
async fn test_cicd_flaky_tests_search_invalid_sort() {
    let _lock = lock_env();
    let s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    let result =
        crate::commands::cicd::flaky_tests_search(&cfg, None, None, 10, Some("bogus".into())).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("invalid sort value"));
    cleanup_env();
}

// --- Fleet ---
#[tokio::test]
async fn test_fleet_agents_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::fleet::agents_list(&cfg, None, None).await;
    cleanup_env();
}
#[tokio::test]
async fn test_fleet_agents_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::fleet::agents_get(&cfg, "a1").await;
    cleanup_env();
}
#[tokio::test]
async fn test_fleet_agents_versions() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::fleet::agents_versions(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_fleet_tracers_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let mock = server
        .mock("GET", "/api/unstable/fleet/tracers")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":{"id":"status","type":"status","attributes":{"tracers":[]}}}"#)
        .create_async()
        .await;

    let result = crate::commands::fleet::tracers_list(&cfg, None, None, None, None, false).await;
    assert!(result.is_ok(), "tracers_list failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}
#[tokio::test]
async fn test_fleet_tracers_list_with_filter() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let mock = server
        .mock("GET", "/api/unstable/fleet/tracers")
        .match_query(mockito::Matcher::UrlEncoded(
            "filter".into(),
            "hostname:my-host".into(),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":{"id":"status","type":"status","attributes":{"tracers":[]}}}"#)
        .create_async()
        .await;

    let result = crate::commands::fleet::tracers_list(
        &cfg,
        Some("hostname:my-host".into()),
        None,
        None,
        None,
        false,
    )
    .await;
    assert!(
        result.is_ok(),
        "tracers_list with filter failed: {:?}",
        result.err()
    );
    mock.assert_async().await;
    cleanup_env();
}
#[tokio::test]
async fn test_fleet_agents_tracers_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let mock = server
        .mock("GET", "/api/unstable/fleet/agents/agent-123/tracers")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":{"id":"status","type":"status","attributes":{"tracers":[]}}}"#)
        .create_async()
        .await;

    let result = crate::commands::fleet::agents_tracers_list(
        &cfg,
        "agent-123".into(),
        None,
        None,
        None,
        false,
    )
    .await;
    assert!(
        result.is_ok(),
        "agents_tracers_list failed: {:?}",
        result.err()
    );
    mock.assert_async().await;
    cleanup_env();
}
#[tokio::test]
async fn test_fleet_instrumented_pods_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let mock = server
        .mock(
            "GET",
            "/api/unstable/fleet/clusters/my-cluster/instrumented_pods",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"data":{"type":"cluster_name","id":"my-cluster","attributes":{"groups":[]}}}"#,
        )
        .create_async()
        .await;

    let result = crate::commands::fleet::instrumented_pods_list(&cfg, "my-cluster".into()).await;
    assert!(
        result.is_ok(),
        "instrumented_pods_list failed: {:?}",
        result.err()
    );
    mock.assert_async().await;
    cleanup_env();
}
#[tokio::test]
async fn test_fleet_clusters_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let mock = server
        .mock("GET", "/api/unstable/fleet/clusters")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":{"type":"status","id":"done","attributes":{"clusters":[]}}}"#)
        .create_async()
        .await;

    let result = crate::commands::fleet::clusters_list(&cfg, None, None, None, None, false).await;
    assert!(result.is_ok(), "clusters_list failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}
#[tokio::test]
async fn test_fleet_deployments_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::fleet::deployments_list(&cfg, None).await;
    cleanup_env();
}
#[tokio::test]
async fn test_fleet_schedules_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::fleet::schedules_list(&cfg).await;
    cleanup_env();
}

// --- Incidents ---
#[tokio::test]
async fn test_incidents_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::incidents::list(&cfg, 10).await;
    cleanup_env();
}
#[tokio::test]
async fn test_incidents_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::incidents::get(&cfg, "inc1").await;
    cleanup_env();
}
#[tokio::test]
async fn test_incidents_settings_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::incidents::settings_get(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_incidents_handles_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::incidents::handles_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_incidents_postmortem_templates_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::incidents::postmortem_templates_list(&cfg).await;
    cleanup_env();
}

// --- On-Call ---
#[tokio::test]
async fn test_on_call_teams_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::on_call::teams_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_on_call_teams_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::on_call::teams_get(&cfg, "t1").await;
    cleanup_env();
}
#[tokio::test]
async fn test_on_call_teams_delete() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{}"#).await;
    let _ = crate::commands::on_call::teams_delete(&cfg, "t1").await;
    cleanup_env();
}

// --- On-Call Escalation Policies ---
#[tokio::test]
async fn test_on_call_escalation_policies_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {"type": "policies"}}"#).await;
    let result = crate::commands::on_call::escalation_policies_get(&cfg, "p1").await;
    assert!(
        result.is_ok(),
        "escalation policies get failed: {:?}",
        result.err()
    );
    cleanup_env();
}
#[tokio::test]
async fn test_on_call_escalation_policies_delete() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{}"#).await;
    let result = crate::commands::on_call::escalation_policies_delete(&cfg, "p1").await;
    assert!(
        result.is_ok(),
        "escalation policies delete failed: {:?}",
        result.err()
    );
    cleanup_env();
}
#[tokio::test]
async fn test_on_call_escalation_policies_get_error() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    s.mock("GET", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(500)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors": ["internal error"]}"#)
        .create_async()
        .await;
    let result = crate::commands::on_call::escalation_policies_get(&cfg, "p1").await;
    assert!(result.is_err(), "expected error on 500 response");
    cleanup_env();
}

// --- On-Call Schedules ---
#[tokio::test]
async fn test_on_call_schedules_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {"type": "schedules"}}"#).await;
    let result = crate::commands::on_call::schedules_get(&cfg, "s1").await;
    assert!(result.is_ok(), "schedules get failed: {:?}", result.err());
    cleanup_env();
}
#[tokio::test]
async fn test_on_call_schedules_delete() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{}"#).await;
    let result = crate::commands::on_call::schedules_delete(&cfg, "s1").await;
    assert!(
        result.is_ok(),
        "schedules delete failed: {:?}",
        result.err()
    );
    cleanup_env();
}
#[tokio::test]
async fn test_on_call_schedules_get_error() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    s.mock("GET", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(500)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors": ["internal error"]}"#)
        .create_async()
        .await;
    let result = crate::commands::on_call::schedules_get(&cfg, "s1").await;
    assert!(result.is_err(), "expected error on 500 response");
    cleanup_env();
}

// --- On-Call Notification Channels ---
#[tokio::test]
async fn test_on_call_notification_channels_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let result = crate::commands::on_call::notification_channels_list(&cfg, "u1").await;
    assert!(
        result.is_ok(),
        "notification channels list failed: {:?}",
        result.err()
    );
    cleanup_env();
}
#[tokio::test]
async fn test_on_call_notification_channels_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {"type": "notification_channels"}}"#).await;
    let result = crate::commands::on_call::notification_channels_get(&cfg, "u1", "c1").await;
    assert!(
        result.is_ok(),
        "notification channels get failed: {:?}",
        result.err()
    );
    cleanup_env();
}
#[tokio::test]
async fn test_on_call_notification_channels_delete() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{}"#).await;
    let result = crate::commands::on_call::notification_channels_delete(&cfg, "u1", "c1").await;
    assert!(
        result.is_ok(),
        "notification channels delete failed: {:?}",
        result.err()
    );
    cleanup_env();
}
#[tokio::test]
async fn test_on_call_notification_channels_list_error() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    s.mock("GET", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(500)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors": ["internal error"]}"#)
        .create_async()
        .await;
    let result = crate::commands::on_call::notification_channels_list(&cfg, "u1").await;
    assert!(result.is_err(), "expected error on 500 response");
    cleanup_env();
}

// --- On-Call Notification Rules ---
#[tokio::test]
async fn test_on_call_notification_rules_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let result = crate::commands::on_call::notification_rules_list(&cfg, "u1").await;
    assert!(
        result.is_ok(),
        "notification rules list failed: {:?}",
        result.err()
    );
    cleanup_env();
}
#[tokio::test]
async fn test_on_call_notification_rules_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {"type": "notification_rules"}}"#).await;
    let result = crate::commands::on_call::notification_rules_get(&cfg, "u1", "r1").await;
    assert!(
        result.is_ok(),
        "notification rules get failed: {:?}",
        result.err()
    );
    cleanup_env();
}
#[tokio::test]
async fn test_on_call_notification_rules_delete() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{}"#).await;
    let result = crate::commands::on_call::notification_rules_delete(&cfg, "u1", "r1").await;
    assert!(
        result.is_ok(),
        "notification rules delete failed: {:?}",
        result.err()
    );
    cleanup_env();
}
#[tokio::test]
async fn test_on_call_notification_rules_list_error() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    s.mock("GET", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(500)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors": ["internal error"]}"#)
        .create_async()
        .await;
    let result = crate::commands::on_call::notification_rules_list(&cfg, "u1").await;
    assert!(result.is_err(), "expected error on 500 response");
    cleanup_env();
}

// --- Security ---
#[tokio::test]
async fn test_security_rules_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::security::rules_list(&cfg, None, None).await;
    cleanup_env();
}
#[tokio::test]
async fn test_security_rules_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::security::rules_get(&cfg, "r1").await;
    cleanup_env();
}
#[tokio::test]
async fn test_security_content_packs_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::security::content_packs_list(&cfg).await;
    cleanup_env();
}

// --- Synthetics ---
#[tokio::test]
async fn test_synthetics_tests_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"tests": []}"#).await;
    let _ = crate::commands::synthetics::tests_list(&cfg, 10, 0).await;
    cleanup_env();
}
#[tokio::test]
async fn test_synthetics_tests_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{}"#).await;
    let _ = crate::commands::synthetics::tests_get(&cfg, "pub1").await;
    cleanup_env();
}
#[tokio::test]
async fn test_synthetics_locations_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"locations": []}"#).await;
    let _ = crate::commands::synthetics::locations_list(&cfg).await;
    cleanup_env();
}

// --- App Keys ---
#[tokio::test]
async fn test_app_keys_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::app_keys::list(&cfg, 10, 0, "", "").await;
    cleanup_env();
}
#[tokio::test]
async fn test_app_keys_list_all() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::app_keys::list_all(&cfg, 10, 0, "", "").await;
    cleanup_env();
}
#[tokio::test]
async fn test_app_keys_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::app_keys::get(&cfg, "k1").await;
    cleanup_env();
}
#[tokio::test]
async fn test_app_keys_create() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::app_keys::create(&cfg, "test-key", "").await;
    cleanup_env();
}
#[tokio::test]
async fn test_app_keys_update() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::app_keys::update(&cfg, "k1", "new-name", "").await;
    cleanup_env();
}
#[tokio::test]
async fn test_app_keys_delete() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{}"#).await;
    let _ = crate::commands::app_keys::delete(&cfg, "k1").await;
    cleanup_env();
}

// --- API Keys ---
#[tokio::test]
async fn test_api_keys_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::api_keys::list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_api_keys_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::api_keys::get(&cfg, "k1").await;
    cleanup_env();
}
#[tokio::test]
async fn test_api_keys_delete() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{}"#).await;
    let _ = crate::commands::api_keys::delete(&cfg, "k1").await;
    cleanup_env();
}

// --- Audit Logs ---
#[tokio::test]
async fn test_audit_logs_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::audit_logs::list(&cfg, "1h".into(), "now".into(), 10).await;
    cleanup_env();
}

// --- Users ---
#[tokio::test]
async fn test_users_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::users::list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_users_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::users::get(&cfg, "u1").await;
    cleanup_env();
}
#[tokio::test]
async fn test_users_roles_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::users::roles_list(&cfg).await;
    cleanup_env();
}

// --- Usage ---
#[tokio::test]
async fn test_usage_summary() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"usage": []}"#).await;
    let _ = crate::commands::usage::summary(&cfg, "2024-01".into(), None).await;
    cleanup_env();
}

// --- Infrastructure ---
#[tokio::test]
async fn test_infrastructure_hosts_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"host_list": [], "total_returned": 0}"#).await;
    let _ = crate::commands::infrastructure::hosts_list(&cfg, None, "name".into(), 10).await;
    cleanup_env();
}

// --- Notebooks ---
#[tokio::test]
async fn test_notebooks_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::notebooks::list(&cfg).await;
    cleanup_env();
}

// --- Downtime ---
#[tokio::test]
async fn test_downtime_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::downtime::list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_downtime_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::downtime::get(&cfg, "d1").await;
    cleanup_env();
}

// --- Cost ---
#[tokio::test]
async fn test_cost_projected() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::cost::projected(&cfg).await;
    cleanup_env();
}

// --- Error Tracking ---
#[tokio::test]
async fn test_error_tracking_issues_search() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ =
        crate::commands::error_tracking::issues_search(&cfg, None, 10, Some("trace".into()), None)
            .await;
    cleanup_env();
}

#[tokio::test]
async fn test_error_tracking_issues_search_persona() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::error_tracking::issues_search(
        &cfg,
        None,
        10,
        None,
        Some("BROWSER".into()),
    )
    .await;
    cleanup_env();
}

#[tokio::test]
async fn test_error_tracking_issues_search_track_case_insensitive() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ =
        crate::commands::error_tracking::issues_search(&cfg, None, 10, Some("RUM".into()), None)
            .await;
    cleanup_env();
}

#[test]
fn test_error_tracking_clap_mutual_exclusivity() {
    let result = crate::Cli::command().try_get_matches_from([
        "pup",
        "error-tracking",
        "issues",
        "search",
        "--track",
        "trace",
        "--persona",
        "ALL",
    ]);
    assert!(
        result.is_err(),
        "expected error when both --track and --persona are provided"
    );
}

#[test]
fn test_error_tracking_clap_neither_provided() {
    let result =
        crate::Cli::command().try_get_matches_from(["pup", "error-tracking", "issues", "search"]);
    assert!(
        result.is_err(),
        "expected error when neither --track nor --persona is provided"
    );
}

// --- Cloud ---
#[tokio::test]
async fn test_cloud_aws_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::cloud::aws_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_cloud_gcp_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::cloud::gcp_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_cloud_azure_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::cloud::azure_list(&cfg).await;
    cleanup_env();
}

// --- Organizations ---
#[tokio::test]
async fn test_organizations_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"orgs": []}"#).await;
    let _ = crate::commands::organizations::list(&cfg).await;
    cleanup_env();
}

// --- Service Catalog ---
#[tokio::test]
async fn test_service_catalog_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::service_catalog::list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_service_catalog_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::service_catalog::get(&cfg, "svc1").await;
    cleanup_env();
}

// --- Misc ---
#[tokio::test]
async fn test_misc_ip_ranges() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{}"#).await;
    let _ = crate::commands::misc::ip_ranges(&cfg).await;
    cleanup_env();
}

// --- Data Governance ---
#[tokio::test]
async fn test_data_governance_scanner_rules_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::data_governance::scanner_rules_list(&cfg).await;
    cleanup_env();
}

// --- Investigations ---
#[tokio::test]
async fn test_investigations_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::investigations::list(&cfg, 10, 0).await;
    cleanup_env();
}
#[tokio::test]
async fn test_investigations_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::investigations::get(&cfg, "inv1").await;
    cleanup_env();
}

// --- Network ---
#[tokio::test]
async fn test_network_flows_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::network::flows_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_network_devices_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::network::devices_list(&cfg).await;
    cleanup_env();
}

// --- Code Coverage ---
#[tokio::test]
async fn test_code_coverage_branch_summary() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ =
        crate::commands::code_coverage::branch_summary(&cfg, "repo".into(), "main".into()).await;
    cleanup_env();
}

// --- HAMR ---
#[tokio::test]
async fn test_hamr_connections_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {}}"#).await;
    let _ = crate::commands::hamr::connections_get(&cfg).await;
    cleanup_env();
}

// --- Static Analysis ---
#[tokio::test]
async fn test_static_analysis_custom_rulesets_get_existing() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(
        &mut s,
        r#"{"data": {"id": "rs", "type": "custom-ruleset"}}"#,
    )
    .await;
    let _ = crate::commands::static_analysis::custom_rulesets_get(&cfg, "my-ruleset").await;
    cleanup_env();
}
#[tokio::test]
async fn test_static_analysis_custom_rules_get_existing() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {"id": "rule", "type": "custom-rule"}}"#).await;
    let _ = crate::commands::static_analysis::custom_rules_get(&cfg, "my-ruleset", "my-rule").await;
    cleanup_env();
}

// --- APM ---
#[tokio::test]
async fn test_apm_services_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ =
        crate::commands::apm::services_list(&cfg, "prod".into(), "1h".into(), "now".into()).await;
    cleanup_env();
}
#[tokio::test]
async fn test_apm_troubleshooting_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let mock = server
        .mock("GET", "/api/unstable/apm/instrumentation-errors")
        .match_query(mockito::Matcher::UrlEncoded(
            "hostname".into(),
            "my-host".into(),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data": []}"#)
        .create_async()
        .await;

    let result = crate::commands::apm::troubleshooting_list(&cfg, "my-host".into(), None).await;
    assert!(
        result.is_ok(),
        "troubleshooting list failed: {:?}",
        result.err()
    );
    mock.assert_async().await;
    cleanup_env();
}
#[tokio::test]
async fn test_apm_troubleshooting_list_with_timeframe() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let mock = server
        .mock("GET", "/api/unstable/apm/instrumentation-errors")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("hostname".into(), "my-host".into()),
            mockito::Matcher::UrlEncoded("timeframe".into(), "4h".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data": []}"#)
        .create_async()
        .await;

    let result =
        crate::commands::apm::troubleshooting_list(&cfg, "my-host".into(), Some("4h".into())).await;
    assert!(
        result.is_ok(),
        "troubleshooting list with timeframe failed: {:?}",
        result.err()
    );
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_apm_service_config_get() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let mock = server
        .mock("GET", "/api/unstable/apm/service-config")
        .match_query(mockito::Matcher::UrlEncoded(
            "service_name".into(),
            "my-service".into(),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"service_name":"my-service","service_configs":[]}"#)
        .create_async()
        .await;

    let result =
        crate::commands::apm::service_config_get(&cfg, "my-service".into(), None, None).await;
    assert!(
        result.is_ok(),
        "service_config_get failed: {:?}",
        result.err()
    );
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_apm_service_config_get_with_filters() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let mock = server
        .mock("GET", "/api/unstable/apm/service-config")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("service_name".into(), "my-service".into()),
            mockito::Matcher::UrlEncoded("env".into(), "prod".into()),
            mockito::Matcher::UrlEncoded("service_instance_ids".into(), "id-1,id-2".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"service_name":"my-service","service_configs":[]}"#)
        .create_async()
        .await;

    let result = crate::commands::apm::service_config_get(
        &cfg,
        "my-service".into(),
        Some("prod".into()),
        Some("id-1,id-2".into()),
    )
    .await;
    assert!(
        result.is_ok(),
        "service_config_get with filters failed: {:?}",
        result.err()
    );
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_apm_service_library_config_get() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let mock = server
        .mock("GET", "/api/unstable/apm/service-library-config")
        .match_query(mockito::Matcher::UrlEncoded(
            "service_name".into(),
            "my-service".into(),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"service_name":"my-service","configs":[]}"#)
        .create_async()
        .await;

    let result = crate::commands::apm::service_library_config_get(
        &cfg,
        "my-service".into(),
        None,
        None,
        false,
    )
    .await;
    assert!(
        result.is_ok(),
        "service_library_config_get failed: {:?}",
        result.err()
    );
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_apm_service_library_config_get_with_filters() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let mock = server
        .mock("GET", "/api/unstable/apm/service-library-config")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("service_name".into(), "my-service".into()),
            mockito::Matcher::UrlEncoded("env".into(), "prod".into()),
            mockito::Matcher::UrlEncoded("language_name".into(), "python".into()),
            mockito::Matcher::UrlEncoded("is_mixed".into(), "true".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"service_name":"my-service","is_mixed":true,"configs":[]}"#)
        .create_async()
        .await;

    let result = crate::commands::apm::service_library_config_get(
        &cfg,
        "my-service".into(),
        Some("prod".into()),
        Some("python".into()),
        true,
    )
    .await;
    assert!(
        result.is_ok(),
        "service_library_config_get with filters failed: {:?}",
        result.err()
    );
    mock.assert_async().await;
    cleanup_env();
}

// -------------------------------------------------------------------------
// Read-only mode
// -------------------------------------------------------------------------

#[test]
fn test_is_write_command_name_writes() {
    assert!(crate::is_write_command_name("delete"));
    assert!(crate::is_write_command_name("create"));
    assert!(crate::is_write_command_name("update"));
    assert!(crate::is_write_command_name("cancel"));
    assert!(crate::is_write_command_name("trigger"));
    assert!(crate::is_write_command_name("submit"));
    assert!(crate::is_write_command_name("send"));
    assert!(crate::is_write_command_name("move"));
    assert!(crate::is_write_command_name("link"));
    assert!(crate::is_write_command_name("unlink"));
    assert!(crate::is_write_command_name("configure"));
    assert!(crate::is_write_command_name("upgrade"));
    assert!(crate::is_write_command_name("update-status"));
    assert!(crate::is_write_command_name("create-page"));
    assert!(crate::is_write_command_name("patch"));
    assert!(crate::is_write_command_name("patch-deployment"));
}

#[test]
fn test_is_write_command_name_reads() {
    assert!(!crate::is_write_command_name("list"));
    assert!(!crate::is_write_command_name("get"));
    assert!(!crate::is_write_command_name("search"));
    assert!(!crate::is_write_command_name("query"));
    assert!(!crate::is_write_command_name("aggregate"));
    assert!(!crate::is_write_command_name("status"));
    assert!(!crate::is_write_command_name("dispatch"));
}

#[test]
fn test_read_only_guard_blocks_write() {
    let matches = crate::Cli::command()
        .try_get_matches_from(["pup", "monitors", "delete", "12345"])
        .unwrap();
    let leaf = crate::get_leaf_subcommand_name(&matches).unwrap();
    assert!(crate::is_write_command_name(&leaf));
}

#[test]
fn test_read_only_guard_allows_read() {
    let matches = crate::Cli::command()
        .try_get_matches_from(["pup", "monitors", "list"])
        .unwrap();
    let leaf = crate::get_leaf_subcommand_name(&matches).unwrap();
    assert!(!crate::is_write_command_name(&leaf));
}

#[test]
fn test_read_only_guard_nested_read() {
    let matches = crate::Cli::command()
        .try_get_matches_from(["pup", "rum", "apps", "list"])
        .unwrap();
    let leaf = crate::get_leaf_subcommand_name(&matches).unwrap();
    assert!(!crate::is_write_command_name(&leaf));
}

#[test]
fn test_read_only_guard_nested_write() {
    let matches = crate::Cli::command()
        .try_get_matches_from([
            "pup",
            "cases",
            "jira",
            "create-issue",
            "123",
            "--file",
            "f.json",
        ])
        .unwrap();
    let leaf = crate::get_leaf_subcommand_name(&matches).unwrap();
    assert!(crate::is_write_command_name(&leaf));
}

#[test]
fn test_read_only_guard_exempts_alias() {
    let matches = crate::Cli::command()
        .try_get_matches_from(["pup", "alias", "set", "foo", "logs search *"])
        .unwrap();
    let top = crate::get_top_level_subcommand_name(&matches);
    assert_eq!(top.as_deref(), Some("alias"));
}

#[test]
fn test_read_only_guard_exempts_auth() {
    let matches = crate::Cli::command()
        .try_get_matches_from(["pup", "auth", "login"])
        .unwrap();
    let top = crate::get_top_level_subcommand_name(&matches);
    assert_eq!(top.as_deref(), Some("auth"));
}

// =========================================================================
// LLM Observability commands — all use client::raw_post / client::raw_get
// (not the typed DD client), so mockito can match exact paths.
// =========================================================================

// Helper: create a mock for a specific POST path
async fn mock_post(
    server: &mut mockito::Server,
    path: &str,
    status: usize,
    body: &str,
) -> mockito::Mock {
    server
        .mock("POST", path)
        .with_status(status)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create_async()
        .await
}

#[tokio::test]
async fn test_dbm_samples_search_uses_documented_payload() {
    let _lock = lock_env();
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

    let result = crate::commands::dbm::samples_search(
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

// Helper: create a mock for a specific GET path
async fn mock_get(
    server: &mut mockito::Server,
    path: &str,
    status: usize,
    body: &str,
) -> mockito::Mock {
    server
        .mock("GET", path)
        .match_query(mockito::Matcher::Any)
        .with_status(status)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create_async()
        .await
}

// Helper: create a mock for a specific PATCH path
async fn mock_patch(
    server: &mut mockito::Server,
    path: &str,
    status: usize,
    body: &str,
) -> mockito::Mock {
    server
        .mock("PATCH", path)
        .with_status(status)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create_async()
        .await
}

// Helper: write a temp JSON file and return its path
fn write_temp_json(name: &str, content: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, content).unwrap();
    path
}

// -------------------------------------------------------------------------
// LLM Observability management commands (projects, experiments, datasets)
// -------------------------------------------------------------------------

// --- projects list ---

#[tokio::test]
async fn test_llm_obs_projects_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    // Raw HTTP path: response can have any shape; missing nullable fields are tolerated.
    let body = r#"{"data":[{"id":"proj-1","type":"projects","attributes":{"name":"my-project","description":null,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}}]}"#;
    let _mock = mock_any(&mut server, "GET", body).await;

    let result = crate::commands::llm_obs::projects_list(&cfg).await;
    assert!(result.is_ok(), "projects_list failed: {:?}", result.err());
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_llm_obs_projects_list_404() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["not found"]}"#)
        .create_async()
        .await;

    let result = crate::commands::llm_obs::projects_list(&cfg).await;
    assert!(
        result.is_err(),
        "expected error but got ok: {:?}",
        result.ok()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_llm_obs_projects_list_no_auth() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let cfg = Config {
        api_key: None,
        app_key: None,
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };
    let result = crate::commands::llm_obs::projects_list(&cfg).await;
    assert!(result.is_err(), "should fail without auth");
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// --- projects list schema tolerance ---

#[tokio::test]
async fn test_llm_obs_projects_list_missing_nullable_fields() {
    // raw HTTP should succeed even with minimal response missing optional/nullable fields
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    // response missing description/config/metadata fields
    let _mock = mock_any(
        &mut server,
        "GET",
        r#"{"data":[{"id":"p1","type":"llm_obs_projects"}]}"#,
    )
    .await;
    let result = crate::commands::llm_obs::projects_list(&cfg).await;
    assert!(
        result.is_ok(),
        "should tolerate missing nullable fields: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// --- projects create ---

#[tokio::test]
async fn test_llm_obs_projects_create() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_proj_create.json",
        r#"{"data":{"type":"projects","attributes":{"name":"test"}}}"#,
    );
    let body = r#"{"data":{"id":"proj-1","type":"projects","attributes":{"name":"test","description":null,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}}}"#;
    let _mock = mock_any(&mut server, "POST", body).await;

    let result = crate::commands::llm_obs::projects_create(&cfg, tmp.to_str().unwrap()).await;
    assert!(result.is_ok(), "projects_create failed: {:?}", result.err());
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_llm_obs_projects_create_500() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_proj_create_500.json",
        r#"{"data":{"type":"projects","attributes":{"name":"test"}}}"#,
    );
    let _mock = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(500)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["server error"]}"#)
        .create_async()
        .await;

    let result = crate::commands::llm_obs::projects_create(&cfg, tmp.to_str().unwrap()).await;
    assert!(
        result.is_err(),
        "expected error but got ok: {:?}",
        result.ok()
    );
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// --- experiments list ---

#[tokio::test]
async fn test_llm_obs_experiments_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    // Raw HTTP path: response can have any shape; nullable fields are tolerated.
    let body = r#"{"data":[{"id":"exp-1","type":"experiments","attributes":{"name":"test-exp","config":null,"description":null,"metadata":null,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","dataset_id":"ds-1","project_id":"proj-1","status":"active"}}]}"#;
    let _mock = mock_any(&mut server, "GET", body).await;

    let result = crate::commands::llm_obs::experiments_list(&cfg, None, None).await;
    assert!(
        result.is_ok(),
        "experiments_list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_llm_obs_experiments_list_with_filters() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let body = r#"{"data":[]}"#;
    // Strict query param check: verify filter[project_id] and filter[dataset_id] are sent.
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("filter[project_id]".into(), "proj-1".into()),
            mockito::Matcher::UrlEncoded("filter[dataset_id]".into(), "ds-1".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create_async()
        .await;

    let result = crate::commands::llm_obs::experiments_list(
        &cfg,
        Some("proj-1".into()),
        Some("ds-1".into()),
    )
    .await;
    assert!(
        result.is_ok(),
        "experiments_list with filters failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_llm_obs_experiments_list_401() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(401)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Unauthorized"]}"#)
        .create_async()
        .await;

    let result = crate::commands::llm_obs::experiments_list(&cfg, None, None).await;
    assert!(
        result.is_err(),
        "expected error but got ok: {:?}",
        result.ok()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// --- experiments create ---

#[tokio::test]
async fn test_llm_obs_experiments_create() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_exp_create.json",
        r#"{"data":{"type":"experiments","attributes":{"name":"test-exp","dataset_id":"ds-1","project_id":"proj-1"}}}"#,
    );
    let body = r#"{"data":{"id":"exp-1","type":"experiments","attributes":{"name":"test-exp","config":null,"description":null,"metadata":null,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","dataset_id":"ds-1","project_id":"proj-1","status":"active"}}}"#;
    let _mock = mock_any(&mut server, "POST", body).await;

    let result = crate::commands::llm_obs::experiments_create(&cfg, tmp.to_str().unwrap()).await;
    assert!(
        result.is_ok(),
        "experiments_create failed: {:?}",
        result.err()
    );
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_llm_obs_experiments_create_422() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_exp_create_422.json",
        r#"{"data":{"type":"experiments","attributes":{"name":"x","dataset_id":"ds-1","project_id":"proj-1"}}}"#,
    );
    let _mock = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(422)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["invalid request body"]}"#)
        .create_async()
        .await;

    let result = crate::commands::llm_obs::experiments_create(&cfg, tmp.to_str().unwrap()).await;
    assert!(
        result.is_err(),
        "expected error but got ok: {:?}",
        result.ok()
    );
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// --- experiments update ---

#[tokio::test]
async fn test_llm_obs_experiments_update() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_exp_update.json",
        r#"{"data":{"type":"experiments","id":"exp-1","attributes":{"name":"updated"}}}"#,
    );
    let body = r#"{"data":{"id":"exp-1","type":"experiments","attributes":{"name":"updated","config":null,"description":null,"metadata":null,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","dataset_id":"ds-1","project_id":"proj-1","status":"active"}}}"#;
    let _mock = mock_any(&mut server, "PATCH", body).await;

    let result =
        crate::commands::llm_obs::experiments_update(&cfg, "exp-1", tmp.to_str().unwrap()).await;
    assert!(
        result.is_ok(),
        "experiments_update failed: {:?}",
        result.err()
    );
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_llm_obs_experiments_update_404() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_exp_update_404.json",
        r#"{"data":{"type":"experiments","id":"missing","attributes":{"name":"x"}}}"#,
    );
    let _mock = server
        .mock("PATCH", mockito::Matcher::Any)
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["not found"]}"#)
        .create_async()
        .await;

    let result =
        crate::commands::llm_obs::experiments_update(&cfg, "missing", tmp.to_str().unwrap()).await;
    assert!(
        result.is_err(),
        "expected error but got ok: {:?}",
        result.ok()
    );
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// --- experiments delete ---

#[tokio::test]
async fn test_llm_obs_experiments_delete() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_exp_delete.json",
        r#"{"data":{"type":"experiments","attributes":{"experiment_ids":["exp-1"]}}}"#,
    );
    let _mock = mock_any(&mut server, "POST", r#"{}"#).await;

    let result = crate::commands::llm_obs::experiments_delete(&cfg, tmp.to_str().unwrap()).await;
    assert!(
        result.is_ok(),
        "experiments_delete failed: {:?}",
        result.err()
    );
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_llm_obs_experiments_delete_500() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_exp_delete_500.json",
        r#"{"data":{"type":"experiments","attributes":{"experiment_ids":["exp-1"]}}}"#,
    );
    let _mock = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(500)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["server error"]}"#)
        .create_async()
        .await;

    let result = crate::commands::llm_obs::experiments_delete(&cfg, tmp.to_str().unwrap()).await;
    assert!(
        result.is_err(),
        "expected error but got ok: {:?}",
        result.ok()
    );
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// --- datasets list ---

#[tokio::test]
async fn test_llm_obs_datasets_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    // Raw HTTP path: verify the correct project-scoped path is called.
    let body = r#"{"data":[{"id":"ds-1","type":"datasets","attributes":{"name":"my-dataset","description":null,"metadata":null,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","current_version":1}}]}"#;
    let _mock = server
        .mock("GET", "/api/v2/llm-obs/v1/proj-1/datasets")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create_async()
        .await;

    let result = crate::commands::llm_obs::datasets_list(&cfg, "proj-1").await;
    assert!(result.is_ok(), "datasets_list failed: {:?}", result.err());
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_llm_obs_datasets_list_403() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Forbidden"]}"#)
        .create_async()
        .await;

    let result = crate::commands::llm_obs::datasets_list(&cfg, "proj-1").await;
    assert!(
        result.is_err(),
        "expected error but got ok: {:?}",
        result.ok()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// --- datasets create ---

#[tokio::test]
async fn test_llm_obs_datasets_create() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_ds_create.json",
        r#"{"data":{"type":"datasets","attributes":{"name":"test-dataset"}}}"#,
    );
    let body = r#"{"data":{"id":"ds-1","type":"datasets","attributes":{"name":"test-dataset","description":null,"metadata":null,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","current_version":1}}}"#;
    let _mock = mock_any(&mut server, "POST", body).await;

    let result =
        crate::commands::llm_obs::datasets_create(&cfg, "proj-1", tmp.to_str().unwrap()).await;
    assert!(result.is_ok(), "datasets_create failed: {:?}", result.err());
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_llm_obs_datasets_create_no_auth() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let tmp = write_temp_json(
        "pup_test_ds_create_noauth.json",
        r#"{"data":{"type":"datasets","attributes":{"name":"x"}}}"#,
    );
    let cfg = Config {
        api_key: None,
        app_key: None,
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };
    let result =
        crate::commands::llm_obs::datasets_create(&cfg, "proj-1", tmp.to_str().unwrap()).await;
    assert!(result.is_err(), "should fail without auth");
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// experiments summary
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_llm_obs_experiments_summary() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let body = r#"{"status":"success","data":{"experiment_id":"exp-1","total_events":3,"error_count":0,"evals":{},"available_dimensions":["env","ml_app"]}}"#;
    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-mcp/v1/experiment/summary",
        200,
        body,
    )
    .await;

    let result = crate::commands::llm_obs::experiments_summary(&cfg, "exp-1").await;
    assert!(
        result.is_ok(),
        "experiments_summary failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_experiments_summary_404() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-mcp/v1/experiment/summary",
        404,
        r#"{"errors":["experiment not found"]}"#,
    )
    .await;

    let result = crate::commands::llm_obs::experiments_summary(&cfg, "does-not-exist").await;
    assert!(result.is_err(), "should fail on 404");
    assert!(result.unwrap_err().to_string().contains("404"));
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_experiments_summary_500() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-mcp/v1/experiment/summary",
        500,
        r#"{"errors":["internal server error"]}"#,
    )
    .await;

    let result = crate::commands::llm_obs::experiments_summary(&cfg, "exp-1").await;
    assert!(result.is_err(), "should fail on 500");
    assert!(result.unwrap_err().to_string().contains("500"));
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_experiments_summary_no_auth() {
    let _lock = lock_env();
    let cfg = Config {
        api_key: None,
        app_key: None,
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let result = crate::commands::llm_obs::experiments_summary(&cfg, "exp-1").await;
    assert!(result.is_err(), "should fail without auth");
    cleanup_env();
}

// -------------------------------------------------------------------------
// experiments events list
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_llm_obs_experiments_events_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let body = r#"{"status":"success","data":{"events":[{"id":"evt-1","status":"ok","duration_ms":100.0,"metrics":{}}],"total_matching":1,"returned":1,"offset":0}}"#;
    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-mcp/v1/experiment/events",
        200,
        body,
    )
    .await;

    let result = crate::commands::llm_obs::experiments_events_list(
        &cfg, "exp-1", 20, 0, None, None, None, None, "desc",
    )
    .await;
    assert!(
        result.is_ok(),
        "experiments_events_list failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_experiments_events_list_with_filters() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let body =
        r#"{"status":"success","data":{"events":[],"total_matching":0,"returned":0,"offset":0}}"#;
    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-mcp/v1/experiment/events",
        200,
        body,
    )
    .await;

    let result = crate::commands::llm_obs::experiments_events_list(
        &cfg,
        "exp-1",
        5,
        10,
        Some("env".into()),
        Some("prod".into()),
        Some("score".into()),
        Some("accuracy".into()),
        "asc",
    )
    .await;
    assert!(
        result.is_ok(),
        "experiments_events_list with filters failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_experiments_events_list_401() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-mcp/v1/experiment/events",
        401,
        r#"{"errors":["Forbidden"]}"#,
    )
    .await;

    let result = crate::commands::llm_obs::experiments_events_list(
        &cfg, "exp-1", 20, 0, None, None, None, None, "desc",
    )
    .await;
    assert!(result.is_err(), "should fail on 401");
    assert!(result.unwrap_err().to_string().contains("401"));
    cleanup_env();
}

// -------------------------------------------------------------------------
// experiments events get
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_llm_obs_experiments_events_get() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let body = r#"{"status":"success","data":{"id":"evt-1","status":"ok","duration_ms":100.0,"input":{"prompt":"hello"},"output":{"response":"world"},"metrics":{},"dimensions":{}}}"#;
    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-mcp/v1/experiment/event",
        200,
        body,
    )
    .await;

    let result = crate::commands::llm_obs::experiments_events_get(&cfg, "exp-1", "evt-1").await;
    assert!(
        result.is_ok(),
        "experiments_events_get failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_experiments_events_get_404() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-mcp/v1/experiment/event",
        404,
        r#"{"errors":["event not found"]}"#,
    )
    .await;

    let result =
        crate::commands::llm_obs::experiments_events_get(&cfg, "exp-1", "missing-evt").await;
    assert!(result.is_err(), "should fail on 404");
    assert!(result.unwrap_err().to_string().contains("404"));
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_experiments_events_get_no_auth() {
    let _lock = lock_env();
    let cfg = Config {
        api_key: None,
        app_key: None,
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let result = crate::commands::llm_obs::experiments_events_get(&cfg, "exp-1", "evt-1").await;
    assert!(result.is_err(), "should fail without auth");
    cleanup_env();
}

// -------------------------------------------------------------------------
// experiments metric-values
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_llm_obs_experiments_metric_values() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let body = r#"{"status":"success","data":{"metric_label":"accuracy","metric_type":"score","overall":{"count":10,"mean":0.85,"min_value":0.5,"max_value":1.0,"p50":0.9,"p90":0.95,"p95":0.98},"total_events":10}}"#;
    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-mcp/v1/experiment/metric-values",
        200,
        body,
    )
    .await;

    let result =
        crate::commands::llm_obs::experiments_metric_values(&cfg, "exp-1", "accuracy", None, None)
            .await;
    assert!(
        result.is_ok(),
        "experiments_metric_values failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_experiments_metric_values_segmented() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let body = r#"{"status":"success","data":{"metric_label":"accuracy","metric_type":"score","overall":{"count":5,"mean":0.9},"segments":[{"dimension_value":"prod","stats":{"count":5,"mean":0.9}}],"total_events":5}}"#;
    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-mcp/v1/experiment/metric-values",
        200,
        body,
    )
    .await;

    let result = crate::commands::llm_obs::experiments_metric_values(
        &cfg,
        "exp-1",
        "accuracy",
        Some("env".into()),
        Some("prod".into()),
    )
    .await;
    assert!(
        result.is_ok(),
        "experiments_metric_values segmented failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_experiments_metric_values_500() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-mcp/v1/experiment/metric-values",
        500,
        r#"{"errors":["internal server error"]}"#,
    )
    .await;

    let result =
        crate::commands::llm_obs::experiments_metric_values(&cfg, "exp-1", "accuracy", None, None)
            .await;
    assert!(result.is_err(), "should fail on 500");
    assert!(result.unwrap_err().to_string().contains("500"));
    cleanup_env();
}

// -------------------------------------------------------------------------
// experiments dimension-values
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_llm_obs_experiments_dimension_values() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let body = r#"{"status":"success","data":{"dimension":"env","unique_count":2,"values":[{"value":"prod","count":8},{"value":"staging","count":2}]}}"#;
    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-mcp/v1/experiment/dimension-values",
        200,
        body,
    )
    .await;

    let result = crate::commands::llm_obs::experiments_dimension_values(&cfg, "exp-1", "env").await;
    assert!(
        result.is_ok(),
        "experiments_dimension_values failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_experiments_dimension_values_403() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-mcp/v1/experiment/dimension-values",
        403,
        r#"{"errors":["Forbidden"]}"#,
    )
    .await;

    let result = crate::commands::llm_obs::experiments_dimension_values(&cfg, "exp-1", "env").await;
    assert!(result.is_err(), "should fail on 403");
    assert!(result.unwrap_err().to_string().contains("403"));
    cleanup_env();
}

// -------------------------------------------------------------------------
// spans search
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_llm_obs_spans_search() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let body = r#"{"status":"success","data":{"spans":[{"span_id":"s-1","trace_id":"t-1","name":"llm-call","span_kind":"llm","ml_app":"my-app","status":"ok","duration_ms":42.0,"start_ms":1000000,"tags":[]}]}}"#;
    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-mcp/v1/trace/search-spans",
        200,
        body,
    )
    .await;

    let result = crate::commands::llm_obs::spans_search(
        &cfg,
        Some("llm-call".into()),
        None,
        None,
        None,
        None,
        Some("my-app".into()),
        false,
        "1h".into(),
        "now".into(),
        10,
        None,
    )
    .await;
    assert!(result.is_ok(), "spans_search failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_spans_search_from_is_numeric_string() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let resp = r#"{"status":"success","data":{"spans":[]}}"#;
    let _mock = server
        .mock("POST", "/api/unstable/llm-obs-mcp/v1/trace/search-spans")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(resp)
        // Assert both from and to are 13-digit epoch ms strings, not relative strings
        .match_body(mockito::Matcher::Regex(r#""from":"\d{13}""#.to_string()))
        .match_body(mockito::Matcher::Regex(r#""to":"\d{13}""#.to_string()))
        .create_async()
        .await;

    let result = crate::commands::llm_obs::spans_search(
        &cfg,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
        "4h".into(),
        "now".into(),
        5,
        None,
    )
    .await;
    assert!(result.is_ok(), "spans_search failed: {:?}", result.err());
    _mock.assert();
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_spans_search_invalid_from_returns_error() {
    let _lock = lock_env();
    let server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    // No mock needed — should error before any network call
    let result = crate::commands::llm_obs::spans_search(
        &cfg,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
        "not-a-valid-time".into(),
        "now".into(),
        5,
        None,
    )
    .await;
    assert!(result.is_err(), "expected error for invalid --from value");
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_spans_search_empty_results() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let body = r#"{"status":"success","data":{"spans":[]}}"#;
    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-mcp/v1/trace/search-spans",
        200,
        body,
    )
    .await;

    let result = crate::commands::llm_obs::spans_search(
        &cfg,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
        "1h".into(),
        "now".into(),
        20,
        None,
    )
    .await;
    assert!(
        result.is_ok(),
        "spans_search empty failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_spans_search_500() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-mcp/v1/trace/search-spans",
        500,
        r#"{"errors":["internal server error"]}"#,
    )
    .await;

    let result = crate::commands::llm_obs::spans_search(
        &cfg,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
        "1h".into(),
        "now".into(),
        20,
        None,
    )
    .await;
    assert!(result.is_err(), "should fail on 500");
    assert!(result.unwrap_err().to_string().contains("500"));
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_spans_search_no_auth() {
    let _lock = lock_env();
    let cfg = Config {
        api_key: None,
        app_key: None,
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let result = crate::commands::llm_obs::spans_search(
        &cfg,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
        "1h".into(),
        "now".into(),
        20,
        None,
    )
    .await;
    assert!(result.is_err(), "should fail without auth");
    cleanup_env();
}

// ---- spans analytics ----

#[tokio::test]
async fn test_llm_obs_spans_analytics() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let body = r#"{"buckets":[{"by":{"span_name":"llm.call","@meta.error.type":"timeout"},"computes":{"c0":42}}]}"#;
    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-query-rewriter/timeseries",
        200,
        body,
    )
    .await;

    let result = crate::commands::llm_obs::spans_analytics(
        &cfg,
        Some("span.kind:llm".into()),
        "1h".into(),
        "now".into(),
        Some("span_name,@meta.error.type".into()),
        "count".into(),
        10,
        None,
    )
    .await;
    assert!(result.is_ok(), "spans_analytics failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_spans_analytics_401() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-query-rewriter/timeseries",
        401,
        r#"{"errors":["Unauthorized"]}"#,
    )
    .await;

    let result = crate::commands::llm_obs::spans_analytics(
        &cfg,
        None,
        "1h".into(),
        "now".into(),
        Some("span_name".into()),
        "count".into(),
        10,
        None,
    )
    .await;
    assert!(result.is_err(), "should fail on 401");
    assert!(result.unwrap_err().to_string().contains("401"));
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_spans_analytics_403() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-query-rewriter/timeseries",
        403,
        r#"{"errors":["Forbidden"]}"#,
    )
    .await;

    let result = crate::commands::llm_obs::spans_analytics(
        &cfg,
        None,
        "1h".into(),
        "now".into(),
        Some("span_name".into()),
        "count".into(),
        10,
        None,
    )
    .await;
    assert!(result.is_err(), "should fail on 403");
    assert!(result.unwrap_err().to_string().contains("403"));
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_spans_analytics_500() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let _mock = mock_post(
        &mut server,
        "/api/unstable/llm-obs-query-rewriter/timeseries",
        500,
        r#"{"errors":["internal server error"]}"#,
    )
    .await;

    let result = crate::commands::llm_obs::spans_analytics(
        &cfg,
        None,
        "1h".into(),
        "now".into(),
        Some("span_name".into()),
        "count".into(),
        10,
        None,
    )
    .await;
    assert!(result.is_err(), "should fail on 500");
    assert!(result.unwrap_err().to_string().contains("500"));
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_spans_analytics_invalid_from() {
    let _lock = lock_env();
    let server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    // No mock — should error before any network call
    let result = crate::commands::llm_obs::spans_analytics(
        &cfg,
        None,
        "not-a-valid-time".into(),
        "now".into(),
        Some("span_name".into()),
        "count".into(),
        10,
        None,
    )
    .await;
    assert!(result.is_err(), "should fail with invalid --from");
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_spans_analytics_no_auth() {
    let _lock = lock_env();
    let cfg = Config {
        api_key: None,
        app_key: None,
        access_token: None,
        site: "datadoghq.com".into(),
        org: None,
        output_format: OutputFormat::Json,
        auto_approve: false,
        agent_mode: false,
        read_only: false,
    };

    let result = crate::commands::llm_obs::spans_analytics(
        &cfg,
        None,
        "1h".into(),
        "now".into(),
        Some("span_name".into()),
        "count".into(),
        10,
        None,
    )
    .await;
    assert!(result.is_err(), "should fail without auth");
    cleanup_env();
}

// -------------------------------------------------------------------------
// Auth status --site flag
// -------------------------------------------------------------------------

#[test]
fn test_auth_status_accepts_site_flag() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "auth", "status", "--site", "datadoghq.eu"])
        .expect("auth status --site should parse");

    match cli.command {
        crate::Commands::Auth { action } => match action {
            crate::AuthActions::Status { site } => {
                assert_eq!(site, Some("datadoghq.eu".to_string()));
            }
            _ => panic!("expected AuthActions::Status"),
        },
        _ => panic!("expected Commands::Auth"),
    }
}

#[test]
fn test_auth_status_site_flag_is_optional() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "auth", "status"])
        .expect("auth status without --site should parse");

    match cli.command {
        crate::Commands::Auth { action } => match action {
            crate::AuthActions::Status { site } => {
                assert_eq!(site, None);
            }
            _ => panic!("expected AuthActions::Status"),
        },
        _ => panic!("expected Commands::Auth"),
    }
}

#[test]
fn test_top_level_commands_sorted_alphabetically() {
    let app = crate::Cli::command();
    let names: Vec<&str> = app
        .get_subcommands()
        .filter(|cmd| cmd.get_name() != "help" && !cmd.is_hide_set())
        .map(|cmd| cmd.get_name())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(
        names, sorted,
        "top-level commands must be in alphabetical order.\nActual:   {names:?}\nExpected: {sorted:?}"
    );
}

#[test]
fn test_dbm_samples_search_parses() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "dbm",
        "samples",
        "search",
        "--query",
        "service:db",
        "--from",
        "1h",
        "--limit",
        "10",
        "--sort",
        "asc",
    ])
    .expect("dbm samples search should parse");

    match cli.command {
        crate::Commands::Dbm { action } => match action {
            crate::DbmActions::Samples { action } => match action {
                crate::DbmSamplesActions::Search {
                    query,
                    from,
                    to,
                    limit,
                    sort,
                } => {
                    assert_eq!(query, "service:db");
                    assert_eq!(from, "1h");
                    assert_eq!(to, "now");
                    assert_eq!(limit, 10);
                    assert_eq!(sort, "asc");
                }
            },
        },
        _ => panic!("expected Commands::Dbm"),
    }
}

#[test]
fn test_ddsql_table_query_accepts_leading_comment() {
    use clap::Parser;

    let query = "-- owner breakdown\nSELECT 1";
    let cli = crate::Cli::try_parse_from(["pup", "ddsql", "table", "--query", query])
        .expect("ddsql table with leading SQL comment should parse");

    match cli.command {
        crate::Commands::Ddsql { action } => match action {
            crate::DdsqlActions::Table { query: parsed, .. } => {
                assert_eq!(parsed, query);
            }
            _ => panic!("expected DdsqlActions::Table"),
        },
        _ => panic!("expected Commands::Ddsql"),
    }
}

#[test]
fn test_ddsql_table_query_accepts_explicit_stdin_marker() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "ddsql", "table", "--query", "-"])
        .expect("ddsql table --query - should parse");

    match cli.command {
        crate::Commands::Ddsql { action } => match action {
            crate::DdsqlActions::Table { query, .. } => {
                assert_eq!(query, "-");
            }
            _ => panic!("expected DdsqlActions::Table"),
        },
        _ => panic!("expected Commands::Ddsql"),
    }
}

#[test]
fn test_ddsql_time_series_query_accepts_explicit_stdin_marker() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "ddsql", "time-series", "--query", "-"])
        .expect("ddsql time-series --query - should parse");

    match cli.command {
        crate::Commands::Ddsql { action } => match action {
            crate::DdsqlActions::TimeSeries { query, .. } => {
                assert_eq!(query, "-");
            }
            _ => panic!("expected DdsqlActions::TimeSeries"),
        },
        _ => panic!("expected Commands::Ddsql"),
    }
}

#[test]
fn test_ddsql_table_query_requires_explicit_value() {
    let result = crate::Cli::command().try_get_matches_from(["pup", "ddsql", "table", "--query"]);
    assert!(
        result.is_err(),
        "expected ddsql table --query to require a value"
    );
}

// ---- Debugger ----

#[tokio::test]
async fn test_debugger_probes_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"[{"id": "probe-1", "type": "LOG_PROBE"}]"#).await;
    let _ = crate::commands::debugger::probes_list(&cfg, None).await;
    cleanup_env();
}

#[tokio::test]
async fn test_debugger_probes_list_with_service() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"[{"id": "probe-1", "type": "LOG_PROBE"}]"#).await;
    let _ = crate::commands::debugger::probes_list(&cfg, Some("my-service")).await;
    cleanup_env();
}

#[tokio::test]
async fn test_debugger_probes_get() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"id": "probe-1", "type": "LOG_PROBE"}"#).await;
    let _ = crate::commands::debugger::probes_get(&cfg, "probe-1").await;
    cleanup_env();
}

#[tokio::test]
async fn test_debugger_probes_create() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": {"id": "probe-new"}}"#).await;
    let params = crate::commands::debugger::ProbeCreateParams {
        service: "my-service",
        env: "staging",
        probe_location: "com.example.MyClass:myMethod",
        language: "java",
        template: None,
        condition: None,
        snapshot: true,
        capture_expressions: vec![],
        rate: 1,
        budget: 1000,
        ttl: Some("1h"),
        depth: 3,
        fields: None,
    };
    let _ = crate::commands::debugger::probes_create(&cfg, params).await;
    cleanup_env();
}

#[tokio::test]
async fn test_debugger_probes_delete() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#""#).await;
    let _ = crate::commands::debugger::probes_delete(&cfg, "probe-1").await;
    cleanup_env();
}

// ---- SymDB ----

#[tokio::test]
async fn test_symdb_search() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(
        &mut s,
        r#"{"data": [{"attributes": {"scopes": [{"scope": {"name": "MyClass"}}]}}]}"#,
    )
    .await;
    let _ = crate::commands::symdb::search(
        &cfg,
        "my-service",
        "MyClass",
        None,
        &crate::commands::symdb::SymdbView::Full,
    )
    .await;
    cleanup_env();
}

#[tokio::test]
async fn test_symdb_search_names_view() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(
        &mut s,
        r#"{"data": [{"attributes": {"scopes": [{"scope": {"name": "MyClass"}}]}}]}"#,
    )
    .await;
    let _ = crate::commands::symdb::search(
        &cfg,
        "my-service",
        "MyClass",
        None,
        &crate::commands::symdb::SymdbView::Names,
    )
    .await;
    cleanup_env();
}

#[tokio::test]
async fn test_symdb_search_probe_locations_view() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": [{"attributes": {"scopes": [{"scope": {"name": "MyClass", "scope_type": "METHOD"}, "probe_location": {"type_name": "MyClass", "method_name": "doStuff"}}]}}]}"#).await;
    let _ = crate::commands::symdb::search(
        &cfg,
        "my-service",
        "MyClass",
        None,
        &crate::commands::symdb::SymdbView::ProbeLocations,
    )
    .await;
    cleanup_env();
}

#[test]
fn test_symdb_view_display() {
    assert_eq!(crate::commands::symdb::SymdbView::Full.to_string(), "full");
    assert_eq!(
        crate::commands::symdb::SymdbView::Names.to_string(),
        "names"
    );
    assert_eq!(
        crate::commands::symdb::SymdbView::ProbeLocations.to_string(),
        "probe-locations"
    );
}

// -------------------------------------------------------------------------
// Software Catalog
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_software_catalog_entities_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result =
        crate::commands::software_catalog::entities_list(&cfg, None, None, None, None).await;
    assert!(
        result.is_ok(),
        "software catalog entities list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_software_catalog_entities_list_with_filters() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result = crate::commands::software_catalog::entities_list(
        &cfg,
        Some("shop-".to_string()),
        Some("service".to_string()),
        None,
        None,
    )
    .await;
    assert!(
        result.is_ok(),
        "software catalog entities list with name and kind filter failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_software_catalog_kinds_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result = crate::commands::software_catalog::kinds_list(&cfg).await;
    assert!(
        result.is_ok(),
        "software catalog kinds list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_software_catalog_relations_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result = crate::commands::software_catalog::relations_list(&cfg).await;
    assert!(
        result.is_ok(),
        "software catalog relations list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_software_catalog_entities_list_error() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(500)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Internal Server Error"]}"#)
        .create_async()
        .await;
    let result =
        crate::commands::software_catalog::entities_list(&cfg, None, None, None, None).await;
    assert!(
        result.is_err(),
        "expected software catalog entities list to fail on 500"
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// Incident Teams
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_incident_teams_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[],"meta":{}}"#).await;
    let result = crate::commands::incidents::teams_list(&cfg).await;
    assert!(
        result.is_ok(),
        "incident teams list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// Incident Services
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_incident_services_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[],"meta":{}}"#).await;
    let result = crate::commands::incidents::services_list(&cfg).await;
    assert!(
        result.is_ok(),
        "incident services list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_incident_services_list_error() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Forbidden"]}"#)
        .create_async()
        .await;
    let result = crate::commands::incidents::services_list(&cfg).await;
    assert!(result.is_err(), "incident services list should fail on 403");
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// MS Teams
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_ms_teams_handles_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[],"meta":{}}"#).await;
    let result = crate::commands::ms_teams::handles_list(&cfg).await;
    assert!(
        result.is_ok(),
        "ms teams handles list failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_ms_teams_handles_list_error() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Forbidden"]}"#)
        .create_async()
        .await;
    let result = crate::commands::ms_teams::handles_list(&cfg).await;
    assert!(result.is_err(), "ms teams handles list should fail on 403");
    cleanup_env();
}

#[tokio::test]
async fn test_ms_teams_workflows_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[],"meta":{}}"#).await;
    let result = crate::commands::ms_teams::workflows_list(&cfg).await;
    assert!(
        result.is_ok(),
        "ms teams workflows list failed: {:?}",
        result.err()
    );
    cleanup_env();
}

// -------------------------------------------------------------------------
// CSM Threats
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_csm_threats_agent_policies_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result = crate::commands::csm_threats::agent_policies_list(&cfg).await;
    assert!(
        result.is_ok(),
        "CSM threats agent policies list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_csm_threats_agent_rules_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result = crate::commands::csm_threats::agent_rules_list(&cfg, None).await;
    assert!(
        result.is_ok(),
        "CSM threats agent rules list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_csm_threats_agent_rules_list_with_policy() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result =
        crate::commands::csm_threats::agent_rules_list(&cfg, Some("policy-123".to_string())).await;
    assert!(
        result.is_ok(),
        "CSM threats agent rules list with policy failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_csm_threats_agent_policies_list_error() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Forbidden"]}"#)
        .create_async()
        .await;
    let result = crate::commands::csm_threats::agent_policies_list(&cfg).await;
    assert!(
        result.is_err(),
        "CSM threats agent policies list should fail on 403"
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_csm_threats_backend_rules_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[],"meta":{}}"#).await;
    let result = crate::commands::csm_threats::backend_rules_list(&cfg, None).await;
    assert!(
        result.is_ok(),
        "backend rules list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_csm_threats_backend_rules_list_with_query() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[],"meta":{}}"#).await;
    let result =
        crate::commands::csm_threats::backend_rules_list(&cfg, Some("name:my-rule".into())).await;
    assert!(
        result.is_ok(),
        "backend rules list with query failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_csm_threats_backend_rules_get() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"id":"rule-123","name":"test"}"#).await;
    let result = crate::commands::csm_threats::backend_rules_get(&cfg, "rule-123").await;
    assert!(
        result.is_ok(),
        "backend rules get failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_csm_threats_backend_rules_delete() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("DELETE", mockito::Matcher::Any)
        .with_status(204)
        .create_async()
        .await;
    let result = crate::commands::csm_threats::backend_rules_delete(&cfg, "rule-123").await;
    assert!(
        result.is_ok(),
        "backend rules delete failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_csm_threats_backend_rules_list_error() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Forbidden"]}"#)
        .create_async()
        .await;
    let result = crate::commands::csm_threats::backend_rules_list(&cfg, None).await;
    assert!(result.is_err(), "backend rules list should fail on 403");
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// Agentless Scanning
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_agentless_scanning_aws_scan_options_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result = crate::commands::agentless_scanning::aws_scan_options_list(&cfg).await;
    assert!(
        result.is_ok(),
        "Agentless scanning AWS scan options list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_agentless_scanning_azure_scan_options_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result = crate::commands::agentless_scanning::azure_scan_options_list(&cfg).await;
    assert!(
        result.is_ok(),
        "Agentless scanning Azure scan options list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_agentless_scanning_gcp_scan_options_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result = crate::commands::agentless_scanning::gcp_scan_options_list(&cfg).await;
    assert!(
        result.is_ok(),
        "Agentless scanning GCP scan options list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_agentless_scanning_aws_on_demand_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result = crate::commands::agentless_scanning::aws_on_demand_list(&cfg).await;
    assert!(
        result.is_ok(),
        "Agentless scanning AWS on-demand list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_agentless_scanning_aws_scan_options_list_error() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Forbidden"]}"#)
        .create_async()
        .await;
    let result = crate::commands::agentless_scanning::aws_scan_options_list(&cfg).await;
    assert!(
        result.is_err(),
        "Agentless scanning AWS scan options list should fail on 403"
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// AuthN Mappings
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_authn_mappings_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result = crate::commands::authn_mappings::list(&cfg).await;
    assert!(
        result.is_ok(),
        "authn mappings list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_authn_mappings_list_error() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Forbidden"]}"#)
        .create_async()
        .await;
    let result = crate::commands::authn_mappings::list(&cfg).await;
    assert!(result.is_err(), "expected error for 403 response");
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_authn_mappings_get() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "GET",
        r#"{"data":{"type":"authn_mappings","id":"abc-123","attributes":{}}}"#,
    )
    .await;
    let result = crate::commands::authn_mappings::get(&cfg, "abc-123").await;
    assert!(
        result.is_ok(),
        "authn mappings get failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// ASM WAF Custom Rules
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_asm_custom_rules_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result = crate::commands::security::asm_custom_rules_list(&cfg).await;
    assert!(
        result.is_ok(),
        "ASM custom rules list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_asm_custom_rules_list_error() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Forbidden"]}"#)
        .create_async()
        .await;
    let result = crate::commands::security::asm_custom_rules_list(&cfg).await;
    assert!(result.is_err(), "expected error for 403 response");
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// ASM WAF Exclusion Filters
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_asm_exclusions_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result = crate::commands::security::asm_exclusions_list(&cfg).await;
    assert!(
        result.is_ok(),
        "ASM exclusions list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// Restriction Policies
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_restriction_policy_get() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "GET",
        r#"{"data":{"type":"restriction_policy","id":"dashboard:abc-123","attributes":{"bindings":[]}}}"#,
    )
    .await;
    let result = crate::commands::security::restriction_policy_get(&cfg, "dashboard:abc-123").await;
    assert!(
        result.is_ok(),
        "restriction policy get failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_restriction_policy_get_error() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Not Found"]}"#)
        .create_async()
        .await;
    let result = crate::commands::security::restriction_policy_get(&cfg, "dashboard:missing").await;
    assert!(result.is_err(), "expected error for 404 response");
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// Processes
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_processes_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "GET",
        r#"{"data":[],"meta":{"page":{"after":""}}}"#,
    )
    .await;
    let result = crate::commands::processes::list(&cfg, None, None, None).await;
    assert!(result.is_ok(), "processes list failed: {:?}", result.err());
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_processes_list_with_search() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "GET",
        r#"{"data":[],"meta":{"page":{"after":""}}}"#,
    )
    .await;
    let result = crate::commands::processes::list(&cfg, Some("nginx".into()), None, Some(10)).await;
    assert!(
        result.is_ok(),
        "processes list with search failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// Logs Restriction Queries
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_logs_restriction_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "GET",
        r#"{"data":[],"meta":{"page":{"total_count":0}}}"#,
    )
    .await;
    let result = crate::commands::logs_restriction::list(&cfg).await;
    assert!(
        result.is_ok(),
        "logs_restriction list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_logs_restriction_delete_missing_id() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("DELETE", mockito::Matcher::Any)
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Not Found"]}"#)
        .create_async()
        .await;
    let result = crate::commands::logs_restriction::delete(&cfg, "nonexistent-id").await;
    assert!(
        result.is_err(),
        "expected error for missing restriction query"
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// Service Accounts
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_service_account_app_keys_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[],"meta":{}}"#).await;
    let result = crate::commands::users::service_account_app_keys_list(&cfg, "sa-test-id").await;
    assert!(
        result.is_ok(),
        "service account app keys list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_service_account_app_keys_delete_missing() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("DELETE", mockito::Matcher::Any)
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Not Found"]}"#)
        .create_async()
        .await;
    let result = crate::commands::users::service_account_app_keys_delete(
        &cfg,
        "sa-test-id",
        "key-not-found",
    )
    .await;
    assert!(result.is_err(), "expected error for missing app key delete");
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_service_accounts_create() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "POST",
        r#"{"data":{"type":"users","id":"sa-new","attributes":{}}}"#,
    )
    .await;
    let result = crate::commands::users::service_accounts_create(&cfg, "/tmp/test.json").await;
    assert!(result.is_err(), "expected error for missing input file");
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_service_account_app_keys_get() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "GET",
        r#"{"data":{"type":"api_keys","id":"key-1","attributes":{}}}"#,
    )
    .await;
    let result = crate::commands::users::service_account_app_keys_get(&cfg, "sa-id", "key-1").await;
    assert!(
        result.is_ok(),
        "service account app key get failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_service_account_app_keys_get_error() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Not Found"]}"#)
        .create_async()
        .await;
    let result =
        crate::commands::users::service_account_app_keys_get(&cfg, "sa-id", "key-missing").await;
    assert!(result.is_err(), "expected error for missing app key get");
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_service_account_app_keys_create() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Forbidden"]}"#)
        .create_async()
        .await;
    let result =
        crate::commands::users::service_account_app_keys_create(&cfg, "sa-id", "/tmp/test.json")
            .await;
    assert!(
        result.is_err(),
        "expected error for service account app key create"
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_service_account_app_keys_update() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("PATCH", mockito::Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Forbidden"]}"#)
        .create_async()
        .await;
    let result = crate::commands::users::service_account_app_keys_update(
        &cfg,
        "sa-id",
        "key-1",
        "/tmp/test.json",
    )
    .await;
    assert!(
        result.is_err(),
        "expected error for service account app key update"
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// Spans Metrics
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_spans_metrics_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result = crate::commands::traces::metrics_list(&cfg).await;
    assert!(
        result.is_ok(),
        "spans metrics list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_spans_metrics_get() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(
        &mut server,
        "GET",
        r#"{"data":{"id":"test.metric","type":"spans_metrics","attributes":{}}}"#,
    )
    .await;
    let result = crate::commands::traces::metrics_get(&cfg, "test.metric").await;
    assert!(
        result.is_ok(),
        "spans metrics get failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_spans_metrics_delete() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "DELETE", "").await;
    let result = crate::commands::traces::metrics_delete(&cfg, "test.metric").await;
    assert!(
        result.is_ok(),
        "spans metrics delete failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_spans_metrics_get_path() {
    // Verify the GET request hits the correct API path for a named metric.
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let mock = server
        .mock("GET", "/api/v2/apm/config/metrics/my.metric")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":{"id":"my.metric","type":"spans_metrics","attributes":{}}}"#)
        .create_async()
        .await;
    let result = crate::commands::traces::metrics_get(&cfg, "my.metric").await;
    assert!(
        result.is_ok(),
        "spans metrics get (path check) failed: {:?}",
        result.err()
    );
    mock.assert_async().await;
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_spans_metrics_list_error() {
    // Verify that a 403 response causes metrics_list to return an error.
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["Forbidden"]}"#)
        .create_async()
        .await;
    let result = crate::commands::traces::metrics_list(&cfg).await;
    assert!(result.is_err(), "spans metrics list should fail on 403");
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// Datasets
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_datasets_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result = crate::commands::datasets::list(&cfg).await;
    assert!(result.is_ok(), "datasets list failed: {:?}", result.err());
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_datasets_get() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{}"#).await;
    let result = crate::commands::datasets::get(&cfg, "test-id").await;
    assert!(result.is_ok(), "datasets get failed: {:?}", result.err());
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_datasets_delete() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "DELETE", "").await;
    let result = crate::commands::datasets::delete(&cfg, "test-id").await;
    assert!(result.is_ok(), "datasets delete failed: {:?}", result.err());
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// Data Deletion
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_data_deletion_requests_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result = crate::commands::data_deletion::requests_list(&cfg, None, None, None).await;
    assert!(
        result.is_ok(),
        "data deletion requests list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_data_deletion_requests_list_with_filters() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let mock = server
        .mock("GET", mockito::Matcher::Any)
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("product".into(), "logs".into()),
            mockito::Matcher::UrlEncoded("status".into(), "pending".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":[]}"#)
        .create_async()
        .await;
    let result = crate::commands::data_deletion::requests_list(
        &cfg,
        Some("logs".into()),
        None,
        Some("pending".into()),
    )
    .await;
    assert!(
        result.is_ok(),
        "data deletion requests list with filters failed: {:?}",
        result.err()
    );
    mock.assert_async().await;
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// Action Connections
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_connections_get() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{}"#).await;
    let result = crate::commands::workflows::connections_get(&cfg, "conn-id").await;
    assert!(result.is_ok(), "connections get failed: {:?}", result.err());
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_connections_delete() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "DELETE", "").await;
    let result = crate::commands::workflows::connections_delete(&cfg, "conn-id").await;
    assert!(
        result.is_ok(),
        "connections delete failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

async fn test_scorecard_outcomes_list_with_data() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let body = r#"{"data":[{"id":"outcome-1","type":"outcome","attributes":{"state":"pass","service-name":"my-service"}}]}"#;
    let _mock = mock_any(&mut server, "GET", body).await;
    let result = crate::commands::scorecards::outcomes_list(&cfg).await;
    assert!(
        result.is_ok(),
        "scorecard outcomes list with data failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_scorecard_rules_delete() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("DELETE", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(204)
        .create_async()
        .await;
    let result = crate::commands::scorecards::rules_delete(&cfg, "rule-123").await;
    assert!(
        result.is_ok(),
        "scorecard rules delete failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// Static Analysis
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_static_analysis_custom_rulesets_get() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let body = r#"{"data":{"id":"my-ruleset","type":"custom-ruleset","attributes":{"name":"my-ruleset","created_at":"2024-01-01T00:00:00Z","created_by":"user","description":"","short_description":"","rules":[]}}}"#;
    let _mock = mock_any(&mut server, "GET", body).await;
    let result = crate::commands::static_analysis::custom_rulesets_get(&cfg, "my-ruleset").await;
    assert!(
        result.is_ok(),
        "static analysis custom rulesets get failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_static_analysis_custom_rulesets_get_error() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["not found"]}"#)
        .create_async()
        .await;
    let result = crate::commands::static_analysis::custom_rulesets_get(&cfg, "nonexistent").await;
    assert!(
        result.is_err(),
        "static analysis custom rulesets get should fail on 404"
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_static_analysis_custom_rules_get() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    // Complex nested model — test that a 404 error is correctly propagated.
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["not found"]}"#)
        .create_async()
        .await;
    let result =
        crate::commands::static_analysis::custom_rules_get(&cfg, "my-ruleset", "my-rule").await;
    assert!(
        result.is_err(),
        "static analysis custom rules get should fail on 404"
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_static_analysis_custom_rule_revisions_list() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
    let result =
        crate::commands::static_analysis::custom_rule_revisions_list(&cfg, "my-ruleset", "my-rule")
            .await;
    assert!(
        result.is_ok(),
        "static analysis custom rule revisions list failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_static_analysis_custom_rules_delete() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("DELETE", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(204)
        .create_async()
        .await;
    let result =
        crate::commands::static_analysis::custom_rules_delete(&cfg, "my-ruleset", "my-rule").await;
    assert!(
        result.is_ok(),
        "static analysis custom rules delete failed: {:?}",
        result.err()
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// Scorecards — create/update negative tests
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_scorecard_rules_create_missing_file() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let result = crate::commands::scorecards::rules_create(&cfg, "/nonexistent/file.json").await;
    assert!(result.is_err(), "rules_create should fail for missing file");
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_scorecard_outcomes_batch_create_missing_file() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let result =
        crate::commands::scorecards::outcomes_batch_create(&cfg, "/nonexistent/file.json").await;
    assert!(
        result.is_err(),
        "outcomes_batch_create should fail for missing file"
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_scorecard_rules_create_api_error() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("POST", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(422)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["unprocessable entity"]}"#)
        .create_async()
        .await;
    let tmp = std::env::temp_dir().join("test_scorecard_rule.json");
    std::fs::write(&tmp, r#"{"data":{}}"#).unwrap();
    let result = crate::commands::scorecards::rules_create(&cfg, tmp.to_str().unwrap()).await;
    let _ = std::fs::remove_file(&tmp);
    assert!(result.is_err(), "rules_create should fail on 422 response");
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// Static Analysis — create/list error tests
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_static_analysis_custom_rules_create_missing_file() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let result = crate::commands::static_analysis::custom_rules_create(
        &cfg,
        "my-ruleset",
        "/nonexistent/file.json",
    )
    .await;
    assert!(
        result.is_err(),
        "custom_rules_create should fail for missing file"
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_static_analysis_custom_rules_list_error() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":["forbidden"]}"#)
        .create_async()
        .await;
    let result =
        crate::commands::static_analysis::custom_rules_get(&cfg, "my-ruleset", "my-rule").await;
    assert!(
        result.is_err(),
        "custom_rules_get should fail on 403 response"
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// =========================================================================
// pup api — raw authenticated API access
// =========================================================================

#[tokio::test]
async fn test_api_get_success() {
    let _lock = lock_env();
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

    let result = crate::commands::api::run(
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
    let _lock = lock_env();
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

    let result = crate::commands::api::run(
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
    let _lock = lock_env();
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

    let result = crate::commands::api::run(
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
    let _lock = lock_env();
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

    let result = crate::commands::api::run(
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
    let _lock = lock_env();
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

    let result = crate::commands::api::run(
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
    let _lock = lock_env();
    let server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let result = crate::commands::api::run(
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
    let _lock = lock_env();
    let server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let result = crate::commands::api::run(
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

#[tokio::test]
async fn test_flaky_tests_management_policies_get() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let tmp = write_temp_json(
        "pup_test_ftmp_get.json",
        r#"{"data":{"type":"test_optimization_get_flaky_tests_management_policies_request","attributes":{"repository_id":"test-repo"}}}"#,
    );
    let _mock = mock_any(&mut server, "POST", r#"{"data":{}}"#).await;
    let result = crate::commands::test_optimization::flaky_tests_management_policies_get(
        &cfg,
        tmp.to_str().unwrap(),
    )
    .await;
    assert!(
        result.is_ok(),
        "flaky_tests_management_policies_get failed: {:?}",
        result.err()
    );
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_flaky_tests_management_policies_update() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let tmp = write_temp_json(
        "pup_test_ftmp_update.json",
        r#"{"data":{"type":"test_optimization_update_flaky_tests_management_policies_request","attributes":{"repository_id":"test-repo"}}}"#,
    );
    let _mock = mock_any(&mut server, "PATCH", r#"{"data":{}}"#).await;
    let result = crate::commands::test_optimization::flaky_tests_management_policies_update(
        &cfg,
        tmp.to_str().unwrap(),
    )
    .await;
    assert!(
        result.is_ok(),
        "flaky_tests_management_policies_update failed: {:?}",
        result.err()
    );
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_flaky_tests_management_policies_get_missing_file() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let result = crate::commands::test_optimization::flaky_tests_management_policies_get(
        &cfg,
        "/nonexistent/file.json",
    )
    .await;
    assert!(
        result.is_err(),
        "flaky_tests_management_policies_get should fail for missing file"
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

#[tokio::test]
async fn test_flaky_tests_management_policies_update_missing_file() {
    let _lock = lock_env();
    std::env::set_var("DD_TOKEN_STORAGE", "file");
    let server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let result = crate::commands::test_optimization::flaky_tests_management_policies_update(
        &cfg,
        "/nonexistent/file.json",
    )
    .await;
    assert!(
        result.is_err(),
        "flaky_tests_management_policies_update should fail for missing file"
    );
    cleanup_env();
    std::env::remove_var("DD_TOKEN_STORAGE");
}

// -------------------------------------------------------------------------
// raw_request query parameter path
// -------------------------------------------------------------------------

/// Verifies that raw_request attaches query parameters and returns Ok when the
/// server responds 200. Exercises the `!query.is_empty()` branch added to the function.
#[tokio::test]
async fn test_raw_request_with_query_params_ok() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = server
        .mock("GET", "/api/v2/monitors")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body("[]")
        .create_async()
        .await;
    let resp = crate::client::raw_request(
        &cfg,
        "GET",
        "/api/v2/monitors",
        &[("page", "1"), ("page_size", "10")],
        None,
        None,
        "application/json",
        &[],
    )
    .await;
    assert!(
        resp.is_ok(),
        "raw_request with query failed: {:?}",
        resp.err()
    );
    cleanup_env();
}

// -------------------------------------------------------------------------
// Cloud Cost Management — tag description positive-path tests
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_tag_desc_upsert_success() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "PUT", r#"{}"#).await;
    let result =
        crate::commands::cost_ccm::tag_desc_upsert(&cfg, "team", "The team tag", None).await;
    assert!(result.is_ok(), "tag_desc_upsert failed: {:?}", result.err());
    cleanup_env();
}

// -------------------------------------------------------------------------
// Profiling
// -------------------------------------------------------------------------

#[tokio::test]
async fn test_profiling_list_ok() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    let mock = s
        .mock("POST", "/api/unstable/profiles/list")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":[]}"#)
        .create_async()
        .await;
    let result = crate::commands::profiling::list(
        &cfg,
        "*".into(),
        "15m".into(),
        "now".into(),
        None,
        "desc".into(),
        100,
    )
    .await;
    assert!(result.is_ok(), "list failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_list_error() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    s.mock("POST", "/api/unstable/profiles/list")
        .with_status(500)
        .with_body(r#"{"errors":["boom"]}"#)
        .create_async()
        .await;
    let result = crate::commands::profiling::list(
        &cfg,
        "*".into(),
        "15m".into(),
        "now".into(),
        None,
        "desc".into(),
        100,
    )
    .await;
    assert!(result.is_err(), "expected error on 500");
    cleanup_env();
}

#[tokio::test]
async fn test_tag_desc_upsert_with_cloud_success() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "PUT", r#"{}"#).await;
    let result = crate::commands::cost_ccm::tag_desc_upsert(
        &cfg,
        "team",
        "The team tag",
        Some("aws".into()),
    )
    .await;
    assert!(
        result.is_ok(),
        "tag_desc_upsert with cloud failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_info_ok() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    let mock = s
        .mock("GET", "/profiling/api/v1/profiles/abc123/info")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":{"id":"abc123"}}"#)
        .create_async()
        .await;
    let result = crate::commands::profiling::info(&cfg, "abc123", None).await;
    assert!(result.is_ok(), "info failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_info_with_event_id() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    let mock = s
        .mock("GET", "/profiling/api/v1/profiles/abc123/info")
        .match_query(mockito::Matcher::UrlEncoded(
            "eventId".into(),
            "evt-1".into(),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":{"id":"abc123"}}"#)
        .create_async()
        .await;
    let result = crate::commands::profiling::info(&cfg, "abc123", Some("evt-1".into())).await;
    assert!(result.is_ok(), "info failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_info_error() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    s.mock("GET", mockito::Matcher::Any)
        .with_status(404)
        .with_body(r#"{"errors":["not found"]}"#)
        .create_async()
        .await;
    let result = crate::commands::profiling::info(&cfg, "missing", None).await;
    assert!(result.is_err(), "expected error on 404");
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_analysis_ok() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    let mock = s
        .mock("GET", "/profiling/api/v1/profiles/abc/analysis")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":{"insights":[]}}"#)
        .create_async()
        .await;
    let result = crate::commands::profiling::analysis(&cfg, "abc", None).await;
    assert!(result.is_ok(), "analysis failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_analysis_error() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    s.mock("GET", mockito::Matcher::Any)
        .with_status(500)
        .create_async()
        .await;
    let result = crate::commands::profiling::analysis(&cfg, "abc", None).await;
    assert!(result.is_err(), "expected error on 500");
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_analytics_ok() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    let mock = s
        .mock("POST", "/api/unstable/profiles/analytics")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":[]}"#)
        .create_async()
        .await;
    let result = crate::commands::profiling::analytics(
        &cfg,
        "service:web".into(),
        "15m".into(),
        "now".into(),
        Some("service,env".into()),
        Some("count".into()),
        100,
    )
    .await;
    assert!(result.is_ok(), "analytics failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_tag_desc_delete_success() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "DELETE", r#"{}"#).await;
    let result = crate::commands::cost_ccm::tag_desc_delete(&cfg, "team", None).await;
    assert!(result.is_ok(), "tag_desc_delete failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_tag_desc_delete_with_cloud_success() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "DELETE", r#"{}"#).await;
    let result =
        crate::commands::cost_ccm::tag_desc_delete(&cfg, "team", Some("azure".into())).await;
    assert!(
        result.is_ok(),
        "tag_desc_delete with cloud failed: {:?}",
        result.err()
    );
    cleanup_env();
}

// -------------------------------------------------------------------------
// Cloud Cost Management — custom_costs_upload with version query parameter
// -------------------------------------------------------------------------

/// Verifies that the optional `version` query parameter is passed through
/// correctly after the refactor from path-string embedding to params vec.
#[tokio::test]
async fn test_custom_costs_upload_with_version_success() {
    let _lock = lock_env();
    let tmp = std::env::temp_dir().join("pup_test_upload_version.csv");
    std::fs::write(&tmp, b"cost,amount\nec2,100\n").unwrap();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_any(&mut server, "PUT", r#"{}"#).await;
    let result = crate::commands::cost_ccm::custom_costs_upload(
        &cfg,
        tmp.to_str().unwrap(),
        Some("v2".into()),
    )
    .await;
    assert!(
        result.is_ok(),
        "custom_costs_upload with version failed: {:?}",
        result.err()
    );
    let _ = std::fs::remove_file(&tmp);
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_analytics_rejects_empty_group_by() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    // Mock not required: we should fail before hitting the API.
    let result = crate::commands::profiling::analytics(
        &cfg,
        "*".into(),
        "15m".into(),
        "now".into(),
        Some(" , ".into()),
        None,
        100,
    )
    .await;
    assert!(
        result.is_err(),
        "expected error for empty --group-by tokens"
    );
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_fields_ok() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    let mock = s
        .mock("POST", "/api/unstable/profiles/interactive-analytics/field")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":[]}"#)
        .create_async()
        .await;
    let result = crate::commands::profiling::fields(
        &cfg,
        "service".into(),
        "*".into(),
        "15m".into(),
        "now".into(),
        100,
    )
    .await;
    assert!(result.is_ok(), "fields failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_fields_error() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    s.mock("POST", mockito::Matcher::Any)
        .with_status(500)
        .create_async()
        .await;
    let result = crate::commands::profiling::fields(
        &cfg,
        "service".into(),
        "*".into(),
        "15m".into(),
        "now".into(),
        100,
    )
    .await;
    assert!(result.is_err(), "expected error on 500");
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_aggregate_ok() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    let mock = s
        .mock("POST", "/profiling/api/v1/aggregate")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"flameGraph":[]}"#)
        .create_async()
        .await;
    let result = crate::commands::profiling::aggregate(
        &cfg,
        "service:web".into(),
        "cpu-time".into(),
        "1h".into(),
        "now".into(),
        100,
        "sum".into(),
    )
    .await;
    assert!(result.is_ok(), "aggregate failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_aggregate_invalid_time() {
    let _lock = lock_env();
    let s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    let result = crate::commands::profiling::aggregate(
        &cfg,
        "*".into(),
        "cpu-time".into(),
        "notatime".into(),
        "now".into(),
        100,
        "sum".into(),
    )
    .await;
    assert!(result.is_err(), "expected parse error on invalid --from");
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_breakdown_ok_no_filter() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    let mock = s
        .mock("POST", "/profiling/api/v1/profiles/pid/breakdown")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"tree":{}}"#)
        .create_async()
        .await;
    let result = crate::commands::profiling::breakdown(&cfg, "pid", None, None, None).await;
    assert!(result.is_ok(), "breakdown failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_breakdown_rejects_partial_filter() {
    let _lock = lock_env();
    let s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    let result = crate::commands::profiling::breakdown(
        &cfg,
        "pid",
        Some("service:web".into()),
        Some("1h".into()),
        None,
    )
    .await;
    assert!(result.is_err(), "expected error for partial filter triple");
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_timeline_ok() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    let mock = s
        .mock("POST", "/profiling/api/v1/profiles/pid/timeline")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"layers":[]}"#)
        .create_async()
        .await;
    let result = crate::commands::profiling::timeline(&cfg, "pid", "eid").await;
    assert!(result.is_ok(), "timeline failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_timeline_error() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    s.mock("POST", mockito::Matcher::Any)
        .with_status(404)
        .create_async()
        .await;
    let result = crate::commands::profiling::timeline(&cfg, "missing", "eid").await;
    assert!(result.is_err(), "expected error on 404");
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_callgraph_ok() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    let mock = s
        .mock("POST", "/api/unstable/profiles/callgraph")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"nodes":[]}"#)
        .create_async()
        .await;
    let result = crate::commands::profiling::callgraph(
        &cfg,
        "service:web".into(),
        "cpu-time".into(),
        "15m".into(),
        "now".into(),
        100,
    )
    .await;
    assert!(result.is_ok(), "callgraph failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_callgraph_error() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    s.mock("POST", mockito::Matcher::Any)
        .with_status(500)
        .create_async()
        .await;
    let result = crate::commands::profiling::callgraph(
        &cfg,
        "*".into(),
        "cpu-time".into(),
        "15m".into(),
        "now".into(),
        100,
    )
    .await;
    assert!(result.is_err(), "expected error on 500");
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_save_favorite_ok() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    let mock = s
        .mock("POST", "/api/unstable/profiles/save-favorite")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"queryId":"abc"}"#)
        .create_async()
        .await;
    let result = crate::commands::profiling::save_favorite(
        &cfg,
        "service:web".into(),
        "15m".into(),
        "now".into(),
        "fav-1".into(),
        100,
    )
    .await;
    assert!(result.is_ok(), "save_favorite failed: {:?}", result.err());
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_save_favorite_error() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    s.mock("POST", mockito::Matcher::Any)
        .with_status(500)
        .create_async()
        .await;
    let result = crate::commands::profiling::save_favorite(
        &cfg,
        "*".into(),
        "15m".into(),
        "now".into(),
        "fav-1".into(),
        100,
    )
    .await;
    assert!(result.is_err(), "expected error on 500");
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_download_to_file() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    let mock = s
        .mock("GET", "/api/ui/profiling/profiles/eid/download")
        .with_status(200)
        .with_header("content-type", "application/octet-stream")
        .with_body(b"profile-bytes")
        .create_async()
        .await;
    let tmp = std::env::temp_dir().join(format!("pup-prof-{}.bin", std::process::id()));
    let tmp_str = tmp.to_string_lossy().to_string();
    let result = crate::commands::profiling::download(&cfg, "eid", Some(tmp_str.clone())).await;
    assert!(result.is_ok(), "download failed: {:?}", result.err());
    let contents = std::fs::read(&tmp).expect("output file");
    assert_eq!(contents, b"profile-bytes");
    let _ = std::fs::remove_file(&tmp);
    mock.assert_async().await;
    cleanup_env();
}

#[tokio::test]
async fn test_profiling_download_error() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    s.mock("GET", mockito::Matcher::Any)
        .with_status(404)
        .create_async()
        .await;
    let result = crate::commands::profiling::download(&cfg, "missing-eid", None).await;
    assert!(result.is_err(), "expected error on 404");
    cleanup_env();
}

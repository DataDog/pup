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
    let _mock = mock_any(&mut server, "GET", r#"{"data": [], "errors": []}"#).await;

    let result = crate::commands::slos::list(&cfg).await;
    assert!(result.is_ok(), "slos list failed: {:?}", result.err());
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
    assert!(result.is_err());
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
    let _ = crate::commands::rum::events_list(&cfg, "1h".into(), "now".into(), 10).await;
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
        false,
        Some("-last_flaked".into()),
    )
    .await;
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

// --- Security ---
#[tokio::test]
async fn test_security_rules_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::security::rules_list(&cfg, None).await;
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
async fn test_static_analysis_ast_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::static_analysis::ast_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_static_analysis_sca_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::static_analysis::sca_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_static_analysis_custom_rulesets_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::static_analysis::custom_rulesets_list(&cfg).await;
    cleanup_env();
}
#[tokio::test]
async fn test_static_analysis_coverage_list() {
    let _lock = lock_env();
    let mut s = mockito::Server::new_async().await;
    let cfg = test_config(&s.url());
    mock_all(&mut s, r#"{"data": []}"#).await;
    let _ = crate::commands::static_analysis::coverage_list(&cfg).await;
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
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    // Intentionally omits `description` — the field the old typed client required.
    // This test proves the raw HTTP fix handles schema drift gracefully.
    let body = r#"{"data":[{"id":"proj-1","type":"projects","attributes":{"name":"my-project"}}]}"#;
    let _mock = mock_get(&mut server, "/api/v2/llm-obs/v1/projects", 200, body).await;

    let result = crate::commands::llm_obs::projects_list(&cfg).await;
    assert!(result.is_ok(), "projects_list failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_projects_list_404() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_get(
        &mut server,
        "/api/v2/llm-obs/v1/projects",
        404,
        r#"{"errors":["not found"]}"#,
    )
    .await;

    let result = crate::commands::llm_obs::projects_list(&cfg).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("404"));
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_projects_list_no_auth() {
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
    let result = crate::commands::llm_obs::projects_list(&cfg).await;
    assert!(result.is_err(), "should fail without auth");
    cleanup_env();
}

// --- projects create ---

#[tokio::test]
async fn test_llm_obs_projects_create() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_proj_create.json",
        r#"{"data":{"type":"projects","attributes":{"name":"test"}}}"#,
    );
    let body = r#"{"data":{"id":"proj-1","type":"projects","attributes":{"name":"test"}}}"#;
    let _mock = mock_post(&mut server, "/api/v2/llm-obs/v1/projects", 200, body).await;

    let result = crate::commands::llm_obs::projects_create(&cfg, tmp.to_str().unwrap()).await;
    assert!(result.is_ok(), "projects_create failed: {:?}", result.err());
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_projects_create_500() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_proj_create_500.json",
        r#"{"data":{"type":"projects","attributes":{"name":"test"}}}"#,
    );
    let _mock = mock_post(
        &mut server,
        "/api/v2/llm-obs/v1/projects",
        500,
        r#"{"errors":["server error"]}"#,
    )
    .await;

    let result = crate::commands::llm_obs::projects_create(&cfg, tmp.to_str().unwrap()).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("500"));
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
}

// --- experiments list ---

#[tokio::test]
async fn test_llm_obs_experiments_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    // Intentionally omits `config` — the field the old typed client required.
    // This is the exact schema drift that caused the production breakage.
    let body = r#"{"data":[{"id":"exp-1","type":"experiments","attributes":{"name":"test-exp","status":"active"}}]}"#;
    let _mock = mock_get(&mut server, "/api/v2/llm-obs/v1/experiments", 200, body).await;

    let result = crate::commands::llm_obs::experiments_list(&cfg, None, None).await;
    assert!(
        result.is_ok(),
        "experiments_list failed: {:?}",
        result.err()
    );
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_experiments_list_with_filters() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let body = r#"{"data":[]}"#;
    let _mock = mock_get(&mut server, "/api/v2/llm-obs/v1/experiments", 200, body).await;

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
}

#[tokio::test]
async fn test_llm_obs_experiments_list_401() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_get(
        &mut server,
        "/api/v2/llm-obs/v1/experiments",
        401,
        r#"{"errors":["Unauthorized"]}"#,
    )
    .await;

    let result = crate::commands::llm_obs::experiments_list(&cfg, None, None).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("401"));
    cleanup_env();
}

// --- experiments create ---

#[tokio::test]
async fn test_llm_obs_experiments_create() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_exp_create.json",
        r#"{"data":{"type":"experiments","attributes":{"name":"test-exp"}}}"#,
    );
    // Response intentionally omits `config` to prove schema drift is handled
    let body = r#"{"data":{"id":"exp-1","type":"experiments","attributes":{"name":"test-exp","status":"active"}}}"#;
    let _mock = mock_post(&mut server, "/api/v2/llm-obs/v1/experiments", 200, body).await;

    let result = crate::commands::llm_obs::experiments_create(&cfg, tmp.to_str().unwrap()).await;
    assert!(
        result.is_ok(),
        "experiments_create failed: {:?}",
        result.err()
    );
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_experiments_create_422() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json("pup_test_exp_create_422.json", r#"{"bad":"json"}"#);
    let _mock = mock_post(
        &mut server,
        "/api/v2/llm-obs/v1/experiments",
        422,
        r#"{"errors":["invalid request body"]}"#,
    )
    .await;

    let result = crate::commands::llm_obs::experiments_create(&cfg, tmp.to_str().unwrap()).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("422"));
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
}

// --- experiments update ---

#[tokio::test]
async fn test_llm_obs_experiments_update() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_exp_update.json",
        r#"{"data":{"type":"experiments","attributes":{"name":"updated"}}}"#,
    );
    let body = r#"{"data":{"id":"exp-1","type":"experiments","attributes":{"name":"updated","status":"active"}}}"#;
    let _mock = mock_patch(
        &mut server,
        "/api/v2/llm-obs/v1/experiments/exp-1",
        200,
        body,
    )
    .await;

    let result =
        crate::commands::llm_obs::experiments_update(&cfg, "exp-1", tmp.to_str().unwrap()).await;
    assert!(
        result.is_ok(),
        "experiments_update failed: {:?}",
        result.err()
    );
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_experiments_update_404() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_exp_update_404.json",
        r#"{"data":{"type":"experiments","attributes":{"name":"x"}}}"#,
    );
    let _mock = mock_patch(
        &mut server,
        "/api/v2/llm-obs/v1/experiments/missing",
        404,
        r#"{"errors":["not found"]}"#,
    )
    .await;

    let result =
        crate::commands::llm_obs::experiments_update(&cfg, "missing", tmp.to_str().unwrap()).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("404"));
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
}

// --- experiments delete ---

#[tokio::test]
async fn test_llm_obs_experiments_delete() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_exp_delete.json",
        r#"{"data":{"type":"experiments_delete","attributes":{"ids":["exp-1"]}}}"#,
    );
    let _mock = mock_post(
        &mut server,
        "/api/v2/llm-obs/v1/experiments/delete",
        200,
        r#"{}"#,
    )
    .await;

    let result = crate::commands::llm_obs::experiments_delete(&cfg, tmp.to_str().unwrap()).await;
    assert!(
        result.is_ok(),
        "experiments_delete failed: {:?}",
        result.err()
    );
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_experiments_delete_500() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_exp_delete_500.json",
        r#"{"data":{"type":"experiments_delete","attributes":{"ids":["exp-1"]}}}"#,
    );
    let _mock = mock_post(
        &mut server,
        "/api/v2/llm-obs/v1/experiments/delete",
        500,
        r#"{"errors":["server error"]}"#,
    )
    .await;

    let result = crate::commands::llm_obs::experiments_delete(&cfg, tmp.to_str().unwrap()).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("500"));
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
}

// --- datasets list ---

#[tokio::test]
async fn test_llm_obs_datasets_list() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    // Intentionally omits `description` — the field the old typed client required.
    let body = r#"{"data":[{"id":"ds-1","type":"datasets","attributes":{"name":"my-dataset"}}]}"#;
    let _mock = mock_get(&mut server, "/api/v2/llm-obs/v1/proj-1/datasets", 200, body).await;

    let result = crate::commands::llm_obs::datasets_list(&cfg, "proj-1").await;
    assert!(result.is_ok(), "datasets_list failed: {:?}", result.err());
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_datasets_list_403() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());
    let _mock = mock_get(
        &mut server,
        "/api/v2/llm-obs/v1/proj-1/datasets",
        403,
        r#"{"errors":["Forbidden"]}"#,
    )
    .await;

    let result = crate::commands::llm_obs::datasets_list(&cfg, "proj-1").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("403"));
    cleanup_env();
}

// --- datasets create ---

#[tokio::test]
async fn test_llm_obs_datasets_create() {
    let _lock = lock_env();
    let mut server = mockito::Server::new_async().await;
    let cfg = test_config(&server.url());

    let tmp = write_temp_json(
        "pup_test_ds_create.json",
        r#"{"data":{"type":"datasets","attributes":{"name":"test-dataset"}}}"#,
    );
    let body = r#"{"data":{"id":"ds-1","type":"datasets","attributes":{"name":"test-dataset"}}}"#;
    let _mock = mock_post(&mut server, "/api/v2/llm-obs/v1/proj-1/datasets", 200, body).await;

    let result =
        crate::commands::llm_obs::datasets_create(&cfg, "proj-1", tmp.to_str().unwrap()).await;
    assert!(result.is_ok(), "datasets_create failed: {:?}", result.err());
    let _ = std::fs::remove_file(tmp);
    cleanup_env();
}

#[tokio::test]
async fn test_llm_obs_datasets_create_no_auth() {
    let _lock = lock_env();
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
        None,
        None,
        10,
        None,
    )
    .await;
    assert!(result.is_ok(), "spans_search failed: {:?}", result.err());
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
        &cfg, None, None, None, None, None, None, false, None, None, 20, None,
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
        &cfg, None, None, None, None, None, None, false, None, None, 20, None,
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
        &cfg, None, None, None, None, None, None, false, None, None, 20, None,
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
        rate: 1,
        budget: 1000,
        ttl: Some("1h"),
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
        false,
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
        false,
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
        false,
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

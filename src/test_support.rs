//! Shared test utilities for command module integration tests.
//!
//! These helpers use the PUP_MOCK_SERVER mechanism to redirect all DD API calls
//! to a local mockito server, testing command functions without a live API.
//!
//! Only compiled in test builds.

#![cfg(test)]

use crate::config::{Config, OutputFormat};

/// Async lock for tests that mutate process-wide env vars. Delegates to the
/// shared `ENV_LOCK` (tokio Mutex) so async and sync tests serialize on the
/// same primitive — sync tests use `ENV_LOCK.blocking_lock()` directly.
pub(crate) async fn lock_env() -> tokio::sync::MutexGuard<'static, ()> {
    crate::test_utils::ENV_LOCK.lock().await
}

pub(crate) fn test_config(mock_url: &str) -> Config {
    std::env::set_var("PUP_MOCK_SERVER", mock_url);
    std::env::set_var("DD_API_KEY", "test-api-key");
    std::env::set_var("DD_APP_KEY", "test-app-key");

    Config {
        api_key: Some("test-api-key".into()),
        app_key: Some("test-app-key".into()),
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

pub(crate) fn cleanup_env() {
    std::env::remove_var("PUP_MOCK_SERVER");
}

/// Helper: create a catch-all mock that responds 200 with JSON for any request
/// matching the given HTTP method. Used for DD client API tests where the
/// exact path may differ from our expectations.
pub(crate) async fn mock_any(
    server: &mut mockito::Server,
    method: &str,
    body: &str,
) -> mockito::Mock {
    server
        .mock(method, mockito::Matcher::Any)
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create_async()
        .await
}

/// Mock all HTTP methods with the same response body.
pub(crate) async fn mock_all(s: &mut mockito::Server, body: &str) {
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

/// Helper: create a mock for a specific POST path.
pub(crate) async fn mock_post(
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

/// Helper: write a temp JSON file and return its path.
pub(crate) fn write_temp_json(name: &str, content: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, content).unwrap();
    path
}

/// Self-cleaning temporary directory for tests that need real filesystem
/// scratch space. Disambiguates with `subsec_nanos()` to survive parallel
/// test threads. Drops via [`Drop`] so callers don't have to remember cleanup.
pub(crate) struct TempDir(std::path::PathBuf);

impl TempDir {
    pub(crate) fn new(label: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("pup_test_{label}_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    pub(crate) fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

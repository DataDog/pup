//! Raw HTTP client for WASM builds.
//!
//! When compiled for `wasm32`, the typed `datadog-api-client` crate is unavailable
//! (it depends on native C libraries like zstd-sys). This module provides a thin
//! abstraction over `reqwest` that constructs Datadog API URLs, injects auth
//! headers, and returns `serde_json::Value`.

use crate::config::Config;
use anyhow::{bail, Result};

/// Perform a GET request to a Datadog API endpoint.
pub async fn get(cfg: &Config, path: &str, query: &[(&str, String)]) -> Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let client = reqwest::Client::new();
    let mut req = client.get(&url);
    req = apply_auth(req, cfg)?;
    if !query.is_empty() {
        req = req.query(query);
    }
    send(req).await
}

/// Perform a POST request with a JSON body.
pub async fn post(cfg: &Config, path: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let client = reqwest::Client::new();
    let mut req = client.post(&url);
    req = apply_auth(req, cfg)?;
    req = req.json(body);
    send(req).await
}

/// Perform a PUT request with a JSON body.
pub async fn put(cfg: &Config, path: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let client = reqwest::Client::new();
    let mut req = client.put(&url);
    req = apply_auth(req, cfg)?;
    req = req.json(body);
    send(req).await
}

/// Perform a PATCH request with a JSON body.
pub async fn patch(
    cfg: &Config,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let client = reqwest::Client::new();
    let mut req = client.patch(&url);
    req = apply_auth(req, cfg)?;
    req = req.json(body);
    send(req).await
}

/// Perform a DELETE request.
pub async fn delete(cfg: &Config, path: &str) -> Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let client = reqwest::Client::new();
    let mut req = client.delete(&url);
    req = apply_auth(req, cfg)?;
    send(req).await
}

/// Perform a DELETE request with a JSON body.
#[allow(dead_code)]
pub async fn delete_with_body(
    cfg: &Config,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let client = reqwest::Client::new();
    let mut req = client.delete(&url);
    req = apply_auth(req, cfg)?;
    req = req.json(body);
    send(req).await
}

fn apply_auth(req: reqwest::RequestBuilder, cfg: &Config) -> Result<reqwest::RequestBuilder> {
    if let Some(token) = &cfg.access_token {
        Ok(req.header("Authorization", format!("Bearer {token}")))
    } else if let (Some(api_key), Some(app_key)) = (&cfg.api_key, &cfg.app_key) {
        Ok(req
            .header("DD-API-KEY", api_key.as_str())
            .header("DD-APPLICATION-KEY", app_key.as_str()))
    } else {
        bail!(
            "authentication required: set DD_ACCESS_TOKEN for bearer auth, \
             or set DD_API_KEY and DD_APP_KEY for API+APP key auth"
        )
    }
}

async fn send(req: reqwest::RequestBuilder) -> Result<serde_json::Value> {
    let resp = req
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("HTTP request failed: {e}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| anyhow::anyhow!("failed to read response body: {e}"))?;
    if !status.is_success() {
        bail!("API error (HTTP {status}): {body}");
    }
    if body.is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(&body).map_err(|e| anyhow::anyhow!("failed to parse JSON response: {e}"))
}

#[cfg(test)]
mod tests {

    use crate::config::{Config, OutputFormat};
    use crate::test_support::*;

    #[tokio::test]
    async fn test_api_get() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        std::env::set_var("PUP_MOCK_SERVER", server.url());

        let cfg = Config {
            api_key: Some("test-key".into()),
            app_key: Some("test-app".into()),
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        let mock = server
            .mock("GET", "/api/v1/test")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status": "ok"}"#)
            .create_async()
            .await;

        let result = super::get(&cfg, "/api/v1/test", &[]).await;
        assert!(result.is_ok(), "api get failed: {:?}", result.err());
        let val = result.unwrap();
        assert_eq!(val["status"], "ok");
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_api_get_with_query() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        std::env::set_var("PUP_MOCK_SERVER", server.url());

        let cfg = Config {
            api_key: Some("test-key".into()),
            app_key: Some("test-app".into()),
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
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
        let result = super::get(&cfg, "/api/v1/search", &query).await;
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
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        std::env::set_var("PUP_MOCK_SERVER", server.url());

        let cfg = Config {
            api_key: Some("test-key".into()),
            app_key: Some("test-app".into()),
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        let mock = server
            .mock("POST", "/api/v2/test")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"created": true}"#)
            .create_async()
            .await;

        let body = serde_json::json!({"name": "test"});
        let result = super::post(&cfg, "/api/v2/test", &body).await;
        assert!(result.is_ok(), "api post failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_api_put() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        std::env::set_var("PUP_MOCK_SERVER", server.url());

        let cfg = Config {
            api_key: Some("test-key".into()),
            app_key: Some("test-app".into()),
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        let mock = server
            .mock("PUT", "/api/v1/test/123")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"updated": true}"#)
            .create_async()
            .await;

        let body = serde_json::json!({"name": "updated"});
        let result = super::put(&cfg, "/api/v1/test/123", &body).await;
        assert!(result.is_ok(), "api put failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_api_patch() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        std::env::set_var("PUP_MOCK_SERVER", server.url());

        let cfg = Config {
            api_key: Some("test-key".into()),
            app_key: Some("test-app".into()),
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        let mock = server
            .mock("PATCH", "/api/v1/test/123")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"patched": true}"#)
            .create_async()
            .await;

        let body = serde_json::json!({"name": "patched"});
        let result = super::patch(&cfg, "/api/v1/test/123", &body).await;
        assert!(result.is_ok(), "api patch failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_api_delete() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        std::env::set_var("PUP_MOCK_SERVER", server.url());

        let cfg = Config {
            api_key: Some("test-key".into()),
            app_key: Some("test-app".into()),
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        let mock = server
            .mock("DELETE", "/api/v1/test/123")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"deleted": true}"#)
            .create_async()
            .await;

        let result = super::delete(&cfg, "/api/v1/test/123").await;
        assert!(result.is_ok(), "api delete failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_api_error_response() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        std::env::set_var("PUP_MOCK_SERVER", server.url());

        let cfg = Config {
            api_key: Some("test-key".into()),
            app_key: Some("test-app".into()),
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        let mock = server
            .mock("GET", "/api/v1/test/missing")
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors": ["not found"]}"#)
            .create_async()
            .await;

        let result = super::get(&cfg, "/api/v1/test/missing", &[]).await;
        assert!(result.is_err(), "should return error for 404");
        assert!(result.unwrap_err().to_string().contains("404"));
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_api_bearer_auth() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        std::env::set_var("PUP_MOCK_SERVER", server.url());

        let cfg = Config {
            api_key: None,
            app_key: None,
            access_token: Some("test-bearer-token".into()),
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        let mock = server
            .mock("GET", "/api/v1/test")
            .match_header("Authorization", "Bearer test-bearer-token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"auth": "bearer"}"#)
            .create_async()
            .await;

        let result = super::get(&cfg, "/api/v1/test", &[]).await;
        assert!(result.is_ok(), "bearer auth failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_api_no_auth() {
        let _lock = lock_env().await;

        let cfg = Config {
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
        };

        let result = super::get(&cfg, "/api/v1/test", &[]).await;
        assert!(result.is_err(), "should fail without auth");
        assert!(
            result.unwrap_err().to_string().contains("authentication"),
            "error should mention authentication"
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_api_empty_response() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        std::env::set_var("PUP_MOCK_SERVER", server.url());

        let cfg = Config {
            api_key: Some("test-key".into()),
            app_key: Some("test-app".into()),
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        let mock = server
            .mock("DELETE", "/api/v1/test/empty")
            .with_status(204)
            .with_body("")
            .create_async()
            .await;

        let result = super::delete(&cfg, "/api/v1/test/empty").await;
        assert!(result.is_ok(), "empty response failed: {:?}", result.err());
        let val = result.unwrap();
        assert_eq!(val, serde_json::json!({}));
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_api_server_error() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        std::env::set_var("PUP_MOCK_SERVER", server.url());

        let cfg = Config {
            api_key: Some("test-key".into()),
            app_key: Some("test-app".into()),
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        let mock = server
            .mock("GET", "/api/v1/test")
            .with_status(500)
            .with_body(r#"{"errors": ["internal server error"]}"#)
            .create_async()
            .await;

        let result = super::get(&cfg, "/api/v1/test", &[]).await;
        assert!(
            result.is_err(),
            "expected error but got ok: {:?}",
            result.ok()
        );
        assert!(result.unwrap_err().to_string().contains("500"));
        mock.assert_async().await;
        cleanup_env();
    }
}

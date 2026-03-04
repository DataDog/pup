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

/// Like `post`, but returns the parsed JSON body even on non-2xx responses.
pub async fn post_lenient(
    cfg: &Config,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let client = reqwest::Client::new();
    let mut req = client.post(&url);
    req = apply_auth(req, cfg)?;
    req = req.json(body);
    send_lenient(req).await
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

async fn send_lenient(req: reqwest::RequestBuilder) -> Result<serde_json::Value> {
    let resp = req
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("HTTP request failed: {e}"))?;
    let body = resp
        .text()
        .await
        .map_err(|e| anyhow::anyhow!("failed to read response body: {e}"))?;
    if body.is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(&body).map_err(|e| anyhow::anyhow!("failed to parse JSON response: {e}"))
}

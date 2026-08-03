//! Raw/generic Datadog HTTP client support: request routing for endpoints not
//! covered by the typed SDK (the `pup api` passthrough and several hand-written
//! commands), plus the OAuth-exclusion fallback table shared with the typed path.

use crate::config::Config;
use crate::useragent;

/// HTTP error with the status code preserved for programmatic matching.
#[derive(Debug)]
pub struct HttpError {
    pub status: u16,
    pub method: String,
    pub url: String,
    pub body: String,
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} failed (HTTP {}): {}",
            self.method, self.url, self.status, self.body
        )
    }
}

impl std::error::Error for HttpError {}

// ---------------------------------------------------------------------------
// Auth type detection
// ---------------------------------------------------------------------------

// Parse a reqwest response body as JSON without serde_json's default 128-level
// recursion cap. Some Datadog endpoints (e.g. /profiling/api/v1/aggregate)
// return deeply-nested flame-graph trees that exceed it. serde_stacker grows
// the thread stack on demand so disabling the limit can't blow it.
async fn parse_response_json(resp: reqwest::Response) -> anyhow::Result<serde_json::Value> {
    use serde::Deserialize;
    let bytes = resp.bytes().await?;
    // Some endpoints return a success status (e.g. 200) with an empty body, such
    // as GET /api/v2/on-call/pages/{id} which responds with content-length: 0.
    // Treat an empty or whitespace-only body as JSON null rather than failing
    // with "EOF while parsing value at line 1 column 0".
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(serde_json::Value::Null);
    }
    let mut de = serde_json::Deserializer::from_slice(&bytes);
    de.disable_recursion_limit();
    let de = serde_stacker::Deserializer::new(&mut de);
    Ok(serde_json::Value::deserialize(de)?)
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthType {
    None,
    OAuth,
    ApiKeys,
}

impl std::fmt::Display for AuthType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthType::None => write!(f, "None"),
            AuthType::OAuth => write!(f, "OAuth2 Bearer Token"),
            AuthType::ApiKeys => write!(f, "API Keys (DD_API_KEY + DD_APP_KEY)"),
        }
    }
}

#[allow(dead_code)]
pub fn get_auth_type(cfg: &Config) -> AuthType {
    if cfg.has_bearer_token() {
        AuthType::OAuth
    } else if cfg.has_api_keys() {
        AuthType::ApiKeys
    } else {
        AuthType::None
    }
}

// ---------------------------------------------------------------------------
// OAuth-excluded endpoint validation
// ---------------------------------------------------------------------------

struct EndpointRequirement {
    path: &'static str,
    method: &'static str,
}

/// Returns true if the endpoint doesn't support OAuth and requires API key fallback.
#[allow(dead_code)]
pub fn requires_api_key_fallback(method: &str, path: &str) -> bool {
    find_endpoint_requirement(method, path).is_some()
}

/// Returns true if the endpoint accepts an API key without an application key.
pub(crate) fn requires_api_key_only(method: &str, path: &str) -> bool {
    method == "POST" && path == "/api/v1/events"
}

fn find_endpoint_requirement(method: &str, path: &str) -> Option<&'static EndpointRequirement> {
    OAUTH_EXCLUDED_ENDPOINTS.iter().find(|req| {
        if req.method != method {
            return false;
        }
        // Trailing "/" means prefix match (for ID-parameterized paths)
        if req.path.ends_with('/') {
            path.starts_with(&req.path[..req.path.len() - 1])
        } else {
            req.path == path
        }
    })
}

// ---------------------------------------------------------------------------
// Static tables
// ---------------------------------------------------------------------------

/// Endpoints that don't support OAuth.
/// Trailing "/" means prefix match for ID-parameterized paths.
static OAUTH_EXCLUDED_ENDPOINTS: &[EndpointRequirement] = &[
    // API/App Keys (8)
    EndpointRequirement {
        path: "/api/v2/api_keys",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/api_keys/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/api_keys",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/api_keys/",
        method: "DELETE",
    },
    EndpointRequirement {
        path: "/api/v2/application_keys",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/application_keys/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/application_keys/",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/application_keys/",
        method: "PATCH",
    },
    // DDSQL editor tools (3)
    EndpointRequirement {
        path: "/api/unstable/ddsql-editor/tools/ddsql-docs",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/unstable/ddsql-editor/tools/table-names",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/unstable/ddsql-editor/tools/table-data",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/application_keys/",
        method: "DELETE",
    },
    // Fleet Automation (15)
    EndpointRequirement {
        path: "/api/v2/fleet/agents",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/agents/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/agents/versions",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/deployments",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/deployments/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/deployments/configure",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/deployments/upgrade",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/deployments/",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/deployments/",
        method: "DELETE",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/schedules",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/schedules/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/schedules",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/schedules/",
        method: "PATCH",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/schedules/",
        method: "DELETE",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/schedules/",
        method: "POST",
    },
    // Observability Pipelines (6) — API key only, no OAuth support
    EndpointRequirement {
        path: "/api/v2/obs-pipelines/pipelines",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/obs-pipelines/pipelines",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/obs-pipelines/pipelines/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/obs-pipelines/pipelines/",
        method: "PUT",
    },
    EndpointRequirement {
        path: "/api/v2/obs-pipelines/pipelines/",
        method: "DELETE",
    },
    EndpointRequirement {
        path: "/api/v2/obs-pipelines/pipelines/validate",
        method: "POST",
    },
    // Cost / Billing (11) — API key only, no OAuth support
    EndpointRequirement {
        path: "/api/v2/usage/projected_cost",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/usage/cost_by_org",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/cost_by_tag/monthly_cost_attribution",
        method: "GET",
    },
    // Cloud Cost Management config (12)
    EndpointRequirement {
        path: "/api/v2/cost/aws_cur_config",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/cost/aws_cur_config",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/cost/aws_cur_config/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/cost/aws_cur_config/",
        method: "DELETE",
    },
    EndpointRequirement {
        path: "/api/v2/cost/azure_uc_config",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/cost/azure_uc_config",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/cost/azure_uc_config/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/cost/azure_uc_config/",
        method: "DELETE",
    },
    EndpointRequirement {
        path: "/api/v2/cost/gcp_uc_config",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/cost/gcp_uc_config",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/cost/gcp_uc_config/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/cost/gcp_uc_config/",
        method: "DELETE",
    },
    EndpointRequirement {
        path: "/api/v2/cost/oci_config",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/cost/anomalies",
        method: "GET",
    },
    // Profiling (4)
    // No OAuth scope is declared for Continuous Profiler endpoints; force API-key auth.
    EndpointRequirement {
        path: "/profiling/api/v1/",
        method: "POST",
    },
    EndpointRequirement {
        path: "/profiling/api/v1/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/unstable/profiles/",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/ui/profiling/",
        method: "GET",
    },
    // Events intake (1)
    // Posting an event uses the V1 intake endpoint, which authenticates with the
    // API key and does not accept OAuth2 bearer tokens. Listing/getting events
    // (GET) is fine over OAuth, so only POST is excluded.
    EndpointRequirement {
        path: "/api/v1/events",
        method: "POST",
    },
];

// ---------------------------------------------------------------------------
// Raw HTTP helpers
// ---------------------------------------------------------------------------

/// Raw HTTP response returned by [`raw_request`].
#[derive(Debug)]
pub struct HttpResponse {
    /// The `Content-Type` header value from the response, or an empty string if absent.
    pub content_type: String,
    /// The raw response body bytes.
    pub bytes: Vec<u8>,
}

/// Makes an authenticated request with any HTTP method via reqwest.
///
/// - `query` — key/value pairs appended as URL query parameters (reqwest handles percent-encoding).
///   Pass `&[]` when no query parameters are needed.
/// - `body` — raw bytes to send; `content_type` sets the `Content-Type` header when present.
/// - `accept` — value for the `Accept` header (e.g. `"application/json"`, `"*/*"`).
/// - `extra_headers` — additional headers applied after auth and before the body.
/// - Returns an [`HttpResponse`] with the raw bytes and response `Content-Type`.
///   Callers are responsible for decoding the bytes.
#[allow(clippy::too_many_arguments)]
pub async fn raw_request(
    cfg: &Config,
    method: &str,
    path: &str,
    query: &[(&str, &str)],
    body: Option<Vec<u8>>,
    content_type: Option<&str>,
    accept: &str,
    extra_headers: &[(&str, &str)],
) -> anyhow::Result<HttpResponse> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let client = reqwest::Client::new();
    let method_name = method.to_uppercase();
    let method = reqwest::Method::from_bytes(method_name.as_bytes())
        .map_err(|_| anyhow::anyhow!("unsupported HTTP method: {method_name}"))?;
    let mut req = client.request(method, &url);
    if !query.is_empty() {
        req = req.query(query);
    }

    req = apply_auth(req, cfg, &method_name, path)?;

    req = req
        .header("Accept", accept)
        .header("User-Agent", useragent::get());

    for (k, v) in extra_headers {
        req = req.header(*k, *v);
    }

    if let Some(b) = body {
        if let Some(ct) = content_type {
            req = req.header("Content-Type", ct);
        }
        req = req.body(b);
    }

    let resp = req.send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(HttpError {
            status: status.as_u16(),
            method: method_name,
            url,
            body: text,
        }
        .into());
    }

    let resp_ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if resp.status() == reqwest::StatusCode::NO_CONTENT {
        return Ok(HttpResponse {
            content_type: resp_ct,
            bytes: vec![],
        });
    }

    let bytes = resp.bytes().await?.to_vec();
    Ok(HttpResponse {
        content_type: resp_ct,
        bytes,
    })
}

/// Makes an authenticated GET request directly via reqwest.
/// Used for endpoints not covered by the typed DD API client.
/// Pass an empty slice for `query` when no query parameters are needed.
pub async fn raw_get(
    cfg: &Config,
    path: &str,
    query: &[(&str, &str)],
) -> anyhow::Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let client = reqwest::Client::new();
    let mut req = client.get(&url);

    req = apply_auth(req, cfg, "GET", path)?;

    if !query.is_empty() {
        req = req.query(query);
    }

    let resp = req
        .header("Accept", "application/json")
        .header("User-Agent", useragent::get())
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(HttpError {
            status: status.as_u16(),
            method: "GET".into(),
            url,
            body,
        }
        .into());
    }
    parse_response_json(resp).await
}

/// Makes an authenticated PATCH request directly via reqwest.
/// Used for endpoints not covered by the typed DD API client.
#[allow(dead_code)]
pub async fn raw_patch(
    cfg: &Config,
    path: &str,
    body: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let client = reqwest::Client::new();
    let mut req = client.patch(&url);

    req = apply_auth(req, cfg, "PATCH", path)?;

    let resp = req
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("User-Agent", useragent::get())
        .json(&body)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(HttpError {
            status: status.as_u16(),
            method: "PATCH".into(),
            url,
            body,
        }
        .into());
    }
    parse_response_json(resp).await
}

/// Makes an authenticated POST request directly via reqwest.
/// Used for endpoints not covered by the typed DD API client.
pub async fn raw_post(
    cfg: &Config,
    path: &str,
    body: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    raw_post_impl(cfg, path, &url, body, useragent::get()).await
}

/// Like `raw_post`, but with a custom User-Agent string for audit log differentiation.
pub async fn raw_post_with_ua(
    cfg: &Config,
    path: &str,
    body: serde_json::Value,
    ua: String,
) -> anyhow::Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    raw_post_impl(cfg, path, &url, body, ua).await
}

async fn raw_post_impl(
    cfg: &Config,
    path: &str,
    url: &str,
    body: serde_json::Value,
    ua: String,
) -> anyhow::Result<serde_json::Value> {
    let client = reqwest::Client::new();
    let mut req = client.post(url);

    req = apply_auth(req, cfg, "POST", path)?;

    let resp = req
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("User-Agent", ua)
        .json(&body)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(HttpError {
            status: status.as_u16(),
            method: "POST".into(),
            url: url.to_string(),
            body,
        }
        .into());
    }
    parse_response_json(resp).await
}

/// Apply Datadog authentication headers to a request builder.
///
/// Chooses between OAuth bearer and API-key auth based on `cfg` and the
/// per-endpoint requirements in [`requires_api_key_fallback`]. OAuth-excluded
/// endpoints require both API and application keys except for the V1 event
/// intake endpoint, which requires only the API key. Exposed so the generic
/// `pup api` passthrough reuses the same auth routing as the typed clients.
pub fn apply_auth(
    mut req: reqwest::RequestBuilder,
    cfg: &Config,
    method: &str,
    path: &str,
) -> anyhow::Result<reqwest::RequestBuilder> {
    // Events post is also in the broader OAuth-excluded table, so handle its
    // API-key-only requirement before the fallback branch that adds both keys.
    if requires_api_key_only(method, path) {
        if let Some(api_key) = &cfg.api_key {
            return Ok(req.header("DD-API-KEY", api_key.as_str()));
        }

        anyhow::bail!(
            "{method} {path} requires DD_API_KEY; OAuth2 bearer tokens are not supported"
        );
    }

    if requires_api_key_fallback(method, path) {
        if let (Some(api_key), Some(app_key)) = (&cfg.api_key, &cfg.app_key) {
            req = req
                .header("DD-API-KEY", api_key.as_str())
                .header("DD-APPLICATION-KEY", app_key.as_str());
            return Ok(req);
        }

        anyhow::bail!(
            "{method} {path} requires DD_API_KEY and DD_APP_KEY; OAuth2 bearer tokens are not supported"
        );
    }

    if let Some(token) = &cfg.access_token {
        req = req.header("Authorization", format!("Bearer {token}"));
        return Ok(req);
    }

    if let (Some(api_key), Some(app_key)) = (&cfg.api_key, &cfg.app_key) {
        req = req
            .header("DD-API-KEY", api_key.as_str())
            .header("DD-APPLICATION-KEY", app_key.as_str());
        return Ok(req);
    }

    anyhow::bail!("no authentication configured")
}

/// POST a JSON:API document. Wraps `attributes` in `{data:{type,attributes}}`
/// and sends with `Content-Type: application/vnd.api+json`. Use for routes
/// whose decoder is configured for JSON:API.
pub async fn raw_post_jsonapi(
    cfg: &Config,
    path: &str,
    resource_type: &str,
    attributes: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let envelope = serde_json::json!({
        "data": { "type": resource_type, "attributes": attributes },
    });
    let client = reqwest::Client::new();
    let mut req = client.post(&url);
    req = apply_auth(req, cfg, "POST", path)?;
    let resp = req
        .header("Content-Type", "application/vnd.api+json")
        .header("Accept", "application/vnd.api+json")
        .header("User-Agent", useragent::get())
        .json(&envelope)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("POST {url} failed (HTTP {status}): {body}");
    }
    parse_response_json(resp).await
}

pub async fn raw_put(
    cfg: &Config,
    path: &str,
    body: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let client = reqwest::Client::new();
    let req = client.put(&url);
    let req = apply_auth(req, cfg, "PUT", path)?;
    let resp = req
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("User-Agent", useragent::get())
        .json(&body)
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NO_CONTENT {
        return Ok(serde_json::Value::Null);
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("PUT {url} failed (HTTP {status}): {body}");
    }
    parse_response_json(resp).await
}

/// Like `raw_post`, but returns the parsed JSON body even on non-2xx responses.
/// Callers are responsible for inspecting the body for errors.
pub async fn raw_post_lenient(
    cfg: &Config,
    path: &str,
    body: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let client = reqwest::Client::new();
    let mut req = client.post(&url);

    if let Some(token) = &cfg.access_token {
        req = req.header("Authorization", format!("Bearer {token}"));
    } else if let (Some(api_key), Some(app_key)) = (&cfg.api_key, &cfg.app_key) {
        req = req
            .header("DD-API-KEY", api_key.as_str())
            .header("DD-APPLICATION-KEY", app_key.as_str());
    } else {
        anyhow::bail!("no authentication configured");
    }

    let resp = req
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("User-Agent", useragent::get())
        .json(&body)
        .send()
        .await?;
    parse_response_json(resp).await
}

/// Makes an authenticated DELETE request directly via reqwest.
/// Used for endpoints not covered by the typed DD API client.
pub async fn raw_delete(cfg: &Config, path: &str) -> anyhow::Result<()> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let client = reqwest::Client::new();
    let mut req = client.delete(&url);

    req = apply_auth(req, cfg, "DELETE", path)?;

    let resp = req
        .header("Accept", "application/json")
        .header("User-Agent", useragent::get())
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(HttpError {
            status: status.as_u16(),
            method: "DELETE".into(),
            url,
            body,
        }
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::test_support::*;

    fn test_cfg() -> Config {
        Config {
            api_key: Some("test".into()),
            app_key: Some("test".into()),
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: crate::config::OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        }
    }

    #[test]
    fn test_auth_type_api_keys() {
        let cfg = test_cfg();
        assert_eq!(get_auth_type(&cfg), AuthType::ApiKeys);
    }

    #[test]
    fn test_auth_type_bearer() {
        let mut cfg = test_cfg();
        cfg.access_token = Some("token".into());
        assert_eq!(get_auth_type(&cfg), AuthType::OAuth);
    }

    #[test]
    fn test_auth_type_none() {
        let mut cfg = test_cfg();
        cfg.api_key = None;
        cfg.app_key = None;
        assert_eq!(get_auth_type(&cfg), AuthType::None);
    }

    #[test]
    fn test_auth_type_display() {
        assert_eq!(AuthType::OAuth.to_string(), "OAuth2 Bearer Token");
        assert_eq!(
            AuthType::ApiKeys.to_string(),
            "API Keys (DD_API_KEY + DD_APP_KEY)"
        );
        assert_eq!(AuthType::None.to_string(), "None");
    }

    #[test]
    fn test_no_fallback_for_logs() {
        assert!(!requires_api_key_fallback("POST", "/api/v2/logs/events"));
        assert!(!requires_api_key_fallback(
            "POST",
            "/api/v2/logs/events/search"
        ));
    }

    #[test]
    fn test_no_fallback_for_rum() {
        assert!(!requires_api_key_fallback(
            "GET",
            "/api/v2/rum/applications"
        ));
        assert!(!requires_api_key_fallback(
            "GET",
            "/api/v2/rum/applications/abc-123"
        ));
    }

    #[test]
    fn test_no_fallback_for_events_search() {
        assert!(!requires_api_key_fallback("POST", "/api/v2/events/search"));
    }

    #[test]
    fn test_fallback_for_events_post() {
        // Posting an event (V1 intake) requires only the API key; reading events
        // does not require API-key fallback.
        assert!(requires_api_key_fallback("POST", "/api/v1/events"));
        assert!(requires_api_key_only("POST", "/api/v1/events"));
        assert!(!requires_api_key_fallback("GET", "/api/v1/events"));
        assert!(!requires_api_key_only("GET", "/api/v1/events"));
        assert!(!requires_api_key_only("POST", "/api/v1/events/12345"));
    }

    #[test]
    fn test_no_fallback_for_logs_saved_views() {
        assert!(!requires_api_key_fallback("GET", "/api/v1/logs/views"));
        assert!(!requires_api_key_fallback("GET", "/api/v1/logs/views/123"));
        assert!(!requires_api_key_fallback("POST", "/api/v1/logs/views"));
        assert!(!requires_api_key_fallback(
            "DELETE",
            "/api/v1/logs/views/123"
        ));
    }

    #[test]
    fn test_no_fallback_for_standard_endpoints() {
        assert!(!requires_api_key_fallback("GET", "/api/v1/monitor"));
        assert!(!requires_api_key_fallback("GET", "/api/v1/dashboard"));
        assert!(!requires_api_key_fallback("GET", "/api/v2/incidents"));
    }

    #[test]
    fn test_prefix_matching_with_id() {
        // Trailing "/" in the pattern should match paths with IDs
        assert!(requires_api_key_fallback(
            "DELETE",
            "/api/v2/api_keys/key-123"
        ));
        assert!(requires_api_key_fallback(
            "GET",
            "/api/v2/fleet/agents/agent-123"
        ));
    }

    #[test]
    fn test_method_must_match() {
        // RUM events/search is POST-excluded, but GET should not match
        assert!(!requires_api_key_fallback(
            "GET",
            "/api/v2/rum/events/search"
        ));
    }

    #[test]
    fn test_oauth_excluded_count() {
        assert_eq!(OAUTH_EXCLUDED_ENDPOINTS.len(), 55);
    }

    #[test]
    fn test_no_fallback_for_notebooks() {
        assert!(!requires_api_key_fallback("GET", "/api/v1/notebooks"));
        assert!(!requires_api_key_fallback("GET", "/api/v1/notebooks/12345"));
        assert!(!requires_api_key_fallback("POST", "/api/v1/notebooks"));
    }

    #[test]
    fn test_requires_api_key_fallback_fleet() {
        assert!(requires_api_key_fallback("GET", "/api/v2/fleet/agents"));
        assert!(requires_api_key_fallback(
            "GET",
            "/api/v2/fleet/agents/agent-123"
        ));
    }

    #[test]
    fn test_requires_api_key_fallback_api_keys() {
        assert!(requires_api_key_fallback("GET", "/api/v2/api_keys"));
        assert!(requires_api_key_fallback("POST", "/api/v2/api_keys"));
        assert!(requires_api_key_fallback(
            "DELETE",
            "/api/v2/api_keys/key-123"
        ));
    }

    #[test]
    fn test_requires_api_key_fallback_ddsql_editor_tools() {
        assert!(requires_api_key_fallback(
            "GET",
            "/api/unstable/ddsql-editor/tools/ddsql-docs"
        ));
        assert!(requires_api_key_fallback(
            "GET",
            "/api/unstable/ddsql-editor/tools/table-names"
        ));
        assert!(requires_api_key_fallback(
            "POST",
            "/api/unstable/ddsql-editor/tools/table-data"
        ));
    }

    #[test]
    fn test_no_fallback_for_error_tracking() {
        assert!(!requires_api_key_fallback(
            "POST",
            "/api/v2/error_tracking/issues/search"
        ));
    }

    // Verify raw_request reaches the auth check (and fails there) for both the
    // empty-query and non-empty-query paths. This ensures the `if !query.is_empty()`
    // branch compiles and runs without panic.
    #[test]
    fn test_raw_request_no_auth_empty_query() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut cfg = test_cfg();
        cfg.api_key = None;
        cfg.app_key = None;
        let err = rt
            .block_on(raw_request(
                &cfg,
                "GET",
                "/api/v2/monitors",
                &[],
                None,
                None,
                "application/json",
                &[],
            ))
            .unwrap_err();
        assert!(
            err.to_string().contains("no authentication configured"),
            "expected auth error, got: {err}"
        );
    }

    #[test]
    fn test_raw_request_no_auth_with_query() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut cfg = test_cfg();
        cfg.api_key = None;
        cfg.app_key = None;
        let err = rt
            .block_on(raw_request(
                &cfg,
                "GET",
                "/api/v2/monitors",
                &[("page", "1"), ("page_size", "10")],
                None,
                None,
                "application/json",
                &[],
            ))
            .unwrap_err();
        assert!(
            err.to_string().contains("no authentication configured"),
            "expected auth error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_raw_events_post_sends_only_api_key() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("POST", "/api/v1/events")
            .match_header("DD-API-KEY", "test-api-key")
            .match_header("DD-APPLICATION-KEY", mockito::Matcher::Missing)
            .match_header("Authorization", mockito::Matcher::Missing)
            .with_status(202)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":"ok"}"#)
            .expect(1)
            .create_async()
            .await;

        let result = raw_request(
            &cfg,
            "POST",
            "/api/v1/events",
            &[],
            Some(br#"{"title":"test","text":"test"}"#.to_vec()),
            Some("application/json"),
            "application/json",
            &[],
        )
        .await;

        assert!(result.is_ok(), "raw event post failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_raw_delete_uses_oauth_bearer() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        cfg.access_token = Some("token".into());
        let mock = server
            .mock("DELETE", "/api/v1/logs/views/123")
            .match_header("Authorization", "Bearer token")
            .match_header("DD-API-KEY", mockito::Matcher::Missing)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"deleted_logs_saved_view_id":123}"#)
            .expect(1)
            .create_async()
            .await;

        let result = raw_delete(&cfg, "/api/v1/logs/views/123").await;

        assert!(result.is_ok(), "raw delete failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[test]
    fn test_other_oauth_excluded_endpoints_still_require_both_keys() {
        let mut cfg = test_cfg();
        cfg.app_key = None;
        let req = reqwest::Client::new().post("https://api.datadoghq.com/api/v2/api_keys");

        let err = match apply_auth(req, &cfg, "POST", "/api/v2/api_keys") {
            Ok(_) => panic!("API key management should require both keys"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("DD_API_KEY and DD_APP_KEY"));
    }

    #[test]
    fn test_requires_api_key_fallback_profiling() {
        // /profiling/api/v1/*
        assert!(requires_api_key_fallback(
            "POST",
            "/profiling/api/v1/aggregate"
        ));
        assert!(requires_api_key_fallback(
            "GET",
            "/profiling/api/v1/profiles/abc/info"
        ));
        assert!(requires_api_key_fallback(
            "GET",
            "/profiling/api/v1/profiles/abc/analysis"
        ));
        assert!(requires_api_key_fallback(
            "POST",
            "/profiling/api/v1/profiles/abc/breakdown"
        ));
        assert!(requires_api_key_fallback(
            "POST",
            "/profiling/api/v1/profiles/abc/timeline"
        ));
        // /api/unstable/profiles/*
        assert!(requires_api_key_fallback(
            "POST",
            "/api/unstable/profiles/list"
        ));
        assert!(requires_api_key_fallback(
            "POST",
            "/api/unstable/profiles/analytics"
        ));
        assert!(requires_api_key_fallback(
            "POST",
            "/api/unstable/profiles/insights"
        ));
        assert!(requires_api_key_fallback(
            "POST",
            "/api/unstable/profiles/callgraph"
        ));
        assert!(requires_api_key_fallback(
            "POST",
            "/api/unstable/profiles/interactive-analytics/field"
        ));
        assert!(requires_api_key_fallback(
            "POST",
            "/api/unstable/profiles/save-favorite"
        ));
        // /api/ui/profiling/*
        assert!(requires_api_key_fallback(
            "GET",
            "/api/ui/profiling/profiles/abc/download"
        ));
    }

    /// Verifies that raw_request attaches query parameters and returns Ok when the
    /// server responds 200. Exercises the `!query.is_empty()` branch added to the function.
    #[tokio::test]
    async fn test_raw_request_with_query_params_ok() {
        let _lock = lock_env().await;
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
        let resp = super::raw_request(
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

    /// Regression test: a 200 response with an empty body must parse as JSON null
    /// instead of failing with "EOF while parsing value at line 1 column 0".
    #[tokio::test]
    async fn test_raw_get_empty_body_returns_null() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", "/api/v2/on-call/pages/12345")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("")
            .create_async()
            .await;
        let resp = super::raw_get(&cfg, "/api/v2/on-call/pages/12345", &[]).await;
        assert!(
            resp.is_ok(),
            "raw_get with empty body failed: {:?}",
            resp.err()
        );
        assert_eq!(resp.unwrap(), serde_json::Value::Null);
        cleanup_env();
    }

    /// A whitespace-only body is also unparseable JSON and must be treated as null.
    #[tokio::test]
    async fn test_raw_get_whitespace_body_returns_null() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", "/api/v2/on-call/pages/12345")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("  \n\t ")
            .create_async()
            .await;
        let resp = super::raw_get(&cfg, "/api/v2/on-call/pages/12345", &[]).await;
        assert!(
            resp.is_ok(),
            "raw_get with whitespace body failed: {:?}",
            resp.err()
        );
        assert_eq!(resp.unwrap(), serde_json::Value::Null);
        cleanup_env();
    }

    /// A non-empty JSON body must still parse normally (the empty-body guard must
    /// not shadow the regular parse path).
    #[tokio::test]
    async fn test_raw_get_nonempty_body_parses() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", "/api/v2/on-call/pages/12345")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"id": "12345"}}"#)
            .create_async()
            .await;
        let resp = super::raw_get(&cfg, "/api/v2/on-call/pages/12345", &[])
            .await
            .expect("raw_get with JSON body should succeed");
        assert_eq!(resp["data"]["id"], "12345");
        cleanup_env();
    }
}

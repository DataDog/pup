use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};

#[cfg(not(target_arch = "wasm32"))]
use async_trait::async_trait;
#[cfg(not(target_arch = "wasm32"))]
use reqwest_middleware::{Middleware, Next};
#[cfg(not(target_arch = "wasm32"))]
use task_local_extensions::Extensions;

use crate::config::Config;

// ---------------------------------------------------------------------------
// Bearer token middleware (native only — requires task-local-extensions)
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
struct BearerAuthMiddleware {
    token: String,
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl Middleware for BearerAuthMiddleware {
    async fn handle(
        &self,
        mut req: reqwest::Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<reqwest::Response> {
        req.headers_mut().insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", self.token).parse().unwrap(),
        );
        next.run(req, extensions).await
    }
}

// ---------------------------------------------------------------------------
// DD Configuration builder
// ---------------------------------------------------------------------------

/// Creates a DD API Configuration with all unstable ops enabled.
///
/// Explicitly injects `cfg` credentials so API key auth works on targets where
/// `std::env::var` is unavailable (e.g. wasm32-unknown-unknown).
///
/// If PUP_MOCK_SERVER is set, redirects all API calls to the mock server.
pub fn make_dd_config(cfg: &Config) -> datadog_api_client::datadog::Configuration {
    let mut dd_cfg = datadog_api_client::datadog::Configuration::new();

    // Enable all unstable operations.
    for op in UNSTABLE_OPS {
        dd_cfg.set_unstable_operation_enabled(op, true);
    }

    // Inject auth from cfg — supplements env vars and is required on WASM
    // targets where std::env::var always returns Err.
    if let Some(api_key) = &cfg.api_key {
        dd_cfg.set_auth_key(
            "apiKeyAuth",
            datadog_api_client::datadog::APIKey {
                key: api_key.clone(),
                prefix: "".to_owned(),
            },
        );
    }
    if let Some(app_key) = &cfg.app_key {
        dd_cfg.set_auth_key(
            "appKeyAuth",
            datadog_api_client::datadog::APIKey {
                key: app_key.clone(),
                prefix: "".to_owned(),
            },
        );
    }

    // If PUP_MOCK_SERVER is set, redirect all requests to the mock server.
    // The DD client uses server templates like "{protocol}://{name}" at index 1.
    if let Ok(mock_url) = std::env::var("PUP_MOCK_SERVER") {
        dd_cfg.server_index = 1;
        let url = mock_url
            .trim_start_matches("http://")
            .trim_start_matches("https://");
        let protocol = if mock_url.starts_with("https") {
            "https"
        } else {
            "http"
        };
        dd_cfg
            .server_variables
            .insert("protocol".into(), protocol.into());
        dd_cfg.server_variables.insert("name".into(), url.into());
    } else {
        // Server index 0 only accepts production sites (datadoghq.com, us3, us5,
        // ap1, ap2, eu, gov). Server index 2 uses the same URL template but with
        // no enum restriction, so it works for any site including staging
        // (datad0g.com). Use index 2 for non-standard sites.
        static STANDARD_SITES: &[&str] = &[
            "datadoghq.com",
            "us3.datadoghq.com",
            "us5.datadoghq.com",
            "ap1.datadoghq.com",
            "ap2.datadoghq.com",
            "datadoghq.eu",
            "ddog-gov.com",
        ];
        let site = std::env::var("DD_SITE").unwrap_or_default();
        if !site.is_empty() && !STANDARD_SITES.contains(&site.as_str()) {
            dd_cfg.server_index = 2;
        }
    }

    dd_cfg
}

/// Creates a reqwest middleware client that injects a bearer token on every
/// request. Returns `None` if no bearer token is configured, or on WASM
/// targets where the token-injection middleware is unavailable.
pub fn make_bearer_client(cfg: &Config) -> Option<ClientWithMiddleware> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let token = cfg.access_token.as_ref()?.clone();
        let reqwest_client = reqwest::Client::builder()
            .build()
            .expect("failed to build reqwest client");
        let client = ClientBuilder::new(reqwest_client)
            .with(BearerAuthMiddleware { token })
            .build();
        return Some(client);
    }
    #[allow(unreachable_code)]
    None
}

// ---------------------------------------------------------------------------
// Unstable operations table — used by make_dd_config
// ---------------------------------------------------------------------------

/// All 85 unstable operations (snake_case for the Rust DD client).
static UNSTABLE_OPS: &[&str] = &[
    // Incidents (16)
    "v2.list_incidents",
    "v2.search_incidents",
    "v2.get_incident",
    "v2.create_incident",
    "v2.update_incident",
    "v2.delete_incident",
    "v2.create_global_incident_handle",
    "v2.delete_global_incident_handle",
    "v2.get_global_incident_settings",
    "v2.list_global_incident_handles",
    "v2.update_global_incident_handle",
    "v2.update_global_incident_settings",
    "v2.create_incident_postmortem_template",
    "v2.delete_incident_postmortem_template",
    "v2.get_incident_postmortem_template",
    "v2.list_incident_postmortem_templates",
    "v2.update_incident_postmortem_template",
    // Fleet Automation (14)
    "v2.list_fleet_agents",
    "v2.get_fleet_agent_info",
    "v2.list_fleet_agent_versions",
    "v2.list_fleet_deployments",
    "v2.get_fleet_deployment",
    "v2.create_fleet_deployment_configure",
    "v2.create_fleet_deployment_upgrade",
    "v2.cancel_fleet_deployment",
    "v2.list_fleet_schedules",
    "v2.get_fleet_schedule",
    "v2.create_fleet_schedule",
    "v2.update_fleet_schedule",
    "v2.delete_fleet_schedule",
    "v2.trigger_fleet_schedule",
    // ServiceNow (9)
    "v2.create_service_now_template",
    "v2.delete_service_now_template",
    "v2.get_service_now_template",
    "v2.list_service_now_assignment_groups",
    "v2.list_service_now_business_services",
    "v2.list_service_now_instances",
    "v2.list_service_now_templates",
    "v2.list_service_now_users",
    "v2.update_service_now_template",
    // Jira (7)
    "v2.create_jira_issue_template",
    "v2.delete_jira_account",
    "v2.delete_jira_issue_template",
    "v2.get_jira_issue_template",
    "v2.list_jira_accounts",
    "v2.list_jira_issue_templates",
    "v2.update_jira_issue_template",
    // Cases (5)
    "v2.create_case_jira_issue",
    "v2.link_jira_issue_to_case",
    "v2.unlink_jira_issue",
    "v2.create_case_service_now_ticket",
    "v2.move_case_to_project",
    // Content Packs (3)
    "v2.activate_content_pack",
    "v2.deactivate_content_pack",
    "v2.get_content_packs_states",
    // Code Coverage (2)
    "v2.get_code_coverage_branch_summary",
    "v2.get_code_coverage_commit_summary",
    // OCI Integration (2)
    "v2.create_tenancy_config",
    "v2.get_tenancy_configs",
    // HAMR (2)
    "v2.create_hamr_org_connection",
    "v2.get_hamr_org_connection",
    // Entity Risk Scores (1)
    "v2.list_entity_risk_scores",
    // Security Findings (1)
    "v2.list_findings",
    // SLO Status (1)
    "v2.get_slo_status",
    // Flaky Tests (2)
    "v2.search_flaky_tests",
    "v2.update_flaky_tests",
    // Incidents Import (1)
    "v2.import_incident",
    // Change Management (6)
    "v2.create_change_request",
    "v2.create_change_request_branch",
    "v2.delete_change_request_decision",
    "v2.get_change_request",
    "v2.update_change_request",
    "v2.update_change_request_decision",
    // Cloud Authentication (4)
    "v2.create_aws_cloud_auth_persona_mapping",
    "v2.delete_aws_cloud_auth_persona_mapping",
    "v2.get_aws_cloud_auth_persona_mapping",
    "v2.list_aws_cloud_auth_persona_mappings",
    // LLM Observability (8)
    "v2.create_llm_obs_project",
    "v2.list_llm_obs_projects",
    "v2.create_llm_obs_experiment",
    "v2.list_llm_obs_experiments",
    "v2.update_llm_obs_experiment",
    "v2.delete_llm_obs_experiments",
    "v2.create_llm_obs_dataset",
    "v2.list_llm_obs_datasets",
];

// ---------------------------------------------------------------------------
// Auth type detection
// ---------------------------------------------------------------------------

use crate::useragent;

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
    // Cost / Billing (9) — API key only, no OAuth support
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
];

// ---------------------------------------------------------------------------
// Raw HTTP helpers
// ---------------------------------------------------------------------------

/// Raw HTTP response returned by [`raw_request`].
pub struct HttpResponse {
    /// The `Content-Type` header value from the response, or an empty string if absent.
    pub content_type: String,
    /// The raw response body bytes.
    pub bytes: Vec<u8>,
}

/// Makes an authenticated request with any HTTP method via reqwest.
///
/// - `body` — raw bytes to send; `content_type` sets the `Content-Type` header when present.
/// - `accept` — value for the `Accept` header (e.g. `"application/json"`, `"*/*"`).
/// - `extra_headers` — additional headers applied after auth and before the body.
/// - Returns an [`HttpResponse`] with the raw bytes and response `Content-Type`.
///   Callers are responsible for decoding the bytes.
pub async fn raw_request(
    cfg: &Config,
    method: &str,
    path: &str,
    body: Option<Vec<u8>>,
    content_type: Option<&str>,
    accept: &str,
    extra_headers: &[(&str, &str)],
) -> anyhow::Result<HttpResponse> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let client = reqwest::Client::new();
    let method = reqwest::Method::from_bytes(method.to_uppercase().as_bytes())
        .map_err(|_| anyhow::anyhow!("unsupported HTTP method: {method}"))?;
    let mut req = client.request(method, &url);

    if let Some(token) = &cfg.access_token {
        req = req.header("Authorization", format!("Bearer {token}"));
    } else if let (Some(api_key), Some(app_key)) = (&cfg.api_key, &cfg.app_key) {
        req = req
            .header("DD-API-KEY", api_key.as_str())
            .header("DD-APPLICATION-KEY", app_key.as_str());
    } else {
        anyhow::bail!("no authentication configured");
    }

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
        anyhow::bail!("API error (HTTP {status}): {text}");
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

    if let Some(token) = &cfg.access_token {
        req = req.header("Authorization", format!("Bearer {token}"));
    } else if let (Some(api_key), Some(app_key)) = (&cfg.api_key, &cfg.app_key) {
        req = req
            .header("DD-API-KEY", api_key.as_str())
            .header("DD-APPLICATION-KEY", app_key.as_str());
    } else {
        anyhow::bail!("no authentication configured");
    }

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
        anyhow::bail!("GET {url} failed (HTTP {status}): {body}");
    }
    Ok(resp.json().await?)
}

/// Makes an authenticated PATCH request directly via reqwest.
/// Used for endpoints not covered by the typed DD API client.
pub async fn raw_patch(
    cfg: &Config,
    path: &str,
    body: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let client = reqwest::Client::new();
    let mut req = client.patch(&url);

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
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("PATCH {url} failed (HTTP {status}): {body}");
    }
    Ok(resp.json().await?)
}

/// Makes an authenticated POST request directly via reqwest.
/// Used for endpoints not covered by the typed DD API client.
pub async fn raw_post(
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
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("POST {url} failed (HTTP {status}): {body}");
    }
    Ok(resp.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::test_utils::ENV_LOCK;

    fn test_cfg() -> Config {
        Config {
            api_key: Some("test".into()),
            app_key: Some("test".into()),
            access_token: None,
            site: "datadoghq.com".into(),
            org: None,
            output_format: crate::config::OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
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
    fn test_unstable_ops_count() {
        assert_eq!(UNSTABLE_OPS.len(), 85);
    }

    #[test]
    fn test_oauth_excluded_count() {
        assert_eq!(OAUTH_EXCLUDED_ENDPOINTS.len(), 45);
    }

    #[test]
    fn test_make_bearer_client_none_without_token() {
        let cfg = test_cfg();
        assert!(make_bearer_client(&cfg).is_none());
    }

    #[test]
    fn test_make_bearer_client_some_with_token() {
        let mut cfg = test_cfg();
        cfg.access_token = Some("test-token".into());
        assert!(make_bearer_client(&cfg).is_some());
    }

    #[test]
    fn test_make_dd_config_returns_valid() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let cfg = test_cfg();
        // Ensure env vars are set for DD client
        std::env::set_var("DD_API_KEY", "test-key");
        std::env::set_var("DD_APP_KEY", "test-app-key");
        std::env::remove_var("PUP_MOCK_SERVER");
        let dd_cfg = make_dd_config(&cfg);
        // Verify unstable ops are enabled (server_index should be default 0)
        assert_eq!(dd_cfg.server_index, 0);
        std::env::remove_var("DD_API_KEY");
        std::env::remove_var("DD_APP_KEY");
    }

    #[test]
    fn test_make_dd_config_with_mock_server() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let cfg = test_cfg();
        std::env::set_var("DD_API_KEY", "test-key");
        std::env::set_var("DD_APP_KEY", "test-app-key");
        std::env::set_var("PUP_MOCK_SERVER", "http://127.0.0.1:9999");
        let dd_cfg = make_dd_config(&cfg);
        assert_eq!(dd_cfg.server_index, 1);
        assert_eq!(dd_cfg.server_variables.get("protocol").unwrap(), "http");
        assert_eq!(
            dd_cfg.server_variables.get("name").unwrap(),
            "127.0.0.1:9999"
        );
        std::env::remove_var("PUP_MOCK_SERVER");
        std::env::remove_var("DD_API_KEY");
        std::env::remove_var("DD_APP_KEY");
    }

    #[test]
    fn test_make_dd_config_https_mock() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let cfg = test_cfg();
        std::env::set_var("DD_API_KEY", "test-key");
        std::env::set_var("DD_APP_KEY", "test-app-key");
        std::env::set_var("PUP_MOCK_SERVER", "https://mock.example.com");
        let dd_cfg = make_dd_config(&cfg);
        assert_eq!(dd_cfg.server_variables.get("protocol").unwrap(), "https");
        assert_eq!(
            dd_cfg.server_variables.get("name").unwrap(),
            "mock.example.com"
        );
        std::env::remove_var("PUP_MOCK_SERVER");
        std::env::remove_var("DD_API_KEY");
        std::env::remove_var("DD_APP_KEY");
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
    fn test_no_fallback_for_error_tracking() {
        assert!(!requires_api_key_fallback(
            "POST",
            "/api/v2/error_tracking/issues/search"
        ));
    }
}

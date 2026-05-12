use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};

#[cfg(not(target_arch = "wasm32"))]
use async_trait::async_trait;
#[cfg(not(target_arch = "wasm32"))]
use http::Extensions;
#[cfg(not(target_arch = "wasm32"))]
use reqwest_middleware::{Middleware, Next};

use crate::config::Config;

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
#[cfg(not(target_arch = "wasm32"))]
struct BearerAuthMiddleware {
    token: String,
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl Middleware for BearerAuthMiddleware {
    async fn handle(
        &self,
        mut req: reqwest_middleware::reqwest::Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<reqwest_middleware::reqwest::Response> {
        req.headers_mut().insert(
            reqwest_middleware::reqwest::header::AUTHORIZATION,
            format!("Bearer {}", self.token).parse().unwrap(),
        );
        next.run(req, extensions).await
    }
}

// The `datadog-api-client` SDK's `Configuration.user_agent` is `pub(crate)`
// with no setter, so the only way to override it from outside the crate is
// via middleware that mutates the header after the SDK builds the request.
#[cfg(not(target_arch = "wasm32"))]
struct UserAgentMiddleware;

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl Middleware for UserAgentMiddleware {
    async fn handle(
        &self,
        mut req: reqwest_middleware::reqwest::Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<reqwest_middleware::reqwest::Response> {
        if let Ok(ua) =
            reqwest_middleware::reqwest::header::HeaderValue::from_str(&crate::useragent::get())
        {
            req.headers_mut()
                .insert(reqwest_middleware::reqwest::header::USER_AGENT, ua);
        }
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
    // PAT wins over the long-lived app_key per documented precedence
    // (OAuth > PAT/SAT > API+App Key). Same order as `apply_auth`.
    if let Some(pat) = &cfg.pat {
        // PATs/SATs ride in DD-APPLICATION-KEY for OAuth-excluded endpoints
        // (api_keys, application_keys, ddsql-editor). Populating the SDK's
        // appKeyAuth slot with the PAT lets `make_api_no_auth!`-routed calls
        // succeed without DD_APP_KEY.
        let pat_apikey = datadog_api_client::datadog::APIKey {
            key: pat.clone(),
            prefix: "".to_owned(),
        };
        dd_cfg.set_auth_key("appKeyAuth", pat_apikey.clone());
        // The SDK may also require the apiKeyAuth slot to be populated before
        // dispatching a request (some operations mark DD-API-KEY as required
        // in the upstream OpenAPI spec). Per Datadog's PAT spec, DD-API-KEY is
        // ignored when DD-APPLICATION-KEY carries a PAT, so reusing the PAT
        // value here is safe and unblocks SDK-routed calls.
        if cfg.api_key.is_none() {
            dd_cfg.set_auth_key("apiKeyAuth", pat_apikey);
        }
    } else if let Some(app_key) = &cfg.app_key {
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
        //
        // The SDK populates `server_variables["site"]` from the DD_SITE env var
        // at Configuration::default() time. We override it with `cfg.site` so
        // programmatic site resolution (e.g. `--org` picking up a saved site
        // from the session registry) reaches the SDK without requiring the
        // user to also set DD_SITE.
        static STANDARD_SITES: &[&str] = &[
            "datadoghq.com",
            "us3.datadoghq.com",
            "us5.datadoghq.com",
            "ap1.datadoghq.com",
            "ap2.datadoghq.com",
            "datadoghq.eu",
            "ddog-gov.com",
        ];
        if !STANDARD_SITES.contains(&cfg.site.as_str()) {
            dd_cfg.server_index = 2;
        }
        dd_cfg
            .server_variables
            .insert("site".into(), cfg.site.clone());
    }

    dd_cfg
}

/// Builds a reqwest middleware client for SDK API calls. Always installs
/// `UserAgentMiddleware` so requests carry pup's branded `User-Agent`
/// instead of the SDK's `datadog-api-client-rust/...` default. When
/// `send_bearer` is true, installs `BearerAuthMiddleware` populated from the
/// OAuth access token if present, else from the PAT — both flow as
/// `Authorization: Bearer <token>` per Datadog's PAT spec. OAuth-incompatible
/// endpoints (see `OAUTH_EXCLUDED_ENDPOINTS`) pass `false` so the SDK falls
/// back to header auth from the `Configuration` (DD-API-KEY/DD-APPLICATION-KEY,
/// where DD-APPLICATION-KEY may carry a PAT).
///
/// Returns `None` on WASM targets; callers use the SDK default client there.
pub fn make_dd_client(cfg: &Config, send_bearer: bool) -> Option<ClientWithMiddleware> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let reqwest_client = reqwest_middleware::reqwest::Client::builder()
            .build()
            .expect("failed to build reqwest client");
        let mut builder = ClientBuilder::new(reqwest_client).with(UserAgentMiddleware);
        if send_bearer {
            // OAuth access_token wins over PAT; both ride as Bearer.
            let bearer = cfg.access_token.as_deref().or(cfg.pat.as_deref());
            if let Some(token) = bearer {
                builder = builder.with(BearerAuthMiddleware {
                    token: token.to_string(),
                });
            }
        }
        return Some(builder.build());
    }
    #[allow(unreachable_code)]
    {
        let _ = (cfg, send_bearer);
        None
    }
}

#[macro_export]
macro_rules! make_api {
    ($api:ty, $cfg:expr) => {{
        let cfg = $cfg;
        let dd_cfg = $crate::client::make_dd_config(cfg);
        match $crate::client::make_dd_client(cfg, true) {
            Some(c) => <$api>::with_client_and_config(dd_cfg, c),
            None => <$api>::with_config(dd_cfg),
        }
    }};
}

/// `make_api!` variant that skips bearer auth — for OAuth-incompatible endpoints.
#[macro_export]
macro_rules! make_api_no_auth {
    ($api:ty, $cfg:expr) => {{
        let cfg = $cfg;
        let dd_cfg = $crate::client::make_dd_config(cfg);
        match $crate::client::make_dd_client(cfg, false) {
            Some(c) => <$api>::with_client_and_config(dd_cfg, c),
            None => <$api>::with_config(dd_cfg),
        }
    }};
}

// ---------------------------------------------------------------------------
// Unstable operations table — used by make_dd_config
// ---------------------------------------------------------------------------

/// All unstable operations (snake_case for the Rust DD client).
static UNSTABLE_OPS: &[&str] = &[
    // Incidents (26)
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
    // Incident Teams (5)
    "v2.create_incident_team",
    "v2.delete_incident_team",
    "v2.get_incident_team",
    "v2.list_incident_teams",
    "v2.update_incident_team",
    // Incident Services (5)
    "v2.create_incident_service",
    "v2.delete_incident_service",
    "v2.get_incident_service",
    "v2.list_incident_services",
    "v2.update_incident_service",
    // Fleet Automation (18)
    "v2.list_fleet_agents",
    "v2.get_fleet_agent_info",
    "v2.list_fleet_agent_versions",
    "v2.list_fleet_agent_tracers",
    "v2.list_fleet_tracers",
    "v2.list_fleet_clusters",
    "v2.list_fleet_instrumented_pods",
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
    // Indicators of Compromise (2)
    "v2.list_indicators_of_compromise",
    "v2.get_indicator_of_compromise",
    // Security Monitoring Terraform export (3)
    "v2.bulk_export_security_monitoring_terraform_resources",
    "v2.export_security_monitoring_terraform_resource",
    "v2.convert_security_monitoring_terraform_resource",
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
    // Org Group Policies (11)
    "v2.list_org_group_policies",
    "v2.get_org_group_policy",
    "v2.create_org_group_policy",
    "v2.update_org_group_policy",
    "v2.delete_org_group_policy",
    "v2.list_org_group_policy_overrides",
    "v2.get_org_group_policy_override",
    "v2.create_org_group_policy_override",
    "v2.update_org_group_policy_override",
    "v2.delete_org_group_policy_override",
    "v2.list_org_group_policy_configs",
    // Security Findings (1)
    "v2.list_findings",
    // SLO Status (1)
    "v2.get_slo_status",
    // Flaky Tests (4)
    "v2.search_flaky_tests",
    "v2.update_flaky_tests",
    "v2.get_flaky_tests_management_policies",
    "v2.update_flaky_tests_management_policies",
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
    // LLM Observability (18)
    "v2.create_llm_obs_project",
    "v2.list_llm_obs_projects",
    "v2.create_llm_obs_experiment",
    "v2.list_llm_obs_experiments",
    "v2.update_llm_obs_experiment",
    "v2.delete_llm_obs_experiments",
    "v2.create_llm_obs_dataset",
    "v2.list_llm_obs_datasets",
    "v2.create_llm_obs_annotation_queue",
    "v2.list_llm_obs_annotation_queues",
    "v2.update_llm_obs_annotation_queue",
    "v2.delete_llm_obs_annotation_queue",
    "v2.create_llm_obs_annotation_queue_interactions",
    "v2.delete_llm_obs_annotation_queue_interactions",
    "v2.get_llm_obs_annotated_interactions",
    "v2.get_llm_obs_custom_eval_config",
    "v2.update_llm_obs_custom_eval_config",
    "v2.delete_llm_obs_custom_eval_config",
    // Logs Restriction Queries (9)
    "v2.list_restriction_queries",
    "v2.get_restriction_query",
    "v2.create_restriction_query",
    "v2.update_restriction_query",
    "v2.delete_restriction_query",
    "v2.list_restriction_query_roles",
    "v2.add_role_to_restriction_query",
    "v2.remove_role_from_restriction_query",
    "v2.get_role_restriction_query",
    // Datasets (5)
    "v2.create_dataset",
    "v2.delete_dataset",
    "v2.get_all_datasets",
    "v2.get_dataset",
    "v2.update_dataset",
    // Data Deletion (3)
    "v2.cancel_data_deletion_request",
    "v2.create_data_deletion_request",
    "v2.get_data_deletion_requests",
    // Service Scorecards (7)
    "v2.create_scorecard_outcomes_batch",
    "v2.create_scorecard_rule",
    "v2.delete_scorecard_rule",
    "v2.list_scorecard_outcomes",
    "v2.list_scorecard_rules",
    "v2.update_scorecard_outcomes_async",
    "v2.update_scorecard_rule",
    // Static Analysis (10)
    "v2.create_custom_rule",
    "v2.create_custom_rule_revision",
    "v2.create_sca_resolve_vulnerable_symbols",
    "v2.create_sca_result",
    "v2.delete_custom_rule",
    "v2.delete_custom_ruleset",
    "v2.get_custom_rule",
    "v2.get_custom_rule_revision",
    "v2.get_custom_ruleset",
    "v2.list_custom_rule_revisions",
    "v2.revert_custom_rule_revision",
    "v2.update_custom_ruleset",
    // Bits AI Investigations (3)
    "v2.get_investigation",
    "v2.list_investigations",
    "v2.trigger_investigation",
];

// ---------------------------------------------------------------------------
// Auth type detection
// ---------------------------------------------------------------------------

use crate::useragent;

// Parse a reqwest response body as JSON without serde_json's default 128-level
// recursion cap. Some Datadog endpoints (e.g. /profiling/api/v1/aggregate)
// return deeply-nested flame-graph trees that exceed it. serde_stacker grows
// the thread stack on demand so disabling the limit can't blow it.
async fn parse_response_json(resp: reqwest::Response) -> anyhow::Result<serde_json::Value> {
    use serde::Deserialize;
    let bytes = resp.bytes().await?;
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
    /// Datadog Access Token -- Personal (DD_PAT) or Service (DD_SAT). Both
    /// kinds are wire-identical; the inner value records which one for
    /// status display.
    AccessToken(crate::config::PatKind),
    ApiKeys,
}

impl std::fmt::Display for AuthType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthType::None => write!(f, "None"),
            AuthType::OAuth => write!(f, "OAuth2 Bearer Token"),
            AuthType::AccessToken(kind) => {
                write!(f, "{} ({})", kind.display_name(), kind.env_var())
            }
            AuthType::ApiKeys => write!(f, "API Keys (DD_API_KEY + DD_APP_KEY)"),
        }
    }
}

#[allow(dead_code)]
pub fn get_auth_type(cfg: &Config) -> AuthType {
    // Match auth-precedence in apply_auth: OAuth > PAT/SAT > API+App Key.
    // `pat_kind` is set together with `pat` (see Config::from_env), so binding
    // here lets us guarantee a non-None inner value.
    if cfg.has_bearer_token() {
        AuthType::OAuth
    } else if let Some(kind) = cfg.pat_kind {
        AuthType::AccessToken(kind)
    } else if cfg.has_api_keys() {
        AuthType::ApiKeys
    } else {
        AuthType::None
    }
}

// ---------------------------------------------------------------------------
// OAuth-excluded endpoint validation
//
// The canonical list and lookup logic live in `crate::oauth_excluded` so
// the WASM/browser build (which does not import this module) can consult
// the same table.
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub use crate::oauth_excluded::requires_api_key_fallback;

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

/// Attach auth headers (OAuth bearer, PAT/SAT, or API+App Key) to a raw
/// `reqwest::RequestBuilder`. Respects the documented precedence
/// (OAuth > PAT/SAT > API+App Key) and routes PAT/SAT to `DD-APPLICATION-KEY`
/// for OAuth-excluded endpoints. Single source of truth used by every native
/// request path; `commands/*` helpers should delegate here.
pub fn apply_auth(
    mut req: reqwest::RequestBuilder,
    cfg: &Config,
    method: &str,
    path: &str,
) -> anyhow::Result<reqwest::RequestBuilder> {
    if requires_api_key_fallback(method, path) {
        // OAuth-excluded endpoints do not accept Bearer tokens. They DO accept
        // either DD_API_KEY+DD_APP_KEY or a PAT/SAT in the DD-APPLICATION-KEY
        // slot (Datadog's PAT migration form -- DD-API-KEY is optional/ignored).
        //
        // Precedence here matches the documented tier order: PAT/SAT is preferred
        // over the long-lived API+App Key pair, so it wins when both are set.
        if let Some(pat) = &cfg.pat {
            // PATs/SATs ride in DD-APPLICATION-KEY. DD-API-KEY is optional and
            // ignored for these tokens; include it only if explicitly set so we
            // don't fabricate a header value.
            req = req.header("DD-APPLICATION-KEY", pat.as_str());
            if let Some(api_key) = &cfg.api_key {
                req = req.header("DD-API-KEY", api_key.as_str());
            }
            return Ok(req);
        }
        if let (Some(api_key), Some(app_key)) = (&cfg.api_key, &cfg.app_key) {
            req = req
                .header("DD-API-KEY", api_key.as_str())
                .header("DD-APPLICATION-KEY", app_key.as_str());
            return Ok(req);
        }

        anyhow::bail!(
            "{method} {path} requires DD_PAT/DD_SAT or DD_API_KEY+DD_APP_KEY; \
             OAuth2 bearer tokens are not supported"
        );
    }

    if let Some(token) = &cfg.access_token {
        req = req.header("Authorization", format!("Bearer {token}"));
        return Ok(req);
    }

    if let Some(pat) = &cfg.pat {
        req = req.header("Authorization", format!("Bearer {pat}"));
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

    req = apply_auth(req, cfg, "POST", path)?;

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
    use crate::test_support::*;

    use super::*;
    use crate::config::Config;
    use crate::test_utils::ENV_LOCK;

    fn test_cfg() -> Config {
        Config {
            api_key: Some("test".into()),
            app_key: Some("test".into()),
            access_token: None,
            pat: None,
            pat_kind: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
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

    /// Regression: `apply_auth` on OAuth-excluded endpoints must prefer a PAT
    /// over API+App Key. Otherwise documented precedence (PAT > API+App Key)
    /// is silently reversed for the api_keys / application_keys / ddsql paths.
    #[tokio::test]
    async fn test_apply_auth_excluded_endpoint_pat_beats_api_app_key() {
        let _guard = ENV_LOCK.lock().await;
        let mut server = mockito::Server::new_async().await;
        std::env::set_var("PUP_MOCK_SERVER", server.url());

        let mut cfg = test_cfg();
        cfg.pat = Some("the-pat".into());
        cfg.pat_kind = Some(crate::config::PatKind::Personal);
        // api_key + app_key are also set (inherited from test_cfg) -- PAT must win.

        // /api/v2/api_keys is in OAUTH_EXCLUDED_ENDPOINTS.
        let mock = server
            .mock("GET", "/api/v2/api_keys")
            .match_header("DD-APPLICATION-KEY", "the-pat")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create_async()
            .await;

        let _ = raw_get(&cfg, "/api/v2/api_keys", &[]).await;
        mock.assert_async().await;

        std::env::remove_var("PUP_MOCK_SERVER");
    }

    /// `raw_post_lenient` must use the shared `apply_auth` so PAT/SAT callers
    /// can hit endpoints routed through it (debugger probe create, etc.).
    #[tokio::test]
    async fn test_raw_post_lenient_with_pat() {
        let _guard = ENV_LOCK.lock().await;
        let mut server = mockito::Server::new_async().await;
        std::env::set_var("PUP_MOCK_SERVER", server.url());

        let mut cfg = test_cfg();
        cfg.api_key = None;
        cfg.app_key = None;
        cfg.pat = Some("the-pat".into());
        cfg.pat_kind = Some(crate::config::PatKind::Personal);

        let mock = server
            .mock("POST", "/api/v1/some-endpoint")
            .match_header("Authorization", "Bearer the-pat")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create_async()
            .await;

        let _ = raw_post_lenient(&cfg, "/api/v1/some-endpoint", serde_json::json!({})).await;
        mock.assert_async().await;

        std::env::remove_var("PUP_MOCK_SERVER");
    }

    /// `raw_delete` must use the shared `apply_auth` so PAT/SAT callers can
    /// hit endpoints routed through it (cost-CCM delete paths, etc.).
    #[tokio::test]
    async fn test_raw_delete_with_pat() {
        let _guard = ENV_LOCK.lock().await;
        let mut server = mockito::Server::new_async().await;
        std::env::set_var("PUP_MOCK_SERVER", server.url());

        let mut cfg = test_cfg();
        cfg.api_key = None;
        cfg.app_key = None;
        cfg.pat = Some("the-pat".into());
        cfg.pat_kind = Some(crate::config::PatKind::Personal);

        let mock = server
            .mock("DELETE", "/api/v1/some-resource")
            .match_header("Authorization", "Bearer the-pat")
            .with_status(204)
            .create_async()
            .await;

        let _ = raw_delete(&cfg, "/api/v1/some-resource").await;
        mock.assert_async().await;

        std::env::remove_var("PUP_MOCK_SERVER");
    }

    // Note: we don't unit-test that make_dd_config populates appKeyAuth /
    // apiKeyAuth slots for a PAT-only Config -- the SDK's `auth_keys` field
    // is private. The behavior is exercised indirectly via the manual
    // `apply_auth` path tests above, which assert PAT routing on the wire.

    /// `make_dd_config` must propagate `cfg.site` into the SDK's `site`
    /// server variable, otherwise programmatic site resolution (e.g.
    /// `--org` picking up a saved staging site) silently routes API calls
    /// to api.datadoghq.com.
    #[test]
    fn test_make_dd_config_uses_cfg_site_for_non_standard() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        std::env::remove_var("DD_SITE");

        let mut cfg = test_cfg();
        cfg.site = "datad0g.com".into();

        let dd_cfg = make_dd_config(&cfg);

        assert_eq!(dd_cfg.server_index, 2);
        assert_eq!(
            dd_cfg.server_variables.get("site").map(String::as_str),
            Some("datad0g.com")
        );
    }

    #[test]
    fn test_make_dd_config_uses_cfg_site_for_standard() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        std::env::remove_var("DD_SITE");

        let mut cfg = test_cfg();
        cfg.site = "datadoghq.eu".into();

        let dd_cfg = make_dd_config(&cfg);

        assert_eq!(dd_cfg.server_index, 0);
        assert_eq!(
            dd_cfg.server_variables.get("site").map(String::as_str),
            Some("datadoghq.eu")
        );
    }

    /// `cfg.site` (e.g. resolved from a saved org session) must override any
    /// stale `DD_SITE` env var the user happens to have set in their shell —
    /// otherwise `pup --org staging-org` would silently route to the env's
    /// site instead of the org's saved site.
    #[test]
    fn test_make_dd_config_cfg_site_overrides_env_dd_site() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        std::env::set_var("DD_SITE", "datadoghq.com");

        let mut cfg = test_cfg();
        cfg.site = "datad0g.com".into();

        let dd_cfg = make_dd_config(&cfg);

        std::env::remove_var("DD_SITE");

        assert_eq!(dd_cfg.server_index, 2);
        assert_eq!(
            dd_cfg.server_variables.get("site").map(String::as_str),
            Some("datad0g.com")
        );
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
        assert_eq!(UNSTABLE_OPS.len(), 166);
    }

    // `test_oauth_excluded_count` now lives in `src/oauth_excluded.rs` alongside the canonical list.

    #[test]
    fn test_make_dd_client_some_without_token() {
        // UA middleware is always installed, so the client is always Some on native.
        let cfg = test_cfg();
        assert!(make_dd_client(&cfg, true).is_some());
        assert!(make_dd_client(&cfg, false).is_some());
    }

    #[test]
    fn test_make_dd_client_some_with_token() {
        let mut cfg = test_cfg();
        cfg.access_token = Some("test-token".into());
        assert!(make_dd_client(&cfg, true).is_some());
        assert!(make_dd_client(&cfg, false).is_some());
    }

    #[test]
    fn test_make_api_macro_without_bearer_token() {
        use datadog_api_client::datadogV1::api_monitors::MonitorsAPI;
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let cfg = test_cfg();
        let _api: MonitorsAPI = crate::make_api!(MonitorsAPI, &cfg);
    }

    #[test]
    fn test_make_api_macro_with_bearer_token() {
        use datadog_api_client::datadogV1::api_monitors::MonitorsAPI;
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let mut cfg = test_cfg();
        cfg.access_token = Some("test-token".into());
        let _api: MonitorsAPI = crate::make_api!(MonitorsAPI, &cfg);
    }

    #[test]
    fn test_make_dd_config_returns_valid() {
        let _guard = ENV_LOCK.blocking_lock();
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
        let _guard = ENV_LOCK.blocking_lock();
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
        let _guard = ENV_LOCK.blocking_lock();
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

    /// Verifies that requests built via `make_api!` carry pup's branded
    /// `User-Agent` rather than the SDK's default `datadog-api-client-rust/...`.
    /// The mock only matches when the header starts with `pup/`; if the
    /// middleware fails to override, mockito returns 501 and the SDK call fails.
    #[tokio::test]
    async fn test_make_api_sends_pup_user_agent() {
        use datadog_api_client::datadogV1::api_monitors::{
            ListMonitorsOptionalParams, MonitorsAPI,
        };
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_header("User-Agent", mockito::Matcher::Regex("^pup/".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("[]")
            .expect(1)
            .create_async()
            .await;

        let cfg = test_config(&server.url());
        let api: MonitorsAPI = crate::make_api!(MonitorsAPI, &cfg);
        let resp = api
            .list_monitors(ListMonitorsOptionalParams::default())
            .await;
        assert!(
            resp.is_ok(),
            "make_api! request did not carry pup/ User-Agent: {:?}",
            resp.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    /// Like `test_make_api_sends_pup_user_agent`, but with an OAuth bearer
    /// token configured — verifies that the UA middleware coexists with the
    /// bearer middleware (both headers land on the same request).
    #[tokio::test]
    async fn test_make_api_sends_pup_user_agent_with_bearer() {
        use datadog_api_client::datadogV1::api_monitors::{
            ListMonitorsOptionalParams, MonitorsAPI,
        };
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_header("User-Agent", mockito::Matcher::Regex("^pup/".into()))
            .match_header("Authorization", "Bearer test-bearer-token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("[]")
            .expect(1)
            .create_async()
            .await;

        let mut cfg = test_config(&server.url());
        cfg.access_token = Some("test-bearer-token".into());
        let api: MonitorsAPI = crate::make_api!(MonitorsAPI, &cfg);
        let resp = api
            .list_monitors(ListMonitorsOptionalParams::default())
            .await;
        assert!(
            resp.is_ok(),
            "make_api! with bearer didn't carry both UA and Authorization: {:?}",
            resp.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    /// Same coverage for the no-auth variant. Asserts the UA is overridden
    /// AND that no `Authorization` header leaks through, even when a bearer
    /// token exists in the config — that's the contract of `make_api_no_auth!`.
    #[tokio::test]
    async fn test_make_api_no_auth_sends_pup_user_agent() {
        use datadog_api_client::datadogV2::api_authn_mappings::{
            AuthNMappingsAPI, ListAuthNMappingsOptionalParams,
        };
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_header("User-Agent", mockito::Matcher::Regex("^pup/".into()))
            .match_header("Authorization", mockito::Matcher::Missing)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .expect(1)
            .create_async()
            .await;

        let mut cfg = test_config(&server.url());
        // Set a token so the Authorization-absent assertion meaningfully
        // exercises that `make_api_no_auth!` actively suppresses bearer.
        cfg.access_token = Some("test-bearer-token".into());
        let api: AuthNMappingsAPI = crate::make_api_no_auth!(AuthNMappingsAPI, &cfg);
        let resp = api
            .list_authn_mappings(ListAuthNMappingsOptionalParams::default())
            .await;
        assert!(
            resp.is_ok(),
            "make_api_no_auth! request leaked Authorization or wrong UA: {:?}",
            resp.err()
        );
        mock.assert_async().await;
        cleanup_env();
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
}

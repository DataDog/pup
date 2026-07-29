use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};

#[cfg(not(target_arch = "wasm32"))]
use async_trait::async_trait;
#[cfg(not(target_arch = "wasm32"))]
use http::Extensions;
#[cfg(not(target_arch = "wasm32"))]
use reqwest_middleware::{Middleware, Next};

use crate::config::Config;

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
    if let Some(app_key) = &cfg.app_key {
        dd_cfg.set_auth_key(
            "appKeyAuth",
            datadog_api_client::datadog::APIKey {
                key: app_key.clone(),
                prefix: "".to_owned(),
            },
        );
    }

    // Route the SDK at the single resolved API host. `api_base_url()` already
    // encapsulates every case: the PUP_MOCK_SERVER override, the `api.{site}`
    // derivation for canonical Datadog sites, and the verbatim host for
    // vanity/gateway hosts. We feed it through the SDK's `{protocol}://{name}`
    // template (server index 1) so the host is targeted exactly as resolved —
    // the SDK never re-derives or prepends anything from `site`.
    let base = cfg.api_base_url();
    // A scheme-less value only occurs for a PUP_MOCK_SERVER set without
    // `http(s)://`; default it to plain http (mock servers run HTTP locally).
    // The non-mock path always yields `https://...`, so it never hits the fallback.
    let (protocol, name) = base.split_once("://").unwrap_or(("http", base.as_str()));
    dd_cfg.server_index = 1;
    dd_cfg
        .server_variables
        .insert("protocol".into(), protocol.into());
    dd_cfg.server_variables.insert("name".into(), name.into());

    dd_cfg
}

/// Builds a reqwest middleware client for SDK API calls. Always installs
/// `UserAgentMiddleware` so requests carry pup's branded `User-Agent`
/// instead of the SDK's `datadog-api-client-rust/...` default. When
/// `send_bearer` is true and the config has an access token, also installs
/// `BearerAuthMiddleware`. OAuth-incompatible endpoints (see
/// `raw_client::OAUTH_EXCLUDED_ENDPOINTS`) pass `false` so the SDK falls back
/// to API key headers from the `Configuration`.
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
            if let Some(token) = cfg.access_token.as_ref() {
                builder = builder.with(BearerAuthMiddleware {
                    token: token.clone(),
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
    // Annotations (5)
    "v2.create_annotation",
    "v2.delete_annotation",
    "v2.get_page_annotations",
    "v2.list_annotations",
    "v2.update_annotation",
    // Incidents (26)
    "v2.list_incidents",
    "v2.search_incidents",
    "v2.get_incident",
    "v2.create_incident",
    "v2.update_incident",
    "v2.delete_incident",
    "v2.list_incident_attachments",
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
    // Fleet Automation (16)
    "v2.list_fleet_agents",
    "v2.get_fleet_agent_info",
    "v2.list_fleet_agent_versions",
    "v2.list_fleet_agent_tracers",
    "v2.list_fleet_tracers",
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
    // LLM Observability (21)
    "v2.create_llm_obs_project",
    "v2.list_llm_obs_projects",
    "v2.create_llm_obs_experiment",
    "v2.list_llm_obs_experiments",
    "v2.update_llm_obs_experiment",
    "v2.delete_llm_obs_experiments",
    "v2.create_llm_obs_dataset",
    "v2.list_llm_obs_datasets",
    "v2.batch_update_llm_obs_dataset",
    "v2.clone_llm_obs_dataset",
    "v2.restore_llm_obs_dataset_version",
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
    // Cloud Cost Management — Anomalies (1)
    "v2.list_cost_anomalies",
    // Tag Policies (6)
    "v2.create_tag_policy",
    "v2.delete_tag_policy",
    "v2.get_tag_policy",
    "v2.get_tag_policy_score",
    "v2.list_tag_policies",
    "v2.update_tag_policy",
    // Model Lab (16)
    "v2.delete_model_lab_run",
    "v2.get_model_lab_artifact_content",
    "v2.get_model_lab_project",
    "v2.get_model_lab_run",
    "v2.list_model_lab_project_artifacts",
    "v2.list_model_lab_project_facet_keys",
    "v2.list_model_lab_project_facet_values",
    "v2.list_model_lab_projects",
    "v2.list_model_lab_run_artifacts",
    "v2.list_model_lab_run_facet_keys",
    "v2.list_model_lab_run_facet_values",
    "v2.list_model_lab_runs",
    "v2.pin_model_lab_run",
    "v2.star_model_lab_project",
    "v2.unpin_model_lab_run",
    "v2.unstar_model_lab_project",
];

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

    /// Asserts the SDK is routed at the host produced by `name`/`protocol`,
    /// the single `{protocol}://{name}` template at server index 1.
    fn assert_dd_host(
        dd_cfg: &datadog_api_client::datadog::Configuration,
        protocol: &str,
        host: &str,
    ) {
        assert_eq!(dd_cfg.server_index, 1);
        assert_eq!(
            dd_cfg.server_variables.get("protocol").map(String::as_str),
            Some(protocol)
        );
        assert_eq!(
            dd_cfg.server_variables.get("name").map(String::as_str),
            Some(host)
        );
    }

    /// Canonical Datadog sites derive `api.{site}` — including non-default ones
    /// (staging datad0g.com, datadoghq.eu) resolved programmatically via `--org`
    /// rather than DD_SITE.
    #[test]
    fn test_make_dd_config_canonical_site_derives_api_host() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        std::env::remove_var("DD_SITE");

        let mut cfg = test_cfg();
        cfg.site = "datad0g.com".into();
        assert_dd_host(&make_dd_config(&cfg), "https", "api.datad0g.com");

        cfg.site = "datadoghq.eu".into();
        assert_dd_host(&make_dd_config(&cfg), "https", "api.datadoghq.eu");
    }

    #[test]
    fn test_make_dd_config_literal_host_used_verbatim() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        std::env::remove_var("DD_SITE");

        let mut cfg = test_cfg();
        cfg.site = "mygateway.example.com".into(); // literal, not in KNOWN_SITES
        assert_dd_host(&make_dd_config(&cfg), "https", "mygateway.example.com");
    }

    #[test]
    fn test_make_dd_config_vanity_subdomain_used_verbatim() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        std::env::remove_var("DD_SITE");

        let mut cfg = test_cfg();
        cfg.site = "mycompany.datadoghq.com".into(); // vanity, not in KNOWN_SITES
        assert_dd_host(&make_dd_config(&cfg), "https", "mycompany.datadoghq.com");
    }

    /// A literal host must be targeted verbatim even when the user has DD_SITE
    /// set in their shell — `cfg.site` is the single source of truth and the SDK
    /// never re-derives the host from the `DD_SITE`-populated `site` variable.
    #[test]
    fn test_make_dd_config_literal_host_ignores_env_dd_site() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        std::env::set_var("DD_SITE", "datadoghq.com");

        let mut cfg = test_cfg();
        cfg.site = "mygateway.example.com".into(); // literal, not in KNOWN_SITES

        let dd_cfg = make_dd_config(&cfg);
        std::env::remove_var("DD_SITE");

        assert_dd_host(&dd_cfg, "https", "mygateway.example.com");
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

        assert_dd_host(&dd_cfg, "https", "api.datad0g.com");
    }

    #[test]
    fn test_unstable_ops_count() {
        assert_eq!(UNSTABLE_OPS.len(), 186);
    }

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
        std::env::remove_var("DD_SITE");
        let dd_cfg = make_dd_config(&cfg);
        // Default canonical site datadoghq.com derives api.datadoghq.com.
        assert_dd_host(&dd_cfg, "https", "api.datadoghq.com");
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

    /// A scheme-less PUP_MOCK_SERVER (no `http(s)://`) defaults to plain http —
    /// mock servers run HTTP locally. Exercises the `split_once` fallback branch.
    #[test]
    fn test_make_dd_config_scheme_less_mock_defaults_to_http() {
        let _guard = ENV_LOCK.blocking_lock();
        let cfg = test_cfg();
        std::env::set_var("DD_API_KEY", "test-key");
        std::env::set_var("DD_APP_KEY", "test-app-key");
        std::env::set_var("PUP_MOCK_SERVER", "127.0.0.1:9999");
        let dd_cfg = make_dd_config(&cfg);
        assert_dd_host(&dd_cfg, "http", "127.0.0.1:9999");
        std::env::remove_var("PUP_MOCK_SERVER");
        std::env::remove_var("DD_API_KEY");
        std::env::remove_var("DD_APP_KEY");
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
    /// Uses `ApplicationSecurityAPI` (ASM WAF custom rules), which is still a
    /// genuinely no-auth production call site as of this test.
    #[tokio::test]
    async fn test_make_api_no_auth_sends_pup_user_agent() {
        use datadog_api_client::datadogV2::api_application_security::ApplicationSecurityAPI;
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
        let api: ApplicationSecurityAPI = crate::make_api_no_auth!(ApplicationSecurityAPI, &cfg);
        let resp = api.list_application_security_waf_custom_rules().await;
        assert!(
            resp.is_ok(),
            "make_api_no_auth! request leaked Authorization or wrong UA: {:?}",
            resp.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }
}

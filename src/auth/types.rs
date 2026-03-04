use chrono::Utc;
use serde::{Deserialize, Serialize};

/// OAuth2 token set (JSON cross-compatible with Go/TypeScript versions).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default = "default_token_type")]
    pub token_type: String,
    pub expires_in: i64,
    pub issued_at: i64,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub client_id: String,
}

fn default_token_type() -> String {
    "Bearer".to_string()
}

impl TokenSet {
    /// Returns true if the token is expired or will expire within 5 minutes.
    pub fn is_expired(&self) -> bool {
        let now = Utc::now().timestamp();
        let expires_at = self.issued_at + self.expires_in;
        now >= (expires_at - 300) // 5-minute safety buffer
    }
}

/// DCR client credentials (cross-compatible with Go/TypeScript versions).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCredentials {
    pub client_id: String,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub registered_at: i64,
    pub site: String,
}

/// All known valid OAuth scopes for validation.
pub fn all_known_scopes() -> Vec<&'static str> {
    default_scopes()
}

/// Read-only OAuth scopes for use with --read-only flag.
/// Excludes write, manage, and org-level administrative scopes.
pub fn read_only_scopes() -> Vec<&'static str> {
    vec![
        "apm_read",
        "apm_service_catalog_read",
        "audit_logs_read",
        "azure_configuration_read",
        "bits_investigations_read",
        "cases_read",
        "ci_visibility_read",
        "code_coverage_read",
        "test_optimization_read",
        "dashboards_read",
        "data_scanner_read",
        "error_tracking_read",
        "events_read",
        "disaster_recovery_status_read",
        "hosts_read",
        "incident_read",
        "incident_notification_settings_read",
        "incident_settings_read",
        "integrations_read",
        "llm_observability_read",
        "logs_read_archives",
        "logs_read_config",
        "logs_read_data",
        "logs_read_index_data",
        "metrics_read",
        "monitors_read",
        "notebooks_read",
        "oci_configuration_read",
        "reference_tables_read",
        "rum_apps_read",
        "rum_retention_filters_read",
        "rum_session_replay_read",
        "security_monitoring_filters_read",
        "security_monitoring_findings_read",
        "security_monitoring_rules_read",
        "security_monitoring_signals_read",
        "slos_read",
        "status_pages_settings_read",
        "synthetics_read",
        "synthetics_private_location_read",
        "teams_read",
        "timeseries_query",
        "usage_read",
        "user_access_read",
    ]
}

/// Default OAuth scopes requested during login.
pub fn default_scopes() -> Vec<&'static str> {
    vec![
        // APM
        "apm_read",
        "apm_service_catalog_read",
        // Audit
        "audit_logs_read",
        // Azure
        "azure_configuration_read",
        // BITS
        "bits_investigations_read",
        "bits_investigations_write",
        // Cases
        "cases_read",
        "cases_write",
        // CI Visibility
        "ci_visibility_read",
        "code_coverage_read",
        "dora_metrics_write",
        "test_optimization_read",
        "test_optimization_write",
        // Live Debugger
        "debugger_read",
        "debugger_write",
        "debugger_capture_variables",
        // Dashboards
        "dashboards_read",
        "dashboards_write",
        // Data Scanner
        "data_scanner_read",
        // Error Tracking
        "error_tracking_read",
        // Events
        "events_read",
        // HAMR (disaster recovery)
        "disaster_recovery_status_read",
        "disaster_recovery_status_write",
        // Hosts
        "hosts_read",
        "host_tags_write",
        // Incidents
        "incident_read",
        "incident_write",
        "incident_notification_settings_read",
        "incident_settings_read",
        "incident_settings_write",
        // Integrations (Jira, ServiceNow, Slack, Webhooks)
        "integrations_read",
        "manage_integrations",
        // LLM Observability
        "llm_observability_read",
        "llm_observability_write",
        // Logs
        "logs_generate_metrics",
        "logs_modify_indexes",
        "logs_read_archives",
        "logs_read_config",
        "logs_read_data",
        "logs_read_index_data",
        "logs_write_archives",
        // Metrics
        "metrics_read",
        // Monitors
        "monitors_read",
        "monitors_write",
        "monitors_downtime",
        // Notebooks
        "notebooks_read",
        "notebooks_write",
        // OCI
        "oci_configuration_edit",
        "oci_configuration_read",
        "oci_configurations_manage",
        // Organizations
        "org_management",
        // Reference Tables
        "reference_tables_read",
        "reference_tables_write",
        // RUM
        // RUM
        "rum_apps_read",
        "rum_apps_write",
        "rum_generate_metrics",
        "rum_retention_filters_read",
        "rum_retention_filters_write",
        "rum_session_replay_read",
        // Security
        "security_monitoring_filters_read",
        "security_monitoring_filters_write",
        "security_monitoring_findings_read",
        "security_monitoring_rules_read",
        "security_monitoring_rules_write",
        "security_monitoring_signals_read",
        // SLOs
        "slos_read",
        "slos_write",
        // Status Pages
        "status_pages_settings_read",
        "status_pages_settings_write",
        // Synthetics
        "synthetics_read",
        "synthetics_write",
        "synthetics_private_location_read",
        // Teams
        "teams_manage",
        "teams_read",
        // Timeseries
        "timeseries_query",
        // Usage
        "usage_read",
        // Users
        "user_access_read",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_token(issued_ago_secs: i64, expires_in: i64) -> TokenSet {
        TokenSet {
            access_token: "test".into(),
            refresh_token: "refresh".into(),
            token_type: "Bearer".into(),
            expires_in,
            issued_at: chrono::Utc::now().timestamp() - issued_ago_secs,
            scope: String::new(),
            client_id: String::new(),
        }
    }

    #[test]
    fn test_token_not_expired() {
        let token = make_token(0, 3600); // issued now, expires in 1h
        assert!(!token.is_expired());
    }

    #[test]
    fn test_token_expired() {
        let token = make_token(7200, 3600); // issued 2h ago, expires in 1h = expired 1h ago
        assert!(token.is_expired());
    }

    #[test]
    fn test_token_expires_within_buffer() {
        let token = make_token(3400, 3600); // issued 3400s ago, expires at 3600s = 200s left < 300s buffer
        assert!(token.is_expired());
    }

    #[test]
    fn test_token_just_outside_buffer() {
        let token = make_token(3000, 3600); // issued 3000s ago, expires at 3600s = 600s left > 300s buffer
        assert!(!token.is_expired());
    }

    #[test]
    fn test_default_scopes() {
        let scopes = default_scopes();
        assert_eq!(scopes.len(), 77);
        assert!(scopes.contains(&"dashboards_read"));
        assert!(scopes.contains(&"monitors_read"));
        assert!(scopes.contains(&"logs_read_data"));
        // Batch 2 additions
        assert!(scopes.contains(&"integrations_read"));
        assert!(scopes.contains(&"org_management"));
        assert!(scopes.contains(&"disaster_recovery_status_read"));
        assert!(scopes.contains(&"notebooks_read"));
        assert!(scopes.contains(&"rum_generate_metrics"));
        assert!(scopes.contains(&"ci_visibility_read"));
        assert!(scopes.contains(&"teams_read"));
        assert!(scopes.contains(&"apm_service_catalog_read"));
        assert!(scopes.contains(&"status_pages_settings_read"));
    }

    #[test]
    fn test_read_only_scopes_no_write_or_manage() {
        let ro = read_only_scopes();
        for scope in &ro {
            assert!(
                !scope.ends_with("_write")
                    && !scope.ends_with("_manage")
                    && *scope != "org_management",
                "read_only_scopes should not contain write/manage scope: {scope}"
            );
        }
        assert!(ro.contains(&"dashboards_read"));
        assert!(ro.contains(&"monitors_read"));
        assert!(!ro.contains(&"org_management"));
        assert!(!ro.contains(&"teams_manage"));
        assert!(!ro.contains(&"monitors_write"));
    }

    #[test]
    fn test_read_only_scopes_subset_of_default() {
        let default: std::collections::HashSet<&str> = default_scopes().into_iter().collect();
        for scope in read_only_scopes() {
            assert!(
                default.contains(scope),
                "read_only scope not in default_scopes: {scope}"
            );
        }
    }

    #[test]
    fn test_all_known_scopes_matches_default() {
        assert_eq!(all_known_scopes(), default_scopes());
    }

    #[test]
    fn test_token_serialization_roundtrip() {
        let token = make_token(0, 3600);
        let json = serde_json::to_string(&token).unwrap();
        let parsed: TokenSet = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.access_token, "test");
        assert_eq!(parsed.token_type, "Bearer");
    }
}

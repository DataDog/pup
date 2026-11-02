use crate::config::Config;
use crate::formatter;
use crate::util_ext;
use anyhow::Result;
use datadog_api_client::datadogV1::api_slack_integration::SlackIntegrationAPI;
use datadog_api_client::datadogV1::api_webhooks_integration::WebhooksIntegrationAPI;
use datadog_api_client::datadogV2::api_jira_integration::JiraIntegrationAPI;
use datadog_api_client::datadogV2::api_service_now_integration::ServiceNowIntegrationAPI;
use datadog_api_client::datadogV2::model::{
    JiraIssueTemplateCreateRequest, JiraIssueTemplateUpdateRequest,
    ServiceNowTemplateCreateRequest, ServiceNowTemplateUpdateRequest,
};

// ---- Jira ----

pub async fn jira_accounts_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(JiraIntegrationAPI, cfg);
    let resp = api
        .list_jira_accounts()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Jira accounts: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn jira_templates_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(JiraIntegrationAPI, cfg);
    let resp = api
        .list_jira_issue_templates()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Jira templates: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn jira_templates_get(cfg: &Config, template_id: &str) -> Result<()> {
    let api = crate::make_api!(JiraIntegrationAPI, cfg);
    let uuid = util_ext::parse_uuid(template_id, "template")?;
    let resp = api
        .get_jira_issue_template(uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get Jira template: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn jira_accounts_delete(cfg: &Config, account_id: &str) -> Result<()> {
    let api = crate::make_api!(JiraIntegrationAPI, cfg);
    let uuid = util_ext::parse_uuid(account_id, "account")?;
    api.delete_jira_account(uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete Jira account: {e:?}"))?;
    println!("Jira account {account_id} deleted.");
    Ok(())
}

pub async fn jira_templates_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(JiraIntegrationAPI, cfg);
    let body: JiraIssueTemplateCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_jira_issue_template(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create Jira template: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn jira_templates_update(cfg: &Config, template_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(JiraIntegrationAPI, cfg);
    let uuid = util_ext::parse_uuid(template_id, "template")?;
    let body: JiraIssueTemplateUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_jira_issue_template(uuid, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update Jira template: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn jira_templates_delete(cfg: &Config, template_id: &str) -> Result<()> {
    let api = crate::make_api!(JiraIntegrationAPI, cfg);
    let uuid = util_ext::parse_uuid(template_id, "template")?;
    api.delete_jira_issue_template(uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete Jira template: {e:?}"))?;
    println!("Jira template {template_id} deleted.");
    Ok(())
}

// ---- ServiceNow ----

pub async fn servicenow_instances_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let resp = api
        .list_service_now_instances()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list ServiceNow instances: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn servicenow_templates_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let resp = api
        .list_service_now_templates()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list ServiceNow templates: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn servicenow_templates_get(cfg: &Config, template_id: &str) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let uuid = util_ext::parse_uuid(template_id, "template")?;
    let resp = api
        .get_service_now_template(uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get ServiceNow template: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn servicenow_templates_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let body: ServiceNowTemplateCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_service_now_template(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create ServiceNow template: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn servicenow_templates_update(
    cfg: &Config,
    template_id: &str,
    file: &str,
) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let uuid = util_ext::parse_uuid(template_id, "template")?;
    let body: ServiceNowTemplateUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_service_now_template(uuid, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update ServiceNow template: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn servicenow_templates_delete(cfg: &Config, template_id: &str) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let uuid = util_ext::parse_uuid(template_id, "template")?;
    api.delete_service_now_template(uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete ServiceNow template: {e:?}"))?;
    println!("ServiceNow template {template_id} deleted.");
    Ok(())
}

pub async fn servicenow_users_list(cfg: &Config, instance_name: &str) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let resp = api
        .list_service_now_users(util_ext::parse_uuid(instance_name, "instance")?)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list ServiceNow users: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn servicenow_assignment_groups_list(cfg: &Config, instance_name: &str) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let resp = api
        .list_service_now_assignment_groups(util_ext::parse_uuid(instance_name, "instance")?)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list ServiceNow assignment groups: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn servicenow_business_services_list(cfg: &Config, instance_name: &str) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let resp = api
        .list_service_now_business_services(util_ext::parse_uuid(instance_name, "instance")?)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list ServiceNow business services: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Slack ----

pub async fn slack_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(SlackIntegrationAPI, cfg);
    let resp = api
        .get_slack_integration_channels("main".to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Slack channels: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- PagerDuty ----

pub async fn pagerduty_list(_cfg: &Config) -> Result<()> {
    anyhow::bail!(
        "listing PagerDuty services is not supported by the current API version \
         - use 'get' with a specific service name instead"
    )
}

// ---- Webhooks ----

pub async fn webhooks_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(WebhooksIntegrationAPI, cfg);
    let resp = api
        .get_webhooks_integration("main".to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list webhooks: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Integrations catalog, configuration status, and health ----

/// Documented catalog endpoint. Carries every integration manifest plus an
/// `installed` flag, but accepts no query parameters, so filtering is local.
const CATALOG_PATH: &str = "/api/v2/integrations";

/// Undocumented, but the only endpoint that filters installed integrations
/// server-side and the more accurate of the two `installed` sources. Used to
/// correct the catalog's flag, never as the catalog itself.
const INSTALLED_PATH: &str = "/api/v1/integration";

/// Health endpoints. These are UI-internal: undocumented, authenticated with
/// the same credentials as the public API, and absent on some sites. Every
/// probe fails soft so health checks degrade to "no health data" instead of
/// failing the whole command.
const AWS_ISSUES_PATH: &str = "/api/ui/integration/aws/integrationissues/status";
const AWS_ACCOUNT_ISSUES_PATH: &str = "/api/ui/integration/aws/integrationissues/account";
const GCP_ISSUES_PATH: &str = "/api/ui/v2/integration/gcp/issues";
const OCI_ISSUES_PATH: &str = "/api/ui/integration/oci/issues";

const AWS_ID: &str = "amazon-web-services";
const GCP_ID: &str = "google-cloud-platform";
const OCI_ID: &str = "oracle-cloud-infrastructure";

const STATUS_CONFIGURED: &str = "configured";
const STATUS_AVAILABLE: &str = "available";

const HEALTH_OK: &str = "ok";
const HEALTH_ERROR: &str = "error";

/// Caps how many problems are reported per integration so one badly broken
/// account cannot bury the rest of the output. Truncation is reported.
const MAX_ISSUES_PER_INTEGRATION: usize = 10;

/// Customer-facing columns. The catalog `id` is omitted as an internal
/// handle, and `issues` because the table truncates long cells -- the problem
/// text is printed in full underneath the table instead. Both stay in the
/// JSON/YAML/CSV payload for agents.
const LIST_COLUMNS: &[&str] = &["name", "status", "manage"];

/// Used instead of [`LIST_COLUMNS`] when a health probe actually ran, so the
/// `health` column is shown only when it can say something.
const LIST_COLUMNS_WITH_HEALTH: &[&str] = &["name", "status", "health", "manage"];

/// Integrations pup can configure and manage, mapped to the command that does
/// it. Drives the `manage` field and `--manageable`. Keys are catalog ids.
const MANAGED_BY: &[(&str, &str)] = &[
    (AWS_ID, "pup cloud aws --help"),
    ("azure", "pup cloud azure --help"),
    ("cloudflare", "pup integrations cloudflare accounts list"),
    (
        "confluent-cloud",
        "pup integrations confluent-cloud accounts list",
    ),
    ("custom-webhooks", "pup integrations webhooks list"),
    ("databricks", "pup integrations databricks accounts list"),
    (
        "elastic-cloud",
        "pup integrations elastic-cloud accounts list",
    ),
    ("fastly", "pup integrations fastly accounts list"),
    (GCP_ID, "pup cloud gcp --help"),
    (
        "google-hangouts-chat",
        "pup integrations google-chat handles list",
    ),
    ("jira", "pup integrations jira accounts list"),
    ("microsoft-teams", "pup integrations ms-teams handles list"),
    (OCI_ID, "pup cloud oci --help"),
    ("okta", "pup integrations okta accounts list"),
    (
        "salesforce",
        "pup integrations salesforce organizations list",
    ),
    ("servicenow", "pup integrations servicenow instances list"),
    ("slack", "pup integrations slack list"),
    ("snowflake-web", "pup integrations snowflake accounts list"),
    ("statuspage", "pup integrations statuspage account get"),
    ("twilio", "pup integrations twilio accounts list"),
];

/// Integrations whose health pup can actually determine. Everything else
/// reports no issues because no key-authenticated API exposes its health.
const HEALTH_COVERAGE: &[&str] = &[AWS_ID, GCP_ID, OCI_ID];

#[derive(serde::Deserialize)]
struct CatalogResponse {
    #[serde(default)]
    data: Vec<CatalogEntry>,
}

#[derive(serde::Deserialize)]
struct CatalogEntry {
    #[serde(default)]
    id: String,
    #[serde(default)]
    attributes: CatalogAttributes,
}

#[derive(Default, serde::Deserialize)]
struct CatalogAttributes {
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    installed: bool,
    #[serde(default)]
    title: String,
}

#[derive(serde::Deserialize)]
struct InstalledResponse {
    #[serde(default)]
    integrations: Vec<InstalledEntry>,
}

#[derive(serde::Deserialize)]
struct InstalledEntry {
    #[serde(default)]
    integration_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    installed: bool,
}

/// One row of `pup integrations list`. Deliberately excludes the catalog's
/// `links.self` (a web-ui route, not an API resource) and the non-`Category::`
/// classifier tags, neither of which is meaningful to a customer.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
struct IntegrationRow {
    id: String,
    name: String,
    /// Whether the integration is set up in this organization: `configured` or
    /// `available`. Independent of health -- a configured integration can be
    /// unhealthy.
    status: &'static str,
    /// `ok` or `error` when a health probe ran, `None` when no API reports
    /// this integration's health.
    health: Option<&'static str>,
    /// Actionable problems found by a health probe. Empty when the integration
    /// is healthy *or* when no API reports its health; `health_checked`
    /// distinguishes the two.
    issues: Vec<String>,
    /// Whether pup was able to determine this integration's health at all.
    health_checked: bool,
    /// The pup command that configures this integration, when one exists.
    manage: Option<&'static str>,
    #[serde(skip_serializing_if = "String::is_empty")]
    description: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    categories: Vec<String>,
}

/// Keeps only the customer-facing `Category::` tags and strips the prefix.
/// `Offering::`, `Supported OS::`, and `Submitted Data Type::` are internal
/// classifiers and are dropped entirely.
fn clean_categories(raw: &[String]) -> Vec<String> {
    raw.iter()
        .filter_map(|c| c.strip_prefix("Category::"))
        .map(str::to_string)
        .collect()
}

fn manage_command(id: &str) -> Option<&'static str> {
    MANAGED_BY
        .iter()
        .find(|(key, _)| *key == id)
        .map(|(_, cmd)| *cmd)
}

fn matches_search(row: &IntegrationRow, needle: &str) -> bool {
    let needle = needle.to_lowercase();
    row.id.to_lowercase().contains(&needle) || row.name.to_lowercase().contains(&needle)
}

fn row_status(configured: bool) -> &'static str {
    if configured {
        STATUS_CONFIGURED
    } else {
        STATUS_AVAILABLE
    }
}

/// Health is reported only for integrations a probe actually covered; everything
/// else is `None` rather than a misleading `ok`.
fn row_health(health_checked: bool, issues: &[String]) -> Option<&'static str> {
    if !issues.is_empty() {
        Some(HEALTH_ERROR)
    } else if health_checked {
        Some(HEALTH_OK)
    } else {
        None
    }
}

/// Merges the catalog with the more accurate installed set and any health
/// findings into the rows the command prints.
///
/// The two `installed` sources disagree (each reports integrations the other
/// misses), so configured status is their union: treating either as
/// authoritative would silently drop configured integrations.
fn build_rows(
    catalog: &[CatalogEntry],
    installed: &[InstalledEntry],
    issues: &std::collections::BTreeMap<String, Vec<String>>,
    health_checked: &std::collections::BTreeSet<String>,
) -> Vec<IntegrationRow> {
    let installed_ids: std::collections::BTreeSet<&str> = installed
        .iter()
        .filter(|e| e.installed)
        .map(|e| e.integration_id.as_str())
        .collect();

    let mut rows: Vec<IntegrationRow> = catalog
        .iter()
        .map(|entry| {
            let configured =
                entry.attributes.installed || installed_ids.contains(entry.id.as_str());
            let found = issues.get(&entry.id).cloned().unwrap_or_default();
            let checked = health_checked.contains(&entry.id);
            IntegrationRow {
                status: row_status(configured),
                health: row_health(checked, &found),
                issues: found,
                health_checked: checked,
                manage: manage_command(&entry.id),
                name: if entry.attributes.title.is_empty() {
                    entry.id.clone()
                } else {
                    entry.attributes.title.clone()
                },
                description: entry.attributes.description.clone(),
                categories: clean_categories(&entry.attributes.categories),
                id: entry.id.clone(),
            }
        })
        .collect();

    // The catalog omits some ids that the installed endpoint knows about.
    // Without this, `--configured` would hide integrations that really are
    // configured just because the catalog has no manifest for them.
    let catalog_ids: std::collections::BTreeSet<&str> =
        catalog.iter().map(|e| e.id.as_str()).collect();
    for entry in installed.iter().filter(|e| e.installed) {
        if catalog_ids.contains(entry.integration_id.as_str()) {
            continue;
        }
        let found = issues
            .get(&entry.integration_id)
            .cloned()
            .unwrap_or_default();
        let checked = health_checked.contains(&entry.integration_id);
        rows.push(IntegrationRow {
            status: row_status(true),
            health: row_health(checked, &found),
            issues: found,
            health_checked: checked,
            manage: manage_command(&entry.integration_id),
            name: if entry.name.is_empty() {
                entry.integration_id.clone()
            } else {
                entry.name.clone()
            },
            description: String::new(),
            categories: Vec::new(),
            id: entry.integration_id.clone(),
        });
    }

    rows.sort_by(|a, b| a.id.cmp(&b.id));
    rows
}

// ---- Health probes ----

#[derive(serde::Deserialize)]
struct GcpIssuesResponse {
    #[serde(default)]
    data: Vec<GcpIssuesEntry>,
}

#[derive(serde::Deserialize)]
struct GcpIssuesEntry {
    #[serde(default)]
    attributes: GcpIssuesAttributes,
}

#[derive(Default, serde::Deserialize)]
struct GcpIssuesAttributes {
    #[serde(default)]
    issues: Option<Vec<GcpIssue>>,
}

#[derive(serde::Deserialize)]
struct GcpIssue {
    #[serde(default)]
    message: String,
    #[serde(default)]
    severity: String,
    #[serde(default)]
    resource_container_id: String,
    #[serde(default)]
    remediation_link: String,
    #[serde(default)]
    remediation_note: Option<String>,
}

#[derive(serde::Deserialize)]
struct AwsIssuesStatusResponse {
    #[serde(default)]
    data: Option<AwsIssuesStatusEntry>,
}

#[derive(serde::Deserialize)]
struct AwsIssuesStatusEntry {
    #[serde(default)]
    attributes: AwsIssuesStatusAttributes,
}

#[derive(Default, serde::Deserialize)]
struct AwsIssuesStatusAttributes {
    #[serde(default)]
    account_level_issues: Option<Vec<AwsAccountLevelIssue>>,
}

#[derive(serde::Deserialize)]
struct AwsAccountLevelIssue {
    #[serde(default)]
    account_id: String,
}

#[derive(serde::Deserialize)]
struct AwsAccountIssuesResponse {
    #[serde(default)]
    data: Option<AwsAccountIssuesEntry>,
}

#[derive(serde::Deserialize)]
struct AwsAccountIssuesEntry {
    #[serde(default)]
    attributes: AwsAccountIssuesAttributes,
}

#[derive(Default, serde::Deserialize)]
struct AwsAccountIssuesAttributes {
    #[serde(default)]
    core_issues: Vec<AwsPermissionIssue>,
    #[serde(default)]
    standard_issues: Vec<AwsPermissionIssue>,
    #[serde(default)]
    extended_issues: Vec<AwsPermissionIssue>,
    #[serde(default)]
    log_collection_issues: Vec<AwsPermissionIssue>,
}

#[derive(serde::Deserialize)]
struct AwsPermissionIssue {
    #[serde(default)]
    permission: String,
}

#[derive(serde::Deserialize)]
struct OciIssuesResponse {
    #[serde(default)]
    data: Vec<OciIssuesEntry>,
}

#[derive(serde::Deserialize)]
struct OciIssuesEntry {
    #[serde(default)]
    id: String,
    #[serde(default)]
    attributes: OciIssuesAttributes,
}

#[derive(Default, serde::Deserialize)]
struct OciIssuesAttributes {
    #[serde(default)]
    critical: Vec<OciIssue>,
    #[serde(default)]
    warning: Vec<OciIssue>,
}

#[derive(serde::Deserialize)]
struct OciIssue {
    #[serde(default)]
    issue_type: String,
    #[serde(default)]
    value: String,
}

/// `None` when the response could not be decoded. An undecodable body is an
/// unknown answer, not a clean bill of health, and the caller keeps the
/// integration out of `checked` so it is never reported as `ok`.
fn gcp_messages(body: serde_json::Value) -> Option<Vec<String>> {
    let parsed = serde_json::from_value::<GcpIssuesResponse>(body).ok()?;
    let messages = parsed
        .data
        .into_iter()
        .filter_map(|entry| entry.attributes.issues)
        .flatten()
        .map(|issue| {
            let mut msg = String::new();
            if !issue.severity.is_empty() {
                msg.push_str(&format!("[{}] ", issue.severity));
            }
            if !issue.resource_container_id.is_empty() {
                msg.push_str(&format!("{}: ", issue.resource_container_id));
            }
            msg.push_str(&issue.message);
            if let Some(note) = issue.remediation_note.filter(|n| !n.is_empty()) {
                msg.push_str(&format!(" Fix: {note}"));
            }
            if !issue.remediation_link.is_empty() {
                msg.push_str(&format!(" See {}", issue.remediation_link));
            }
            msg
        })
        .collect();
    Some(messages)
}

/// `None` when the response could not be decoded; see [`gcp_messages`].
fn aws_flagged_accounts(body: serde_json::Value) -> Option<Vec<String>> {
    let parsed = serde_json::from_value::<AwsIssuesStatusResponse>(body).ok()?;
    let mut accounts: Vec<String> = parsed
        .data
        .and_then(|entry| entry.attributes.account_level_issues)
        .unwrap_or_default()
        .into_iter()
        .map(|issue| issue.account_id)
        .filter(|id| !id.is_empty())
        .collect();
    accounts.sort();
    accounts.dedup();
    Some(accounts)
}

/// `None` when the response could not be decoded; see [`gcp_messages`]. The
/// account reached this function because the summary already flagged it, so a
/// response carrying no detail still yields the generic issue rather than
/// erasing a problem the summary reported.
fn aws_account_messages(account_id: &str, body: serde_json::Value) -> Option<Vec<String>> {
    let parsed = serde_json::from_value::<AwsAccountIssuesResponse>(body).ok()?;
    let Some(attrs) = parsed.data.map(|entry| entry.attributes) else {
        return Some(vec![format!(
            "AWS account {account_id} has integration issues"
        )]);
    };
    let mut permissions: Vec<String> = [
        attrs.core_issues,
        attrs.standard_issues,
        attrs.extended_issues,
        attrs.log_collection_issues,
    ]
    .into_iter()
    .flatten()
    .map(|issue| issue.permission)
    .filter(|p| !p.is_empty())
    .collect();
    permissions.sort();
    permissions.dedup();

    if permissions.is_empty() {
        return Some(vec![format!(
            "AWS account {account_id} has integration issues"
        )]);
    }
    Some(vec![format!(
        "AWS account {account_id} is missing IAM permissions: {}. \
         Update the Datadog integration role policy to grant them.",
        permissions.join(", ")
    )])
}

/// `None` when the response could not be decoded; see [`gcp_messages`].
fn oci_messages(body: serde_json::Value) -> Option<Vec<String>> {
    let parsed = serde_json::from_value::<OciIssuesResponse>(body).ok()?;
    let mut messages = Vec::new();
    for entry in parsed.data {
        let tenancy = if entry.id.is_empty() {
            "tenancy".to_string()
        } else {
            entry.id.clone()
        };
        for (severity, issues) in [
            ("critical", entry.attributes.critical),
            ("warning", entry.attributes.warning),
        ] {
            for issue in issues {
                messages.push(format!(
                    "[{severity}] {tenancy}: {} {}",
                    issue.issue_type, issue.value
                ));
            }
        }
    }
    Some(messages)
}

/// Outcome of probing the integrations whose health pup can read: the problems
/// found, and which integrations the probes actually answered for. A probe that
/// errors leaves its integration out of `checked` so the command can say
/// "unknown" instead of implying "healthy".
#[derive(Default)]
struct HealthReport {
    issues: std::collections::BTreeMap<String, Vec<String>>,
    checked: std::collections::BTreeSet<String>,
}

async fn collect_health(cfg: &Config) -> HealthReport {
    let mut report = HealthReport::default();

    match crate::raw_client::raw_get(cfg, GCP_ISSUES_PATH, &[]).await {
        Ok(body) => match gcp_messages(body) {
            Some(messages) => {
                report.checked.insert(GCP_ID.to_string());
                if !messages.is_empty() {
                    report.issues.insert(GCP_ID.to_string(), messages);
                }
            }
            None => warn_decode("Google Cloud"),
        },
        Err(e) => warn_probe("Google Cloud", &e),
    }

    match aws_issues_get(cfg, AWS_ISSUES_PATH, &[]).await {
        Ok(body) => match aws_flagged_accounts(body) {
            Some(accounts) => {
                report.checked.insert(AWS_ID.to_string());
                let mut messages = Vec::new();
                for account_id in accounts {
                    // The summary already flagged this account, so a detail
                    // call that fails or cannot be decoded still reports the
                    // problem -- only the remediation text is lost.
                    let detail = aws_issues_get(
                        cfg,
                        AWS_ACCOUNT_ISSUES_PATH,
                        &[("aws_account_id", account_id.as_str())],
                    )
                    .await
                    .ok()
                    .and_then(|body| aws_account_messages(&account_id, body));
                    match detail {
                        Some(account_messages) => messages.extend(account_messages),
                        None => messages.push(format!(
                            "AWS account {account_id} has integration issues (details unavailable)"
                        )),
                    }
                }
                if !messages.is_empty() {
                    report.issues.insert(AWS_ID.to_string(), messages);
                }
            }
            None => warn_decode("AWS"),
        },
        Err(e) => warn_probe("AWS", &e),
    }

    match crate::raw_client::raw_get(cfg, OCI_ISSUES_PATH, &[]).await {
        Ok(body) => match oci_messages(body) {
            Some(messages) => {
                report.checked.insert(OCI_ID.to_string());
                if !messages.is_empty() {
                    report.issues.insert(OCI_ID.to_string(), messages);
                }
            }
            None => warn_decode("Oracle Cloud Infrastructure"),
        },
        Err(e) => {
            // A 404 here means no OCI tenancy is configured, which is a
            // definitive "nothing wrong", not a failed probe.
            if is_not_found(&e) {
                report.checked.insert(OCI_ID.to_string());
            }
            warn_probe("Oracle Cloud Infrastructure", &e);
        }
    }

    for messages in report.issues.values_mut() {
        if messages.len() > MAX_ISSUES_PER_INTEGRATION {
            let hidden = messages.len() - MAX_ISSUES_PER_INTEGRATION;
            messages.truncate(MAX_ISSUES_PER_INTEGRATION);
            messages.push(format!("... and {hidden} more"));
        }
    }

    report
}

/// The AWS issues endpoints require a call-source header that `raw_get` does
/// not set.
async fn aws_issues_get(
    cfg: &Config,
    path: &str,
    query: &[(&str, &str)],
) -> Result<serde_json::Value> {
    let resp = crate::raw_client::raw_request(
        cfg,
        "GET",
        path,
        query,
        None,
        None,
        "application/json",
        &[("Dd-Aws-Api-Call-Source", "ui")],
    )
    .await?;
    if resp.bytes.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    Ok(serde_json::from_slice(&resp.bytes)?)
}

fn is_not_found(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<crate::raw_client::HttpError>()
        .is_some_and(|http| http.status == 404)
}

/// A 404 means the integration is not configured on this org, which is not
/// worth reporting; anything else may hide a real problem, so it is surfaced.
fn warn_probe(label: &str, error: &anyhow::Error) {
    if is_not_found(error) {
        return;
    }
    eprintln!("warning: could not read {label} integration health: {error}");
}

/// A probe that answers with a body pup cannot decode leaves health unknown,
/// not healthy. The integration stays out of `HealthReport::checked`, so it is
/// reported as unknown rather than `ok` and silently excluded from `--broken`.
fn warn_decode(label: &str) {
    eprintln!(
        "warning: could not interpret the {label} integration health response; \
         reporting its health as unknown"
    );
}

// ---- Integrations list ----

async fn fetch_catalog(cfg: &Config) -> Result<Vec<CatalogEntry>> {
    let body = crate::raw_client::raw_get(cfg, CATALOG_PATH, &[])
        .await
        .map_err(|e| anyhow::anyhow!("failed to list integrations: {e}"))?;
    let parsed: CatalogResponse = serde_json::from_value(body)
        .map_err(|e| anyhow::anyhow!("failed to parse integrations catalog: {e}"))?;
    Ok(parsed.data)
}

/// Best-effort: the installed endpoint is undocumented, so a failure degrades
/// to the catalog's own `installed` flag rather than failing the command.
async fn fetch_installed(cfg: &Config) -> Vec<InstalledEntry> {
    match crate::raw_client::raw_get(cfg, INSTALLED_PATH, &[("installed", "true")]).await {
        // A shape change here would otherwise be silent, and every integration
        // would be reported as `available` -- wrong in the most misleading
        // direction. Say so instead.
        Ok(body) => match serde_json::from_value::<InstalledResponse>(body) {
            Ok(parsed) => parsed.integrations,
            Err(e) => {
                eprintln!("warning: could not parse installed integrations: {e}");
                Vec::new()
            }
        },
        Err(e) => {
            eprintln!("warning: could not read installed integrations: {e}");
            Vec::new()
        }
    }
}

pub async fn list(
    cfg: &Config,
    configured: bool,
    available: bool,
    broken: bool,
    manageable: bool,
    search: Option<&str>,
) -> Result<()> {
    let catalog = fetch_catalog(cfg).await?;

    // The catalog's own `installed` flag misses integrations the v1 endpoint
    // knows about, so configured status is always corrected against it.
    let installed = fetch_installed(cfg).await;

    // Health costs three more requests and only answers for three
    // integrations, so it is probed only for the views that report on it.
    let health = if configured || broken {
        collect_health(cfg).await
    } else {
        HealthReport::default()
    };

    let rows = build_rows(&catalog, &installed, &health.issues, &health.checked);
    let rows: Vec<IntegrationRow> = rows
        .into_iter()
        .filter(|row| !configured || row.status != STATUS_AVAILABLE)
        .filter(|row| !available || row.status == STATUS_AVAILABLE)
        .filter(|row| !broken || row.health == Some(HEALTH_ERROR))
        .filter(|row| !manageable || row.manage.is_some())
        .filter(|row| search.is_none_or(|needle| matches_search(row, needle)))
        .collect();

    let columns = if health.checked.is_empty() {
        LIST_COLUMNS
    } else {
        LIST_COLUMNS_WITH_HEALTH
    };

    // The empty-set message and the issue details below the table are prose for
    // a human reading a rendered table. With `--jq` the caller asked for a
    // projection, so the formatter handles the rows -- including an empty set,
    // which jq turns into `0` or `[]` rather than a sentence -- and the details
    // are suppressed because they would describe rows the projection dropped.
    let human_table = cfg.output_format == crate::config::OutputFormat::Table && cfg.jq.is_none();

    if human_table && rows.is_empty() {
        println!("No results found");
    } else {
        formatter::output_with_table(cfg, &rows, formatter::TableOptions::new(columns))?;
        if human_table {
            print_issue_details(&rows);
        }
    }

    // On stderr in every format, so a piped or machine-read `--broken` result
    // still carries the caveat without anything non-parseable on stdout.
    if broken {
        eprintln!("{}", health_coverage_note(&health));
    }
    Ok(())
}

/// The table truncates long cells, so the full remediation text is printed
/// underneath where a customer can act on it.
fn print_issue_details(rows: &[IntegrationRow]) {
    let with_issues: Vec<&IntegrationRow> = rows.iter().filter(|r| !r.issues.is_empty()).collect();
    if with_issues.is_empty() {
        return;
    }
    println!();
    for row in with_issues {
        println!("{} ({}):", row.name, row.id);
        for issue in &row.issues {
            println!("  - {issue}");
        }
        if let Some(cmd) = row.manage {
            println!("  manage with: {cmd}");
        }
    }
}

/// States plainly which integrations the health answer actually covers, so an
/// empty `--broken` result is not mistaken for "everything is healthy".
fn health_coverage_note(health: &HealthReport) -> String {
    let unchecked: Vec<&str> = HEALTH_COVERAGE
        .iter()
        .copied()
        .filter(|id| !health.checked.contains(*id))
        .collect();
    let mut note = format!(
        "note: health data is only available for {}; \
         other integrations are never reported as broken.",
        HEALTH_COVERAGE.join(", ")
    );
    if !unchecked.is_empty() {
        note.push_str(&format!(
            " Health could not be read for: {}.",
            unchecked.join(", ")
        ));
    }
    note
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_integrations_jira_accounts_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::jira_accounts_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_integrations_jira_templates_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::jira_templates_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_integrations_servicenow_instances_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::servicenow_instances_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_integrations_servicenow_templates_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::servicenow_templates_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_integrations_slack_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::slack_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_integrations_webhooks_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::webhooks_list(&cfg).await;
        cleanup_env();
    }

    // ---- integrations list ----

    fn catalog_entry(id: &str, title: &str, installed: bool) -> super::CatalogEntry {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "attributes": {
                "categories": ["Category::Cloud", "Offering::Integration"],
                "description": "desc",
                "installed": installed,
                "title": title,
            }
        }))
        .unwrap()
    }

    fn installed_entry(id: &str, name: &str, installed: bool) -> super::InstalledEntry {
        serde_json::from_value(serde_json::json!({
            "integration_id": id,
            "name": name,
            "installed": installed,
        }))
        .unwrap()
    }

    #[test]
    fn test_clean_categories_drops_internal_classifiers() {
        let raw = vec![
            "Category::Cloud".to_string(),
            "Offering::Integration".to_string(),
            "Supported OS::Linux".to_string(),
            "Submitted Data Type::Metrics".to_string(),
        ];
        assert_eq!(super::clean_categories(&raw), vec!["Cloud".to_string()]);
    }

    #[test]
    fn test_build_rows_unions_both_installed_signals() {
        let catalog = vec![
            catalog_entry("slack", "Slack", true),
            catalog_entry("jira", "Jira", false),
            catalog_entry("okta", "Okta", false),
        ];
        let installed = vec![
            installed_entry("jira", "Jira", true),
            installed_entry("okta", "Okta", false),
        ];
        let rows = super::build_rows(
            &catalog,
            &installed,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeSet::new(),
        );
        let status = |id: &str| rows.iter().find(|r| r.id == id).unwrap().status;
        assert_eq!(status("slack"), super::STATUS_CONFIGURED);
        assert_eq!(status("jira"), super::STATUS_CONFIGURED);
        assert_eq!(status("okta"), super::STATUS_AVAILABLE);
    }

    #[test]
    fn test_build_rows_backfills_ids_missing_from_catalog() {
        let rows = super::build_rows(
            &[catalog_entry("slack", "Slack", false)],
            &[installed_entry("pagerduty", "PagerDuty", true)],
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeSet::new(),
        );
        let row = rows.iter().find(|r| r.id == "pagerduty").unwrap();
        assert_eq!(row.status, super::STATUS_CONFIGURED);
        assert_eq!(row.name, "PagerDuty");
    }

    #[test]
    fn test_build_rows_marks_issues_as_error() {
        let mut issues = std::collections::BTreeMap::new();
        issues.insert(
            super::AWS_ID.to_string(),
            vec!["missing permission".to_string()],
        );
        let rows = super::build_rows(
            &[catalog_entry(super::AWS_ID, "AWS", true)],
            &[],
            &issues,
            &std::collections::BTreeSet::from([super::AWS_ID.to_string()]),
        );
        // Configuration state and health are independent: a broken integration
        // is still configured.
        assert_eq!(rows[0].status, super::STATUS_CONFIGURED);
        assert_eq!(rows[0].health, Some(super::HEALTH_ERROR));
        assert!(rows[0].health_checked);
        assert_eq!(rows[0].issues, vec!["missing permission".to_string()]);
        assert_eq!(rows[0].manage, Some("pup cloud aws --help"));
    }

    #[test]
    fn test_build_rows_configured_without_health_check_is_not_error() {
        let rows = super::build_rows(
            &[catalog_entry(super::AWS_ID, "AWS", true)],
            &[],
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeSet::new(),
        );
        assert_eq!(rows[0].status, super::STATUS_CONFIGURED);
        // No probe ran, so health is unknown rather than a misleading `ok`.
        assert_eq!(rows[0].health, None);
        assert!(!rows[0].health_checked);
    }

    #[test]
    fn test_row_health_reports_ok_only_when_probed() {
        assert_eq!(super::row_health(true, &[]), Some(super::HEALTH_OK));
        assert_eq!(super::row_health(false, &[]), None);
        assert_eq!(
            super::row_health(true, &["boom".to_string()]),
            Some(super::HEALTH_ERROR)
        );
    }

    #[test]
    fn test_scope_flags_partition_the_catalog() {
        let rows = super::build_rows(
            &[
                catalog_entry(super::AWS_ID, "AWS", true),
                catalog_entry("zoom", "Zoom", false),
            ],
            &[],
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeSet::new(),
        );
        let configured: Vec<_> = rows
            .iter()
            .filter(|r| r.status != super::STATUS_AVAILABLE)
            .map(|r| r.id.as_str())
            .collect();
        let available: Vec<_> = rows
            .iter()
            .filter(|r| r.status == super::STATUS_AVAILABLE)
            .map(|r| r.id.as_str())
            .collect();
        assert_eq!(configured, vec![super::AWS_ID]);
        assert_eq!(available, vec!["zoom"]);
    }

    #[test]
    fn test_health_column_only_shown_when_probed() {
        assert!(!super::LIST_COLUMNS.contains(&"health"));
        assert!(super::LIST_COLUMNS_WITH_HEALTH.contains(&"health"));
        // The catalog id is an internal handle and stays out of both views.
        assert!(!super::LIST_COLUMNS.contains(&"id"));
        assert!(!super::LIST_COLUMNS_WITH_HEALTH.contains(&"id"));
    }

    #[test]
    fn test_matches_search_is_case_insensitive_on_id_and_name() {
        let rows = super::build_rows(
            &[catalog_entry(
                "google-cloud-platform",
                "Google Cloud",
                false,
            )],
            &[],
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeSet::new(),
        );
        assert!(super::matches_search(&rows[0], "GOOGLE cloud"));
        assert!(super::matches_search(&rows[0], "PLATFORM"));
        assert!(!super::matches_search(&rows[0], "azure"));
    }

    #[test]
    fn test_gcp_messages_includes_remediation() {
        let body = serde_json::json!({
            "data": [{
                "attributes": {
                    "issues": [{
                        "message": "Missing role",
                        "severity": "critical",
                        "resource_container_id": "proj-1",
                        "remediation_link": "https://example.test/fix",
                        "remediation_note": "grant roles/viewer"
                    }]
                }
            }]
        });
        let messages = super::gcp_messages(body).expect("decodable");
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("[critical]"));
        assert!(messages[0].contains("proj-1"));
        assert!(messages[0].contains("grant roles/viewer"));
        assert!(messages[0].contains("https://example.test/fix"));
    }

    #[test]
    fn test_gcp_messages_tolerates_null_issues() {
        let body = serde_json::json!({"data": [{"attributes": {"issues": null}}]});
        assert_eq!(super::gcp_messages(body), Some(Vec::new()));
    }

    #[test]
    fn test_aws_flagged_accounts_dedupes() {
        let body = serde_json::json!({
            "data": {"attributes": {"account_level_issues": [
                {"account_id": "111", "issue_category": 0},
                {"account_id": "111", "issue_category": 1},
                {"account_id": "222", "issue_category": 0}
            ]}}
        });
        assert_eq!(
            super::aws_flagged_accounts(body),
            Some(vec!["111".to_string(), "222".to_string()])
        );
    }

    #[test]
    fn test_aws_flagged_accounts_empty_when_no_issues() {
        let body = serde_json::json!({"data": {"attributes": {"account_level_issues": null}}});
        assert_eq!(super::aws_flagged_accounts(body), Some(Vec::new()));
    }

    #[test]
    fn test_aws_account_messages_lists_permissions() {
        let body = serde_json::json!({
            "data": {"attributes": {
                "core_issues": [{"permission": "ec2:Describe*", "last_reported_at": "now"}],
                "log_collection_issues": [{"permission": "s3:GetObject", "last_reported_at": "now"}]
            }}
        });
        let messages = super::aws_account_messages("111", body).expect("decodable");
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("ec2:Describe*"));
        assert!(messages[0].contains("s3:GetObject"));
    }

    #[test]
    fn test_oci_messages_buckets_by_severity() {
        let body = serde_json::json!({
            "data": [{
                "id": "tenancy-1",
                "attributes": {
                    "critical": [{"issue_type": "auth", "value": "key expired"}],
                    "warning": [{"issue_type": "quota", "value": "near limit"}]
                }
            }]
        });
        let messages = super::oci_messages(body).expect("decodable");
        assert_eq!(messages.len(), 2);
        assert!(messages[0].contains("[critical] tenancy-1: auth key expired"));
        assert!(messages[1].contains("[warning]"));
    }

    /// An undecodable body must not read as "no issues": that would mark the
    /// integration healthy and hide it from `--broken`.
    #[test]
    fn test_health_parsers_report_undecodable_bodies_as_unknown() {
        let garbage = serde_json::json!({"data": "not the expected shape"});
        assert_eq!(super::gcp_messages(garbage.clone()), None);
        assert_eq!(super::aws_flagged_accounts(garbage.clone()), None);
        assert_eq!(super::aws_account_messages("111", garbage.clone()), None);
        assert_eq!(super::oci_messages(garbage), None);
    }

    /// The summary flagged this account, so detail that carries no issues must
    /// still surface the problem rather than erase it.
    #[test]
    fn test_aws_account_messages_keeps_flag_when_detail_is_absent() {
        let messages = super::aws_account_messages("111", serde_json::json!({"data": null}))
            .expect("decodable");
        assert_eq!(
            messages,
            vec!["AWS account 111 has integration issues".to_string()]
        );
    }

    #[test]
    fn test_health_coverage_note_reports_unchecked_probes() {
        let mut health = super::HealthReport::default();
        health.checked.insert(super::AWS_ID.to_string());
        let note = super::health_coverage_note(&health);
        assert!(note.contains(super::GCP_ID));
        assert!(note.contains("Health could not be read for"));
    }

    #[tokio::test]
    async fn test_integrations_list_catalog_default() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(
            &mut s,
            r#"{"data": [{"id": "slack", "attributes": {"title": "Slack", "installed": false}}]}"#,
        )
        .await;
        super::list(&cfg, false, false, false, false, None)
            .await
            .unwrap();
        cleanup_env();
    }

    #[tokio::test]
    async fn test_integrations_list_configured_filter() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": [], "integrations": []}"#).await;
        super::list(&cfg, true, false, false, false, None)
            .await
            .unwrap();
        cleanup_env();
    }

    #[tokio::test]
    async fn test_integrations_list_broken_filter() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": [], "integrations": []}"#).await;
        super::list(&cfg, false, false, true, false, None)
            .await
            .unwrap();
        cleanup_env();
    }

    /// With `--jq` the rows go to the formatter in every format, so the
    /// projection is applied instead of being short-circuited by the
    /// "No results found" message or decorated with unfiltered issue details.
    #[tokio::test]
    async fn test_integrations_list_table_with_jq_goes_through_the_formatter() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let mut cfg = test_config(&s.url());
        cfg.output_format = crate::config::OutputFormat::Table;
        cfg.jq = Some("length".to_string());
        mock_all(&mut s, r#"{"data": [], "integrations": []}"#).await;
        super::list(&cfg, false, false, false, false, None)
            .await
            .unwrap();
        cleanup_env();
    }

    #[tokio::test]
    async fn test_integrations_list_catalog_error_is_reported() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(500)
            .with_body("boom")
            .create_async()
            .await;
        let err = super::list(&cfg, false, false, false, false, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("failed to list integrations"));
        cleanup_env();
    }
}

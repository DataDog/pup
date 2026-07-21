use crate::commands::ddsql;
use crate::config::Config;
use crate::formatter;
use crate::util;
use crate::util_ext;
use anyhow::Result;
use datadog_api_client::datadogV2::api_application_security::ApplicationSecurityAPI;
use datadog_api_client::datadogV2::api_entity_risk_scores::{
    EntityRiskScoresAPI, ListEntityRiskScoresOptionalParams,
};
use datadog_api_client::datadogV2::api_restriction_policies::{
    RestrictionPoliciesAPI, UpdateRestrictionPolicyOptionalParams,
};
use datadog_api_client::datadogV2::api_security_monitoring::{
    GetIndicatorOfCompromiseOptionalParams, ListFindingsOptionalParams,
    ListIndicatorsOfCompromiseOptionalParams, ListSecurityMonitoringRulesOptionalParams,
    ListSecurityMonitoringSuppressionsOptionalParams,
    SearchSecurityMonitoringSignalsOptionalParams, SecurityMonitoringAPI,
};
use datadog_api_client::datadogV2::model::{
    ApplicationSecurityWafCustomRuleCreateRequest, ApplicationSecurityWafCustomRuleUpdateRequest,
    ApplicationSecurityWafExclusionFilterCreateRequest,
    ApplicationSecurityWafExclusionFilterUpdateRequest, MuteFindingsRequest,
    RestrictionPolicyUpdateRequest, SecurityMonitoringRuleBulkExportAttributes,
    SecurityMonitoringRuleBulkExportData, SecurityMonitoringRuleBulkExportDataType,
    SecurityMonitoringRuleBulkExportPayload, SecurityMonitoringRuleConvertBulkPayload,
    SecurityMonitoringRuleConvertPayload, SecurityMonitoringRuleSort,
    SecurityMonitoringSignalListRequest, SecurityMonitoringSignalListRequestFilter,
    SecurityMonitoringSignalListRequestPage, SecurityMonitoringSignalsSort,
    SecurityMonitoringSuppressionCreateRequest, SecurityMonitoringSuppressionSort,
    SecurityMonitoringSuppressionUpdateRequest, SecurityMonitoringTerraformBulkExportRequest,
    SecurityMonitoringTerraformConvertRequest, SecurityMonitoringTerraformResourceType,
};

const SCHEMA_URL: &str = "https://docs.datadoghq.com/security/guide/findings-schema.md";
const SCHEMA_SECTION_MARKER: &str = "## Schema Reference";

/// Fetch the security findings schema reference from Datadog docs.
///
/// Downloads the markdown page at runtime, extracts everything after
/// "## Schema Reference", and strips template directives ({% ... %})
/// so the output is clean, readable plaintext/markdown.
async fn fetch_schema_markdown() -> Result<String> {
    let resp = reqwest::Client::new()
        .get(SCHEMA_URL)
        .header("User-Agent", crate::useragent::get())
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("failed to fetch schema from {SCHEMA_URL}: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        anyhow::bail!("failed to fetch schema from {SCHEMA_URL} (HTTP {status})");
    }

    let body = resp
        .text()
        .await
        .map_err(|e| anyhow::anyhow!("failed to read schema response: {e}"))?;

    // Extract everything after "## Schema Reference"
    let schema_section = body
        .find(SCHEMA_SECTION_MARKER)
        .map(|pos| &body[pos..])
        .ok_or_else(|| {
            anyhow::anyhow!(
                "schema page did not contain expected section '{SCHEMA_SECTION_MARKER}'"
            )
        })?;

    // Trim trailing sections that aren't part of the schema reference
    let schema_section = schema_section
        .find("## Further reading")
        .map(|pos| schema_section[..pos].trim_end())
        .unwrap_or(schema_section);

    // Strip template directives: {% ... %}
    let mut cleaned = strip_template_directives(schema_section);

    // Add source attribution
    cleaned.push_str("\n\n---\n*This schema was fetched from Datadog public documentation.*\n");

    Ok(cleaned)
}

/// Remove template directives like {% collapsible-section %}, {% /collapsible-section %},
/// {% callout %}, {% /callout %}, {% tab %}, etc. Also removes lines that become empty
/// after stripping.
fn strip_template_directives(input: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    for line in input.lines() {
        let trimmed = line.trim();
        // Skip lines that are entirely a template directive
        if trimmed.starts_with("{%") && trimmed.ends_with("%}") {
            continue;
        }
        // Strip inline template directives (e.g., "## Schema Reference{% #schema-reference %}")
        if let Some(pos) = line.find("{%") {
            let cleaned = line[..pos].trim_end();
            if !cleaned.is_empty() {
                lines.push(cleaned);
            }
        } else {
            lines.push(line);
        }
    }
    lines.join("\n")
}

pub async fn findings_schema(cfg: &Config) -> Result<()> {
    let schema = fetch_schema_markdown().await?;

    if cfg.agent_mode {
        eprintln!(
            "Use these fields with `pup security findings analyze --query \"SELECT ... FROM dd.security_findings(...)\"`"
        );
    }

    println!("{schema}");
    Ok(())
}

// ---- Findings Analyze ----

pub async fn findings_analyze(
    cfg: &Config,
    query: &str,
    from: &str,
    to: &str,
    limit: i64,
) -> Result<()> {
    if !query.contains("dd.security_findings") {
        eprintln!("Warning: query doesn't use dd.security_findings(). Did you mean to use `pup ddsql table`?");
    }

    match ddsql::execute_ddsql_query_with_command(
        cfg,
        query,
        from,
        to,
        Some(limit as i32),
        Some("security-findings-analyze"),
    )
    .await
    {
        Ok(rows) => formatter::output(cfg, &rows),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("400") || msg.contains("Bad Request") {
                eprintln!("Error: Invalid query. Check that:");
                eprintln!("  - Column names in ARRAY use @ prefix (e.g., @severity, not severity)");
                eprintln!(
                    "  - AS clause types are valid (VARCHAR, BIGINT, DECIMAL, BOOLEAN, TIMESTAMP)"
                );
                eprintln!("  - Column count in ARRAY matches the AS clause");
                eprintln!("  - Field names are valid — common fields:");
                eprintln!("      @severity  @status  @finding_type  @rule.name  @title");
                eprintln!("      @resource_name  @resource_type  @compliance.evaluation");
                eprintln!("      @severity_details.adjusted.score  @risk.is_production");
                eprintln!("  - Run `pup security findings schema` for the full field reference");
                eprintln!();
                eprintln!("Raw API error: {msg}");
            } else {
                eprintln!(
                    "Hint: run `pup security findings schema` to see available fields and types."
                );
            }
            Err(e)
        }
    }
}

pub async fn rules_list(
    cfg: &Config,
    filter: Option<String>,
    sort: Option<String>,
    page_size: i64,
    page_number: i64,
) -> Result<()> {
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let mut params = ListSecurityMonitoringRulesOptionalParams::default()
        .page_size(page_size)
        .page_number(page_number);
    if let Some(s) = sort {
        params = params.sort(parse_rule_sort(&s));
    }
    if let Some(f) = filter {
        params = params.query(f);
    }
    let resp = api
        .list_security_monitoring_rules(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list rules: {e:?}"))?;
    formatter::output(cfg, &resp)
}

fn parse_rule_sort(s: &str) -> SecurityMonitoringRuleSort {
    match s {
        "name" => SecurityMonitoringRuleSort::NAME,
        "-name" => SecurityMonitoringRuleSort::NAME_DESCENDING,
        "creation_date" => SecurityMonitoringRuleSort::CREATION_DATE,
        "-creation_date" => SecurityMonitoringRuleSort::CREATION_DATE_DESCENDING,
        "update_date" => SecurityMonitoringRuleSort::UPDATE_DATE,
        "-update_date" => SecurityMonitoringRuleSort::UPDATE_DATE_DESCENDING,
        "enabled" => SecurityMonitoringRuleSort::ENABLED,
        "-enabled" => SecurityMonitoringRuleSort::ENABLED_DESCENDING,
        "type" => SecurityMonitoringRuleSort::TYPE,
        "-type" => SecurityMonitoringRuleSort::TYPE_DESCENDING,
        "highest_severity" => SecurityMonitoringRuleSort::HIGHEST_SEVERITY,
        "-highest_severity" => SecurityMonitoringRuleSort::HIGHEST_SEVERITY_DESCENDING,
        "source" => SecurityMonitoringRuleSort::SOURCE,
        "-source" => SecurityMonitoringRuleSort::SOURCE_DESCENDING,
        _ => SecurityMonitoringRuleSort::NAME,
    }
}

pub async fn rules_get(cfg: &Config, rule_id: &str) -> Result<()> {
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let resp = api
        .get_security_monitoring_rule(rule_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn signals_search(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    limit: i32,
    sort: Option<String>,
) -> Result<()> {
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);

    let from_dt = util_ext::parse_time_to_datetime(&from)?;
    let to_dt = util_ext::parse_time_to_datetime(&to)?;

    let sort_val = match sort.as_deref().unwrap_or("-timestamp") {
        "timestamp" | "asc" => SecurityMonitoringSignalsSort::TIMESTAMP_ASCENDING,
        "-timestamp" | "desc" => SecurityMonitoringSignalsSort::TIMESTAMP_DESCENDING,
        other => anyhow::bail!(
            "invalid --sort value: {other:?}\nExpected: timestamp (ascending) or -timestamp (descending)"
        ),
    };

    let body = SecurityMonitoringSignalListRequest::new()
        .filter(
            SecurityMonitoringSignalListRequestFilter::new()
                .query(query)
                .from(from_dt)
                .to(to_dt),
        )
        .page(SecurityMonitoringSignalListRequestPage::new().limit(limit))
        .sort(sort_val);

    let params = SearchSecurityMonitoringSignalsOptionalParams::default().body(body);
    let resp = api
        .search_security_monitoring_signals(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search signals: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn signals_investigation_queries(cfg: &Config, signal_id: &str) -> Result<()> {
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let resp = api
        .get_investigation_log_queries_matching_signal(signal_id.to_string())
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "failed to get investigation log queries for signal '{signal_id}': {e:?}"
            )
        })?;
    formatter::output(cfg, &resp)
}

pub async fn signals_suggested_actions(cfg: &Config, signal_id: &str) -> Result<()> {
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let resp = api
        .get_suggested_actions_matching_signal(signal_id.to_string())
        .await
        .map_err(|e| {
            anyhow::anyhow!("failed to get suggested actions for signal '{signal_id}': {e:?}")
        })?;
    formatter::output(cfg, &resp)
}

pub async fn findings_search(cfg: &Config, query: Option<String>, limit: i64) -> Result<()> {
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let mut params = ListFindingsOptionalParams::default().page_limit(limit);
    if let Some(q) = query {
        params = params.filter_tags(q);
    }
    let resp = api
        .list_findings(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search findings: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Mute Findings ----

/// Mute or unmute security findings (stable, SDK #1519/#1660).
/// Accepts up to 100 finding IDs per request. The `--file` must contain a
/// JSON body shaped as `MuteFindingsRequest` (see Datadog docs).
pub async fn findings_mute(cfg: &Config, file: &str) -> Result<()> {
    let body: MuteFindingsRequest = util::read_json_file(file)?;
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let resp = api
        .mute_security_findings(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to mute findings: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Bulk Export ----

pub async fn rules_bulk_export(cfg: &Config, rule_ids: Vec<String>) -> Result<()> {
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let attrs = SecurityMonitoringRuleBulkExportAttributes::new(rule_ids);
    let data = SecurityMonitoringRuleBulkExportData::new(
        attrs,
        SecurityMonitoringRuleBulkExportDataType::SECURITY_MONITORING_RULES_BULK_EXPORT,
    );
    let body = SecurityMonitoringRuleBulkExportPayload::new(data);
    let resp = api
        .bulk_export_security_monitoring_rules(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to bulk export security rules: {e:?}"))?;
    // resp is Vec<u8> (ZIP data), output as raw bytes to stdout
    let output = String::from_utf8_lossy(&resp);
    println!("{output}");
    Ok(())
}

// ---- Terraform export ----

fn parse_terraform_resource_type(s: &str) -> Result<SecurityMonitoringTerraformResourceType> {
    Ok(match s {
        "suppressions" => SecurityMonitoringTerraformResourceType::SUPPRESSIONS,
        "critical_assets" => SecurityMonitoringTerraformResourceType::CRITICAL_ASSETS,
        _ => {
            anyhow::bail!("invalid resource-type '{s}' — use one of: suppressions, critical_assets")
        }
    })
}

pub async fn rules_to_terraform(cfg: &Config, file: &str) -> Result<()> {
    let body: SecurityMonitoringRuleConvertPayload = util::read_json_file(file)?;
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let resp = api
        .convert_security_monitoring_rule_from_json_to_terraform(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to convert rule to Terraform: {e:?}"))?;
    formatter::output(cfg, &resp)
}

/// Bulk convert existing security monitoring rules to Terraform (SDK #1675).
/// The `--file` must contain a JSON body shaped as
/// `SecurityMonitoringRuleConvertBulkPayload`. Returns a ZIP archive written
/// to stdout (pipe to a file if you want to save it).
pub async fn rules_bulk_convert(cfg: &Config, file: &str) -> Result<()> {
    let body: SecurityMonitoringRuleConvertBulkPayload = util::read_json_file(file)?;
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let bytes = api
        .bulk_convert_existing_security_monitoring_rules(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to bulk convert security rules: {e:?}"))?;
    let output = String::from_utf8_lossy(&bytes);
    println!("{output}");
    Ok(())
}

pub async fn terraform_export(cfg: &Config, resource_type: &str, resource_id: &str) -> Result<()> {
    let rt = parse_terraform_resource_type(resource_type)?;
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let resp = api
        .export_security_monitoring_terraform_resource(rt, resource_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to export Terraform resource: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn terraform_bulk_export(
    cfg: &Config,
    resource_type: &str,
    file: &str,
    output_file: &str,
) -> Result<()> {
    let rt = parse_terraform_resource_type(resource_type)?;
    let body: SecurityMonitoringTerraformBulkExportRequest = util::read_json_file(file)?;
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let bytes = api
        .bulk_export_security_monitoring_terraform_resources(rt, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to bulk export Terraform resources: {e:?}"))?;
    std::fs::write(output_file, &bytes)
        .map_err(|e| anyhow::anyhow!("failed to write to '{output_file}': {e}"))?;
    eprintln!(
        "Wrote {} bytes (zip archive) to '{output_file}'.",
        bytes.len()
    );
    Ok(())
}

pub async fn terraform_convert(cfg: &Config, resource_type: &str, file: &str) -> Result<()> {
    let rt = parse_terraform_resource_type(resource_type)?;
    let body: SecurityMonitoringTerraformConvertRequest = util::read_json_file(file)?;
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let resp = api
        .convert_security_monitoring_terraform_resource(rt, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to convert Terraform resource: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Content Packs ----

pub async fn content_packs_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let resp = api
        .get_content_packs_states()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list content packs: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn content_packs_activate(cfg: &Config, pack_id: &str) -> Result<()> {
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    api.activate_content_pack(pack_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to activate content pack: {e:?}"))?;
    println!("Content pack '{pack_id}' activated successfully.");
    Ok(())
}

pub async fn content_packs_deactivate(cfg: &Config, pack_id: &str) -> Result<()> {
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    api.deactivate_content_pack(pack_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to deactivate content pack: {e:?}"))?;
    println!("Content pack '{pack_id}' deactivated successfully.");
    Ok(())
}

// ---- Indicators of Compromise ----

pub async fn iocs_list(
    cfg: &Config,
    query: Option<String>,
    limit: Option<i32>,
    offset: Option<i32>,
    sort_column: Option<String>,
    sort_order: Option<String>,
) -> Result<()> {
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let mut params = ListIndicatorsOfCompromiseOptionalParams::default();
    if let Some(q) = query {
        params.query = Some(q);
    }
    if let Some(l) = limit {
        params.limit = Some(l);
    }
    if let Some(o) = offset {
        params.offset = Some(o);
    }
    if let Some(c) = sort_column {
        params.sort_column = Some(c);
    }
    if let Some(o) = sort_order {
        params.sort_order = Some(o);
    }
    let resp = api
        .list_indicators_of_compromise(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list indicators of compromise: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn iocs_get(cfg: &Config, indicator: &str) -> Result<()> {
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let resp = api
        .get_indicator_of_compromise(
            indicator.to_string(),
            GetIndicatorOfCompromiseOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to get indicator of compromise: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Risk Scores ----

pub async fn risk_scores_list(cfg: &Config, query: Option<String>) -> Result<()> {
    let api = crate::make_api!(EntityRiskScoresAPI, cfg);
    let mut params = ListEntityRiskScoresOptionalParams::default();
    if let Some(q) = query {
        params = params.filter_query(q);
    }
    let resp = api
        .list_entity_risk_scores(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list entity risk scores: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Suppressions ----

fn parse_suppression_sort(s: &str) -> SecurityMonitoringSuppressionSort {
    match s {
        "name" => SecurityMonitoringSuppressionSort::NAME,
        "-name" => SecurityMonitoringSuppressionSort::NAME_DESCENDING,
        "start_date" => SecurityMonitoringSuppressionSort::START_DATE,
        "-start_date" => SecurityMonitoringSuppressionSort::START_DATE_DESCENDING,
        "expiration_date" => SecurityMonitoringSuppressionSort::EXPIRATION_DATE,
        "-expiration_date" => SecurityMonitoringSuppressionSort::EXPIRATION_DATE_DESCENDING,
        "update_date" => SecurityMonitoringSuppressionSort::UPDATE_DATE,
        "-update_date" => SecurityMonitoringSuppressionSort::UPDATE_DATE_DESCENDING,
        "-creation_date" => SecurityMonitoringSuppressionSort::CREATION_DATE_DESCENDING,
        "enabled" => SecurityMonitoringSuppressionSort::ENABLED,
        "-enabled" => SecurityMonitoringSuppressionSort::ENABLED_DESCENDING,
        _ => SecurityMonitoringSuppressionSort::NAME,
    }
}

pub async fn suppressions_list(cfg: &Config, sort: Option<String>) -> Result<()> {
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let mut params = ListSecurityMonitoringSuppressionsOptionalParams::default();
    if let Some(s) = sort {
        params = params.sort(parse_suppression_sort(&s));
    }
    let resp = api
        .list_security_monitoring_suppressions(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list suppressions: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn suppressions_get(cfg: &Config, suppression_id: &str) -> Result<()> {
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let resp = api
        .get_security_monitoring_suppression(suppression_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get suppression: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn suppressions_create(cfg: &Config, file: &str) -> Result<()> {
    let body: SecurityMonitoringSuppressionCreateRequest = util::read_json_file(file)?;
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let resp = api
        .create_security_monitoring_suppression(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create suppression: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn suppressions_update(cfg: &Config, suppression_id: &str, file: &str) -> Result<()> {
    let body: SecurityMonitoringSuppressionUpdateRequest = util::read_json_file(file)?;
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    let resp = api
        .update_security_monitoring_suppression(suppression_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update suppression: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn suppressions_delete(cfg: &Config, suppression_id: &str) -> Result<()> {
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    api.delete_security_monitoring_suppression(suppression_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete suppression: {e:?}"))?;
    println!("Suppression '{suppression_id}' deleted.");
    Ok(())
}

pub async fn suppressions_validate(cfg: &Config, file: &str) -> Result<()> {
    let body: SecurityMonitoringSuppressionCreateRequest = util::read_json_file(file)?;
    let api = crate::make_api!(SecurityMonitoringAPI, cfg);
    api.validate_security_monitoring_suppression(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to validate suppression: {e:?}"))?;
    println!("Suppression is valid.");
    Ok(())
}

// ---- ASM WAF Custom Rules ----

pub async fn asm_custom_rules_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api_no_auth!(ApplicationSecurityAPI, cfg);
    let resp = api
        .list_application_security_waf_custom_rules()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list ASM WAF custom rules: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn asm_custom_rules_get(cfg: &Config, custom_rule_id: &str) -> Result<()> {
    let api = crate::make_api_no_auth!(ApplicationSecurityAPI, cfg);
    let resp = api
        .get_application_security_waf_custom_rule(custom_rule_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get ASM WAF custom rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn asm_custom_rules_create(cfg: &Config, file: &str) -> Result<()> {
    let body: ApplicationSecurityWafCustomRuleCreateRequest = util::read_json_file(file)?;
    let api = crate::make_api_no_auth!(ApplicationSecurityAPI, cfg);
    let resp = api
        .create_application_security_waf_custom_rule(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create ASM WAF custom rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn asm_custom_rules_update(cfg: &Config, custom_rule_id: &str, file: &str) -> Result<()> {
    let body: ApplicationSecurityWafCustomRuleUpdateRequest = util::read_json_file(file)?;
    let api = crate::make_api_no_auth!(ApplicationSecurityAPI, cfg);
    let resp = api
        .update_application_security_waf_custom_rule(custom_rule_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update ASM WAF custom rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn asm_custom_rules_delete(cfg: &Config, custom_rule_id: &str) -> Result<()> {
    let api = crate::make_api_no_auth!(ApplicationSecurityAPI, cfg);
    api.delete_application_security_waf_custom_rule(custom_rule_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete ASM WAF custom rule: {e:?}"))?;
    println!("ASM WAF custom rule '{custom_rule_id}' deleted.");
    Ok(())
}

// ---- ASM WAF Exclusion Filters ----

pub async fn asm_exclusions_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api_no_auth!(ApplicationSecurityAPI, cfg);
    let resp = api
        .list_application_security_waf_exclusion_filters()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list ASM WAF exclusion filters: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn asm_exclusions_get(cfg: &Config, exclusion_filter_id: &str) -> Result<()> {
    let api = crate::make_api_no_auth!(ApplicationSecurityAPI, cfg);
    let resp = api
        .get_application_security_waf_exclusion_filter(exclusion_filter_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get ASM WAF exclusion filter: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn asm_exclusions_create(cfg: &Config, file: &str) -> Result<()> {
    let body: ApplicationSecurityWafExclusionFilterCreateRequest = util::read_json_file(file)?;
    let api = crate::make_api_no_auth!(ApplicationSecurityAPI, cfg);
    let resp = api
        .create_application_security_waf_exclusion_filter(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create ASM WAF exclusion filter: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn asm_exclusions_update(
    cfg: &Config,
    exclusion_filter_id: &str,
    file: &str,
) -> Result<()> {
    let body: ApplicationSecurityWafExclusionFilterUpdateRequest = util::read_json_file(file)?;
    let api = crate::make_api_no_auth!(ApplicationSecurityAPI, cfg);
    let resp = api
        .update_application_security_waf_exclusion_filter(exclusion_filter_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update ASM WAF exclusion filter: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn asm_exclusions_delete(cfg: &Config, exclusion_filter_id: &str) -> Result<()> {
    let api = crate::make_api_no_auth!(ApplicationSecurityAPI, cfg);
    api.delete_application_security_waf_exclusion_filter(exclusion_filter_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete ASM WAF exclusion filter: {e:?}"))?;
    println!("ASM WAF exclusion filter '{exclusion_filter_id}' deleted.");
    Ok(())
}

// ---- Restriction Policies ----

pub async fn restriction_policy_get(cfg: &Config, resource_id: &str) -> Result<()> {
    let api = crate::make_api!(RestrictionPoliciesAPI, cfg);
    let resp = api
        .get_restriction_policy(resource_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get restriction policy: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn restriction_policy_update(cfg: &Config, resource_id: &str, file: &str) -> Result<()> {
    let body: RestrictionPolicyUpdateRequest = util::read_json_file(file)?;
    let api = crate::make_api!(RestrictionPoliciesAPI, cfg);
    let resp = api
        .update_restriction_policy(
            resource_id.to_string(),
            body,
            UpdateRestrictionPolicyOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to update restriction policy: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn restriction_policy_delete(cfg: &Config, resource_id: &str) -> Result<()> {
    let api = crate::make_api!(RestrictionPoliciesAPI, cfg);
    api.delete_restriction_policy(resource_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete restriction policy: {e:?}"))?;
    println!("Restriction policy for '{resource_id}' deleted.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    use super::*;

    #[test]
    fn test_strip_template_directives_removes_standalone() {
        let input = "## Heading\n{% collapsible-section #foo %}\n### Sub\nContent here.\n{% /collapsible-section %}";
        let result = strip_template_directives(input);
        assert_eq!(result, "## Heading\n### Sub\nContent here.");
    }

    #[test]
    fn test_strip_template_directives_removes_inline() {
        let input = "## Schema Reference{% #schema-reference %}\nSome text.";
        let result = strip_template_directives(input);
        assert_eq!(result, "## Schema Reference\nSome text.");
    }

    #[test]
    fn test_strip_template_directives_preserves_tables() {
        let input = "| Name | Type |\n| ---- | ---- |\n| `severity` | string |";
        let result = strip_template_directives(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_strip_template_directives_empty_input() {
        assert_eq!(strip_template_directives(""), "");
    }

    #[test]
    fn test_strip_template_directives_mixed() {
        let input = "Line 1\n{% tab title=\"Foo\" %}\nLine 2\n{% /tab %}\nLine 3";
        let result = strip_template_directives(input);
        assert_eq!(result, "Line 1\nLine 2\nLine 3");
    }

    #[tokio::test]
    async fn test_security_rules_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::rules_list(&cfg, None, None, 10, 0).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_security_rules_list_with_pagination() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::rules_list(
            &cfg,
            Some("test".to_string()),
            Some("name".to_string()),
            50,
            2,
        )
        .await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_security_rules_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::rules_get(&cfg, "r1").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_security_content_packs_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::content_packs_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_security_iocs_list() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"data":{},"meta":{}}"#).await;
        let result = super::iocs_list(&cfg, None, None, None, None, None).await;
        assert!(result.is_ok(), "iocs list failed: {:?}", result.err());
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_security_iocs_list_with_params() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"data":{},"meta":{}}"#).await;
        let result = super::iocs_list(
            &cfg,
            Some("indicator_type:ip".to_string()),
            Some(50),
            Some(100),
            Some("score".to_string()),
            Some("desc".to_string()),
        )
        .await;
        assert!(
            result.is_ok(),
            "iocs list with params failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_security_iocs_list_error() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Forbidden"]}"#)
            .create_async()
            .await;
        let result = super::iocs_list(&cfg, None, None, None, None, None).await;
        assert!(result.is_err(), "expected error for 403 response");
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_security_iocs_get() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"data":{}}"#).await;
        let result = super::iocs_get(&cfg, "1.2.3.4").await;
        assert!(result.is_ok(), "iocs get failed: {:?}", result.err());
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_security_iocs_get_not_found() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Not found"]}"#)
            .create_async()
            .await;
        let result = super::iocs_get(&cfg, "missing").await;
        assert!(result.is_err(), "expected error for 404 response");
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_security_rules_to_terraform() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let tmp = write_temp_json("pup_test_rule_to_tf.json", r#"{"rule":{}}"#);
        let _mock = mock_any(
            &mut server,
            "POST",
            r#"{"rule_id":"abc","resource":"resource \"x\" \"y\" {}"}"#,
        )
        .await;
        let result = super::rules_to_terraform(&cfg, tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "rules_to_terraform failed: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_security_terraform_export() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{}"#).await;
        let result = super::terraform_export(&cfg, "suppressions", "abc-123").await;
        assert!(
            result.is_ok(),
            "terraform_export failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_security_terraform_export_bad_resource_type() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused.local");
        let result = super::terraform_export(&cfg, "bogus", "abc-123").await;
        assert!(result.is_err(), "expected resource-type parse error");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid resource-type"));
    }

    #[tokio::test]
    async fn test_security_terraform_bulk_export() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let body_tmp = write_temp_json(
            "pup_test_tf_bulk.json",
            r#"{"data":{"type":"bulk_export_resources","attributes":{"resource_ids":["abc"]}}}"#,
        );
        let zip_bytes: &[u8] = b"PK\x03\x04fake-zip-bytes";
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/zip")
            .with_body(zip_bytes)
            .create_async()
            .await;
        let out = std::env::temp_dir().join("pup_test_tf_bulk_out.zip");
        let out_str = out.to_str().unwrap();
        let result = super::terraform_bulk_export(
            &cfg,
            "critical_assets",
            body_tmp.to_str().unwrap(),
            out_str,
        )
        .await;
        assert!(
            result.is_ok(),
            "terraform_bulk_export failed: {:?}",
            result.err()
        );
        let written = std::fs::read(&out).expect("expected output file to exist");
        assert_eq!(written, zip_bytes);
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(body_tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_security_terraform_bulk_export_bad_type() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused.local");
        let result = super::terraform_bulk_export(
            &cfg,
            "bogus",
            "/nonexistent/path.json",
            "/nonexistent/out.zip",
        )
        .await;
        assert!(result.is_err(), "expected resource-type parse error");
    }

    #[tokio::test]
    async fn test_security_terraform_convert() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let tmp = write_temp_json(
            "pup_test_tf_convert.json",
            r#"{"data":{"id":"abc","type":"convert_resource","attributes":{"resource_json":{}}}}"#,
        );
        let _mock = mock_any(&mut server, "POST", r#"{}"#).await;
        let result = super::terraform_convert(&cfg, "suppressions", tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "terraform_convert failed: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_security_terraform_convert_403() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let tmp = write_temp_json(
            "pup_test_tf_convert_403.json",
            r#"{"data":{"id":"abc","type":"convert_resource","attributes":{"resource_json":{}}}}"#,
        );
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Forbidden"]}"#)
            .create_async()
            .await;
        let result = super::terraform_convert(&cfg, "suppressions", tmp.to_str().unwrap()).await;
        assert!(result.is_err(), "expected 403 error");
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_asm_custom_rules_list() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
        let result = super::asm_custom_rules_list(&cfg).await;
        assert!(
            result.is_ok(),
            "ASM custom rules list failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_asm_custom_rules_list_error() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Forbidden"]}"#)
            .create_async()
            .await;
        let result = super::asm_custom_rules_list(&cfg).await;
        assert!(result.is_err(), "expected error for 403 response");
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_asm_exclusions_list() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
        let result = super::asm_exclusions_list(&cfg).await;
        assert!(
            result.is_ok(),
            "ASM exclusions list failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_restriction_policy_get() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(
            &mut server,
            "GET",
            r#"{"data":{"type":"restriction_policy","id":"dashboard:abc-123","attributes":{"bindings":[]}}}"#,
        )
        .await;
        let result = super::restriction_policy_get(&cfg, "dashboard:abc-123").await;
        assert!(
            result.is_ok(),
            "restriction policy get failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_restriction_policy_get_error() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Not Found"]}"#)
            .create_async()
            .await;
        let result = super::restriction_policy_get(&cfg, "dashboard:missing").await;
        assert!(result.is_err(), "expected error for 404 response");
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_signals_search_invalid_sort() {
        let cfg = test_config("http://unused.local");
        let result = super::signals_search(
            &cfg,
            "*".into(),
            "1h".into(),
            "now".into(),
            10,
            Some("invalid".into()),
        )
        .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid --sort value"));
    }

    #[tokio::test]
    async fn test_findings_mute_ok() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let tmp = write_temp_json(
            "pup_test_findings_mute.json",
            r#"{"data":{"type":"mute","attributes":{"mute":{"is_muted":true,"reason":"FALSE_POSITIVE"}},"relationships":{"findings":{"data":[]}}}}"#,
        );
        let _mock = mock_any(
            &mut server,
            "PATCH",
            r#"{"data":{"id":"mute-job-1","type":"mute_findings_response"}}"#,
        )
        .await;
        let result = super::findings_mute(&cfg, tmp.to_str().unwrap()).await;
        assert!(result.is_ok(), "findings_mute failed: {:?}", result.err());
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_findings_mute_error() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let tmp = write_temp_json(
            "pup_test_findings_mute_err.json",
            r#"{"data":{"type":"mute","attributes":{"mute":{"is_muted":true,"reason":"FALSE_POSITIVE"}},"relationships":{"findings":{"data":[]}}}}"#,
        );
        let _mock = server
            .mock("PATCH", mockito::Matcher::Any)
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Forbidden"]}"#)
            .create_async()
            .await;
        let result = super::findings_mute(&cfg, tmp.to_str().unwrap()).await;
        assert!(result.is_err(), "expected error for 403 response");
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_rules_bulk_convert_ok() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let tmp = write_temp_json(
            "pup_test_rules_bulk_convert.json",
            r#"{"data":{"type":"security_monitoring_rules_convert_bulk","attributes":{"ruleIds":["abc-123"]}}}"#,
        );
        let zip_bytes: &[u8] = b"PK\x03\x04fake-zip-bytes";
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/zip")
            .with_body(zip_bytes)
            .create_async()
            .await;
        let result = super::rules_bulk_convert(&cfg, tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "rules_bulk_convert failed: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_rules_bulk_convert_error() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let tmp = write_temp_json(
            "pup_test_rules_bulk_convert_err.json",
            r#"{"data":{"type":"security_monitoring_rules_convert_bulk","attributes":{"ruleIds":["bad"]}}}"#,
        );
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Bad Request"]}"#)
            .create_async()
            .await;
        let result = super::rules_bulk_convert(&cfg, tmp.to_str().unwrap()).await;
        assert!(result.is_err(), "expected error for 400 response");
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }
}

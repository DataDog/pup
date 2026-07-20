use crate::config::Config;
use crate::formatter;
use crate::util;
use crate::util_ext;
use anyhow::{bail, Result};
use datadog_api_client::datadogV2::api_status_pages::{
    CreateComponentOptionalParams, CreateDegradationOptionalParams,
    CreateMaintenanceOptionalParams, CreateStatusPageOptionalParams, GetComponentOptionalParams,
    GetDegradationOptionalParams, GetMaintenanceOptionalParams, GetStatusPageOptionalParams,
    ListComponentsOptionalParams, ListDegradationsOptionalParams, ListMaintenancesOptionalParams,
    ListStatusPagesOptionalParams, StatusPagesAPI, UpdateComponentOptionalParams,
    UpdateDegradationOptionalParams, UpdateMaintenanceOptionalParams,
    UpdateStatusPageOptionalParams,
};
use datadog_api_client::datadogV2::model::{
    CreateComponentRequest, CreateDegradationRequest, CreateMaintenanceRequest,
    CreateStatusPageRequest, PatchComponentRequest, PatchDegradationRequest,
    PatchMaintenanceRequest, PatchStatusPageRequest,
};

// ---------------------------------------------------------------------------
// Helper: build a StatusPagesAPI with bearer-token support
// ---------------------------------------------------------------------------

fn make_api(cfg: &Config) -> StatusPagesAPI {
    crate::make_api!(StatusPagesAPI, cfg)
}

pub async fn pages_list(cfg: &Config) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .list_status_pages(ListStatusPagesOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list status pages: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn pages_get(cfg: &Config, page_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let uuid = util_ext::parse_uuid(page_id, "page")?;
    let resp = api
        .get_status_page(uuid, GetStatusPageOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get status page: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn pages_delete(cfg: &Config, page_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let uuid = util_ext::parse_uuid(page_id, "page")?;
    api.delete_status_page(uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete status page: {e:?}"))?;
    println!("Status page {page_id} deleted.");
    Ok(())
}

pub async fn pages_update(cfg: &Config, page_id: &str, file: &str) -> Result<()> {
    let page_uuid = util_ext::parse_uuid(page_id, "page")?;
    let body: PatchStatusPageRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .update_status_page(page_uuid, body, UpdateStatusPageOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to update status page: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn components_list(cfg: &Config, page_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let uuid = util_ext::parse_uuid(page_id, "page")?;
    let resp = api
        .list_components(uuid, ListComponentsOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list components: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn components_get(cfg: &Config, page_id: &str, component_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let page_uuid = util_ext::parse_uuid(page_id, "page")?;
    let component_uuid = util_ext::parse_uuid(component_id, "component")?;
    let resp = api
        .get_component(
            page_uuid,
            component_uuid,
            GetComponentOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to get component: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn components_update(
    cfg: &Config,
    page_id: &str,
    component_id: &str,
    file: &str,
) -> Result<()> {
    let page_uuid = util_ext::parse_uuid(page_id, "page")?;
    let component_uuid = util_ext::parse_uuid(component_id, "component")?;
    let body: PatchComponentRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .update_component(
            page_uuid,
            component_uuid,
            body,
            UpdateComponentOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to update component: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn degradations_list(cfg: &Config) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .list_degradations(ListDegradationsOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list degradations: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn degradations_get(cfg: &Config, page_id: &str, degradation_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let page_uuid = util_ext::parse_uuid(page_id, "page")?;
    let degradation_uuid = util_ext::parse_uuid(degradation_id, "degradation")?;
    let resp = api
        .get_degradation(
            page_uuid,
            degradation_uuid,
            GetDegradationOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to get degradation: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn degradations_create(cfg: &Config, page_id: &str, file: &str) -> Result<()> {
    let page_uuid = util_ext::parse_uuid(page_id, "page")?;
    let body: CreateDegradationRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_degradation(page_uuid, body, CreateDegradationOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to create degradation: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn degradations_update(
    cfg: &Config,
    page_id: &str,
    degradation_id: &str,
    file: &str,
) -> Result<()> {
    let page_uuid = util_ext::parse_uuid(page_id, "page")?;
    let degradation_uuid = util_ext::parse_uuid(degradation_id, "degradation")?;
    let body: PatchDegradationRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .update_degradation(
            page_uuid,
            degradation_uuid,
            body,
            UpdateDegradationOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to update degradation: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn components_delete(cfg: &Config, page_id: &str, component_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let page_uuid = util_ext::parse_uuid(page_id, "page")?;
    let component_uuid = util_ext::parse_uuid(component_id, "component")?;
    api.delete_component(page_uuid, component_uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete component: {e:?}"))?;
    println!("Component {component_id} deleted from page {page_id}.");
    Ok(())
}

pub async fn degradations_delete(cfg: &Config, page_id: &str, degradation_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let page_uuid = util_ext::parse_uuid(page_id, "page")?;
    let degradation_uuid = util_ext::parse_uuid(degradation_id, "degradation")?;
    api.delete_degradation(page_uuid, degradation_uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete degradation: {e:?}"))?;
    println!("Degradation {degradation_id} deleted from page {page_id}.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Pages create
// ---------------------------------------------------------------------------

pub async fn pages_create(cfg: &Config, file: &str) -> Result<()> {
    let body: CreateStatusPageRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_status_page(body, CreateStatusPageOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to create status page: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Components create
// ---------------------------------------------------------------------------

pub async fn components_create(cfg: &Config, page_id: &str, file: &str) -> Result<()> {
    let page_uuid = util_ext::parse_uuid(page_id, "page")?;
    let body: CreateComponentRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_component(page_uuid, body, CreateComponentOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to create component: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Maintenances
// ---------------------------------------------------------------------------

pub async fn maintenances_list(cfg: &Config) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .list_maintenances(ListMaintenancesOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list maintenances: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn maintenances_get(cfg: &Config, page_id: &str, maintenance_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let page_uuid = util_ext::parse_uuid(page_id, "page")?;
    let maintenance_uuid = util_ext::parse_uuid(maintenance_id, "maintenance")?;
    let resp = api
        .get_maintenance(
            page_uuid,
            maintenance_uuid,
            GetMaintenanceOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to get maintenance: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn maintenances_create(cfg: &Config, page_id: &str, file: &str) -> Result<()> {
    let page_uuid = util_ext::parse_uuid(page_id, "page")?;
    let body: CreateMaintenanceRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_maintenance(page_uuid, body, CreateMaintenanceOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to create maintenance: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn maintenances_update(
    cfg: &Config,
    page_id: &str,
    maintenance_id: &str,
    file: &str,
) -> Result<()> {
    let page_uuid = util_ext::parse_uuid(page_id, "page")?;
    let maintenance_uuid = util_ext::parse_uuid(maintenance_id, "maintenance")?;
    let body: PatchMaintenanceRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .update_maintenance(
            page_uuid,
            maintenance_uuid,
            body,
            UpdateMaintenanceOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to update maintenance: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Third-party status pages (fetched from updog.ai, no DD auth needed)
// ---------------------------------------------------------------------------

use serde::Deserialize;

const THIRD_PARTY_OUTAGES_URL: &str = "https://updog.ai/data/third-party-outages.json";
const SPARKLINE_WIDTH: usize = 30;
const DAY_MS: i64 = 86_400_000;

const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_RED: &str = "\x1b[31m";
const ANSI_DIM: &str = "\x1b[2m";
const ANSI_RESET: &str = "\x1b[0m";

#[derive(Deserialize, serde::Serialize, Clone)]
struct ThirdPartyOutagesResponse {
    data: ThirdPartyOutagesData,
}

#[derive(Deserialize, serde::Serialize, Clone)]
struct ThirdPartyOutagesData {
    attributes: ThirdPartyOutagesAttributes,
}

#[derive(Deserialize, serde::Serialize, Clone)]
struct ThirdPartyOutagesAttributes {
    provider_data: Vec<ThirdPartyProvider>,
}

#[derive(Deserialize, serde::Serialize, Clone)]
struct ThirdPartyProvider {
    provider_name: String,
    #[serde(default)]
    provider_service: String,
    display_name: String,
    #[serde(default)]
    outages: Vec<ThirdPartyOutage>,
    monitoring_start_date: i64,
    // Remaining fields kept for JSON/YAML passthrough
    #[serde(skip_serializing_if = "Option::is_none")]
    integration_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    monitored_api_patterns: Vec<String>,
}

#[derive(Deserialize, serde::Serialize, Clone)]
struct ThirdPartyOutage {
    start: i64,
    #[serde(default)]
    end: i64,
    status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    impacted_region: String,
}

fn filter_providers(
    providers: Vec<ThirdPartyProvider>,
    search: Option<&str>,
    active_only: bool,
) -> Vec<ThirdPartyProvider> {
    providers
        .into_iter()
        .filter(|p| {
            if let Some(q) = search {
                let q = q.to_lowercase();
                if !p.provider_name.to_lowercase().contains(&q)
                    && !p.display_name.to_lowercase().contains(&q)
                {
                    return false;
                }
            }
            if active_only {
                let has_active = p.outages.iter().any(|o| o.status != "resolved");
                if !has_active {
                    return false;
                }
            }
            true
        })
        .collect()
}

fn provider_current_status(provider: &ThirdPartyProvider) -> &str {
    for outage in &provider.outages {
        if outage.status != "resolved" {
            return &outage.status;
        }
    }
    "operational"
}

fn build_sparkline(provider: &ThirdPartyProvider, now_ms: i64) -> String {
    let mut s = String::new();
    for i in (0..SPARKLINE_WIDTH).rev() {
        let bucket_start = now_ms - (i as i64 + 1) * DAY_MS;
        let bucket_end = now_ms - i as i64 * DAY_MS;

        if bucket_end <= provider.monitoring_start_date {
            s.push_str(ANSI_DIM);
            s.push('·');
            s.push_str(ANSI_RESET);
            continue;
        }

        let has_outage = provider.outages.iter().any(|o| {
            let outage_end = if o.end == 0 { now_ms } else { o.end };
            o.start < bucket_end && outage_end > bucket_start
        });

        if has_outage {
            s.push_str(ANSI_RED);
        } else {
            s.push_str(ANSI_GREEN);
        }
        s.push('█');
        s.push_str(ANSI_RESET);
    }
    s
}

fn format_third_party_table(providers: &[ThirdPartyProvider]) -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    // Compute visible column widths (ANSI codes must not influence these).
    let wp = providers
        .iter()
        .map(|p| p.provider_name.len())
        .max()
        .unwrap_or(0)
        .max("PROVIDER".len());
    let wd = providers
        .iter()
        .map(|p| p.display_name.len())
        .max()
        .unwrap_or(0)
        .max("DISPLAY NAME".len());
    let ws = providers
        .iter()
        .map(|p| p.provider_service.len())
        .max()
        .unwrap_or(0)
        .max("SERVICE".len());
    let wst = providers
        .iter()
        .map(|p| provider_current_status(p).len())
        .max()
        .unwrap_or(0)
        .max("STATUS".len());
    // UPTIME column is always exactly SPARKLINE_WIDTH visible chars.
    let wu = SPARKLINE_WIDTH;

    let sep = format!(
        "+-{}-+-{}-+-{}-+-{}-+-{}-+",
        "-".repeat(wp),
        "-".repeat(wd),
        "-".repeat(ws),
        "-".repeat(wu),
        "-".repeat(wst),
    );
    let hsep = sep.replace('-', "=");

    let mut s = String::new();
    s.push_str(&sep);
    s.push('\n');
    s.push_str(&format!(
        "| {:<wp$} | {:<wd$} | {:<ws$} | {:<wu$} | {:<wst$} |\n",
        "PROVIDER",
        "DISPLAY NAME",
        "SERVICE",
        "UPTIME",
        "STATUS",
        wp = wp,
        wd = wd,
        ws = ws,
        wu = wu,
        wst = wst,
    ));
    s.push_str(&hsep);
    s.push('\n');

    for p in providers {
        let sparkline = build_sparkline(p, now_ms);
        let status = provider_current_status(p);
        // The sparkline is exactly `wu` visible chars; ANSI bytes are invisible so
        // we cannot use a format-width specifier for it — assemble that column manually.
        s.push_str(&format!(
            "| {:<wp$} | {:<wd$} | {:<ws$} | {sparkline} | {:<wst$} |\n",
            p.provider_name,
            p.display_name,
            p.provider_service,
            status,
            wp = wp,
            wd = wd,
            ws = ws,
            wst = wst,
        ));
    }

    s.push_str(&sep);
    s
}

pub async fn third_party_list(cfg: &Config, search: Option<&str>, active: bool) -> Result<()> {
    let client = reqwest::Client::new();
    let resp = client
        .get(THIRD_PARTY_OUTAGES_URL)
        .header("Accept", "application/json")
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        bail!("failed to fetch third-party outages from updog.ai (HTTP {status})");
    }
    let parsed: ThirdPartyOutagesResponse = resp.json().await?;
    let providers = filter_providers(parsed.data.attributes.provider_data, search, active);

    if cfg.output_format == crate::config::OutputFormat::Table {
        if providers.is_empty() {
            println!("No results found");
        } else {
            println!("{}", format_third_party_table(&providers));
        }
        return Ok(());
    }

    formatter::output(cfg, &providers)
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    use super::*;

    fn make_provider(
        name: &str,
        display: &str,
        monitoring_start: i64,
        outages: Vec<ThirdPartyOutage>,
    ) -> ThirdPartyProvider {
        ThirdPartyProvider {
            provider_name: name.to_string(),
            display_name: display.to_string(),
            provider_service: String::new(),
            outages,
            monitoring_start_date: monitoring_start,
            integration_id: None,
            status_url: None,
            monitored_api_patterns: vec![],
        }
    }

    fn make_outage(start: i64, end: i64, status: &str) -> ThirdPartyOutage {
        ThirdPartyOutage {
            start,
            end,
            status: status.to_string(),
            impacted_region: String::new(),
        }
    }

    #[test]
    fn test_provider_current_status_operational() {
        let p = make_provider("a", "A", 0, vec![make_outage(1, 2, "resolved")]);
        assert_eq!(provider_current_status(&p), "operational");
    }

    #[test]
    fn test_provider_current_status_active() {
        let p = make_provider("a", "A", 0, vec![make_outage(1, 0, "active")]);
        assert_eq!(provider_current_status(&p), "active");
    }

    #[test]
    fn test_provider_current_status_no_outages() {
        let p = make_provider("a", "A", 0, vec![]);
        assert_eq!(provider_current_status(&p), "operational");
    }

    #[test]
    fn test_filter_no_filter() {
        let providers = vec![
            make_provider("aws-s3", "Amazon S3", 0, vec![]),
            make_provider("stripe", "Stripe", 0, vec![]),
        ];
        let result = filter_providers(providers, None, false);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_filter_by_provider_name() {
        let providers = vec![
            make_provider("aws-s3", "Amazon S3", 0, vec![]),
            make_provider("stripe", "Stripe", 0, vec![]),
        ];
        let result = filter_providers(providers, Some("aws"), false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].provider_name, "aws-s3");
    }

    #[test]
    fn test_filter_by_display_name() {
        let providers = vec![
            make_provider("aws-s3", "Amazon S3", 0, vec![]),
            make_provider("stripe", "Stripe", 0, vec![]),
        ];
        let result = filter_providers(providers, Some("amazon"), false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].provider_name, "aws-s3");
    }

    #[test]
    fn test_filter_active_only() {
        let providers = vec![
            make_provider("aws-s3", "Amazon S3", 0, vec![make_outage(1, 0, "active")]),
            make_provider("stripe", "Stripe", 0, vec![make_outage(1, 2, "resolved")]),
        ];
        let result = filter_providers(providers, None, true);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].provider_name, "aws-s3");
    }

    #[test]
    fn test_filter_no_match() {
        let providers = vec![make_provider("aws-s3", "Amazon S3", 0, vec![])];
        let result = filter_providers(providers, Some("nonexistent"), false);
        assert!(result.is_empty());
    }

    #[test]
    fn test_sparkline_all_green() {
        let now_ms = 1_705_276_800_000i64; // 2024-01-15 00:00:00 UTC
        let p = make_provider("test", "Test", now_ms - 60 * DAY_MS, vec![]);
        let sparkline = build_sparkline(&p, now_ms);
        assert!(!sparkline.contains(ANSI_RED));
        assert!(sparkline.contains(ANSI_GREEN));
        assert!(!sparkline.contains('·'));
    }

    #[test]
    fn test_sparkline_with_active_outage() {
        let now_ms = 1_705_276_800_000i64;
        let p = make_provider(
            "test",
            "Test",
            now_ms - 60 * DAY_MS,
            vec![make_outage(now_ms - DAY_MS / 2, 0, "active")],
        );
        let sparkline = build_sparkline(&p, now_ms);
        assert!(sparkline.contains(ANSI_RED));
    }

    #[test]
    fn test_sparkline_dim_dots_before_monitoring() {
        let now_ms = 1_705_276_800_000i64;
        // Monitoring started 10 days ago → first 20 buckets should be dim dots
        let p = make_provider("test", "Test", now_ms - 10 * DAY_MS, vec![]);
        let sparkline = build_sparkline(&p, now_ms);
        let plain: String = {
            let mut out = String::new();
            let mut in_escape = false;
            for c in sparkline.chars() {
                if c == '\x1b' {
                    in_escape = true;
                    continue;
                }
                if in_escape {
                    if c == 'm' {
                        in_escape = false;
                    }
                    continue;
                }
                out.push(c);
            }
            out
        };
        let dot_count = plain.chars().filter(|&c| c == '·').count();
        assert!(dot_count >= 19, "expected ≥19 dim dots, got {dot_count}");
        assert!(sparkline.contains(ANSI_DIM));
    }

    #[test]
    fn test_sparkline_outage_outside_window() {
        let now_ms = 1_705_276_800_000i64;
        // Outage 45 days ago — outside 30-day window
        let p = make_provider(
            "test",
            "Test",
            now_ms - 60 * DAY_MS,
            vec![make_outage(
                now_ms - 45 * DAY_MS,
                now_ms - 44 * DAY_MS,
                "resolved",
            )],
        );
        let sparkline = build_sparkline(&p, now_ms);
        assert!(!sparkline.contains(ANSI_RED));
    }

    #[test]
    fn test_format_third_party_table_headers() {
        let providers = vec![make_provider("aws-s3", "Amazon S3", 0, vec![])];
        let output = format_third_party_table(&providers);
        for header in &["PROVIDER", "DISPLAY NAME", "SERVICE", "UPTIME", "STATUS"] {
            assert!(output.contains(header), "missing header: {header}");
        }
    }

    #[test]
    fn test_format_third_party_table_data() {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let providers = vec![make_provider(
            "aws-s3",
            "Amazon S3",
            now_ms - 60 * DAY_MS,
            vec![make_outage(now_ms - DAY_MS / 2, 0, "active")],
        )];
        let output = format_third_party_table(&providers);
        assert!(output.contains("aws-s3"));
        assert!(output.contains("Amazon S3"));
        assert!(output.contains("active"));
        assert!(output.contains('█'));
    }

    #[tokio::test]
    async fn test_status_pages_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::pages_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_status_pages_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::pages_get(&cfg, "p1").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_status_pages_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let _ = super::pages_delete(&cfg, "p1").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_status_pages_components_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::components_list(&cfg, "p1").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_status_pages_degradations_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::degradations_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_status_pages_third_party_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::third_party_list(&cfg, None, false).await;
        cleanup_env();
    }
}

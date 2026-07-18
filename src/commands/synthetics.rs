use anyhow::Result;
use datadog_api_client::datadogV1::api_synthetics::{
    ListTestsOptionalParams, SearchTestsOptionalParams, SyntheticsAPI,
};
use datadog_api_client::datadogV2::api_synthetics::{
    GetSyntheticsBrowserTestResultOptionalParams, GetSyntheticsTestResultOptionalParams,
    GetSyntheticsTestVersionOptionalParams, ListSyntheticsBrowserTestLatestResultsOptionalParams,
    ListSyntheticsDowntimesOptionalParams, ListSyntheticsTestLatestResultsOptionalParams,
    ListSyntheticsTestVersionsOptionalParams, SearchSuitesOptionalParams,
    SyntheticsAPI as SyntheticsV2API,
};
use datadog_api_client::datadogV2::model::{
    DeletedSuitesRequestDelete, DeletedSuitesRequestDeleteAttributes,
    DeletedSuitesRequestDeleteRequest, SuiteCreateEditRequest, SyntheticsDowntimeRequest,
    SyntheticsTestResultRunType, SyntheticsTestResultStatus,
};

use crate::config::Config;
use crate::formatter;

fn synthetics_intake_base_url(cfg: &Config) -> String {
    if cfg.site == "datadoghq.com" || cfg.site == "datad0g.com" {
        format!("https://intake.synthetics.{}/api/v1", cfg.site)
    } else {
        format!("{}/api/v1", cfg.api_base_url())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn build_auth_headers(cfg: &Config) -> anyhow::Result<reqwest::header::HeaderMap> {
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
    let mut headers = HeaderMap::new();

    // Prefer OAuth2 bearer token, falling back to API + app keys.
    if let Some(token) = cfg.access_token.as_ref() {
        headers.insert(
            reqwest::header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))?,
        );
    } else if let (Some(api_key), Some(app_key)) = (cfg.api_key.as_ref(), cfg.app_key.as_ref()) {
        headers.insert(
            HeaderName::from_static("dd-api-key"),
            HeaderValue::from_str(api_key)?,
        );
        headers.insert(
            HeaderName::from_static("dd-application-key"),
            HeaderValue::from_str(app_key)?,
        );
    } else {
        anyhow::bail!(
            "'synthetics tests run' requires authentication: run 'pup auth login' or set DD_API_KEY and DD_APP_KEY"
        );
    }

    headers.insert(
        reqwest::header::USER_AGENT,
        HeaderValue::from_str(&crate::useragent::get())?,
    );
    Ok(headers)
}

const POLL_INTERVAL_SECS: u64 = 5;
const TRIGGER_APP: &str = "pup_cli";

#[cfg(not(target_arch = "wasm32"))]
pub async fn tests_run(
    cfg: &Config,
    public_ids: Vec<String>,
    use_tunnel: bool,
    timeout_secs: u64,
) -> Result<()> {
    if public_ids.is_empty() {
        anyhow::bail!("at least one public ID is required");
    }

    let auth_headers = build_auth_headers(cfg)?;
    let intake_url = synthetics_intake_base_url(cfg);
    let client = reqwest::Client::new();

    let active_tunnel = if use_tunnel {
        eprintln!(
            "Fetching tunnel presigned URL for {} test(s)...",
            public_ids.len()
        );
        let query: Vec<(&str, &str)> = public_ids
            .iter()
            .map(|id| ("test_id", id.as_str()))
            .collect();
        let tunnel_resp = client
            .get(format!("{intake_url}/synthetics/ci/tunnel"))
            .query(&query)
            .headers(auth_headers.clone())
            .send()
            .await?;
        if !tunnel_resp.status().is_success() {
            let status = tunnel_resp.status();
            let body = tunnel_resp.text().await.unwrap_or_default();
            anyhow::bail!("failed to get tunnel URL (HTTP {status}): {body}");
        }
        let tunnel_json: serde_json::Value = tunnel_resp.json().await?;
        let presigned_url = tunnel_json["url"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'url' in tunnel response"))?
            .to_string();

        eprintln!("Starting tunnel...");
        let (tunnel_info, tunnel) =
            crate::tunnel::Tunnel::start(&presigned_url, public_ids.clone()).await?;
        eprintln!("Tunnel connected (id: {})", tunnel_info.id);
        Some((tunnel_info, tunnel))
    } else {
        None
    };

    let tests_payload: Vec<serde_json::Value> = public_ids
        .iter()
        .map(|id| {
            let mut test = serde_json::json!({ "public_id": id });
            if let Some((ref info, _)) = active_tunnel {
                test["tunnel"] = serde_json::json!({
                    "id": info.id,
                    "host": info.host,
                    "privateKey": info.private_key,
                });
            }
            test
        })
        .collect();

    let trigger_payload = serde_json::json!({ "tests": tests_payload });

    eprintln!("Triggering {} test(s)...", public_ids.len());
    let trigger_resp = client
        .post(format!("{intake_url}/synthetics/tests/trigger/ci"))
        .headers(auth_headers.clone())
        .header("X-Trigger-App", TRIGGER_APP)
        .json(&trigger_payload)
        .send()
        .await?;
    if !trigger_resp.status().is_success() {
        let status = trigger_resp.status();
        let body = trigger_resp.text().await.unwrap_or_default();
        if let Some((_, tunnel)) = active_tunnel {
            tunnel.stop();
        }
        anyhow::bail!("failed to trigger tests (HTTP {status}): {body}");
    }
    let trigger_json: serde_json::Value = trigger_resp.json().await?;
    let batch_id = trigger_json["batch_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'batch_id' in trigger response"))?
        .to_string();
    eprintln!("Batch ID: {batch_id}");

    let poll_url = format!(
        "{}/api/v1/synthetics/ci/batch/{batch_id}",
        cfg.api_base_url()
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

    let final_result = loop {
        tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;

        let batch_resp = client
            .get(&poll_url)
            .headers(auth_headers.clone())
            .send()
            .await?;
        if !batch_resp.status().is_success() {
            let status = batch_resp.status();
            let body = batch_resp.text().await.unwrap_or_default();
            if let Some((_, tunnel)) = active_tunnel {
                tunnel.stop();
            }
            anyhow::bail!("failed to poll batch (HTTP {status}): {body}");
        }
        let batch_json: serde_json::Value = batch_resp.json().await?;
        let status = batch_json["data"]["status"].as_str().unwrap_or("unknown");
        eprintln!("Batch status: {status}");

        if status != "in_progress" {
            break batch_json;
        }
        if std::time::Instant::now() >= deadline {
            if let Some((_, tunnel)) = active_tunnel {
                tunnel.stop();
            }
            anyhow::bail!("timeout after {timeout_secs}s waiting for test results");
        }
    };

    if let Some((_, tunnel)) = active_tunnel {
        tunnel.stop();
    }
    let results = &final_result["data"]["results"];
    if cfg.output_format == crate::config::OutputFormat::Table {
        let table_rows: Vec<serde_json::Value> = results
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|r| {
                        serde_json::json!({
                            "status": r["status"],
                            "test_name": r["test_name"],
                            "test_public_id": r["test_public_id"],
                            "location": r["location"],
                            "duration_ms": r["duration"],
                            "test_type": r["test_type"],
                            "execution_rule": r["execution_rule"],
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        formatter::output(cfg, &table_rows)
    } else {
        formatter::output(cfg, results)
    }
}

pub async fn tests_list(cfg: &Config, page_size: i64, page_number: i64) -> Result<()> {
    let api = crate::make_api!(SyntheticsAPI, cfg);
    let resp = api
        .list_tests(
            ListTestsOptionalParams::default()
                .page_size(page_size)
                .page_number(page_number),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to list tests: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn tests_get(cfg: &Config, public_id: &str) -> Result<()> {
    let api = crate::make_api!(SyntheticsAPI, cfg);
    let resp = api
        .get_test(public_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get test: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn tests_search(
    cfg: &Config,
    text: Option<String>,
    facets_only: bool,
    include_full_config: bool,
    count: i64,
    start: i64,
    sort: Option<String>,
) -> Result<()> {
    let api = crate::make_api!(SyntheticsAPI, cfg);

    let mut params = SearchTestsOptionalParams::default();
    if let Some(t) = text {
        params = params.text(t);
    }
    if facets_only {
        params = params.facets_only(true);
    }
    if include_full_config {
        params = params.include_full_config(true);
    }
    if count != 50 {
        params = params.count(count);
    }
    if start != 0 {
        params = params.start(start);
    }
    if let Some(s) = sort {
        params = params.sort(s);
    }

    let resp = api
        .search_tests(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search tests: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn locations_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(SyntheticsAPI, cfg);
    let resp = api
        .list_locations()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list locations: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Suites (V2 API) ----

pub async fn suites_list(cfg: &Config, query: Option<String>) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let mut params = SearchSuitesOptionalParams::default();
    if let Some(q) = query {
        params = params.query(q);
    }
    let resp = api
        .search_suites(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list synthetic suites: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn suites_get(cfg: &Config, suite_id: &str) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let resp = api
        .get_synthetics_suite(suite_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get synthetic suite: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn suites_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let body: SuiteCreateEditRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_synthetics_suite(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create synthetic suite: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn suites_update(cfg: &Config, suite_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let body: SuiteCreateEditRequest = crate::util::read_json_file(file)?;
    let resp = api
        .edit_synthetics_suite(suite_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update synthetic suite: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn suites_delete(cfg: &Config, suite_ids: Vec<String>) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let attrs = DeletedSuitesRequestDeleteAttributes::new(suite_ids);
    let data = DeletedSuitesRequestDelete::new(attrs);
    let body = DeletedSuitesRequestDeleteRequest::new(data);
    let resp = api
        .delete_synthetics_suites(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete synthetic suites: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Tests (V2 API) ----

pub async fn tests_get_fast_result(cfg: &Config, result_id: &str) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let resp = api
        .get_synthetics_fast_test_result(result_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get fast test result: {e:?}"))?;
    formatter::output(cfg, &resp)
}

fn parse_result_status(s: &str) -> Result<SyntheticsTestResultStatus> {
    Ok(match s {
        "passed" => SyntheticsTestResultStatus::PASSED,
        "failed" => SyntheticsTestResultStatus::FAILED,
        "no_data" => SyntheticsTestResultStatus::NO_DATA,
        _ => anyhow::bail!("invalid status '{s}' — use one of: passed, failed, no_data"),
    })
}

fn parse_result_run_type(s: &str) -> Result<SyntheticsTestResultRunType> {
    Ok(match s {
        "scheduled" => SyntheticsTestResultRunType::SCHEDULED,
        "fast" => SyntheticsTestResultRunType::FAST,
        "ci" => SyntheticsTestResultRunType::CI,
        "triggered" => SyntheticsTestResultRunType::TRIGGERED,
        _ => anyhow::bail!("invalid run-type '{s}' — use one of: scheduled, fast, ci, triggered"),
    })
}

pub async fn tests_get_result(
    cfg: &Config,
    public_id: &str,
    result_id: &str,
    event_id: Option<String>,
    timestamp: Option<i64>,
) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let mut params = GetSyntheticsTestResultOptionalParams::default();
    if let Some(e) = event_id {
        params.event_id = Some(e);
    }
    if let Some(t) = timestamp {
        params.timestamp = Some(t);
    }
    let resp = api
        .get_synthetics_test_result(public_id.to_string(), result_id.to_string(), params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get test result: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn tests_get_browser_result(
    cfg: &Config,
    public_id: &str,
    result_id: &str,
    event_id: Option<String>,
    timestamp: Option<i64>,
) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let mut params = GetSyntheticsBrowserTestResultOptionalParams::default();
    if let Some(e) = event_id {
        params.event_id = Some(e);
    }
    if let Some(t) = timestamp {
        params.timestamp = Some(t);
    }
    let resp = api
        .get_synthetics_browser_test_result(public_id.to_string(), result_id.to_string(), params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get browser test result: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[allow(clippy::too_many_arguments)]
pub async fn tests_list_latest_results(
    cfg: &Config,
    public_id: &str,
    from_ts: Option<i64>,
    to_ts: Option<i64>,
    status: Option<String>,
    run_type: Option<String>,
    probe_dc: Option<Vec<String>>,
    device_id: Option<Vec<String>>,
) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let mut params = ListSyntheticsTestLatestResultsOptionalParams::default();
    if let Some(t) = from_ts {
        params.from_ts = Some(t);
    }
    if let Some(t) = to_ts {
        params.to_ts = Some(t);
    }
    if let Some(s) = status {
        params.status = Some(parse_result_status(&s)?);
    }
    if let Some(r) = run_type {
        params.run_type = Some(parse_result_run_type(&r)?);
    }
    if let Some(p) = probe_dc {
        params.probe_dc = Some(p);
    }
    if let Some(d) = device_id {
        params.device_id = Some(d);
    }
    let resp = api
        .list_synthetics_test_latest_results(public_id.to_string(), params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list latest test results: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[allow(clippy::too_many_arguments)]
pub async fn tests_list_latest_browser_results(
    cfg: &Config,
    public_id: &str,
    from_ts: Option<i64>,
    to_ts: Option<i64>,
    status: Option<String>,
    run_type: Option<String>,
    probe_dc: Option<Vec<String>>,
    device_id: Option<Vec<String>>,
) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let mut params = ListSyntheticsBrowserTestLatestResultsOptionalParams::default();
    if let Some(t) = from_ts {
        params.from_ts = Some(t);
    }
    if let Some(t) = to_ts {
        params.to_ts = Some(t);
    }
    if let Some(s) = status {
        params.status = Some(parse_result_status(&s)?);
    }
    if let Some(r) = run_type {
        params.run_type = Some(parse_result_run_type(&r)?);
    }
    if let Some(p) = probe_dc {
        params.probe_dc = Some(p);
    }
    if let Some(d) = device_id {
        params.device_id = Some(d);
    }
    let resp = api
        .list_synthetics_browser_test_latest_results(public_id.to_string(), params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list latest browser test results: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn tests_poll_results(cfg: &Config, result_ids: Vec<String>) -> Result<()> {
    if result_ids.is_empty() {
        anyhow::bail!("at least one result-id is required");
    }
    // The endpoint takes a JSON-encoded array as a query string parameter.
    let encoded = serde_json::to_string(&result_ids)
        .map_err(|e| anyhow::anyhow!("failed to encode result IDs: {e}"))?;
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let resp = api
        .poll_synthetics_test_results(encoded)
        .await
        .map_err(|e| anyhow::anyhow!("failed to poll test results: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn tests_get_version(
    cfg: &Config,
    public_id: &str,
    version: i64,
    include_change_metadata: bool,
) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let mut params = GetSyntheticsTestVersionOptionalParams::default();
    if include_change_metadata {
        params = params.include_change_metadata(true);
    }
    let resp = api
        .get_synthetics_test_version(public_id.to_string(), version, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get test version: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn tests_list_versions(
    cfg: &Config,
    public_id: &str,
    limit: Option<i64>,
    last_version_number: Option<i64>,
) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let mut params = ListSyntheticsTestVersionsOptionalParams::default();
    if let Some(l) = limit {
        params = params.limit(l);
    }
    if let Some(v) = last_version_number {
        params = params.last_version_number(v);
    }
    let resp = api
        .list_synthetics_test_versions(public_id.to_string(), params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list test versions: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Downtimes (V2 API) ----

pub async fn downtime_list(
    cfg: &Config,
    filter_test_ids: Option<String>,
    filter_active: Option<String>,
) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let mut params = ListSyntheticsDowntimesOptionalParams::default();
    if let Some(ids) = filter_test_ids {
        params = params.filter_test_ids(ids);
    }
    if let Some(active) = filter_active {
        params = params.filter_active(active);
    }
    let resp = api
        .list_synthetics_downtimes(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list synthetics downtimes: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn downtime_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let body: SyntheticsDowntimeRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_synthetics_downtime(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create synthetics downtime: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn downtime_delete(cfg: &Config, downtime_id: &str) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    api.delete_synthetics_downtime(downtime_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete synthetics downtime: {e:?}"))?;
    println!("Synthetics downtime {downtime_id} deleted.");
    Ok(())
}

// ---- Multistep (V2 API) ----

pub async fn multistep_get_subtests(cfg: &Config, public_id: &str) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let resp = api
        .get_api_multistep_subtests(public_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get multistep subtests: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn multistep_get_subtest_parents(cfg: &Config, public_id: &str) -> Result<()> {
    let api = crate::make_api!(SyntheticsV2API, cfg);
    let resp = api
        .get_api_multistep_subtest_parents(public_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get multistep subtest parents: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_synthetics_tests_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"tests": []}"#).await;
        let _ = super::tests_list(&cfg, 10, 0).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_synthetics_tests_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let _ = super::tests_get(&cfg, "pub1").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_synthetics_locations_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"locations": []}"#).await;
        let _ = super::locations_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_synthetics_tests_get_result() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let _mock = mock_any(&mut s, "GET", r#"{}"#).await;
        let result = super::tests_get_result(&cfg, "abc-def-ghi", "result-1", None, None).await;
        assert!(
            result.is_ok(),
            "tests_get_result failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_synthetics_tests_get_result_with_params() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let _mock = mock_any(&mut s, "GET", r#"{}"#).await;
        let result = super::tests_get_result(
            &cfg,
            "abc-def-ghi",
            "result-1",
            Some("evt-1".into()),
            Some(1700000000),
        )
        .await;
        assert!(
            result.is_ok(),
            "tests_get_result with params failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_synthetics_tests_get_result_404() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["not found"]}"#)
            .create_async()
            .await;
        let result = super::tests_get_result(&cfg, "abc-def-ghi", "missing", None, None).await;
        assert!(result.is_err(), "expected 404 error");
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_synthetics_tests_get_browser_result() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let _mock = mock_any(&mut s, "GET", r#"{}"#).await;
        let result =
            super::tests_get_browser_result(&cfg, "abc-def-ghi", "result-1", None, None).await;
        assert!(
            result.is_ok(),
            "tests_get_browser_result failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_synthetics_tests_list_latest_results() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let _mock = mock_any(&mut s, "GET", r#"{}"#).await;
        let result = super::tests_list_latest_results(
            &cfg,
            "abc-def-ghi",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        assert!(
            result.is_ok(),
            "tests_list_latest_results failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_synthetics_tests_list_latest_results_with_filters() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let _mock = mock_any(&mut s, "GET", r#"{}"#).await;
        let result = super::tests_list_latest_results(
            &cfg,
            "abc-def-ghi",
            Some(1700000000000),
            Some(1700001000000),
            Some("passed".into()),
            Some("scheduled".into()),
            Some(vec!["aws:us-east-1".into()]),
            Some(vec!["chrome.laptop_large".into()]),
        )
        .await;
        assert!(
            result.is_ok(),
            "tests_list_latest_results with filters failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_synthetics_tests_list_latest_results_bad_status() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused.local");
        let result = super::tests_list_latest_results(
            &cfg,
            "abc-def-ghi",
            None,
            None,
            Some("bogus".into()),
            None,
            None,
            None,
        )
        .await;
        assert!(result.is_err(), "expected status parse error");
        assert!(result.unwrap_err().to_string().contains("invalid status"));
    }

    #[tokio::test]
    async fn test_synthetics_tests_list_latest_results_bad_run_type() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused.local");
        let result = super::tests_list_latest_results(
            &cfg,
            "abc-def-ghi",
            None,
            None,
            None,
            Some("bogus".into()),
            None,
            None,
        )
        .await;
        assert!(result.is_err(), "expected run-type parse error");
        assert!(result.unwrap_err().to_string().contains("invalid run-type"));
    }

    #[tokio::test]
    async fn test_synthetics_tests_list_latest_browser_results() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let _mock = mock_any(&mut s, "GET", r#"{}"#).await;
        let result = super::tests_list_latest_browser_results(
            &cfg,
            "abc-def-ghi",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
        assert!(
            result.is_ok(),
            "tests_list_latest_browser_results failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_synthetics_tests_poll_results() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let _mock = mock_any(&mut s, "GET", r#"{}"#).await;
        let result = super::tests_poll_results(&cfg, vec!["r1".into(), "r2".into()]).await;
        assert!(
            result.is_ok(),
            "tests_poll_results failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_synthetics_tests_poll_results_empty() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused.local");
        let result = super::tests_poll_results(&cfg, vec![]).await;
        assert!(result.is_err(), "expected empty result_ids error");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("at least one result-id"));
    }

    #[tokio::test]
    async fn test_synthetics_downtime_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let _mock = mock_any(&mut s, "GET", r#"{"data":[]}"#).await;
        let result = super::downtime_list(&cfg, None, None).await;
        assert!(result.is_ok(), "downtime_list failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_synthetics_downtime_list_with_filters() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let _mock = mock_any(&mut s, "GET", r#"{"data":[]}"#).await;
        let result = super::downtime_list(
            &cfg,
            Some("abc-def-ghi".to_string()),
            Some("true".to_string()),
        )
        .await;
        assert!(
            result.is_ok(),
            "downtime_list with filters failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_synthetics_downtime_list_error() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Forbidden"]}"#)
            .create_async()
            .await;
        let result = super::downtime_list(&cfg, None, None).await;
        assert!(result.is_err(), "expected 403 error from downtime_list");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_synthetics_downtime_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let _mock = mock_any(
            &mut s,
            "POST",
            r#"{"data":{"id":"dt-123","type":"downtime","attributes":{"createdAt":"2024-01-01T00:00:00+00:00","createdBy":"u1","createdByName":"User One","description":"","isEnabled":true,"name":"test","tags":[],"testIds":[],"timeSlots":[],"updatedAt":"2024-01-01T00:00:00+00:00","updatedBy":"u1","updatedByName":"User One"}}}"#,
        )
        .await;
        let tmp = write_temp_json(
            "downtime_create.json",
            r#"{"data":{"type":"downtime","attributes":{"name":"test","isEnabled":true,"testIds":[],"timeSlots":[]}}}"#,
        );
        let result = super::downtime_create(&cfg, tmp.to_str().unwrap()).await;
        assert!(result.is_ok(), "downtime_create failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_synthetics_downtime_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let _mock = server_mock_delete(&mut s).await;
        let result = super::downtime_delete(&cfg, "dt-abc-123").await;
        assert!(result.is_ok(), "downtime_delete failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_synthetics_downtime_delete_error() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("DELETE", mockito::Matcher::Any)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Not Found"]}"#)
            .create_async()
            .await;
        let result = super::downtime_delete(&cfg, "nonexistent-dt").await;
        assert!(result.is_err(), "expected 404 error from downtime_delete");
        cleanup_env();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_build_auth_headers_prefers_bearer() {
        let mut cfg = test_config("http://unused.local");
        cfg.access_token = Some("tok-123".into());
        let headers = super::build_auth_headers(&cfg).expect("bearer headers");
        assert_eq!(headers.get("authorization").unwrap(), "Bearer tok-123");
        assert!(headers.get("dd-api-key").is_none());
        assert!(headers.get("dd-application-key").is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_build_auth_headers_falls_back_to_api_keys() {
        let mut cfg = test_config("http://unused.local");
        cfg.access_token = None;
        cfg.api_key = Some("api-1".into());
        cfg.app_key = Some("app-1".into());
        let headers = super::build_auth_headers(&cfg).expect("api-key headers");
        assert_eq!(headers.get("dd-api-key").unwrap(), "api-1");
        assert_eq!(headers.get("dd-application-key").unwrap(), "app-1");
        assert!(headers.get("authorization").is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_build_auth_headers_requires_some_auth() {
        let mut cfg = test_config("http://unused.local");
        cfg.access_token = None;
        cfg.api_key = None;
        cfg.app_key = None;
        let err = super::build_auth_headers(&cfg).expect_err("should require auth");
        assert!(err.to_string().contains("requires authentication"));
    }

    async fn server_mock_delete(s: &mut mockito::Server) -> mockito::Mock {
        s.mock("DELETE", mockito::Matcher::Any)
            .with_status(204)
            .with_header("content-type", "application/json")
            .with_body("")
            .create_async()
            .await
    }
}

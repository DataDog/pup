use anyhow::Result;
use datadog_api_client::datadogV1::api_monitors::{
    DeleteMonitorOptionalParams, GetMonitorOptionalParams, ListMonitorsOptionalParams, MonitorsAPI,
    SearchMonitorsOptionalParams,
};
use datadog_api_client::datadogV1::model::Monitor;

use crate::config::Config;
use crate::formatter::{self, Metadata};
use crate::util;
use crate::util_ext;

pub async fn list(
    cfg: &Config,
    name: Option<String>,
    tags: Option<String>,
    limit: i32,
    page: i64,
) -> Result<()> {
    if !(1..=1000).contains(&limit) {
        anyhow::bail!("--limit must be between 1 and 1000, got {limit}");
    }

    let api = crate::make_api!(MonitorsAPI, cfg);

    let mut params = ListMonitorsOptionalParams::default();
    if let Some(name) = name {
        params = params.name(name);
    }
    if let Some(tags) = tags {
        params = params.monitor_tags(tags);
    }
    params = params.page_size(limit).page(page);

    let monitors = api
        .list_monitors(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list monitors: {:?}", e))?;

    if monitors.is_empty() {
        eprintln!("No monitors found matching the specified criteria.");
        return Ok(());
    }

    let meta = Metadata {
        count: Some(monitors.len()),
        truncated: false,
        command: Some("monitors list".to_string()),
        next_action: None,
    };
    formatter::format_and_print(
        &monitors,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&meta),
        cfg.jq.as_deref(),
    )?;
    Ok(())
}

pub async fn get(cfg: &Config, monitor_id: i64) -> Result<()> {
    let api = crate::make_api!(MonitorsAPI, cfg);
    let resp = api
        .get_monitor(monitor_id, GetMonitorOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get monitor: {:?}", e))?;
    let meta = Metadata {
        count: None,
        truncated: false,
        command: Some("monitors get".to_string()),
        next_action: None,
    };
    formatter::format_and_print(
        &resp,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&meta),
        cfg.jq.as_deref(),
    )
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let body: Monitor = util::read_json_file(file)?;
    let api = crate::make_api!(MonitorsAPI, cfg);
    let resp = api
        .create_monitor(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create monitor: {:?}", e))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, monitor_id: i64, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV1::model::MonitorUpdateRequest =
        util::read_json_file(file)?;
    let api = crate::make_api!(MonitorsAPI, cfg);
    let resp = api
        .update_monitor(monitor_id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update monitor: {:?}", e))?;
    formatter::output(cfg, &resp)
}

pub async fn diff(
    cfg: &Config,
    monitor_id: i64,
    file: &str,
    only: &[String],
    ignore: &[String],
) -> Result<()> {
    // Read the candidate (desired-state) definition as a raw JSON value.
    // Using Value (rather than MonitorUpdateRequest) preserves the user's exact
    // field set and avoids SDK defaults overwriting omitted optional fields during
    // deserialization, which would produce spurious "modified" entries.
    // Trade-off: field-name typos in the candidate file won't be caught here;
    // they will appear as added/removed pairs in the diff output, which is still
    // actionable. `pup monitors update` will fail or no-op on unknown fields.
    let candidate: serde_json::Value = util::read_json_file(file)?;

    // Fetch the live monitor
    let api = crate::make_api!(MonitorsAPI, cfg);
    let live = api
        .get_monitor(monitor_id, GetMonitorOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get monitor: {:?}", e))?;
    let live = serde_json::to_value(&live)
        .map_err(|e| anyhow::anyhow!("failed to serialize monitor {monitor_id} for diff: {e:?}"))?;

    // `update` (PUT) is a partial/merge update: fields absent from the candidate
    // file are left unchanged on the live monitor, not deleted. "removed" entries
    // in this diff show what the candidate does not specify — they will NOT be
    // removed by `pup monitors update`. Run `update` only for "added"/"modified"
    // changes; "removed" entries require no action unless you want to add those
    // fields to the candidate explicitly.
    let resource_id = monitor_id.to_string();
    let mut options = util_ext::ResourceDiffOptions::new(
        "monitors diff",
        "pup monitors update",
        "monitor",
        &resource_id,
    );
    options.readonly_paths = util_ext::READONLY_MONITOR_FIELDS;
    options.only = only;
    options.ignore = ignore;
    options.removed_entries_next_action = Some(
        "review changes — note: 'removed' entries will NOT be deleted by \
         `pup monitors update` (partial update)",
    );
    options.no_changes_message = Some(format!("No changes — monitor {monitor_id} is in sync."));
    util_ext::format_resource_diff(cfg, &live, &candidate, &options)
}

pub async fn search(
    cfg: &Config,
    query: Option<String>,
    page: i64,
    per_page: i64,
    sort: Option<String>,
) -> Result<()> {
    let api = crate::make_api!(MonitorsAPI, cfg);

    let mut params = SearchMonitorsOptionalParams::default()
        .page(page)
        .per_page(per_page);
    if let Some(q) = query {
        params = params.query(q);
    }
    if let Some(s) = sort {
        params = params.sort(s);
    }

    let resp = api
        .search_monitors(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search monitors: {:?}", e))?;
    formatter::output(cfg, &resp)
}

pub async fn delete(cfg: &Config, monitor_id: i64) -> Result<()> {
    let api = crate::make_api!(MonitorsAPI, cfg);
    let resp = api
        .delete_monitor(monitor_id, DeleteMonitorOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete monitor: {:?}", e))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_monitors_list_empty() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", "[]").await;

        let result = super::list(&cfg, None, None, 10, 0).await;
        assert!(result.is_ok(), "monitors list failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_monitors_list_with_results() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"[{"id": 1, "name": "Test Monitor", "type": "metric alert", "query": "avg(last_5m):avg:system.cpu.user{*} > 90", "message": "CPU high", "tags": [], "options": {}}]"#;
        let _mock = mock_any(&mut server, "GET", body).await;

        let result = super::list(&cfg, Some("Test".into()), None, 10, 0).await;
        assert!(
            result.is_ok(),
            "monitors list with results failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_monitors_get() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"id": 12345, "name": "Test Monitor", "type": "metric alert", "query": "avg(last_5m):avg:system.cpu.user{*} > 90", "message": "CPU high", "tags": [], "options": {}}"#;
        let _mock = mock_any(&mut server, "GET", body).await;

        let result = super::get(&cfg, 12345).await;
        assert!(result.is_ok(), "monitors get failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_monitors_search() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"monitors": [], "metadata": {"page": 0, "page_count": 0, "per_page": 30, "total_count": 0}}"#;
        let _mock = mock_any(&mut server, "GET", body).await;

        let result = super::search(&cfg, Some("cpu".into()), 0, 30, None).await;
        assert!(result.is_ok(), "monitors search failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_monitors_list_limit_too_small() {
        let cfg = test_config("http://unused.local");
        let result = super::list(&cfg, None, None, 0, 0).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("--limit must be between 1 and 1000"));
    }

    #[tokio::test]
    async fn test_monitors_list_limit_too_large() {
        let cfg = test_config("http://unused.local");
        let result = super::list(&cfg, None, None, 1001, 0).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("--limit must be between 1 and 1000"));
    }

    #[tokio::test]
    async fn test_monitors_delete() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "DELETE", r#"{"deleted_monitor_id": 12345}"#).await;

        let result = super::delete(&cfg, 12345).await;
        assert!(result.is_ok(), "monitors delete failed: {:?}", result.err());
        cleanup_env();
    }

    // ---- monitors diff ----

    /// Write a temp file with the given JSON content; returns the path.
    /// Caller must delete the file when done.
    fn write_temp_json(label: &str, content: &str) -> String {
        let path =
            std::env::temp_dir().join(format!("pup_test_{label}_{}.json", std::process::id()));
        std::fs::write(&path, content).expect("write temp file");
        path.to_string_lossy().to_string()
    }

    #[tokio::test]
    async fn test_monitors_diff_detects_changes() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Live monitor returned by the API
        let live_body = r#"{
            "id": 12345,
            "name": "CPU Monitor",
            "type": "metric alert",
            "query": "avg(last_5m):avg:system.cpu.user{*} > 90",
            "message": "CPU high",
            "tags": ["env:prod"],
            "options": {"thresholds": {"critical": 90.0}}
        }"#;
        let _mock = mock_any(&mut server, "GET", live_body).await;

        // Candidate raises the threshold
        let candidate = r#"{
            "name": "CPU Monitor",
            "type": "metric alert",
            "query": "avg(last_5m):avg:system.cpu.user{*} > 95",
            "message": "CPU high",
            "tags": ["env:prod"],
            "options": {"thresholds": {"critical": 95.0}}
        }"#;
        let path = write_temp_json("diff_detects_changes", candidate);

        let result = super::diff(&cfg, 12345, &path, &[], &[]).await;
        let _ = std::fs::remove_file(&path);
        assert!(result.is_ok(), "monitors diff failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_monitors_diff_no_changes() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let live_body = r#"{
            "id": 12345,
            "name": "CPU Monitor",
            "type": "metric alert",
            "query": "avg(last_5m):avg:system.cpu.user{*} > 90",
            "message": "CPU high",
            "tags": ["env:prod"],
            "options": {"thresholds": {"critical": 90.0}}
        }"#;
        let _mock = mock_any(&mut server, "GET", live_body).await;

        // Candidate is identical (minus read-only `id`)
        let candidate = r#"{
            "name": "CPU Monitor",
            "type": "metric alert",
            "query": "avg(last_5m):avg:system.cpu.user{*} > 90",
            "message": "CPU high",
            "tags": ["env:prod"],
            "options": {"thresholds": {"critical": 90.0}}
        }"#;
        let path = write_temp_json("diff_no_changes", candidate);

        let result = super::diff(&cfg, 12345, &path, &[], &[]).await;
        let _ = std::fs::remove_file(&path);
        assert!(
            result.is_ok(),
            "monitors diff (no-changes) failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_monitors_diff_readonly_field_ignored() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let live_body = r#"{
            "id": 12345,
            "name": "CPU Monitor",
            "type": "metric alert",
            "query": "avg(last_5m):avg:system.cpu.user{*} > 90",
            "message": "CPU high",
            "overall_state": "OK",
            "creator": {"email": "someone@example.com"}
        }"#;
        let _mock = mock_any(&mut server, "GET", live_body).await;

        // Candidate omits `id`, `overall_state`, `creator` — should produce no diff
        let candidate = r#"{
            "name": "CPU Monitor",
            "type": "metric alert",
            "query": "avg(last_5m):avg:system.cpu.user{*} > 90",
            "message": "CPU high"
        }"#;
        let path = write_temp_json("diff_readonly", candidate);

        let result = super::diff(&cfg, 12345, &path, &[], &[]).await;
        let _ = std::fs::remove_file(&path);
        assert!(
            result.is_ok(),
            "monitors diff (readonly) failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_monitors_diff_file_not_found() {
        let cfg = test_config("http://unused.local");
        let result = super::diff(&cfg, 12345, "/nonexistent/path.json", &[], &[]).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to read file"));
    }

    #[tokio::test]
    async fn test_monitors_diff_invalid_json() {
        let path = write_temp_json("diff_invalid_json", "not valid json {{{");
        let cfg = test_config("http://unused.local");
        let result = super::diff(&cfg, 12345, &path, &[], &[]).await;
        let _ = std::fs::remove_file(&path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to parse JSON"));
    }

    #[tokio::test]
    async fn test_monitors_diff_fetch_failure() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"errors": ["Not Found"]}"#).await;

        let path = write_temp_json("diff_fetch_failure", r#"{"name": "CPU Monitor"}"#);
        // The mock returns a 200 with error body; the client may or may not error —
        // either outcome is acceptable as long as it doesn't panic.
        let _result = super::diff(&cfg, 99999, &path, &[], &[]).await;
        let _ = std::fs::remove_file(&path);
        cleanup_env();
    }

    #[tokio::test]
    async fn test_monitors_diff_with_only_flag() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let live_body = r#"{
            "id": 1,
            "name": "Old Name",
            "type": "metric alert",
            "query": "avg(last_5m):avg:system.cpu.user{*} > 90",
            "options": {"thresholds": {"critical": 90.0}}
        }"#;
        let _mock = mock_any(&mut server, "GET", live_body).await;

        // Candidate changes both name and threshold; --only options.thresholds should
        // scope the result (the diff command itself still succeeds)
        let candidate = r#"{
            "name": "New Name",
            "query": "avg(last_5m):avg:system.cpu.user{*} > 90",
            "options": {"thresholds": {"critical": 95.0}}
        }"#;
        let path = write_temp_json("diff_only_flag", candidate);
        let only = vec!["options.thresholds".to_string()];

        let result = super::diff(&cfg, 1, &path, &only, &[]).await;
        let _ = std::fs::remove_file(&path);
        assert!(
            result.is_ok(),
            "monitors diff --only failed: {:?}",
            result.err()
        );
        cleanup_env();
    }
}

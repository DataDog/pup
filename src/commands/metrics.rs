use anyhow::Result;

/// Matches a metric name against a glob pattern where `*` is a wildcard.
/// Falls back to substring match when no `*` is present.
fn matches_glob(text: &str, pattern: &str) -> bool {
    if !pattern.contains('*') {
        return text.contains(pattern);
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut remaining = text;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !remaining.starts_with(part) {
                return false;
            }
            remaining = &remaining[part.len()..];
        } else if i == parts.len() - 1 {
            return remaining.ends_with(part);
        } else {
            match remaining.find(part) {
                Some(pos) => remaining = &remaining[pos + part.len()..],
                None => return false,
            }
        }
    }
    true
}

use datadog_api_client::datadogV1::api_metrics::{
    ListActiveMetricsOptionalParams, MetricsAPI as MetricsV1API,
};
use datadog_api_client::datadogV1::model::MetricMetadata;
use datadog_api_client::datadogV2::api_metrics::MetricsAPI as MetricsV2API;
use datadog_api_client::datadogV2::model::MetricPayload;

use crate::config::Config;
use crate::formatter;
use crate::util;
use crate::util_ext;

pub async fn list(
    cfg: &Config,
    filter: Option<String>,
    from: String,
    tag_filter: Option<String>,
) -> Result<()> {
    let api = crate::make_api!(MetricsV1API, cfg);

    let from_ts = util_ext::parse_time_to_unix(&from)?;
    let mut params = ListActiveMetricsOptionalParams::default();
    if let Some(tf) = tag_filter {
        params = params.tag_filter(tf);
    }

    let resp = api
        .list_active_metrics(from_ts, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list metrics: {e:?}"))?;

    // Client-side filter if provided
    if let Some(pattern) = filter {
        let pattern = pattern.to_lowercase();
        let metrics = resp.metrics.as_deref().unwrap_or(&[]);
        let filtered: Vec<&str> = metrics
            .iter()
            .filter(|m| matches_glob(&m.to_lowercase(), &pattern))
            .map(|m| m.as_str())
            .collect();
        let output = serde_json::json!({
            "from": resp.from,
            "metrics": filtered,
        });
        return formatter::output(cfg, &output);
    }

    formatter::output(cfg, &resp)
}

pub async fn search(cfg: &Config, query: String, from: String, to: String) -> Result<()> {
    let api = crate::make_api!(MetricsV1API, cfg);

    let from_ts = util_ext::parse_time_to_unix(&from)?;
    let to_ts = util_ext::parse_time_to_unix(&to)?;

    let resp = api
        .query_metrics(from_ts, to_ts, query)
        .await
        .map_err(|e| anyhow::anyhow!("failed to query metrics: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metadata_get(cfg: &Config, metric_name: &str) -> Result<()> {
    let api = crate::make_api!(MetricsV1API, cfg);
    let resp = api
        .get_metric_metadata(metric_name.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get metric metadata: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn query(cfg: &Config, query: String, from: String, to: String) -> Result<()> {
    let api = crate::make_api!(MetricsV1API, cfg);

    let from_ts = util_ext::parse_time_to_unix(&from)?;
    let to_ts = util_ext::parse_time_to_unix(&to)?;

    let resp = api
        .query_metrics(from_ts, to_ts, query)
        .await
        .map_err(|e| anyhow::anyhow!("failed to query metrics: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metadata_update(cfg: &Config, metric_name: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(MetricsV1API, cfg);
    let body: MetricMetadata = util::read_json_file(file)?;
    let resp = api
        .update_metric_metadata(metric_name.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update metric metadata: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn submit(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(MetricsV2API, cfg);
    let body: MetricPayload = util::read_json_file(file)?;
    let resp = api
        .submit_metrics(
            body,
            datadog_api_client::datadogV2::api_metrics::SubmitMetricsOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to submit metrics: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn tags_list(cfg: &Config, metric_name: &str, window_seconds: Option<i64>) -> Result<()> {
    use datadog_api_client::datadogV2::api_metrics::ListTagsByMetricNameOptionalParams;

    let api = crate::make_api!(MetricsV2API, cfg);
    let mut params = ListTagsByMetricNameOptionalParams::default();
    if let Some(w) = window_seconds {
        params = params.window_seconds(w);
    }
    let resp = api
        .list_tags_by_metric_name(metric_name.to_string(), params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list tags for metric {metric_name}: {e:?}"))?;
    formatter::output(cfg, &resp)
}

/// Query timeseries data using the v2 metrics API.
///
/// The request body is provided as a JSON file matching the
/// `TimeseriesFormulaQueryRequest` schema. Use `cross_org_uuids` inside the
/// individual query objects in that file to enable cross-organisation queries
/// (SDK PR #1564).
pub async fn query_timeseries(cfg: &Config, file: &str) -> Result<()> {
    use datadog_api_client::datadogV2::model::TimeseriesFormulaQueryRequest;

    let api = crate::make_api!(MetricsV2API, cfg);
    let body: TimeseriesFormulaQueryRequest = util::read_json_file(file)?;
    let resp = api
        .query_timeseries_data(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to query timeseries data: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    use super::matches_glob;

    #[test]
    fn test_glob_suffix_wildcard() {
        assert!(matches_glob("system.cpu.user", "system.*"));
        assert!(matches_glob("system.mem.used", "system.*"));
        assert!(!matches_glob("datadog.agent.cpu", "system.*"));
    }

    #[test]
    fn test_glob_prefix_wildcard() {
        assert!(matches_glob("avg.system.cpu", "*.cpu"));
        assert!(!matches_glob("system.cpu.user", "*.cpu"));
    }

    #[test]
    fn test_glob_both_wildcards() {
        assert!(matches_glob("system.cpu.user", "*.cpu.*"));
        assert!(matches_glob("datadog.system.cpu.idle", "*.cpu.*"));
        assert!(!matches_glob("system.mem.used", "*.cpu.*"));
    }

    #[test]
    fn test_glob_no_wildcard_substring() {
        assert!(matches_glob("datadog.estimated_usage.platform", "platform"));
        assert!(matches_glob("system.cpu.user", "cpu"));
        assert!(!matches_glob("system.mem.used", "cpu"));
    }

    #[test]
    fn test_glob_wildcard_only() {
        assert!(matches_glob("anything", "*"));
        assert!(matches_glob("", "*"));
    }

    #[test]
    fn test_glob_exact_with_dot() {
        assert!(matches_glob("system.cpu.user", "system.cpu.user"));
        assert!(!matches_glob("system.cpu.idle", "system.cpu.user"));
    }

    #[tokio::test]
    async fn test_metrics_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(
            &mut server,
            "GET",
            r#"{"metrics": [], "from": "2024-01-01T00:00:00Z"}"#,
        )
        .await;

        let result = super::list(&cfg, None, "1h".into(), None).await;
        assert!(result.is_ok(), "metrics list failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_metrics_query() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(
            &mut server,
            "GET",
            r#"{"status": "ok", "res_type": "time_series", "series": [], "from_date": 0, "to_date": 0, "query": "avg:system.cpu.user{*}"}"#,
        )
        .await;

        let result = super::query(
            &cfg,
            "avg:system.cpu.user{*}".into(),
            "1h".into(),
            "now".into(),
        )
        .await;
        assert!(result.is_ok(), "metrics query failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_metrics_metadata_get() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(
            &mut server,
            "GET",
            r#"{"type": "gauge", "description": "CPU usage", "unit": "percent"}"#,
        )
        .await;

        let result = super::metadata_get(&cfg, "system.cpu.user").await;
        assert!(
            result.is_ok(),
            "metrics metadata get failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_metrics_tags_list_no_window() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"data": null}"#).await;

        let result = super::tags_list(&cfg, "system.cpu.user", None).await;
        assert!(
            result.is_ok(),
            "metrics tags list failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_metrics_tags_list_with_window() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"data": null}"#).await;

        let result = super::tags_list(&cfg, "system.cpu.user", Some(3600)).await;
        assert!(
            result.is_ok(),
            "metrics tags list with window_seconds failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_metrics_query_timeseries() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(
            &mut server,
            "POST",
            r#"{"data": {"type": "timeseries_response", "attributes": {"times": [], "values": [], "series": []}}}"#,
        )
        .await;

        let body = r#"{
            "data": {
                "attributes": {
                    "formulas": [{"formula": "a"}],
                    "from": 0,
                    "interval": 5000,
                    "queries": [{"data_source": "metrics", "query": "avg:system.cpu.user{*}", "name": "a"}],
                    "to": 3600000
                },
                "type": "timeseries_request"
            }
        }"#;
        let tmp = write_temp_json("timeseries_test.json", body);
        let result = super::query_timeseries(&cfg, tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "metrics query timeseries failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_metrics_query_timeseries_bad_file() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _ = mock_any(&mut server, "POST", r#"{}"#).await;

        let result = super::query_timeseries(&cfg, "/nonexistent/path/request.json").await;
        assert!(result.is_err(), "expected error for missing file, got ok");
        cleanup_env();
    }
}

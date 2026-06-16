use anyhow::{bail, Result};
use datadog_api_client::datadogV2::api_logs::{ListLogsOptionalParams, LogsAPI};
use datadog_api_client::datadogV2::api_logs_archives::LogsArchivesAPI;
use datadog_api_client::datadogV2::api_logs_custom_destinations::LogsCustomDestinationsAPI;
use datadog_api_client::datadogV2::api_logs_metrics::LogsMetricsAPI;
use datadog_api_client::datadogV2::model::{
    LogsListRequest, LogsListRequestPage, LogsQueryFilter, LogsSort, LogsStorageTier,
};

use crate::client;
use crate::config::Config;
use crate::formatter;
use crate::util;

pub struct AggregateArgs {
    pub query: String,
    pub from: String,
    pub to: String,
    pub compute: Vec<String>,
    pub group_by: Vec<String>,
    pub limit: i32,
    pub storage: Option<String>,
    pub sort: String,
}

fn normalize_storage_tier(storage: Option<String>) -> Result<Option<String>> {
    match storage {
        None => Ok(None),
        Some(s) => match s.to_lowercase().as_str() {
            "indexes" => Ok(Some("indexes".into())),
            "online-archives" | "online_archives" => Ok(Some("online-archives".into())),
            "flex" => Ok(Some("flex".into())),
            other => anyhow::bail!(
                "unknown storage tier {:?}; valid values are: indexes, online-archives, flex",
                other
            ),
        },
    }
}

fn parse_storage_tier(storage: Option<String>) -> Result<Option<LogsStorageTier>> {
    match normalize_storage_tier(storage)? {
        None => Ok(None),
        Some(tier) => match tier.as_str() {
            "indexes" => Ok(Some(LogsStorageTier::INDEXES)),
            "online-archives" => Ok(Some(LogsStorageTier::ONLINE_ARCHIVES)),
            "flex" => Ok(Some(LogsStorageTier::FLEX)),
            _ => unreachable!("storage tier is normalized"),
        },
    }
}

/// Split a comma-separated compute string into individual compute expressions,
/// respecting parentheses so that `percentile(@duration, 95)` is not split.
pub fn split_compute_args(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut depth = 0u32;
    for ch in input.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    result.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        result.push(trimmed);
    }
    result
}

const VALID_SORT_AGGREGATIONS: &[&str] = &[
    "count",
    "cardinality",
    "pc75",
    "pc90",
    "pc95",
    "pc98",
    "pc99",
    "sum",
    "min",
    "max",
];

fn parse_aggregate_sort(sort: &str) -> Result<serde_json::Value> {
    let sort = sort.trim().to_lowercase();
    if !VALID_SORT_AGGREGATIONS.contains(&sort.as_str()) {
        bail!(
            "unknown sort aggregation {:?}; valid values are: {}",
            sort,
            VALID_SORT_AGGREGATIONS.join(", ")
        );
    }
    Ok(serde_json::json!({
        "type": "measure",
        "order": "desc",
        "aggregation": sort
    }))
}

#[allow(clippy::too_many_arguments)]
fn build_aggregate_body(
    query: String,
    from_ms: i64,
    to_ms: i64,
    computes: Vec<String>,
    group_bys: Vec<String>,
    limit: i32,
    storage: Option<String>,
    sort: &str,
) -> Result<serde_json::Value> {
    let storage_tier = normalize_storage_tier(storage)?;

    let mut filter = serde_json::json!({
        "query": query,
        "from": from_ms.to_string(),
        "to": to_ms.to_string()
    });
    if let Some(tier) = storage_tier {
        filter["storage_tier"] = serde_json::Value::String(tier);
    }

    let compute_arr: Vec<serde_json::Value> = computes
        .iter()
        .map(|c| {
            let (aggregation, metric) = util::parse_compute_raw(c)?;
            let mut obj = serde_json::json!({ "aggregation": aggregation });
            if let Some(m) = metric {
                obj["metric"] = serde_json::Value::String(m);
            }
            Ok(obj)
        })
        .collect::<Result<Vec<_>>>()?;

    let mut body = serde_json::json!({
        "filter": filter,
        "compute": compute_arr
    });

    if !group_bys.is_empty() {
        let sort_obj = parse_aggregate_sort(sort)?;
        let group_by_arr: Vec<serde_json::Value> = group_bys
            .iter()
            .map(|facet| {
                let mut obj = serde_json::json!({ "facet": facet, "sort": sort_obj });
                if limit > 0 {
                    obj["limit"] = serde_json::json!(limit);
                }
                obj
            })
            .collect();
        body["group_by"] = serde_json::json!(group_by_arr);
    }

    Ok(body)
}

fn parse_logs_sort(sort: &str) -> LogsSort {
    match sort {
        "timestamp" | "asc" | "+timestamp" => LogsSort::TIMESTAMP_ASCENDING,
        _ => LogsSort::TIMESTAMP_DESCENDING,
    }
}

pub async fn search(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    limit: i32,
    sort: String,
    storage: Option<String>,
) -> Result<()> {
    let api = crate::make_api!(LogsAPI, cfg);

    let from_ms = util::parse_time_to_unix_millis(&from)?;
    let to_ms = util::parse_time_to_unix_millis(&to)?;

    let storage_tier = parse_storage_tier(storage)?;

    let mut filter = LogsQueryFilter::new()
        .query(query)
        .from(from_ms.to_string())
        .to(to_ms.to_string());
    if let Some(tier) = storage_tier {
        filter = filter.storage_tier(tier);
    }

    let body = LogsListRequest::new()
        .filter(filter)
        .page(LogsListRequestPage::new().limit(limit))
        .sort(parse_logs_sort(&sort));

    let params = ListLogsOptionalParams::default().body(body);

    let resp = api
        .list_logs(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search logs: {:?}", e))?;

    let meta = if cfg.agent_mode {
        let count = resp.data.as_ref().map(|d| d.len());
        let truncated = count.is_some_and(|c| c as i32 >= limit);
        Some(formatter::Metadata {
            count,
            truncated,
            command: Some("logs search".into()),
            next_action: if truncated {
                Some(format!(
                    "Results may be truncated at {limit}. Use --limit={} or narrow the --query",
                    limit + 1
                ))
            } else {
                None
            },
        })
    } else {
        None
    };
    formatter::format_and_print(
        &resp,
        &cfg.output_format,
        cfg.agent_mode,
        meta.as_ref(),
        cfg.jq.as_deref(),
    )?;
    Ok(())
}

/// Alias for `search` with the same interface.
pub async fn list(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    limit: i32,
    sort: String,
    storage: Option<String>,
) -> Result<()> {
    search(cfg, query, from, to, limit, sort, storage).await
}

/// Alias for `search` with the same interface.
pub async fn query(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    limit: i32,
    sort: String,
    storage: Option<String>,
) -> Result<()> {
    search(cfg, query, from, to, limit, sort, storage).await
}

pub async fn aggregate(cfg: &Config, args: AggregateArgs) -> Result<()> {
    let AggregateArgs {
        query,
        from,
        to,
        mut compute,
        group_by,
        limit,
        storage,
        sort,
    } = args;
    if compute.is_empty() {
        compute.push("count".into());
    }
    let from_ms = util::parse_time_to_unix_millis(&from)?;
    let to_ms = util::parse_time_to_unix_millis(&to)?;
    let body = build_aggregate_body(
        query, from_ms, to_ms, compute, group_by, limit, storage, &sort,
    )?;
    let data = client::raw_post(cfg, "/api/v2/logs/analytics/aggregate", body).await?;
    formatter::output(cfg, &data)?;
    Ok(())
}

pub async fn archives_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(LogsArchivesAPI, cfg);

    let resp = api
        .list_logs_archives()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list log archives: {:?}", e))?;

    formatter::output(cfg, &resp)?;
    Ok(())
}

pub async fn archives_get(cfg: &Config, archive_id: &str) -> Result<()> {
    let api = crate::make_api!(LogsArchivesAPI, cfg);

    let resp = api
        .get_logs_archive(archive_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get log archive: {:?}", e))?;

    formatter::output(cfg, &resp)?;
    Ok(())
}

pub async fn archives_delete(cfg: &Config, archive_id: &str) -> Result<()> {
    let api = crate::make_api!(LogsArchivesAPI, cfg);

    api.delete_logs_archive(archive_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete log archive: {:?}", e))?;

    println!("Log archive {archive_id} deleted.");
    Ok(())
}

pub async fn custom_destinations_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(LogsCustomDestinationsAPI, cfg);

    let resp = api
        .list_logs_custom_destinations()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list custom destinations: {:?}", e))?;

    formatter::output(cfg, &resp)?;
    Ok(())
}

pub async fn custom_destinations_get(cfg: &Config, destination_id: &str) -> Result<()> {
    let api = crate::make_api!(LogsCustomDestinationsAPI, cfg);

    let resp = api
        .get_logs_custom_destination(destination_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get custom destination: {:?}", e))?;

    formatter::output(cfg, &resp)?;
    Ok(())
}

pub async fn metrics_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(LogsMetricsAPI, cfg);

    let resp = api
        .list_logs_metrics()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list log-based metrics: {:?}", e))?;

    formatter::output(cfg, &resp)?;
    Ok(())
}

pub async fn metrics_get(cfg: &Config, metric_id: &str) -> Result<()> {
    let api = crate::make_api!(LogsMetricsAPI, cfg);

    let resp = api
        .get_logs_metric(metric_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get log-based metric: {:?}", e))?;

    formatter::output(cfg, &resp)?;
    Ok(())
}

pub async fn metrics_delete(cfg: &Config, metric_id: &str) -> Result<()> {
    let api = crate::make_api!(LogsMetricsAPI, cfg);

    api.delete_logs_metric(metric_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete log-based metric: {:?}", e))?;

    println!("Log-based metric {metric_id} deleted.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Restriction Queries (raw HTTP - not available in typed client)
// ---------------------------------------------------------------------------

pub async fn restriction_queries_list(cfg: &Config) -> Result<()> {
    let data = client::raw_get(cfg, "/api/v2/logs/config/restriction_queries", &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn restriction_queries_get(cfg: &Config, query_id: &str) -> Result<()> {
    let path = format!("/api/v2/logs/config/restriction_queries/{query_id}");
    let data = client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

#[cfg(test)]
mod tests {
    use crate::config::{Config, OutputFormat};
    use crate::test_support::*;

    use super::*;

    #[test]
    fn test_normalize_storage_tier_alias() {
        let tier = normalize_storage_tier(Some("online_archives".into())).unwrap();
        assert_eq!(tier.unwrap(), "online-archives");
    }

    #[test]
    fn test_build_aggregate_body_includes_compute_group_by_limit_and_storage() {
        let body = build_aggregate_body(
            "service:web".into(),
            1,
            2,
            vec!["avg(@duration)".into()],
            vec!["service".into()],
            3,
            Some("flex".into()),
            "count",
        )
        .unwrap();

        assert_eq!(
            body,
            serde_json::json!({
                "filter": {
                    "query": "service:web",
                    "from": "1",
                    "to": "2",
                    "storage_tier": "flex"
                },
                "compute": [{
                    "aggregation": "avg",
                    "metric": "@duration"
                }],
                "group_by": [{
                    "facet": "service",
                    "limit": 3,
                    "sort": {
                        "type": "measure",
                        "order": "desc",
                        "aggregation": "count"
                    }
                }]
            })
        );
    }

    #[test]
    fn test_build_aggregate_body_omits_group_by_for_plain_count() {
        let body = build_aggregate_body(
            "*".into(),
            1,
            2,
            vec!["count".into()],
            vec![],
            10,
            None,
            "count",
        )
        .unwrap();

        assert_eq!(
            body,
            serde_json::json!({
                "filter": {
                    "query": "*",
                    "from": "1",
                    "to": "2"
                },
                "compute": [{
                    "aggregation": "count"
                }]
            })
        );
    }

    #[test]
    fn test_build_aggregate_body_multiple_computes() {
        let body = build_aggregate_body(
            "*".into(),
            1,
            2,
            vec![
                "count".into(),
                "avg(@duration)".into(),
                "percentile(@duration, 95)".into(),
            ],
            vec![],
            10,
            None,
            "count",
        )
        .unwrap();

        assert_eq!(
            body,
            serde_json::json!({
                "filter": {
                    "query": "*",
                    "from": "1",
                    "to": "2"
                },
                "compute": [
                    { "aggregation": "count" },
                    { "aggregation": "avg", "metric": "@duration" },
                    { "aggregation": "pc95", "metric": "@duration" }
                ]
            })
        );
    }

    #[test]
    fn test_build_aggregate_body_multiple_group_bys() {
        let body = build_aggregate_body(
            "*".into(),
            1,
            2,
            vec!["count".into()],
            vec!["service".into(), "status".into()],
            5,
            None,
            "count",
        )
        .unwrap();

        assert_eq!(
            body,
            serde_json::json!({
                "filter": {
                    "query": "*",
                    "from": "1",
                    "to": "2"
                },
                "compute": [{ "aggregation": "count" }],
                "group_by": [
                    { "facet": "service", "limit": 5, "sort": { "type": "measure", "order": "desc", "aggregation": "count" } },
                    { "facet": "status", "limit": 5, "sort": { "type": "measure", "order": "desc", "aggregation": "count" } }
                ]
            })
        );
    }

    #[test]
    fn test_parse_aggregate_sort_valid_values() {
        for agg in VALID_SORT_AGGREGATIONS {
            let sort = parse_aggregate_sort(agg).unwrap();
            assert_eq!(sort["aggregation"], *agg);
            assert_eq!(sort["order"], "desc");
            assert_eq!(sort["type"], "measure");
        }
    }

    #[test]
    fn test_parse_aggregate_sort_case_insensitive() {
        let sort = parse_aggregate_sort("PC95").unwrap();
        assert_eq!(sort["aggregation"], "pc95");
    }

    #[test]
    fn test_parse_aggregate_sort_trims_whitespace() {
        let sort = parse_aggregate_sort("  sum  ").unwrap();
        assert_eq!(sort["aggregation"], "sum");
    }

    #[test]
    fn test_parse_aggregate_sort_invalid() {
        let err = parse_aggregate_sort("invalid").unwrap_err();
        assert!(err.to_string().contains("unknown sort aggregation"));
    }

    #[test]
    fn test_build_aggregate_body_sort_pc95() {
        let body = build_aggregate_body(
            "*".into(),
            1,
            2,
            vec!["count".into()],
            vec!["host".into()],
            10,
            None,
            "pc95",
        )
        .unwrap();

        assert_eq!(
            body["group_by"][0]["sort"],
            serde_json::json!({
                "type": "measure",
                "order": "desc",
                "aggregation": "pc95"
            })
        );
    }

    #[test]
    fn test_build_aggregate_body_sort_not_included_without_group_by() {
        let body = build_aggregate_body(
            "*".into(),
            1,
            2,
            vec!["count".into()],
            vec![],
            10,
            None,
            "pc95",
        )
        .unwrap();

        assert!(body.get("group_by").is_none());
    }

    #[test]
    fn test_split_compute_args_single() {
        assert_eq!(split_compute_args("count"), vec!["count"]);
    }

    #[test]
    fn test_split_compute_args_multiple() {
        assert_eq!(
            split_compute_args("count,avg(@duration),max(@duration)"),
            vec!["count", "avg(@duration)", "max(@duration)"]
        );
    }

    #[test]
    fn test_split_compute_args_preserves_parens_with_comma() {
        assert_eq!(
            split_compute_args("count,percentile(@duration, 95)"),
            vec!["count", "percentile(@duration, 95)"]
        );
    }

    #[test]
    fn test_split_compute_args_trims_whitespace() {
        assert_eq!(
            split_compute_args(" count , avg(@duration) "),
            vec!["count", "avg(@duration)"]
        );
    }

    #[test]
    fn test_split_compute_args_empty() {
        assert!(split_compute_args("").is_empty());
    }

    #[tokio::test]
    async fn test_logs_search() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "POST", r#"{"data": [], "meta": {"page": {}}}"#).await;

        let result = super::search(
            &cfg,
            "status:error".into(),
            "1h".into(),
            "now".into(),
            10,
            "-timestamp".into(),
            None,
        )
        .await;
        assert!(result.is_ok(), "logs search failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_logs_search_with_oauth() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        std::env::set_var("PUP_MOCK_SERVER", server.url());

        let cfg = Config {
            api_key: None,
            app_key: None,
            access_token: Some("token".into()),
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        let _mock = mock_any(&mut server, "POST", r#"{"data": []}"#).await;

        let result = super::search(
            &cfg,
            "status:error".into(),
            "1h".into(),
            "now".into(),
            10,
            "-timestamp".into(),
            None,
        )
        .await;
        assert!(result.is_ok(), "logs search should work with OAuth");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_logs_aggregate() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "POST", r#"{"data": {"buckets": []}}"#).await;

        let result = super::aggregate(
            &cfg,
            super::AggregateArgs {
                query: "*".into(),
                from: "1h".into(),
                to: "now".into(),
                compute: vec!["count".into()],
                group_by: vec![],
                limit: 10,
                storage: None,
                sort: "count".into(),
            },
        )
        .await;
        assert!(result.is_ok(), "logs aggregate failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_logs_aggregate_multiple_computes() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "POST", r#"{"data": {"buckets": []}}"#).await;

        let result = super::aggregate(
            &cfg,
            super::AggregateArgs {
                query: "*".into(),
                from: "1h".into(),
                to: "now".into(),
                compute: super::split_compute_args(
                    "count,avg(@duration),percentile(@duration, 95)",
                ),
                group_by: vec!["service".into(), "status".into()],
                limit: 10,
                storage: None,
                sort: "count".into(),
            },
        )
        .await;
        assert!(
            result.is_ok(),
            "logs aggregate with multiple computes failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_logs_search_with_flex_storage() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "POST", r#"{"data": [], "meta": {"page": {}}}"#).await;

        let result = super::search(
            &cfg,
            "*".into(),
            "1h".into(),
            "now".into(),
            10,
            "-timestamp".into(),
            Some("flex".into()),
        )
        .await;
        assert!(
            result.is_ok(),
            "logs search with flex failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_logs_search_with_online_archives_storage() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "POST", r#"{"data": [], "meta": {"page": {}}}"#).await;

        let result = super::search(
            &cfg,
            "*".into(),
            "1h".into(),
            "now".into(),
            10,
            "-timestamp".into(),
            Some("online-archives".into()),
        )
        .await;
        assert!(
            result.is_ok(),
            "logs search with online-archives failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_logs_search_with_invalid_storage_tier() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let result = super::search(
            &cfg,
            "*".into(),
            "1h".into(),
            "now".into(),
            10,
            "-timestamp".into(),
            Some("invalid-tier".into()),
        )
        .await;
        assert!(
            result.is_err(),
            "logs search with invalid storage tier should fail"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("unknown storage tier"),
            "error should mention unknown storage tier"
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_logs_aggregate_with_flex_storage() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "POST", r#"{"data": {"buckets": []}}"#).await;

        let result = super::aggregate(
            &cfg,
            super::AggregateArgs {
                query: "*".into(),
                from: "1h".into(),
                to: "now".into(),
                compute: vec!["count".into()],
                group_by: vec![],
                limit: 10,
                storage: Some("flex".into()),
                sort: "count".into(),
            },
        )
        .await;
        assert!(
            result.is_ok(),
            "logs aggregate with flex failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_logs_archives_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"data": []}"#).await;

        let result = super::archives_list(&cfg).await;
        assert!(
            result.is_ok(),
            "logs archives list failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_logs_custom_destinations_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"data": []}"#).await;

        let result = super::custom_destinations_list(&cfg).await;
        assert!(
            result.is_ok(),
            "logs custom destinations list failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_logs_metrics_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"data": []}"#).await;

        let result = super::metrics_list(&cfg).await;
        assert!(
            result.is_ok(),
            "logs metrics list failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_logs_restriction_queries_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // restriction_queries_list uses raw HTTP (not DD client), so mock specific path
        let _mock = server
            .mock("GET", "/api/v2/logs/config/restriction_queries")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": []}"#)
            .create_async()
            .await;

        let result = super::restriction_queries_list(&cfg).await;
        assert!(
            result.is_ok(),
            "logs restriction queries list failed: {:?}",
            result.err()
        );
        cleanup_env();
    }
}

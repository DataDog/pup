use anyhow::{bail, Result};
use datadog_api_client::datadogV2::api_logs::{ListLogsOptionalParams, LogsAPI};
use datadog_api_client::datadogV2::api_logs_archives::LogsArchivesAPI;
use datadog_api_client::datadogV2::api_logs_custom_destinations::LogsCustomDestinationsAPI;
use datadog_api_client::datadogV2::api_logs_metrics::LogsMetricsAPI;
use datadog_api_client::datadogV2::model::{
    LogsListRequest, LogsListRequestPage, LogsQueryFilter, LogsSort, LogsStorageTier,
};

use crate::config::Config;
use crate::formatter;
use crate::raw_client;
use crate::util;
use crate::util_ext;

const SAVED_VIEWS_PATH: &str = "/api/v1/logs/views";

pub struct AggregateArgs {
    pub query: String,
    pub from: String,
    pub to: String,
    pub compute: Vec<String>,
    pub group_by: Vec<String>,
    pub limit: i32,
    pub index: Vec<String>,
    pub storage: Option<String>,
    pub sort: String,
    pub interval: Option<String>,
}

pub struct SearchArgs {
    pub query: String,
    pub from: String,
    pub to: String,
    pub limit: i32,
    pub sort: String,
    pub storage: Option<String>,
    pub index: Vec<String>,
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
    index: Vec<String>,
    storage: Option<String>,
    sort: &str,
    interval: Option<String>,
) -> Result<serde_json::Value> {
    let storage_tier = normalize_storage_tier(storage)?;
    let interval = match interval {
        Some(iv) => Some(util_ext::parse_duration_to_millis(&iv)?.to_string()),
        None => None,
    };

    let mut filter = serde_json::json!({
        "query": query,
        "from": from_ms.to_string(),
        "to": to_ms.to_string()
    });
    if !index.is_empty() {
        filter["indexes"] = serde_json::json!(index);
    }
    if let Some(tier) = storage_tier {
        filter["storage_tier"] = serde_json::Value::String(tier);
    }

    let compute_arr: Vec<serde_json::Value> = computes
        .iter()
        .map(|c| {
            let (aggregation, metric) = util_ext::parse_compute_raw(c)?;
            let mut obj = serde_json::json!({ "aggregation": aggregation });
            if let Some(m) = metric {
                obj["metric"] = serde_json::Value::String(m);
            }
            if let Some(iv) = &interval {
                obj["type"] = serde_json::Value::String("timeseries".into());
                obj["interval"] = serde_json::Value::String(iv.clone());
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

pub async fn search(cfg: &Config, args: SearchArgs) -> Result<()> {
    let SearchArgs {
        query,
        from,
        to,
        limit,
        sort,
        storage,
        index,
    } = args;
    let api = crate::make_api!(LogsAPI, cfg);

    let from_ms = util_ext::parse_time_to_unix_millis(&from)?;
    let to_ms = util_ext::parse_time_to_unix_millis(&to)?;

    let storage_tier = parse_storage_tier(storage)?;

    let mut filter = LogsQueryFilter::new()
        .query(query)
        .from(from_ms.to_string())
        .to(to_ms.to_string());
    if !index.is_empty() {
        filter = filter.indexes(index);
    }
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
pub async fn list(cfg: &Config, args: SearchArgs) -> Result<()> {
    search(cfg, args).await
}

/// Alias for `search` with the same interface.
pub async fn query(cfg: &Config, args: SearchArgs) -> Result<()> {
    search(cfg, args).await
}

pub async fn aggregate(cfg: &Config, args: AggregateArgs) -> Result<()> {
    let AggregateArgs {
        query,
        from,
        to,
        mut compute,
        group_by,
        limit,
        index,
        storage,
        sort,
        interval,
    } = args;
    if compute.is_empty() {
        compute.push("count".into());
    }
    let from_ms = util_ext::parse_time_to_unix_millis(&from)?;
    let to_ms = util_ext::parse_time_to_unix_millis(&to)?;
    let body = build_aggregate_body(
        query, from_ms, to_ms, compute, group_by, limit, index, storage, &sort, interval,
    )?;
    let data = raw_client::raw_post(cfg, "/api/v2/logs/analytics/aggregate", body).await?;
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
    let data = raw_client::raw_get(cfg, "/api/v2/logs/config/restriction_queries", &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn restriction_queries_get(cfg: &Config, query_id: &str) -> Result<()> {
    let path = format!("/api/v2/logs/config/restriction_queries/{query_id}");
    let data = raw_client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

// ---------------------------------------------------------------------------
// Saved Views (raw HTTP - not available in typed client)
// ---------------------------------------------------------------------------

pub async fn saved_views_list(cfg: &Config) -> Result<()> {
    let data = raw_client::raw_get(cfg, SAVED_VIEWS_PATH, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn saved_views_get(cfg: &Config, view_id: &str) -> Result<()> {
    let path = format!("{SAVED_VIEWS_PATH}/{view_id}");
    let data = raw_client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn saved_views_create(cfg: &Config, file: &str) -> Result<()> {
    let body: serde_json::Value = util::read_json_file(file)?;
    let data = raw_client::raw_post(cfg, SAVED_VIEWS_PATH, body).await?;
    formatter::output(cfg, &data)
}

pub async fn saved_views_delete(cfg: &Config, view_id: &str) -> Result<()> {
    let path = format!("{SAVED_VIEWS_PATH}/{view_id}");
    raw_client::raw_delete(cfg, &path).await?;
    println!("Log saved view {view_id} deleted.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::config::{Config, OutputFormat};
    use crate::test_support::*;

    use super::*;

    fn search_args(query: &str, storage: Option<String>, index: Vec<String>) -> SearchArgs {
        SearchArgs {
            query: query.into(),
            from: "1h".into(),
            to: "now".into(),
            limit: 10,
            sort: "-timestamp".into(),
            storage,
            index,
        }
    }

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
            vec![],
            Some("flex".into()),
            "count",
            None,
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
            vec![],
            None,
            "count",
            None,
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
            vec![],
            None,
            "count",
            None,
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
            vec![],
            None,
            "count",
            None,
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
            vec![],
            None,
            "pc95",
            None,
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
            vec![],
            None,
            "pc95",
            None,
        )
        .unwrap();

        assert!(body.get("group_by").is_none());
    }

    #[test]
    fn test_build_aggregate_body_omits_empty_indexes() {
        let body = build_aggregate_body(
            "*".into(),
            1,
            2,
            vec!["count".into()],
            vec![],
            10,
            vec![],
            None,
            "count",
            None,
        )
        .unwrap();

        assert!(body["filter"].get("indexes").is_none());
    }

    #[test]
    fn test_build_aggregate_body_includes_indexes() {
        let body = build_aggregate_body(
            "*".into(),
            1,
            2,
            vec!["count".into()],
            vec![],
            10,
            vec!["main".into(), "web".into()],
            None,
            "count",
            None,
        )
        .unwrap();

        assert_eq!(
            body["filter"]["indexes"],
            serde_json::json!(["main", "web"])
        );
    }

    #[test]
    fn test_build_aggregate_body_timeseries_interval() {
        let body = build_aggregate_body(
            "*".into(),
            1,
            2,
            vec!["count".into(), "avg(@duration)".into()],
            vec![],
            10,
            vec![],
            None,
            "count",
            Some("5m".into()),
        )
        .unwrap();

        assert_eq!(
            body["compute"],
            serde_json::json!([
                { "aggregation": "count", "type": "timeseries", "interval": "300000" },
                { "aggregation": "avg", "metric": "@duration", "type": "timeseries", "interval": "300000" }
            ])
        );
    }

    #[test]
    fn test_build_aggregate_body_invalid_interval() {
        let err = build_aggregate_body(
            "*".into(),
            1,
            2,
            vec!["count".into()],
            vec![],
            10,
            vec![],
            None,
            "count",
            Some("bogus".into()),
        )
        .unwrap_err();
        assert!(err.to_string().contains("unable to parse duration"));
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

        let result = super::search(&cfg, search_args("status:error", None, vec![])).await;
        assert!(result.is_ok(), "logs search failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_logs_search_with_indexes() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .match_body(mockito::Matcher::Regex(
                r#""indexes":\["main","web"\]"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": [], "meta": {"page": {}}}"#)
            .create_async()
            .await;

        let result = super::search(
            &cfg,
            search_args("*", None, vec!["main".into(), "web".into()]),
        )
        .await;
        assert!(
            result.is_ok(),
            "logs search with indexes failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_saved_views_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", "/api/v1/logs/views")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"logs_views": []}"#)
            .create_async()
            .await;

        let result = super::saved_views_list(&cfg).await;
        assert!(
            result.is_ok(),
            "saved views list failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_saved_views_get() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", "/api/v1/logs/views/123")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"logs_view": {"id": 123}}"#)
            .create_async()
            .await;

        let result = super::saved_views_get(&cfg, "123").await;
        assert!(result.is_ok(), "saved views get failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_saved_views_create() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let tmp = TempDir::new("saved_views_create");
        let file = tmp.path().join("view.json");
        std::fs::write(&file, r#"{"name":"Errors","search":"status:error"}"#).unwrap();
        let _mock = server
            .mock("POST", "/api/v1/logs/views")
            .match_body(mockito::Matcher::Regex(r#""name":"Errors""#.to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id": 123, "name": "Errors"}"#)
            .create_async()
            .await;

        let result = super::saved_views_create(&cfg, file.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "saved views create failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_saved_views_create_missing_file_errors() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let result = super::saved_views_create(&cfg, "/tmp/__pup_missing_saved_view__.json").await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("failed to read"));
        cleanup_env();
    }

    #[tokio::test]
    async fn test_saved_views_delete() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("DELETE", "/api/v1/logs/views/123")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"deleted_logs_saved_view_id": 123}"#)
            .create_async()
            .await;

        let result = super::saved_views_delete(&cfg, "123").await;
        assert!(
            result.is_ok(),
            "saved views delete failed: {:?}",
            result.err()
        );
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

        let result = super::search(&cfg, search_args("status:error", None, vec![])).await;
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
                index: vec![],
                storage: None,
                sort: "count".into(),
                interval: None,
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
                index: vec![],
                storage: None,
                sort: "count".into(),
                interval: None,
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

        let result = super::search(&cfg, search_args("*", Some("flex".into()), vec![])).await;
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
            search_args("*", Some("online-archives".into()), vec![]),
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

        let result =
            super::search(&cfg, search_args("*", Some("invalid-tier".into()), vec![])).await;
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
                index: vec![],
                storage: Some("flex".into()),
                sort: "count".into(),
                interval: None,
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

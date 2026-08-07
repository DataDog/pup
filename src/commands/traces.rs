use anyhow::{bail, Result};
use datadog_api_client::datadogV2::api_spans::SpansAPI;
use datadog_api_client::datadogV2::api_spans_metrics::SpansMetricsAPI;
use datadog_api_client::datadogV2::model::{
    SpansAggregateData, SpansAggregateRequest, SpansAggregateRequestAttributes,
    SpansAggregateRequestType, SpansAggregationFunction, SpansCompute, SpansGroupBy,
    SpansListRequest, SpansListRequestAttributes, SpansListRequestData, SpansListRequestPage,
    SpansListRequestType, SpansQueryFilter, SpansSort,
};

use crate::config::Config;
use crate::formatter;
use crate::util;
use crate::util_ext;

// ---------------------------------------------------------------------------
// Spans Metrics
// ---------------------------------------------------------------------------

pub async fn metrics_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(SpansMetricsAPI, cfg);
    let resp = api
        .list_spans_metrics()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list spans metrics: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metrics_get(cfg: &Config, metric_id: &str) -> Result<()> {
    let api = crate::make_api!(SpansMetricsAPI, cfg);
    let resp = api
        .get_spans_metric(metric_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get spans metric: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metrics_create(cfg: &Config, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::SpansMetricCreateRequest =
        util::read_json_file(file)?;
    let api = crate::make_api!(SpansMetricsAPI, cfg);
    let resp = api
        .create_spans_metric(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create spans metric: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metrics_update(cfg: &Config, metric_id: &str, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::SpansMetricUpdateRequest =
        util::read_json_file(file)?;
    let api = crate::make_api!(SpansMetricsAPI, cfg);
    let resp = api
        .update_spans_metric(metric_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update spans metric: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metrics_delete(cfg: &Config, metric_id: &str) -> Result<()> {
    let api = crate::make_api!(SpansMetricsAPI, cfg);
    api.delete_spans_metric(metric_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete spans metric: {e:?}"))?;
    Ok(())
}

/// Validate the sort parameter.
fn validate_sort(sort: &str) -> Result<()> {
    match sort {
        "timestamp" | "-timestamp" => Ok(()),
        _ => bail!(
            "invalid --sort value: {sort:?}\nExpected: timestamp (ascending) or -timestamp (descending)"
        ),
    }
}

/// Parse a compute string into (SpansAggregationFunction, Option<metric>).
fn parse_compute(input: &str) -> Result<(SpansAggregationFunction, Option<String>)> {
    let (func, metric) = util_ext::parse_compute_raw(input)?;
    let agg = match func.as_str() {
        "count" => SpansAggregationFunction::COUNT,
        "avg" => SpansAggregationFunction::AVG,
        "sum" => SpansAggregationFunction::SUM,
        "min" => SpansAggregationFunction::MIN,
        "max" => SpansAggregationFunction::MAX,
        "median" => SpansAggregationFunction::MEDIAN,
        "cardinality" => SpansAggregationFunction::CARDINALITY,
        "pc75" => SpansAggregationFunction::PERCENTILE_75,
        "pc90" => SpansAggregationFunction::PERCENTILE_90,
        "pc95" => SpansAggregationFunction::PERCENTILE_95,
        "pc98" => SpansAggregationFunction::PERCENTILE_98,
        "pc99" => SpansAggregationFunction::PERCENTILE_99,
        _ => bail!("unknown aggregation function: {func}"),
    };
    Ok((agg, metric))
}

#[allow(clippy::too_many_arguments)]
pub async fn search(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    limit: i32,
    sort: String,
    cursor: Option<String>,
    live: bool,
) -> Result<()> {
    validate_sort(&sort)?;

    let api = crate::make_api!(SpansAPI, cfg);

    let from_ms = util_ext::parse_time_to_unix_millis(&from)?;
    // Live search always ends at now so the query lands in Datadog's live
    // (recent, unsampled) trace buffer rather than the indexed store.
    let to_ms = if live {
        util_ext::parse_time_to_unix_millis("now")?
    } else {
        util_ext::parse_time_to_unix_millis(&to)?
    };

    if !(1..=1000).contains(&limit) {
        anyhow::bail!("--limit must be between 1 and 1000, got {limit}");
    }
    let page_limit = limit;
    let spans_sort = match sort.as_str() {
        "timestamp" => SpansSort::TIMESTAMP_ASCENDING,
        _ => SpansSort::TIMESTAMP_DESCENDING,
    };

    let mut page = SpansListRequestPage::new().limit(page_limit);
    if let Some(c) = cursor {
        page = page.cursor(c);
    }

    let body = SpansListRequest::new().data(
        SpansListRequestData::new()
            .type_(SpansListRequestType::SEARCH_REQUEST)
            .attributes(
                SpansListRequestAttributes::new()
                    .filter(
                        SpansQueryFilter::new()
                            .query(query)
                            .from(from_ms.to_string())
                            .to(to_ms.to_string()),
                    )
                    .page(page)
                    .sort(spans_sort),
            ),
    );

    let resp = api
        .list_spans(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search spans: {:?}", e))?;

    let next_cursor = resp
        .meta
        .as_ref()
        .and_then(|m| m.page.as_ref())
        .and_then(|p| p.after.clone());

    let meta = if cfg.agent_mode {
        let count = resp.data.as_ref().map(|d| d.len());
        Some(formatter::Metadata {
            count,
            truncated: next_cursor.is_some(),
            command: Some("traces search".into()),
            next_action: next_cursor.map(|c| {
                format!("More results available. Use --cursor=\"{c}\" to page backwards through older spans.")
            }),
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

pub async fn aggregate(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    compute: String,
    group_by: Option<String>,
) -> Result<()> {
    let (agg_fn, metric) = parse_compute(&compute)?;

    let api = crate::make_api!(SpansAPI, cfg);

    let from_ms = util_ext::parse_time_to_unix_millis(&from)?;
    let to_ms = util_ext::parse_time_to_unix_millis(&to)?;

    let mut spans_compute = SpansCompute::new(agg_fn);
    if let Some(m) = metric {
        spans_compute = spans_compute.metric(m);
    }

    let mut attrs = SpansAggregateRequestAttributes::new()
        .compute(vec![spans_compute])
        .filter(
            SpansQueryFilter::new()
                .query(query)
                .from(from_ms.to_string())
                .to(to_ms.to_string()),
        );

    if let Some(facet) = group_by {
        attrs = attrs.group_by(vec![SpansGroupBy::new(facet)]);
    }

    let body = SpansAggregateRequest::new().data(
        SpansAggregateData::new()
            .type_(SpansAggregateRequestType::AGGREGATE_REQUEST)
            .attributes(attrs),
    );

    let resp = api
        .aggregate_spans(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to aggregate spans: {:?}", e))?;

    let meta = if cfg.agent_mode {
        Some(formatter::Metadata {
            count: None,
            truncated: false,
            command: Some("traces aggregate".into()),
            next_action: None,
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use crate::test_support::*;

    use super::*;
    use datadog_api_client::datadogV2::model::SpansAggregationFunction;

    #[test]
    fn test_parse_compute_count() {
        let (agg, metric) = parse_compute("count").unwrap();
        assert_eq!(agg, SpansAggregationFunction::COUNT);
        assert!(metric.is_none());
    }

    #[test]
    fn test_parse_compute_avg() {
        let (agg, metric) = parse_compute("avg(@duration)").unwrap();
        assert_eq!(agg, SpansAggregationFunction::AVG);
        assert_eq!(metric.unwrap(), "@duration");
    }

    #[test]
    fn test_parse_compute_sum() {
        let (agg, metric) = parse_compute("sum(@duration)").unwrap();
        assert_eq!(agg, SpansAggregationFunction::SUM);
        assert_eq!(metric.unwrap(), "@duration");
    }

    #[test]
    fn test_parse_compute_min() {
        let (agg, metric) = parse_compute("min(@duration)").unwrap();
        assert_eq!(agg, SpansAggregationFunction::MIN);
        assert_eq!(metric.unwrap(), "@duration");
    }

    #[test]
    fn test_parse_compute_max() {
        let (agg, metric) = parse_compute("max(@duration)").unwrap();
        assert_eq!(agg, SpansAggregationFunction::MAX);
        assert_eq!(metric.unwrap(), "@duration");
    }

    #[test]
    fn test_parse_compute_median() {
        let (agg, metric) = parse_compute("median(@duration)").unwrap();
        assert_eq!(agg, SpansAggregationFunction::MEDIAN);
        assert_eq!(metric.unwrap(), "@duration");
    }

    #[test]
    fn test_parse_compute_cardinality() {
        let (agg, metric) = parse_compute("cardinality(@usr.id)").unwrap();
        assert_eq!(agg, SpansAggregationFunction::CARDINALITY);
        assert_eq!(metric.unwrap(), "@usr.id");
    }

    #[test]
    fn test_parse_compute_percentile_99() {
        let (agg, metric) = parse_compute("percentile(@duration, 99)").unwrap();
        assert_eq!(agg, SpansAggregationFunction::PERCENTILE_99);
        assert_eq!(metric.unwrap(), "@duration");
    }

    #[test]
    fn test_parse_compute_percentile_95() {
        let (agg, metric) = parse_compute("percentile(@duration, 95)").unwrap();
        assert_eq!(agg, SpansAggregationFunction::PERCENTILE_95);
        assert_eq!(metric.unwrap(), "@duration");
    }

    #[test]
    fn test_parse_compute_percentile_90() {
        let (agg, metric) = parse_compute("percentile(@duration, 90)").unwrap();
        assert_eq!(agg, SpansAggregationFunction::PERCENTILE_90);
        assert_eq!(metric.unwrap(), "@duration");
    }

    #[test]
    fn test_parse_compute_empty() {
        assert!(parse_compute("").is_err());
    }

    #[test]
    fn test_parse_compute_invalid() {
        assert!(parse_compute("invalid").is_err());
    }

    #[test]
    fn test_parse_compute_unknown_function() {
        assert!(parse_compute("foo(@bar)").is_err());
    }

    #[test]
    fn test_parse_compute_unsupported_percentile() {
        assert!(parse_compute("percentile(@duration, 50)").is_err());
    }

    #[test]
    fn test_parse_compute_percentile_missing_value() {
        assert!(parse_compute("percentile(@duration)").is_err());
    }

    #[test]
    fn test_parse_compute_count_with_field_rejected() {
        let err = parse_compute("count(@duration)").unwrap_err();
        assert!(err.to_string().contains("does not accept a field"));
    }

    #[test]
    fn test_validate_sort_valid() {
        assert!(validate_sort("timestamp").is_ok());
        assert!(validate_sort("-timestamp").is_ok());
    }

    #[test]
    fn test_validate_sort_invalid() {
        assert!(validate_sort("garbage").is_err());
        assert!(validate_sort("").is_err());
        assert!(validate_sort("asc").is_err());
    }

    #[tokio::test]
    async fn test_spans_metrics_list() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
        let result = super::metrics_list(&cfg).await;
        assert!(
            result.is_ok(),
            "spans metrics list failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_spans_metrics_list_accepts_oauth_bearer_token() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        // Simulate OAuth-only auth: bearer token configured, no API/APP keys.
        cfg.api_key = None;
        cfg.app_key = None;
        cfg.access_token = Some("oauth-bearer-token".into());
        std::env::remove_var("DD_API_KEY");
        std::env::remove_var("DD_APP_KEY");

        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .match_header("Authorization", "Bearer oauth-bearer-token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        let result = super::metrics_list(&cfg).await;
        assert!(
            result.is_ok(),
            "spans metrics list with OAuth bearer failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_spans_metrics_get() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(
            &mut server,
            "GET",
            r#"{"data":{"id":"test.metric","type":"spans_metrics","attributes":{}}}"#,
        )
        .await;
        let result = super::metrics_get(&cfg, "test.metric").await;
        assert!(
            result.is_ok(),
            "spans metrics get failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_spans_metrics_delete() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "DELETE", "").await;
        let result = super::metrics_delete(&cfg, "test.metric").await;
        assert!(
            result.is_ok(),
            "spans metrics delete failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_spans_metrics_get_path() {
        // Verify the GET request hits the correct API path for a named metric.
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/v2/apm/config/metrics/my.metric")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":{"id":"my.metric","type":"spans_metrics","attributes":{}}}"#)
            .create_async()
            .await;
        let result = super::metrics_get(&cfg, "my.metric").await;
        assert!(
            result.is_ok(),
            "spans metrics get (path check) failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_spans_metrics_list_error() {
        // Verify that a 403 response causes metrics_list to return an error.
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
        let result = super::metrics_list(&cfg).await;
        assert!(result.is_err(), "spans metrics list should fail on 403");
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_search_limit_too_small() {
        let cfg = test_config("http://unused.local");
        let result = super::search(
            &cfg,
            "*".into(),
            "1h".into(),
            "now".into(),
            0,
            "-timestamp".into(),
            None,
            false,
        )
        .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("--limit must be between 1 and 1000"));
    }

    #[tokio::test]
    async fn test_search_limit_too_large() {
        let cfg = test_config("http://unused.local");
        let result = super::search(
            &cfg,
            "*".into(),
            "1h".into(),
            "now".into(),
            1001,
            "-timestamp".into(),
            None,
            false,
        )
        .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("--limit must be between 1 and 1000"));
    }

    #[tokio::test]
    async fn test_search_sends_cursor_and_surfaces_next_cursor() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        cfg.agent_mode = true;
        let mock = server
            .mock("POST", "/api/v2/spans/events/search")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "data": {"attributes": {"page": {"cursor": "prev-cursor"}}}
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[],"meta":{"page":{"after":"next-cursor"}}}"#)
            .create_async()
            .await;

        let result = super::search(
            &cfg,
            "*".into(),
            "1h".into(),
            "now".into(),
            50,
            "-timestamp".into(),
            Some("prev-cursor".into()),
            false,
        )
        .await;

        assert!(result.is_ok(), "search failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_search_live_forces_to_now() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("POST", "/api/v2/spans/events/search")
            .match_body(mockito::Matcher::Regex(r#""to":"\d{13}""#.to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        // --to is set far in the past; --live must override it to "now".
        let result = super::search(
            &cfg,
            "*".into(),
            "1h".into(),
            "2000-01-01T00:00:00Z".into(),
            50,
            "-timestamp".into(),
            None,
            true,
        )
        .await;

        assert!(result.is_ok(), "live search failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }
}

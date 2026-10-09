use anyhow::{bail, Context, Result};
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
use crate::raw_client;
use crate::util;
use crate::util_ext;

fn validate_trace_id(trace_id: &str) -> Result<()> {
    let hexadecimal = trace_id.len() == 32 && trace_id.bytes().all(|b| b.is_ascii_hexdigit());
    let decimal = !trace_id.is_empty()
        && trace_id.len() <= 39
        && trace_id.bytes().all(|b| b.is_ascii_digit());
    if !hexadecimal && !decimal {
        bail!("trace ID must be 32 hexadecimal characters or a decimal string of up to 39 digits");
    }
    Ok(())
}

/// Reject a trace response that lacks the spans array or the truncation flag.
fn validate_trace_response(response: &serde_json::Value) -> Result<()> {
    response
        .pointer("/data/attributes/spans")
        .and_then(serde_json::Value::as_array)
        .context("trace response is missing the spans array")?;
    response
        .pointer("/data/attributes/is_truncated")
        .and_then(serde_json::Value::as_bool)
        .context("trace response is missing the is_truncated flag")?;
    Ok(())
}

/// Retrieve the stored trace rather than only its indexed spans. Keep the raw
/// JSON representation so numeric and string span IDs retain their precision.
pub async fn get(cfg: &Config, trace_id: &str) -> Result<()> {
    validate_trace_id(trace_id)?;
    let response = raw_client::raw_get(cfg, &format!("/api/v2/trace/{trace_id}"), &[])
        .await
        .context("failed to get trace")?;
    validate_trace_response(&response)?;
    formatter::format_and_print(&response, &cfg.output_format, cfg.jq.as_deref())
}

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

    formatter::format_and_print(&resp, &cfg.output_format, cfg.jq.as_deref())?;
    let next_cursor = resp
        .meta
        .as_ref()
        .and_then(|m| m.page.as_ref())
        .and_then(|p| p.after.as_deref());
    crate::output::eprint_next_page_hint(cfg, next_cursor);
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

    formatter::format_and_print(&resp, &cfg.output_format, cfg.jq.as_deref())?;
    Ok(())
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use crate::test_support::*;

    use super::*;
    use datadog_api_client::datadogV2::model::SpansAggregationFunction;

    #[test]
    fn test_validate_trace_id() {
        for id in [
            "af3f0726cca34527c552a24cf0f406e6",
            "AF3F0726CCA34527C552A24CF0F406E6",
            "14218605424905815782",
            "232942159015317696491581973883660797670",
        ] {
            assert!(validate_trace_id(id).is_ok(), "rejected {id}");
        }
        for id in [
            "",
            "../trace",
            "1?unexpected=value",
            "abc",
            "-1",
            "+1",
            "af3f0726cca34527c552a24cf0f40xyz",
            "3079693305339682435991237200369920885916",
        ] {
            assert!(validate_trace_id(id).is_err(), "accepted {id}");
        }
    }

    #[test]
    fn test_trace_response_validates_and_keeps_id_precision() {
        let response: serde_json::Value = serde_json::from_str(
            r#"{"data":{"attributes":{"is_truncated":true,"spans":[
                {"spanID":17158905238077369281,"parentID":"9329962688430045371"}
            ]}}}"#,
        )
        .unwrap();
        assert!(validate_trace_response(&response).is_ok());
        assert_eq!(
            response["data"]["attributes"]["spans"][0]["spanID"].as_u64(),
            Some(17158905238077369281)
        );
        assert_eq!(
            response["data"]["attributes"]["spans"][0]["parentID"],
            "9329962688430045371"
        );
        let roundtrip: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&response).unwrap()).unwrap();
        assert_eq!(response, roundtrip);
    }

    #[test]
    fn test_trace_response_rejects_incomplete_response() {
        for response in [
            serde_json::json!(null),
            serde_json::json!({"data":{"attributes":{"is_truncated":false}}}),
            serde_json::json!({"data":{"attributes":{"spans":[],"is_truncated":"false"}}}),
        ] {
            assert!(validate_trace_response(&response).is_err());
        }
        let response = serde_json::json!({"data":{"attributes":{"spans":[],"is_truncated":false}}});
        assert!(validate_trace_response(&response).is_ok());
    }

    #[tokio::test]
    async fn test_get_trace_uses_full_trace_endpoint_and_oauth() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        cfg.access_token = Some("test-trace-token".into());
        let mock = server.mock("GET", "/api/v2/trace/a8e0e1080f4403c7c7dc4c2e8eae3a34")
            .match_query(mockito::Matcher::Missing)
            .match_header("authorization", "Bearer test-trace-token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":{"id":"a8e0e1080f4403c7c7dc4c2e8eae3a34","type":"trace","attributes":{"is_truncated":false,"spans":[{"spanID":"17158905238077369281","parentID":"0"}]}}}"#)
            .create_async().await;
        let result = get(&cfg, "a8e0e1080f4403c7c7dc4c2e8eae3a34").await;
        cleanup_env();
        assert!(result.is_ok(), "{result:?}");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_get_trace_rejects_invalid_input() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused.local");
        assert!(get(&cfg, "../trace").await.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_get_trace_propagates_http_errors() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        for status in [403, 404, 429] {
            let mock = server
                .mock("GET", "/api/v2/trace/14401469471269993012")
                .with_status(status)
                .with_body(r#"{"errors":[{"detail":"trace unavailable"}]}"#)
                .create_async()
                .await;
            let error = get(&cfg, "14401469471269993012").await.unwrap_err();
            assert_eq!(
                error
                    .downcast_ref::<raw_client::HttpError>()
                    .unwrap()
                    .status,
                status as u16
            );
            mock.assert_async().await;
        }
        cleanup_env();
    }

    #[tokio::test]
    async fn test_get_trace_rejects_malformed_response() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        for body in ["not JSON", r#"{"data":{"attributes":{"spans":[]}}}"#] {
            let mock = server
                .mock("GET", "/api/v2/trace/14401469471269993012")
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(body)
                .create_async()
                .await;
            assert!(get(&cfg, "14401469471269993012").await.is_err());
            mock.assert_async().await;
        }
        cleanup_env();
    }

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
        let _lock = lock_env().await;
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
        cleanup_env();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("--limit must be between 1 and 1000"));
    }

    #[tokio::test]
    async fn test_search_limit_too_large() {
        let _lock = lock_env().await;
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
        cleanup_env();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("--limit must be between 1 and 1000"));
    }

    #[tokio::test]
    async fn test_search_sends_cursor() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
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

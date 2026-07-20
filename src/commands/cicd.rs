use anyhow::Result;
use datadog_api_client::datadogV2::api_ci_visibility_pipelines::{
    CIVisibilityPipelinesAPI, SearchCIAppPipelineEventsOptionalParams,
};
use datadog_api_client::datadogV2::api_ci_visibility_tests::{
    CIVisibilityTestsAPI, ListCIAppTestEventsOptionalParams, SearchCIAppTestEventsOptionalParams,
};
use datadog_api_client::datadogV2::api_dora_metrics::DORAMetricsAPI;
use datadog_api_client::datadogV2::api_test_optimization::{
    SearchFlakyTestsOptionalParams, TestOptimizationAPI,
};
use datadog_api_client::datadogV2::model::{
    CIAppAggregationFunction, CIAppCompute, CIAppPipelineEventsRequest,
    CIAppPipelinesAggregateRequest, CIAppPipelinesGroupBy, CIAppPipelinesQueryFilter,
    CIAppQueryPageOptions, CIAppSort, CIAppTestEventsRequest, CIAppTestsAggregateRequest,
    CIAppTestsGroupBy, CIAppTestsQueryFilter, DORADeploymentPatchRequest, FlakyTestsSearchRequest,
    UpdateFlakyTestsRequest,
};

use crate::config::Config;
use crate::formatter;
use crate::util_ext;

pub async fn pipelines_list(
    cfg: &Config,
    query: Option<String>,
    from: String,
    to: String,
    limit: i32,
    branch: Option<String>,
    pipeline_name: Option<String>,
) -> Result<()> {
    let api = crate::make_api!(CIVisibilityPipelinesAPI, cfg);

    let from_str = util_ext::parse_time_to_datetime(&from)?.to_rfc3339();
    let to_str = util_ext::parse_time_to_datetime(&to)?.to_rfc3339();

    let mut query_parts: Vec<String> = Vec::new();
    if let Some(q) = query {
        query_parts.push(q);
    }
    if let Some(b) = branch {
        query_parts.push(format!("@git.branch:\"{}\"", b.replace('"', "\\\"")));
    }
    if let Some(p) = pipeline_name {
        query_parts.push(format!("@ci.pipeline.name:\"{}\"", p.replace('"', "\\\"")));
    }

    let mut filter = CIAppPipelinesQueryFilter::new().from(from_str).to(to_str);
    if !query_parts.is_empty() {
        filter = filter.query(query_parts.join(" "));
    }

    let body = CIAppPipelineEventsRequest::new()
        .filter(filter)
        .page(CIAppQueryPageOptions::new().limit(limit))
        .sort(CIAppSort::TIMESTAMP_DESCENDING);

    let params = SearchCIAppPipelineEventsOptionalParams::default().body(body);
    let resp = api
        .search_ci_app_pipeline_events(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list pipelines: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn tests_list(
    cfg: &Config,
    query: Option<String>,
    from: String,
    to: String,
    limit: i32,
) -> Result<()> {
    let api = crate::make_api!(CIVisibilityTestsAPI, cfg);

    let from_dt = util_ext::parse_time_to_datetime(&from)?;
    let to_dt = util_ext::parse_time_to_datetime(&to)?;

    let mut params = ListCIAppTestEventsOptionalParams::default()
        .filter_from(from_dt)
        .filter_to(to_dt)
        .page_limit(limit);
    if let Some(q) = query {
        params = params.filter_query(q);
    }

    let resp = api
        .list_ci_app_test_events(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list tests: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn events_search(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    limit: i32,
    sort: String,
    level: String,
) -> Result<()> {
    let api = crate::make_api!(CIVisibilityPipelinesAPI, cfg);

    let from_str = util_ext::parse_time_to_datetime(&from)?.to_rfc3339();
    let to_str = util_ext::parse_time_to_datetime(&to)?.to_rfc3339();

    let sort_val = match sort.as_str() {
        "asc" | "timestamp" => CIAppSort::TIMESTAMP_ASCENDING,
        "desc" | "-timestamp" => CIAppSort::TIMESTAMP_DESCENDING,
        other => anyhow::bail!(
            "invalid --sort value: {other:?}\nExpected: asc (ascending) or desc (descending)"
        ),
    };

    let level = level.to_lowercase();
    let effective_query = match level.as_str() {
        "pipeline" => query,
        "stage" | "job" | "step" => match query.trim() {
            "" => format!("@ci.level:{level}"),
            q => format!("@ci.level:{level} {q}"),
        },
        other => anyhow::bail!(
            "invalid --level value: {other:?}\nExpected: pipeline, stage, job, or step"
        ),
    };

    let filter = CIAppPipelinesQueryFilter::new()
        .from(from_str)
        .to(to_str)
        .query(effective_query);

    let body = CIAppPipelineEventsRequest::new()
        .filter(filter)
        .page(CIAppQueryPageOptions::new().limit(limit))
        .sort(sort_val);

    let params = SearchCIAppPipelineEventsOptionalParams::default().body(body);
    let resp = api
        .search_ci_app_pipeline_events(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search pipeline events: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn events_aggregate(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    compute: String,
    group_by: Option<String>,
    limit: i64,
) -> Result<()> {
    let api = crate::make_api!(CIVisibilityPipelinesAPI, cfg);

    let from_str = util_ext::parse_time_to_datetime(&from)?.to_rfc3339();
    let to_str = util_ext::parse_time_to_datetime(&to)?.to_rfc3339();
    let compute_spec = build_ci_compute_spec(&compute)?;

    let filter = CIAppPipelinesQueryFilter::new()
        .from(from_str)
        .to(to_str)
        .query(query);

    let mut body = CIAppPipelinesAggregateRequest::new()
        .compute(vec![compute_spec])
        .filter(filter);

    if let Some(gb) = group_by {
        body = body.group_by(vec![CIAppPipelinesGroupBy::new(gb).limit(limit)]);
    }

    let resp = api
        .aggregate_ci_app_pipeline_events(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to aggregate pipeline events: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn tests_search(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    limit: i32,
) -> Result<()> {
    let api = crate::make_api!(CIVisibilityTestsAPI, cfg);

    let from_str = util_ext::parse_time_to_datetime(&from)?.to_rfc3339();
    let to_str = util_ext::parse_time_to_datetime(&to)?.to_rfc3339();

    let filter = CIAppTestsQueryFilter::new()
        .from(from_str)
        .to(to_str)
        .query(query);

    let body = CIAppTestEventsRequest::new()
        .filter(filter)
        .page(CIAppQueryPageOptions::new().limit(limit))
        .sort(CIAppSort::TIMESTAMP_DESCENDING);

    let params = SearchCIAppTestEventsOptionalParams::default().body(body);
    let resp = api
        .search_ci_app_test_events(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search test events: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn tests_aggregate(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    compute: String,
    group_by: Option<String>,
    limit: i64,
) -> Result<()> {
    let api = crate::make_api!(CIVisibilityTestsAPI, cfg);

    let from_str = util_ext::parse_time_to_datetime(&from)?.to_rfc3339();
    let to_str = util_ext::parse_time_to_datetime(&to)?.to_rfc3339();
    let compute_spec = build_ci_compute_spec(&compute)?;

    let filter = CIAppTestsQueryFilter::new()
        .from(from_str)
        .to(to_str)
        .query(query);

    let mut body = CIAppTestsAggregateRequest::new()
        .compute(vec![compute_spec])
        .filter(filter);

    if let Some(gb) = group_by {
        body = body.group_by(vec![CIAppTestsGroupBy::new(gb).limit(limit)]);
    }

    let resp = api
        .aggregate_ci_app_test_events(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to aggregate test events: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Pipelines Get ----

pub async fn pipelines_get(cfg: &Config, pipeline_id: &str) -> Result<()> {
    let api = crate::make_api!(CIVisibilityPipelinesAPI, cfg);

    let filter = CIAppPipelinesQueryFilter::new().query(pipeline_id.to_string());

    let body = CIAppPipelineEventsRequest::new().filter(filter);

    let params = SearchCIAppPipelineEventsOptionalParams::default().body(body);
    let resp = api
        .search_ci_app_pipeline_events(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get pipeline: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- DORA Metrics ----

pub async fn dora_patch_deployment(cfg: &Config, deployment_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(DORAMetricsAPI, cfg);
    let body: DORADeploymentPatchRequest = crate::util::read_json_file(file)?;
    api.patch_dora_deployment(deployment_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to patch DORA deployment: {e:?}"))?;
    println!("DORA deployment '{deployment_id}' patched successfully.");
    Ok(())
}

// ---- Flaky Tests ----

pub async fn flaky_tests_search(
    cfg: &Config,
    query: Option<String>,
    cursor: Option<String>,
    limit: i64,
    sort: Option<String>,
) -> Result<()> {
    let api = crate::make_api!(TestOptimizationAPI, cfg);

    use datadog_api_client::datadogV2::model::{
        FlakyTestsSearchFilter, FlakyTestsSearchPageOptions, FlakyTestsSearchRequestAttributes,
        FlakyTestsSearchRequestData, FlakyTestsSearchRequestDataType, FlakyTestsSearchSort,
    };
    let mut attrs = FlakyTestsSearchRequestAttributes::new();
    if let Some(q) = query {
        let filter = FlakyTestsSearchFilter::new().query(q);
        attrs = attrs.filter(filter);
    }
    let mut page_opts = FlakyTestsSearchPageOptions::new().limit(limit);
    if let Some(c) = cursor {
        page_opts = page_opts.cursor(c);
    }
    attrs = attrs.page(page_opts);
    if let Some(s) = sort {
        let sort_val = match s.as_str() {
            "fqn" => FlakyTestsSearchSort::FQN_ASCENDING,
            "-fqn" => FlakyTestsSearchSort::FQN_DESCENDING,
            "first_flaked" => FlakyTestsSearchSort::FIRST_FLAKED_ASCENDING,
            "-first_flaked" => FlakyTestsSearchSort::FIRST_FLAKED_DESCENDING,
            "last_flaked" => FlakyTestsSearchSort::LAST_FLAKED_ASCENDING,
            "-last_flaked" => FlakyTestsSearchSort::LAST_FLAKED_DESCENDING,
            "failure_rate" => FlakyTestsSearchSort::FAILURE_RATE_ASCENDING,
            "-failure_rate" => FlakyTestsSearchSort::FAILURE_RATE_DESCENDING,
            "pipelines_failed" => FlakyTestsSearchSort::PIPELINES_FAILED_ASCENDING,
            "-pipelines_failed" => FlakyTestsSearchSort::PIPELINES_FAILED_DESCENDING,
            "pipelines_duration_lost" => FlakyTestsSearchSort::PIPELINES_DURATION_LOST_ASCENDING,
            "-pipelines_duration_lost" => FlakyTestsSearchSort::PIPELINES_DURATION_LOST_DESCENDING,
            _ => anyhow::bail!("invalid sort value: {s}. Valid values: fqn, -fqn, first_flaked, -first_flaked, last_flaked, -last_flaked, failure_rate, -failure_rate, pipelines_failed, -pipelines_failed, pipelines_duration_lost, -pipelines_duration_lost"),
        };
        attrs = attrs.sort(sort_val);
    }
    let data = FlakyTestsSearchRequestData::new()
        .attributes(attrs)
        .type_(FlakyTestsSearchRequestDataType::SEARCH_FLAKY_TESTS_REQUEST);
    let body = FlakyTestsSearchRequest::new().data(data);

    let params = SearchFlakyTestsOptionalParams::default().body(body);
    let resp = api
        .search_flaky_tests(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search flaky tests: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn flaky_tests_update(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(TestOptimizationAPI, cfg);
    let body: UpdateFlakyTestsRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_flaky_tests(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update flaky tests: {e:?}"))?;
    formatter::output(cfg, &resp)
}

fn parse_ci_agg(compute: &str) -> Result<(CIAppAggregationFunction, Option<String>)> {
    let (func, metric) = util_ext::parse_compute_raw(compute)?;
    let agg = match func.as_str() {
        "count" => CIAppAggregationFunction::COUNT,
        "avg" => CIAppAggregationFunction::AVG,
        "sum" => CIAppAggregationFunction::SUM,
        "min" => CIAppAggregationFunction::MIN,
        "max" => CIAppAggregationFunction::MAX,
        "median" => CIAppAggregationFunction::MEDIAN,
        "cardinality" => CIAppAggregationFunction::CARDINALITY,
        "pc75" => CIAppAggregationFunction::PERCENTILE_75,
        "pc90" => CIAppAggregationFunction::PERCENTILE_90,
        "pc95" => CIAppAggregationFunction::PERCENTILE_95,
        "pc98" => CIAppAggregationFunction::PERCENTILE_98,
        "pc99" => CIAppAggregationFunction::PERCENTILE_99,
        _ => anyhow::bail!("unknown aggregation function: {func}"),
    };
    Ok((agg, metric))
}

fn build_ci_compute_spec(compute: &str) -> Result<CIAppCompute> {
    let (agg_fn, metric) = parse_ci_agg(compute)?;
    let mut spec = CIAppCompute::new(agg_fn);
    if let Some(m) = metric {
        spec = spec.metric(m);
    }
    Ok(spec)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_cicd_pipelines_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::pipelines_list(&cfg, None, "1h".into(), "now".into(), 10, None, None).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cicd_tests_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::tests_list(&cfg, None, "1h".into(), "now".into(), 10).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cicd_flaky_tests_search() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::flaky_tests_search(
            &cfg,
            Some("@test.service:my-service".into()),
            None,
            50,
            Some("-last_flaked".into()),
        )
        .await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cicd_flaky_tests_search_invalid_sort() {
        let _lock = lock_env().await;
        let s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let result = super::flaky_tests_search(&cfg, None, None, 10, Some("bogus".into())).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid sort value"));
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cicd_events_search_invalid_sort() {
        let cfg = test_config("http://unused.local");
        let result = super::events_search(
            &cfg,
            "*".into(),
            "1h".into(),
            "now".into(),
            10,
            "bogus".into(),
            "pipeline".into(),
        )
        .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid --sort value"));
    }

    #[tokio::test]
    async fn test_cicd_events_search_invalid_level() {
        let cfg = test_config("http://unused.local");
        let result = super::events_search(
            &cfg,
            "*".into(),
            "1h".into(),
            "now".into(),
            10,
            "desc".into(),
            "bogus".into(),
        )
        .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid --level value"));
    }

    #[tokio::test]
    async fn test_parse_ci_agg_invalid() {
        let result = super::parse_ci_agg("bogus");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid --compute format"));
    }

    #[tokio::test]
    async fn test_cicd_events_aggregate() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {"buckets": []}}"#).await;
        let _ = super::events_aggregate(
            &cfg,
            "@ci.status:error".into(),
            "1h".into(),
            "now".into(),
            "count".into(),
            Some("@git.branch".into()),
            10,
        )
        .await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cicd_events_aggregate_invalid_compute() {
        let cfg = test_config("http://unused.local");
        let result = super::events_aggregate(
            &cfg,
            "*".into(),
            "1h".into(),
            "now".into(),
            "bogus".into(),
            None,
            10,
        )
        .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid --compute format"));
    }

    #[tokio::test]
    async fn test_cicd_tests_aggregate() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {"buckets": []}}"#).await;
        let _ = super::tests_aggregate(
            &cfg,
            "@test.status:fail".into(),
            "1h".into(),
            "now".into(),
            "count".into(),
            Some("@test.service".into()),
            10,
        )
        .await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cicd_tests_aggregate_invalid_compute() {
        let cfg = test_config("http://unused.local");
        let result = super::tests_aggregate(
            &cfg,
            "*".into(),
            "1h".into(),
            "now".into(),
            "bogus".into(),
            None,
            10,
        )
        .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid --compute format"));
    }
}

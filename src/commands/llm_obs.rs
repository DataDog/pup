use anyhow::Result;
use datadog_api_client::datadogV2::api_llm_observability::{
    LLMObservabilityAPI, ListLLMObsAnnotationQueuesOptionalParams,
};
use datadog_api_client::datadogV2::model::{
    LLMObsAnnotationQueueInteractionsRequest, LLMObsAnnotationQueueRequest,
    LLMObsAnnotationQueueUpdateRequest, LLMObsDatasetRequest,
    LLMObsDeleteAnnotationQueueInteractionsRequest, LLMObsDeleteExperimentsRequest,
    LLMObsExperimentRequest, LLMObsExperimentUpdateRequest, LLMObsProjectRequest,
};

use crate::client;
use crate::config::Config;
use crate::formatter;
use crate::util;

fn make_api(cfg: &Config) -> LLMObservabilityAPI {
    let dd_cfg = client::make_dd_config(cfg);
    match client::make_bearer_client(cfg) {
        Some(c) => LLMObservabilityAPI::with_client_and_config(dd_cfg, c),
        None => LLMObservabilityAPI::with_config(dd_cfg),
    }
}

// ---- Projects ----

pub async fn projects_create(cfg: &Config, file: &str) -> Result<()> {
    let body: LLMObsProjectRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_llm_obs_project(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create LLM obs project: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn projects_list(cfg: &Config) -> Result<()> {
    let resp = client::raw_get(cfg, "/api/v2/llm-obs/v1/projects", &[])
        .await
        .map_err(|e| anyhow::anyhow!("failed to list LLM obs projects: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Experiments ----

pub async fn experiments_create(cfg: &Config, file: &str) -> Result<()> {
    let body: LLMObsExperimentRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_llm_obs_experiment(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create LLM obs experiment: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn experiments_list(
    cfg: &Config,
    filter_project_id: Option<String>,
    filter_dataset_id: Option<String>,
) -> Result<()> {
    let mut query: Vec<(&str, String)> = Vec::new();
    if let Some(ref pid) = filter_project_id {
        query.push(("filter[project_id]", pid.clone()));
    }
    if let Some(ref did) = filter_dataset_id {
        query.push(("filter[dataset_id]", did.clone()));
    }
    let query_refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let resp = client::raw_get(cfg, "/api/v2/llm-obs/v1/experiments", &query_refs)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list LLM obs experiments: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn experiments_update(cfg: &Config, experiment_id: &str, file: &str) -> Result<()> {
    let body: LLMObsExperimentUpdateRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .update_llm_obs_experiment(experiment_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update LLM obs experiment: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn experiments_delete(cfg: &Config, file: &str) -> Result<()> {
    let body: LLMObsDeleteExperimentsRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    api.delete_llm_obs_experiments(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete LLM obs experiments: {e:?}"))?;
    eprintln!("LLM obs experiments deleted.");
    Ok(())
}

// ---- Datasets ----

pub async fn datasets_create(cfg: &Config, project_id: &str, file: &str) -> Result<()> {
    let body: LLMObsDatasetRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_llm_obs_dataset(project_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create LLM obs dataset: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn datasets_list(cfg: &Config, project_id: &str) -> Result<()> {
    let path = format!("/api/v2/llm-obs/v1/{project_id}/datasets");
    let resp = client::raw_get(cfg, &path, &[])
        .await
        .map_err(|e| anyhow::anyhow!("failed to list LLM obs datasets: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Experiment analytics (no typed equivalent — unstable MCP endpoints) ----

pub async fn experiments_summary(cfg: &Config, experiment_id: &str) -> Result<()> {
    let body = serde_json::json!({ "experiment_id": experiment_id });
    let resp = client::raw_post(cfg, "/api/unstable/llm-obs-mcp/v1/experiment/summary", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get experiment summary: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[allow(clippy::too_many_arguments)]
pub async fn experiments_events_list(
    cfg: &Config,
    experiment_id: &str,
    limit: u32,
    offset: u32,
    filter_dimension_key: Option<String>,
    filter_dimension_value: Option<String>,
    filter_metric_label: Option<String>,
    sort_by_metric: Option<String>,
    sort_direction: &str,
) -> Result<()> {
    let mut body = serde_json::json!({
        "experiment_id": experiment_id,
        "limit": limit,
        "offset": offset,
        "sort_direction": sort_direction,
    });
    if let Some(k) = filter_dimension_key {
        body["filter_dimension_key"] = serde_json::json!(k);
    }
    if let Some(v) = filter_dimension_value {
        body["filter_dimension_value"] = serde_json::json!(v);
    }
    if let Some(l) = filter_metric_label {
        body["filter_metric_label"] = serde_json::json!(l);
    }
    if let Some(m) = sort_by_metric {
        body["sort_by_metric_label"] = serde_json::json!(m);
    }
    let resp = client::raw_post(cfg, "/api/unstable/llm-obs-mcp/v1/experiment/events", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list experiment events: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn experiments_events_get(
    cfg: &Config,
    experiment_id: &str,
    event_id: &str,
) -> Result<()> {
    let body = serde_json::json!({ "experiment_id": experiment_id, "event_id": event_id });
    let resp = client::raw_post(cfg, "/api/unstable/llm-obs-mcp/v1/experiment/event", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get experiment event: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn experiments_metric_values(
    cfg: &Config,
    experiment_id: &str,
    metric_label: &str,
    segment_by_dimension: Option<String>,
    segment_dimension_value: Option<String>,
) -> Result<()> {
    let mut body =
        serde_json::json!({ "experiment_id": experiment_id, "metric_label": metric_label });
    if let Some(d) = segment_by_dimension {
        body["segment_by_dimension"] = serde_json::json!(d);
    }
    if let Some(v) = segment_dimension_value {
        body["segment_dimension_value"] = serde_json::json!(v);
    }
    let resp = client::raw_post(
        cfg,
        "/api/unstable/llm-obs-mcp/v1/experiment/metric-values",
        body,
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to get experiment metric values: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn experiments_dimension_values(
    cfg: &Config,
    experiment_id: &str,
    dimension_key: &str,
) -> Result<()> {
    let body =
        serde_json::json!({ "experiment_id": experiment_id, "dimension_key": dimension_key });
    let resp = client::raw_post(
        cfg,
        "/api/unstable/llm-obs-mcp/v1/experiment/dimension-values",
        body,
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to get experiment dimension values: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Spans (no typed equivalent — unstable MCP endpoint) ----

// ---- Annotation Queues ----

pub async fn annotation_queues_create(cfg: &Config, file: &str) -> Result<()> {
    let body: LLMObsAnnotationQueueRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_llm_obs_annotation_queue(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create annotation queue: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn annotation_queues_list(
    cfg: &Config,
    project_id: Option<String>,
    queue_ids: Option<Vec<String>>,
) -> Result<()> {
    let api = make_api(cfg);
    let mut params = ListLLMObsAnnotationQueuesOptionalParams::default();
    if let Some(pid) = project_id {
        params = params.project_id(pid);
    }
    if let Some(ids) = queue_ids {
        params = params.queue_ids(ids);
    }
    let resp = api
        .list_llm_obs_annotation_queues(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list annotation queues: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn annotation_queues_update(cfg: &Config, queue_id: &str, file: &str) -> Result<()> {
    let body: LLMObsAnnotationQueueUpdateRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .update_llm_obs_annotation_queue(queue_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update annotation queue: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn annotation_queues_delete(cfg: &Config, queue_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.delete_llm_obs_annotation_queue(queue_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete annotation queue: {e:?}"))?;
    eprintln!("Annotation queue deleted.");
    Ok(())
}

pub async fn annotation_queue_interactions_add(
    cfg: &Config,
    queue_id: &str,
    file: &str,
) -> Result<()> {
    let body: LLMObsAnnotationQueueInteractionsRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_llm_obs_annotation_queue_interactions(queue_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to add interactions: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn annotation_queue_interactions_delete(
    cfg: &Config,
    queue_id: &str,
    file: &str,
) -> Result<()> {
    let body: LLMObsDeleteAnnotationQueueInteractionsRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    api.delete_llm_obs_annotation_queue_interactions(queue_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete interactions: {e:?}"))?;
    eprintln!("Annotation queue interactions deleted.");
    Ok(())
}

pub async fn annotation_queue_interactions_list(cfg: &Config, queue_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_llm_obs_annotated_interactions(queue_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list annotated interactions: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Spans (no typed equivalent — unstable MCP endpoint) ----

#[allow(clippy::too_many_arguments)]
pub async fn spans_search(
    cfg: &Config,
    query: Option<String>,
    trace_id: Option<String>,
    span_id: Option<String>,
    span_kind: Option<String>,
    span_name: Option<String>,
    ml_app: Option<String>,
    root_spans_only: bool,
    from: String,
    to: String,
    limit: u32,
    cursor: Option<String>,
) -> Result<()> {
    let mut body = serde_json::json!({ "limit": limit });
    if root_spans_only {
        body["root_spans_only"] = serde_json::json!(true);
    }
    if let Some(q) = query {
        body["query"] = serde_json::json!(q);
    }
    if let Some(t) = trace_id {
        body["trace_id"] = serde_json::json!(t);
    }
    if let Some(s) = span_id {
        body["span_id"] = serde_json::json!(s);
    }
    if let Some(k) = span_kind {
        body["span_kind"] = serde_json::json!(k);
    }
    if let Some(n) = span_name {
        body["span_name"] = serde_json::json!(n);
    }
    if let Some(a) = ml_app {
        body["ml_app"] = serde_json::json!(a);
    }
    let from_ms = crate::util::parse_time_to_unix_millis(&from)
        .map_err(|e| anyhow::anyhow!("invalid --from value: {e}"))?;
    body["from"] = serde_json::json!(from_ms.to_string());

    let to_ms = crate::util::parse_time_to_unix_millis(&to)
        .map_err(|e| anyhow::anyhow!("invalid --to value: {e}"))?;
    body["to"] = serde_json::json!(to_ms.to_string());
    if let Some(c) = cursor {
        body["cursor"] = serde_json::json!(c);
    }
    let resp = client::raw_post(cfg, "/api/unstable/llm-obs-mcp/v1/trace/search-spans", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search spans: {e:?}"))?;
    formatter::output(cfg, &resp)
}

fn build_analytics_filter(query: Option<String>, ml_app: Option<String>) -> String {
    match (query, ml_app) {
        (Some(q), Some(app)) => format!("{q} @ml_app:{app}"),
        (Some(q), None) => q,
        (None, Some(app)) => format!("@ml_app:{app}"),
        (None, None) => String::new(),
    }
}

fn parse_group_by_facets(group_by: Option<String>, limit: u32) -> Vec<serde_json::Value> {
    group_by
        .map(|g| {
            g.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .map(|facet| serde_json::json!({ "facet": facet, "limit": limit }))
                .collect()
        })
        .unwrap_or_default()
}

fn flatten_analytics_response(
    resp: &serde_json::Value,
    facets: &[String],
    compute: &str,
) -> Result<Vec<serde_json::Value>> {
    let buckets = resp
        .get("buckets")
        .and_then(|b| b.as_array())
        .ok_or_else(|| anyhow::anyhow!("unexpected response: missing 'buckets' array"))?;

    let rows = buckets
        .iter()
        .map(|bucket| {
            let by = bucket.get("by").and_then(|b| b.as_object());
            let computes = bucket.get("computes").and_then(|c| c.as_object());

            let mut row = serde_json::Map::new();
            for facet in facets {
                let val = by
                    .and_then(|b| b.get(facet))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                row.insert(facet.clone(), val);
            }
            let val = computes
                .and_then(|c| c.get("c0"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            row.insert(compute.to_string(), val);

            serde_json::Value::Object(row)
        })
        .collect();

    Ok(rows)
}

#[allow(clippy::too_many_arguments)]
pub async fn spans_analytics(
    cfg: &Config,
    query: Option<String>,
    from: String,
    to: String,
    group_by: Option<String>,
    compute: String,
    limit: u32,
    ml_app: Option<String>,
) -> Result<()> {
    let filter_query = build_analytics_filter(query, ml_app);
    let from_ms = crate::util::parse_time_to_unix_millis(&from)
        .map_err(|e| anyhow::anyhow!("invalid --from value: {e}"))?;
    let to_ms = crate::util::parse_time_to_unix_millis(&to)
        .map_err(|e| anyhow::anyhow!("invalid --to value: {e}"))?;
    let facets: Vec<String> = group_by
        .as_deref()
        .map(|g| {
            g.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let group_by_arr: Vec<serde_json::Value> = facets
        .iter()
        .map(|f| serde_json::json!({ "facet": f, "limit": limit }))
        .collect();
    let body = serde_json::json!({
        "search":   { "query": filter_query },
        "time":     { "from": from_ms.to_string(), "to": to_ms.to_string() },
        "indexes":  ["llmobs"],
        "type":     "llmobs",
        "computes": [{ "aggregation": compute, "name": "c0" }],
        "groupBy":  group_by_arr,
    });
    let resp = client::raw_post(cfg, "/api/unstable/llm-obs-query-rewriter/timeseries", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to run spans analytics: {e:?}"))?;
    let rows = flatten_analytics_response(&resp, &facets, &compute)?;
    formatter::output(cfg, &rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- build_analytics_filter ---

    #[test]
    fn test_filter_query_only() {
        assert_eq!(
            build_analytics_filter(Some("span.kind:llm".into()), None),
            "span.kind:llm"
        );
    }

    #[test]
    fn test_filter_ml_app_only() {
        assert_eq!(
            build_analytics_filter(None, Some("my-app".into())),
            "@ml_app:my-app"
        );
    }

    #[test]
    fn test_filter_query_and_ml_app() {
        assert_eq!(
            build_analytics_filter(Some("span.kind:llm".into()), Some("my-app".into())),
            "span.kind:llm @ml_app:my-app"
        );
    }

    #[test]
    fn test_filter_neither() {
        assert_eq!(build_analytics_filter(None, None), "");
    }

    // --- parse_group_by_facets ---

    #[test]
    fn test_group_by_single() {
        let result = parse_group_by_facets(Some("span_name".into()), 10);
        assert_eq!(
            result,
            vec![serde_json::json!({"facet": "span_name", "limit": 10})]
        );
    }

    #[test]
    fn test_group_by_multiple() {
        let result = parse_group_by_facets(
            Some("span_name,@meta.error.type,@meta.error.message".into()),
            10,
        );
        assert_eq!(
            result,
            vec![
                serde_json::json!({"facet": "span_name",           "limit": 10}),
                serde_json::json!({"facet": "@meta.error.type",    "limit": 10}),
                serde_json::json!({"facet": "@meta.error.message", "limit": 10}),
            ]
        );
    }

    #[test]
    fn test_group_by_trims_whitespace() {
        let result = parse_group_by_facets(Some(" span_name , @meta.error.type ".into()), 5);
        assert_eq!(
            result,
            vec![
                serde_json::json!({"facet": "span_name",        "limit": 5}),
                serde_json::json!({"facet": "@meta.error.type", "limit": 5}),
            ]
        );
    }

    #[test]
    fn test_group_by_none() {
        let result = parse_group_by_facets(None, 10);
        assert!(result.is_empty());
    }

    #[test]
    fn test_group_by_filters_empty_segments() {
        let result = parse_group_by_facets(Some("span_name,,@meta.error.type".into()), 10);
        assert_eq!(
            result,
            vec![
                serde_json::json!({"facet": "span_name",        "limit": 10}),
                serde_json::json!({"facet": "@meta.error.type", "limit": 10}),
            ]
        );
    }

    #[test]
    fn test_group_by_limit_applied_to_all() {
        let result = parse_group_by_facets(Some("a,b,c".into()), 25);
        assert!(result.iter().all(|v| v["limit"] == 25));
    }

    // --- flatten_analytics_response ---

    #[test]
    fn test_flatten_analytics_single_facet() {
        let resp = serde_json::json!({
            "buckets": [
                { "by": { "span_name": "llm.call" }, "computes": { "c0": 42 } },
                { "by": { "span_name": "tool.run"  }, "computes": { "c0": 7  } },
            ]
        });
        let rows = flatten_analytics_response(&resp, &["span_name".into()], "count").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["span_name"], "llm.call");
        assert_eq!(rows[0]["count"], 42);
        assert_eq!(rows[1]["span_name"], "tool.run");
        assert_eq!(rows[1]["count"], 7);
    }

    #[test]
    fn test_flatten_analytics_multiple_facets() {
        let resp = serde_json::json!({
            "buckets": [
                {
                    "by": { "span_name": "llm.call", "@ml_app": "my-app" },
                    "computes": { "c0": 10 }
                }
            ]
        });
        let rows =
            flatten_analytics_response(&resp, &["span_name".into(), "@ml_app".into()], "count")
                .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["span_name"], "llm.call");
        assert_eq!(rows[0]["@ml_app"], "my-app");
        assert_eq!(rows[0]["count"], 10);
    }

    #[test]
    fn test_flatten_analytics_no_facets() {
        // Total aggregate with no group-by: one bucket, no "by" fields, just the compute.
        let resp = serde_json::json!({
            "buckets": [
                { "by": {}, "computes": { "c0": 99 } }
            ]
        });
        let rows = flatten_analytics_response(&resp, &[], "count").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["count"], 99);
        // No extra keys beyond the compute label.
        assert_eq!(rows[0].as_object().unwrap().len(), 1);
    }

    #[test]
    fn test_flatten_analytics_empty_buckets() {
        let resp = serde_json::json!({ "buckets": [] });
        let rows = flatten_analytics_response(&resp, &["span_name".into()], "count").unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn test_flatten_analytics_missing_buckets_key() {
        let resp = serde_json::json!({ "data": [] });
        let err = flatten_analytics_response(&resp, &[], "count").unwrap_err();
        assert!(err.to_string().contains("missing 'buckets' array"));
    }

    #[test]
    fn test_flatten_analytics_missing_facet_value_is_null() {
        // If a bucket's "by" object is missing a facet, the cell should be null.
        let resp = serde_json::json!({
            "buckets": [
                { "by": {}, "computes": { "c0": 5 } }
            ]
        });
        let rows = flatten_analytics_response(&resp, &["span_name".into()], "count").unwrap();
        assert_eq!(rows[0]["span_name"], serde_json::Value::Null);
    }

    #[test]
    fn test_flatten_analytics_missing_compute_is_null() {
        // If a bucket has no "computes" object, the compute cell should be null.
        let resp = serde_json::json!({
            "buckets": [
                { "by": { "span_name": "llm.call" } }
            ]
        });
        let rows = flatten_analytics_response(&resp, &["span_name".into()], "count").unwrap();
        assert_eq!(rows[0]["count"], serde_json::Value::Null);
    }
}

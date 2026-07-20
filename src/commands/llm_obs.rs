use anyhow::Result;
use datadog_api_client::datadogV2::api_llm_observability::{
    LLMObservabilityAPI, ListLLMObsAnnotationQueuesOptionalParams,
};
use datadog_api_client::datadogV2::model::{
    LLMObsAnnotationQueueInteractionsRequest, LLMObsAnnotationQueueRequest,
    LLMObsAnnotationQueueUpdateRequest, LLMObsCustomEvalConfigUpdateRequest,
    LLMObsDatasetBatchUpdateRequest, LLMObsDatasetCloneRequest, LLMObsDatasetRequest,
    LLMObsDatasetRestoreVersionRequest, LLMObsDeleteAnnotationQueueInteractionsRequest,
    LLMObsDeleteExperimentsRequest, LLMObsExperimentRequest, LLMObsExperimentUpdateRequest,
    LLMObsProjectRequest,
};

use crate::config::Config;
use crate::formatter;
use crate::raw_client;
use crate::util;
use crate::util_ext;

fn make_api(cfg: &Config) -> LLMObservabilityAPI {
    crate::make_api!(LLMObservabilityAPI, cfg)
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
    let resp = raw_client::raw_get(cfg, "/api/v2/llm-obs/v1/projects", &[])
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
    let resp = raw_client::raw_get(cfg, "/api/v2/llm-obs/v1/experiments", &query_refs)
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
    let resp = raw_client::raw_get(cfg, &path, &[])
        .await
        .map_err(|e| anyhow::anyhow!("failed to list LLM obs datasets: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn datasets_batch_update(
    cfg: &Config,
    project_id: &str,
    dataset_id: &str,
    file: &str,
) -> Result<()> {
    let body: LLMObsDatasetBatchUpdateRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .batch_update_llm_obs_dataset(project_id.to_string(), dataset_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to batch update dataset records: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn datasets_clone(
    cfg: &Config,
    project_id: &str,
    dataset_id: &str,
    file: &str,
) -> Result<()> {
    let body: LLMObsDatasetCloneRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .clone_llm_obs_dataset(project_id.to_string(), dataset_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to clone dataset: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn datasets_restore(
    cfg: &Config,
    project_id: &str,
    dataset_id: &str,
    file: &str,
) -> Result<()> {
    let body: LLMObsDatasetRestoreVersionRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    api.restore_llm_obs_dataset_version(project_id.to_string(), dataset_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to restore dataset version: {e:?}"))?;
    println!("Dataset {dataset_id} restored.");
    Ok(())
}

// ---- Experiment analytics (no typed equivalent — unstable MCP endpoints) ----

pub async fn experiments_summary(cfg: &Config, experiment_id: &str) -> Result<()> {
    let body = serde_json::json!({ "experiment_id": experiment_id });
    let resp = raw_client::raw_post(cfg, "/api/unstable/llm-obs-mcp/v1/experiment/summary", body)
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
    let resp = raw_client::raw_post(cfg, "/api/unstable/llm-obs-mcp/v1/experiment/events", body)
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
    let resp = raw_client::raw_post(cfg, "/api/unstable/llm-obs-mcp/v1/experiment/event", body)
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
    let resp = raw_client::raw_post(
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
    let resp = raw_client::raw_post(
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

// ---- Custom Evaluator Configs ----

pub async fn eval_config_get(cfg: &Config, eval_name: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_llm_obs_custom_eval_config(eval_name.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get LLM obs custom eval config: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn eval_config_update(cfg: &Config, eval_name: &str, file: &str) -> Result<()> {
    let body: LLMObsCustomEvalConfigUpdateRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    api.update_llm_obs_custom_eval_config(eval_name.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update LLM obs custom eval config: {e:?}"))?;
    eprintln!("LLM obs custom eval config '{eval_name}' updated.");
    Ok(())
}

pub async fn eval_config_delete(cfg: &Config, eval_name: &str) -> Result<()> {
    let api = make_api(cfg);
    api.delete_llm_obs_custom_eval_config(eval_name.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete LLM obs custom eval config: {e:?}"))?;
    eprintln!("LLM obs custom eval config '{eval_name}' deleted.");
    Ok(())
}

// ---- Evals (no typed equivalent — unstable MCP endpoint) ----

pub async fn evals_list(cfg: &Config) -> Result<()> {
    let resp = raw_client::raw_post(
        cfg,
        "/api/unstable/llm-obs-mcp/v1/eval/list-for-org",
        serde_json::json!({}),
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to list evals: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn evals_list_by_ml_app(cfg: &Config, ml_app: &str) -> Result<()> {
    let resp = raw_client::raw_post(
        cfg,
        "/api/unstable/llm-obs-mcp/v1/eval/list",
        serde_json::json!({ "ml_app": ml_app }),
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to list evals by ml app: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn evals_get_evaluator(cfg: &Config, eval_name: &str) -> Result<()> {
    let resp = raw_client::raw_post(
        cfg,
        "/api/unstable/llm-obs-mcp/v1/custom-evaluator/get",
        serde_json::json!({ "eval_name": eval_name }),
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to get evaluator: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn evals_get_aggregate_stats(
    cfg: &Config,
    eval_name: &str,
    ml_app: Option<String>,
    from: String,
    to: String,
) -> Result<()> {
    let mut body = serde_json::json!({ "eval_name": eval_name });
    if let Some(a) = ml_app {
        body["ml_app"] = serde_json::json!(a);
    }
    let from_ms = util_ext::parse_time_to_unix_millis(&from)
        .map_err(|e| anyhow::anyhow!("invalid --from value: {e}"))?;
    body["from"] = serde_json::json!(from_ms.to_string());
    let to_ms = util_ext::parse_time_to_unix_millis(&to)
        .map_err(|e| anyhow::anyhow!("invalid --to value: {e}"))?;
    body["to"] = serde_json::json!(to_ms.to_string());
    let resp = raw_client::raw_post(
        cfg,
        "/api/unstable/llm-obs-mcp/v1/eval/aggregate-stats",
        body,
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to get eval aggregate stats: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn evals_create_or_update(cfg: &Config, eval_name: &str, file: &str) -> Result<()> {
    let mut body: serde_json::Value = util::read_json_file(file)?;
    body["eval_name"] = serde_json::json!(eval_name);
    raw_client::raw_post(
        cfg,
        "/api/unstable/llm-obs-mcp/v1/custom-evaluator/create-or-update",
        body,
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to create or update evaluator: {e:?}"))?;
    eprintln!("Evaluator '{eval_name}' created or updated.");
    Ok(())
}

pub async fn evals_delete(cfg: &Config, eval_name: &str) -> Result<()> {
    raw_client::raw_post(
        cfg,
        "/api/unstable/llm-obs-mcp/v1/custom-evaluator/delete",
        serde_json::json!({ "eval_name": eval_name }),
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to delete evaluator: {e:?}"))?;
    eprintln!("Evaluator '{eval_name}' deleted.");
    Ok(())
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
    summary: bool,
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
    let from_ms = util_ext::parse_time_to_unix_millis(&from)
        .map_err(|e| anyhow::anyhow!("invalid --from value: {e}"))?;
    body["from"] = serde_json::json!(from_ms.to_string());

    let to_ms = util_ext::parse_time_to_unix_millis(&to)
        .map_err(|e| anyhow::anyhow!("invalid --to value: {e}"))?;
    body["to"] = serde_json::json!(to_ms.to_string());
    if let Some(c) = cursor {
        body["cursor"] = serde_json::json!(c);
    }
    let resp = raw_client::raw_post(cfg, "/api/unstable/llm-obs-mcp/v1/trace/search-spans", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search spans: {e:?}"))?;
    if summary {
        let slim: Vec<serde_json::Value> = resp["spans"]
            .as_array()
            .map(|spans| {
                spans
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "span_id": s["span_id"],
                            "trace_id": s["trace_id"],
                            "apm_trace_id": s["apm_trace_id"],
                            "name": s["name"],
                            "span_kind": s["span_kind"],
                            "ml_app": s["ml_app"],
                            "service": s["service"],
                            "status": s["status"],
                            "duration_ms": s["duration_ms"],
                            "start_ms": s["start_ms"],
                            "parent_id": s["parent_id"],
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        return formatter::output(cfg, &serde_json::json!({ "spans": slim }));
    }
    formatter::output(cfg, &resp)
}

pub async fn spans_get_trace(
    cfg: &Config,
    trace_id: &str,
    include_tree: bool,
    from: String,
    to: String,
) -> Result<()> {
    let mut body = serde_json::json!({ "trace_id": trace_id });
    if include_tree {
        body["include_tree"] = serde_json::json!(true);
    }
    let from_ms = util_ext::parse_time_to_unix_millis(&from)
        .map_err(|e| anyhow::anyhow!("invalid --from value: {e}"))?;
    body["from"] = serde_json::json!(from_ms.to_string());
    let to_ms = util_ext::parse_time_to_unix_millis(&to)
        .map_err(|e| anyhow::anyhow!("invalid --to value: {e}"))?;
    body["to"] = serde_json::json!(to_ms.to_string());
    let resp = raw_client::raw_post(cfg, "/api/unstable/llm-obs-mcp/v1/trace/get-trace", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get trace: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn spans_get_span_details(
    cfg: &Config,
    trace_id: &str,
    span_ids: Vec<String>,
    from: String,
    to: String,
) -> Result<()> {
    let mut body = serde_json::json!({ "trace_id": trace_id, "span_ids": span_ids });
    let from_ms = util_ext::parse_time_to_unix_millis(&from)
        .map_err(|e| anyhow::anyhow!("invalid --from value: {e}"))?;
    body["from"] = serde_json::json!(from_ms.to_string());
    let to_ms = util_ext::parse_time_to_unix_millis(&to)
        .map_err(|e| anyhow::anyhow!("invalid --to value: {e}"))?;
    body["to"] = serde_json::json!(to_ms.to_string());
    let requested = span_ids.len();
    let resp = raw_client::raw_post(cfg, "/api/unstable/llm-obs-mcp/v1/trace/span-details", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get span details: {e:?}"))?;
    let returned = resp["spans"].as_array().map(|a| a.len()).unwrap_or(0);
    let missing = requested.saturating_sub(returned);
    if missing > 0 {
        eprintln!(
            "warning: {missing} of {requested} requested span(s) not found in trace \
            hierarchy — the span may exist but be orphaned (no path to a root span). \
            Use 'spans get-content' to retrieve its content directly."
        );
    }
    formatter::output(cfg, &resp)
}

#[allow(clippy::too_many_arguments)]
pub async fn spans_get_span_content(
    cfg: &Config,
    trace_id: &str,
    span_id: &str,
    field: &str,
    path: Option<String>,
    max_tokens: Option<u32>,
    from: String,
    to: String,
) -> Result<()> {
    let mut body = serde_json::json!({ "trace_id": trace_id, "span_id": span_id, "field": field });
    if let Some(p) = path {
        body["path"] = serde_json::json!(p);
    }
    if let Some(m) = max_tokens {
        body["max_tokens"] = serde_json::json!(m);
    }
    let from_ms = util_ext::parse_time_to_unix_millis(&from)
        .map_err(|e| anyhow::anyhow!("invalid --from value: {e}"))?;
    body["from"] = serde_json::json!(from_ms.to_string());
    let to_ms = util_ext::parse_time_to_unix_millis(&to)
        .map_err(|e| anyhow::anyhow!("invalid --to value: {e}"))?;
    body["to"] = serde_json::json!(to_ms.to_string());
    let resp = raw_client::raw_post(cfg, "/api/unstable/llm-obs-mcp/v1/trace/span-content", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get span content: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn spans_find_error_spans(
    cfg: &Config,
    trace_id: &str,
    from: String,
    to: String,
) -> Result<()> {
    let mut body = serde_json::json!({ "trace_id": trace_id });
    let from_ms = util_ext::parse_time_to_unix_millis(&from)
        .map_err(|e| anyhow::anyhow!("invalid --from value: {e}"))?;
    body["from"] = serde_json::json!(from_ms.to_string());
    let to_ms = util_ext::parse_time_to_unix_millis(&to)
        .map_err(|e| anyhow::anyhow!("invalid --to value: {e}"))?;
    body["to"] = serde_json::json!(to_ms.to_string());
    let resp = raw_client::raw_post(
        cfg,
        "/api/unstable/llm-obs-mcp/v1/trace/find-error-spans",
        body,
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to find error spans: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[allow(clippy::too_many_arguments)]
pub async fn spans_expand_spans(
    cfg: &Config,
    trace_id: &str,
    span_ids: Vec<String>,
    max_depth: Option<u32>,
    filter_kind: Option<String>,
    from: String,
    to: String,
) -> Result<()> {
    let mut body = serde_json::json!({ "trace_id": trace_id, "span_ids": span_ids });
    if let Some(d) = max_depth {
        body["max_depth"] = serde_json::json!(d);
    }
    if let Some(k) = filter_kind {
        body["filter_kind"] = serde_json::json!(k);
    }
    let from_ms = util_ext::parse_time_to_unix_millis(&from)
        .map_err(|e| anyhow::anyhow!("invalid --from value: {e}"))?;
    body["from"] = serde_json::json!(from_ms.to_string());
    let to_ms = util_ext::parse_time_to_unix_millis(&to)
        .map_err(|e| anyhow::anyhow!("invalid --to value: {e}"))?;
    body["to"] = serde_json::json!(to_ms.to_string());
    let resp = raw_client::raw_post(cfg, "/api/unstable/llm-obs-mcp/v1/trace/expand-spans", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to expand spans: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn spans_get_agent_loop(
    cfg: &Config,
    trace_id: &str,
    span_id: Option<String>,
    max_content_length: Option<u32>,
    from: String,
    to: String,
) -> Result<()> {
    let mut body = serde_json::json!({ "trace_id": trace_id });
    if let Some(s) = span_id {
        body["span_id"] = serde_json::json!(s);
    }
    if let Some(m) = max_content_length {
        body["max_content_length"] = serde_json::json!(m);
    }
    let from_ms = util_ext::parse_time_to_unix_millis(&from)
        .map_err(|e| anyhow::anyhow!("invalid --from value: {e}"))?;
    body["from"] = serde_json::json!(from_ms.to_string());
    let to_ms = util_ext::parse_time_to_unix_millis(&to)
        .map_err(|e| anyhow::anyhow!("invalid --to value: {e}"))?;
    body["to"] = serde_json::json!(to_ms.to_string());
    let resp = raw_client::raw_post(
        cfg,
        "/api/unstable/llm-obs-mcp/v1/trace/get-agent-loop",
        body,
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to get agent loop: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {

    use crate::config::{Config, OutputFormat};
    use crate::test_support::*;

    #[tokio::test]
    async fn test_llm_obs_projects_list() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Raw HTTP path: response can have any shape; missing nullable fields are tolerated.
        let body = r#"{"data":[{"id":"proj-1","type":"projects","attributes":{"name":"my-project","description":null,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}}]}"#;
        let _mock = mock_any(&mut server, "GET", body).await;

        let result = super::projects_list(&cfg).await;
        assert!(result.is_ok(), "projects_list failed: {:?}", result.err());
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_projects_list_404() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["not found"]}"#)
            .create_async()
            .await;

        let result = super::projects_list(&cfg).await;
        assert!(
            result.is_err(),
            "expected error but got ok: {:?}",
            result.ok()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_projects_list_no_auth() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let cfg = Config {
            api_key: None,
            app_key: None,
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };
        let result = super::projects_list(&cfg).await;
        assert!(result.is_err(), "should fail without auth");
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_projects_list_missing_nullable_fields() {
        // raw HTTP should succeed even with minimal response missing optional/nullable fields
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        // response missing description/config/metadata fields
        let _mock = mock_any(
            &mut server,
            "GET",
            r#"{"data":[{"id":"p1","type":"llm_obs_projects"}]}"#,
        )
        .await;
        let result = super::projects_list(&cfg).await;
        assert!(
            result.is_ok(),
            "should tolerate missing nullable fields: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_projects_create() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_proj_create.json",
            r#"{"data":{"type":"projects","attributes":{"name":"test"}}}"#,
        );
        let body = r#"{"data":{"id":"proj-1","type":"projects","attributes":{"name":"test","description":null,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}}}"#;
        let _mock = mock_any(&mut server, "POST", body).await;

        let result = super::projects_create(&cfg, tmp.to_str().unwrap()).await;
        assert!(result.is_ok(), "projects_create failed: {:?}", result.err());
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_projects_create_500() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_proj_create_500.json",
            r#"{"data":{"type":"projects","attributes":{"name":"test"}}}"#,
        );
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["server error"]}"#)
            .create_async()
            .await;

        let result = super::projects_create(&cfg, tmp.to_str().unwrap()).await;
        assert!(
            result.is_err(),
            "expected error but got ok: {:?}",
            result.ok()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_list() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Raw HTTP path: response can have any shape; nullable fields are tolerated.
        let body = r#"{"data":[{"id":"exp-1","type":"experiments","attributes":{"name":"test-exp","config":null,"description":null,"metadata":null,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","dataset_id":"ds-1","project_id":"proj-1","status":"active"}}]}"#;
        let _mock = mock_any(&mut server, "GET", body).await;

        let result = super::experiments_list(&cfg, None, None).await;
        assert!(
            result.is_ok(),
            "experiments_list failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_list_with_filters() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"data":[]}"#;
        // Strict query param check: verify filter[project_id] and filter[dataset_id] are sent.
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("filter[project_id]".into(), "proj-1".into()),
                mockito::Matcher::UrlEncoded("filter[dataset_id]".into(), "ds-1".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let result =
            super::experiments_list(&cfg, Some("proj-1".into()), Some("ds-1".into())).await;
        assert!(
            result.is_ok(),
            "experiments_list with filters failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_list_401() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Unauthorized"]}"#)
            .create_async()
            .await;

        let result = super::experiments_list(&cfg, None, None).await;
        assert!(
            result.is_err(),
            "expected error but got ok: {:?}",
            result.ok()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_create() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_exp_create.json",
            r#"{"data":{"type":"experiments","attributes":{"name":"test-exp","dataset_id":"ds-1","project_id":"proj-1"}}}"#,
        );
        let body = r#"{"data":{"id":"exp-1","type":"experiments","attributes":{"name":"test-exp","config":null,"description":null,"metadata":null,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","dataset_id":"ds-1","project_id":"proj-1","status":"active"}}}"#;
        let _mock = mock_any(&mut server, "POST", body).await;

        let result = super::experiments_create(&cfg, tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "experiments_create failed: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_create_422() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_exp_create_422.json",
            r#"{"data":{"type":"experiments","attributes":{"name":"x","dataset_id":"ds-1","project_id":"proj-1"}}}"#,
        );
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(422)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["invalid request body"]}"#)
            .create_async()
            .await;

        let result = super::experiments_create(&cfg, tmp.to_str().unwrap()).await;
        assert!(
            result.is_err(),
            "expected error but got ok: {:?}",
            result.ok()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_update() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_exp_update.json",
            r#"{"data":{"type":"experiments","id":"exp-1","attributes":{"name":"updated"}}}"#,
        );
        let body = r#"{"data":{"id":"exp-1","type":"experiments","attributes":{"name":"updated","config":null,"description":null,"metadata":null,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","dataset_id":"ds-1","project_id":"proj-1","status":"active"}}}"#;
        let _mock = mock_any(&mut server, "PATCH", body).await;

        let result = super::experiments_update(&cfg, "exp-1", tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "experiments_update failed: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_update_404() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_exp_update_404.json",
            r#"{"data":{"type":"experiments","id":"missing","attributes":{"name":"x"}}}"#,
        );
        let _mock = server
            .mock("PATCH", mockito::Matcher::Any)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["not found"]}"#)
            .create_async()
            .await;

        let result = super::experiments_update(&cfg, "missing", tmp.to_str().unwrap()).await;
        assert!(
            result.is_err(),
            "expected error but got ok: {:?}",
            result.ok()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_delete() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_exp_delete.json",
            r#"{"data":{"type":"experiments","attributes":{"experiment_ids":["exp-1"]}}}"#,
        );
        let _mock = mock_any(&mut server, "POST", r#"{}"#).await;

        let result = super::experiments_delete(&cfg, tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "experiments_delete failed: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_delete_500() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_exp_delete_500.json",
            r#"{"data":{"type":"experiments","attributes":{"experiment_ids":["exp-1"]}}}"#,
        );
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["server error"]}"#)
            .create_async()
            .await;

        let result = super::experiments_delete(&cfg, tmp.to_str().unwrap()).await;
        assert!(
            result.is_err(),
            "expected error but got ok: {:?}",
            result.ok()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_list() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Raw HTTP path: verify the correct project-scoped path is called.
        let body = r#"{"data":[{"id":"ds-1","type":"datasets","attributes":{"name":"my-dataset","description":null,"metadata":null,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","current_version":1}}]}"#;
        let _mock = server
            .mock("GET", "/api/v2/llm-obs/v1/proj-1/datasets")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let result = super::datasets_list(&cfg, "proj-1").await;
        assert!(result.is_ok(), "datasets_list failed: {:?}", result.err());
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_list_403() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Forbidden"]}"#)
            .create_async()
            .await;

        let result = super::datasets_list(&cfg, "proj-1").await;
        assert!(
            result.is_err(),
            "expected error but got ok: {:?}",
            result.ok()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_create() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_ds_create.json",
            r#"{"data":{"type":"datasets","attributes":{"name":"test-dataset"}}}"#,
        );
        let body = r#"{"data":{"id":"ds-1","type":"datasets","attributes":{"name":"test-dataset","description":null,"metadata":null,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","current_version":1}}}"#;
        let _mock = mock_any(&mut server, "POST", body).await;

        let result = super::datasets_create(&cfg, "proj-1", tmp.to_str().unwrap()).await;
        assert!(result.is_ok(), "datasets_create failed: {:?}", result.err());
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_create_no_auth() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let tmp = write_temp_json(
            "pup_test_ds_create_noauth.json",
            r#"{"data":{"type":"datasets","attributes":{"name":"x"}}}"#,
        );
        let cfg = Config {
            api_key: None,
            app_key: None,
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };
        let result = super::datasets_create(&cfg, "proj-1", tmp.to_str().unwrap()).await;
        assert!(result.is_err(), "should fail without auth");
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_summary() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"experiment_id":"exp-1","total_events":3,"error_count":0,"evals":{},"available_dimensions":["env","ml_app"]}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/experiment/summary",
            200,
            body,
        )
        .await;

        let result = super::experiments_summary(&cfg, "exp-1").await;
        assert!(
            result.is_ok(),
            "experiments_summary failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_summary_404() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/experiment/summary",
            404,
            r#"{"errors":["experiment not found"]}"#,
        )
        .await;

        let result = super::experiments_summary(&cfg, "does-not-exist").await;
        assert!(result.is_err(), "should fail on 404");
        assert!(result.unwrap_err().to_string().contains("404"));
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_summary_500() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/experiment/summary",
            500,
            r#"{"errors":["internal server error"]}"#,
        )
        .await;

        let result = super::experiments_summary(&cfg, "exp-1").await;
        assert!(result.is_err(), "should fail on 500");
        assert!(result.unwrap_err().to_string().contains("500"));
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_summary_no_auth() {
        let _lock = lock_env().await;
        let cfg = Config {
            api_key: None,
            app_key: None,
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        let result = super::experiments_summary(&cfg, "exp-1").await;
        assert!(result.is_err(), "should fail without auth");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_events_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"events":[{"id":"evt-1","status":"ok","duration_ms":100.0,"metrics":{}}],"total_matching":1,"returned":1,"offset":0}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/experiment/events",
            200,
            body,
        )
        .await;

        let result =
            super::experiments_events_list(&cfg, "exp-1", 20, 0, None, None, None, None, "desc")
                .await;
        assert!(
            result.is_ok(),
            "experiments_events_list failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_events_list_with_filters() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"events":[],"total_matching":0,"returned":0,"offset":0}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/experiment/events",
            200,
            body,
        )
        .await;

        let result = super::experiments_events_list(
            &cfg,
            "exp-1",
            5,
            10,
            Some("env".into()),
            Some("prod".into()),
            Some("score".into()),
            Some("accuracy".into()),
            "asc",
        )
        .await;
        assert!(
            result.is_ok(),
            "experiments_events_list with filters failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_events_list_401() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/experiment/events",
            401,
            r#"{"errors":["Forbidden"]}"#,
        )
        .await;

        let result =
            super::experiments_events_list(&cfg, "exp-1", 20, 0, None, None, None, None, "desc")
                .await;
        assert!(result.is_err(), "should fail on 401");
        assert!(result.unwrap_err().to_string().contains("401"));
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_events_get() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"id":"evt-1","status":"ok","duration_ms":100.0,"input":{"prompt":"hello"},"output":{"response":"world"},"metrics":{},"dimensions":{}}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/experiment/event",
            200,
            body,
        )
        .await;

        let result = super::experiments_events_get(&cfg, "exp-1", "evt-1").await;
        assert!(
            result.is_ok(),
            "experiments_events_get failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_events_get_404() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/experiment/event",
            404,
            r#"{"errors":["event not found"]}"#,
        )
        .await;

        let result = super::experiments_events_get(&cfg, "exp-1", "missing-evt").await;
        assert!(result.is_err(), "should fail on 404");
        assert!(result.unwrap_err().to_string().contains("404"));
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_events_get_no_auth() {
        let _lock = lock_env().await;
        let cfg = Config {
            api_key: None,
            app_key: None,
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        let result = super::experiments_events_get(&cfg, "exp-1", "evt-1").await;
        assert!(result.is_err(), "should fail without auth");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_metric_values() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"metric_label":"accuracy","metric_type":"score","overall":{"count":10,"mean":0.85,"min_value":0.5,"max_value":1.0,"p50":0.9,"p90":0.95,"p95":0.98},"total_events":10}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/experiment/metric-values",
            200,
            body,
        )
        .await;

        let result = super::experiments_metric_values(&cfg, "exp-1", "accuracy", None, None).await;
        assert!(
            result.is_ok(),
            "experiments_metric_values failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_metric_values_segmented() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"metric_label":"accuracy","metric_type":"score","overall":{"count":5,"mean":0.9},"segments":[{"dimension_value":"prod","stats":{"count":5,"mean":0.9}}],"total_events":5}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/experiment/metric-values",
            200,
            body,
        )
        .await;

        let result = super::experiments_metric_values(
            &cfg,
            "exp-1",
            "accuracy",
            Some("env".into()),
            Some("prod".into()),
        )
        .await;
        assert!(
            result.is_ok(),
            "experiments_metric_values segmented failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_metric_values_500() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/experiment/metric-values",
            500,
            r#"{"errors":["internal server error"]}"#,
        )
        .await;

        let result = super::experiments_metric_values(&cfg, "exp-1", "accuracy", None, None).await;
        assert!(result.is_err(), "should fail on 500");
        assert!(result.unwrap_err().to_string().contains("500"));
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_dimension_values() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"dimension":"env","unique_count":2,"values":[{"value":"prod","count":8},{"value":"staging","count":2}]}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/experiment/dimension-values",
            200,
            body,
        )
        .await;

        let result = super::experiments_dimension_values(&cfg, "exp-1", "env").await;
        assert!(
            result.is_ok(),
            "experiments_dimension_values failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_dimension_values_403() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/experiment/dimension-values",
            403,
            r#"{"errors":["Forbidden"]}"#,
        )
        .await;

        let result = super::experiments_dimension_values(&cfg, "exp-1", "env").await;
        assert!(result.is_err(), "should fail on 403");
        assert!(result.unwrap_err().to_string().contains("403"));
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_eval_config_get() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let body = r#"{"data":{"id":"toxicity","type":"evaluator_config","attributes":{"eval_name":"toxicity","created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}}}"#;
        let _mock = mock_any(&mut server, "GET", body).await;
        let result = super::eval_config_get(&cfg, "toxicity").await;
        assert!(result.is_ok(), "eval_config_get failed: {:?}", result.err());
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_eval_config_get_404() {
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
        let result = super::eval_config_get(&cfg, "missing").await;
        assert!(result.is_err(), "expected 404 error");
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_eval_config_update() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let tmp = write_temp_json(
            "pup_test_eval_config_update.json",
            r#"{"data":{"type":"evaluator_config","attributes":{"target":{"application_name":"my-app","enabled":true}}}}"#,
        );
        let _mock = server
            .mock("PUT", mockito::Matcher::Any)
            .with_status(204)
            .create_async()
            .await;
        let result = super::eval_config_update(&cfg, "toxicity", tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "eval_config_update failed: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_eval_config_update_400() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let tmp = write_temp_json(
            "pup_test_eval_config_update_400.json",
            r#"{"data":{"type":"evaluator_config","attributes":{"target":{"application_name":"my-app","enabled":true}}}}"#,
        );
        let _mock = server
            .mock("PUT", mockito::Matcher::Any)
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["bad request"]}"#)
            .create_async()
            .await;
        let result = super::eval_config_update(&cfg, "toxicity", tmp.to_str().unwrap()).await;
        assert!(result.is_err(), "expected 400 error");
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_eval_config_delete() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("DELETE", mockito::Matcher::Any)
            .with_status(204)
            .create_async()
            .await;
        let result = super::eval_config_delete(&cfg, "toxicity").await;
        assert!(
            result.is_ok(),
            "eval_config_delete failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_eval_config_delete_404() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("DELETE", mockito::Matcher::Any)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["not found"]}"#)
            .create_async()
            .await;
        let result = super::eval_config_delete(&cfg, "missing").await;
        assert!(result.is_err(), "expected 404 error");
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_spans_search() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"spans":[{"span_id":"s-1","trace_id":"t-1","name":"llm-call","span_kind":"llm","ml_app":"my-app","status":"ok","duration_ms":42.0,"start_ms":1000000,"tags":[]}]}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/search-spans",
            200,
            body,
        )
        .await;

        let result = super::spans_search(
            &cfg,
            Some("llm-call".into()),
            None,
            None,
            None,
            None,
            Some("my-app".into()),
            false,
            "1h".into(),
            "now".into(),
            10,
            None,
            false,
        )
        .await;
        assert!(result.is_ok(), "spans_search failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_search_from_is_numeric_string() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let resp = r#"{"spans":[]}"#;
        let _mock = server
            .mock("POST", "/api/unstable/llm-obs-mcp/v1/trace/search-spans")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(resp)
            .match_body(mockito::Matcher::Regex(r#""from":"\d{13}""#.to_string()))
            .match_body(mockito::Matcher::Regex(r#""to":"\d{13}""#.to_string()))
            .create_async()
            .await;

        let result = super::spans_search(
            &cfg,
            None,
            None,
            None,
            None,
            None,
            None,
            false,
            "4h".into(),
            "now".into(),
            5,
            None,
            false,
        )
        .await;
        assert!(result.is_ok(), "spans_search failed: {:?}", result.err());
        _mock.assert();
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_search_invalid_from_returns_error() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let result = super::spans_search(
            &cfg,
            None,
            None,
            None,
            None,
            None,
            None,
            false,
            "not-a-valid-time".into(),
            "now".into(),
            5,
            None,
            false,
        )
        .await;
        assert!(result.is_err(), "expected error for invalid --from value");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_search_empty_results() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"spans":[]}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/search-spans",
            200,
            body,
        )
        .await;

        let result = super::spans_search(
            &cfg,
            None,
            None,
            None,
            None,
            None,
            None,
            false,
            "1h".into(),
            "now".into(),
            20,
            None,
            false,
        )
        .await;
        assert!(
            result.is_ok(),
            "spans_search empty failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_search_500() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/search-spans",
            500,
            r#"{"errors":["internal server error"]}"#,
        )
        .await;

        let result = super::spans_search(
            &cfg,
            None,
            None,
            None,
            None,
            None,
            None,
            false,
            "1h".into(),
            "now".into(),
            20,
            None,
            false,
        )
        .await;
        assert!(result.is_err(), "should fail on 500");
        assert!(result.unwrap_err().to_string().contains("500"));
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_search_no_auth() {
        let _lock = lock_env().await;
        let cfg = Config {
            api_key: None,
            app_key: None,
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        let result = super::spans_search(
            &cfg,
            None,
            None,
            None,
            None,
            None,
            None,
            false,
            "1h".into(),
            "now".into(),
            20,
            None,
            false,
        )
        .await;
        assert!(result.is_err(), "should fail without auth");
        cleanup_env();
    }

    // ---- evals_list ----

    #[tokio::test]
    async fn test_llm_obs_evals_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"evaluators":[{"eval_name":"toxicity","ml_app":"my-app","created_at":"2024-01-01T00:00:00Z"}]}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/eval/list-for-org",
            200,
            body,
        )
        .await;

        let result = super::evals_list(&cfg).await;
        assert!(result.is_ok(), "evals_list failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_evals_list_500() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/eval/list-for-org",
            500,
            r#"{"errors":["internal server error"]}"#,
        )
        .await;

        let result = super::evals_list(&cfg).await;
        assert!(result.is_err(), "should fail on 500");
        assert!(result.unwrap_err().to_string().contains("500"));
        cleanup_env();
    }

    // ---- evals_list_by_ml_app ----

    #[tokio::test]
    async fn test_llm_obs_evals_list_by_ml_app() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"evaluators":[{"eval_name":"faithfulness","ml_app":"my-app","created_at":"2024-01-01T00:00:00Z"}]}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/eval/list",
            200,
            body,
        )
        .await;

        let result = super::evals_list_by_ml_app(&cfg, "my-app").await;
        assert!(
            result.is_ok(),
            "evals_list_by_ml_app failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_evals_list_by_ml_app_404() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/eval/list",
            404,
            r#"{"errors":["not found"]}"#,
        )
        .await;

        let result = super::evals_list_by_ml_app(&cfg, "missing-app").await;
        assert!(result.is_err(), "should fail on 404");
        assert!(result.unwrap_err().to_string().contains("404"));
        cleanup_env();
    }

    // ---- spans_get_trace ----

    #[tokio::test]
    async fn test_llm_obs_spans_get_trace() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"trace_id":"t-1","spans":[{"span_id":"s-1","name":"root","span_kind":"agent","children":[]}]}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/get-trace",
            200,
            body,
        )
        .await;

        let result = super::spans_get_trace(&cfg, "t-1", false, "1h".into(), "now".into()).await;
        assert!(result.is_ok(), "spans_get_trace failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_get_trace_invalid_from() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let result =
            super::spans_get_trace(&cfg, "t-1", false, "not-a-time".into(), "now".into()).await;
        assert!(result.is_err(), "expected error for invalid --from value");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_get_trace_500() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/get-trace",
            500,
            r#"{"errors":["internal server error"]}"#,
        )
        .await;

        let result = super::spans_get_trace(&cfg, "t-1", false, "1h".into(), "now".into()).await;
        assert!(result.is_err(), "should fail on 500");
        assert!(result.unwrap_err().to_string().contains("500"));
        cleanup_env();
    }

    // ---- spans_get_span_details ----

    #[tokio::test]
    async fn test_llm_obs_spans_get_span_details() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body =
            r#"{"spans":[{"span_id":"s-1","name":"llm-call","duration_ms":42.0,"error":null}]}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/span-details",
            200,
            body,
        )
        .await;

        let result = super::spans_get_span_details(
            &cfg,
            "t-1",
            vec!["s-1".into()],
            "1h".into(),
            "now".into(),
        )
        .await;
        assert!(
            result.is_ok(),
            "spans_get_span_details failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_get_span_details_all_found_no_warning() {
        // Regression test: warning must NOT fire when returned == requested.
        // The raw API response has "spans" at top level (no "data" wrapper).
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body =
            r#"{"spans":[{"span_id":"s-1","name":"found"},{"span_id":"s-2","name":"also-found"}]}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/span-details",
            200,
            body,
        )
        .await;

        let result = super::spans_get_span_details(
            &cfg,
            "t-1",
            vec!["s-1".into(), "s-2".into()],
            "1h".into(),
            "now".into(),
        )
        .await;
        assert!(
            result.is_ok(),
            "spans_get_span_details failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_get_span_details_partial_missing() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Two span IDs requested, only one returned — simulates orphaned span
        let body = r#"{"spans":[{"span_id":"s-1","name":"found"}]}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/span-details",
            200,
            body,
        )
        .await;

        let result = super::spans_get_span_details(
            &cfg,
            "t-1",
            vec!["s-1".into(), "s-orphan".into()],
            "1h".into(),
            "now".into(),
        )
        .await;
        assert!(
            result.is_ok(),
            "spans_get_span_details partial missing failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_get_span_details_500() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/span-details",
            500,
            r#"{"errors":["internal server error"]}"#,
        )
        .await;

        let result = super::spans_get_span_details(
            &cfg,
            "t-1",
            vec!["s-1".into()],
            "1h".into(),
            "now".into(),
        )
        .await;
        assert!(result.is_err(), "should fail on 500");
        assert!(result.unwrap_err().to_string().contains("500"));
        cleanup_env();
    }

    // ---- spans_get_span_content ----

    #[tokio::test]
    async fn test_llm_obs_spans_get_span_content() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"span_id":"s-1","field":"output","content":"hello world"}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/span-content",
            200,
            body,
        )
        .await;

        let result = super::spans_get_span_content(
            &cfg,
            "t-1",
            "s-1",
            "output",
            None,
            None,
            "1h".into(),
            "now".into(),
        )
        .await;
        assert!(
            result.is_ok(),
            "spans_get_span_content failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_get_span_content_invalid_from() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let result = super::spans_get_span_content(
            &cfg,
            "t-1",
            "s-1",
            "output",
            None,
            None,
            "bad-time".into(),
            "now".into(),
        )
        .await;
        assert!(result.is_err(), "expected error for invalid --from value");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_get_span_content_500() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/span-content",
            500,
            r#"{"errors":["internal server error"]}"#,
        )
        .await;

        let result = super::spans_get_span_content(
            &cfg,
            "t-1",
            "s-1",
            "output",
            None,
            None,
            "1h".into(),
            "now".into(),
        )
        .await;
        assert!(result.is_err(), "should fail on 500");
        assert!(result.unwrap_err().to_string().contains("500"));
        cleanup_env();
    }

    // ---- spans_find_error_spans ----

    #[tokio::test]
    async fn test_llm_obs_spans_find_error_spans() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"error_spans":[{"span_id":"s-err","name":"llm-call","error":{"type":"ValueError","message":"bad input"}}]}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/find-error-spans",
            200,
            body,
        )
        .await;

        let result = super::spans_find_error_spans(&cfg, "t-1", "1h".into(), "now".into()).await;
        assert!(
            result.is_ok(),
            "spans_find_error_spans failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_find_error_spans_500() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/find-error-spans",
            500,
            r#"{"errors":["internal server error"]}"#,
        )
        .await;

        let result = super::spans_find_error_spans(&cfg, "t-1", "1h".into(), "now".into()).await;
        assert!(result.is_err(), "should fail on 500");
        assert!(result.unwrap_err().to_string().contains("500"));
        cleanup_env();
    }

    // ---- spans_expand_spans ----

    #[tokio::test]
    async fn test_llm_obs_spans_expand_spans() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"spans":[{"span_id":"s-child","parent_id":"s-1","name":"tool-call","span_kind":"tool"}]}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/expand-spans",
            200,
            body,
        )
        .await;

        let result = super::spans_expand_spans(
            &cfg,
            "t-1",
            vec!["s-1".into()],
            None,
            None,
            "1h".into(),
            "now".into(),
        )
        .await;
        assert!(
            result.is_ok(),
            "spans_expand_spans failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_expand_spans_500() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/expand-spans",
            500,
            r#"{"errors":["internal server error"]}"#,
        )
        .await;

        let result = super::spans_expand_spans(
            &cfg,
            "t-1",
            vec!["s-1".into()],
            None,
            None,
            "1h".into(),
            "now".into(),
        )
        .await;
        assert!(result.is_err(), "should fail on 500");
        assert!(result.unwrap_err().to_string().contains("500"));
        cleanup_env();
    }

    // ---- spans_get_agent_loop ----

    #[tokio::test]
    async fn test_llm_obs_spans_get_agent_loop() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"trace_id":"t-1","steps":[{"step":1,"span_id":"s-1","action":"tool_call","content":"search query"}]}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/get-agent-loop",
            200,
            body,
        )
        .await;

        let result =
            super::spans_get_agent_loop(&cfg, "t-1", None, None, "1h".into(), "now".into()).await;
        assert!(
            result.is_ok(),
            "spans_get_agent_loop failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_get_agent_loop_500() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/get-agent-loop",
            500,
            r#"{"errors":["internal server error"]}"#,
        )
        .await;

        let result =
            super::spans_get_agent_loop(&cfg, "t-1", None, None, "1h".into(), "now".into()).await;
        assert!(result.is_err(), "should fail on 500");
        assert!(result.unwrap_err().to_string().contains("500"));
        cleanup_env();
    }

    // ---- evals_get_evaluator ----

    #[tokio::test]
    async fn test_llm_obs_evals_get_evaluator() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body =
            r#"{"evaluator":{"eval_name":"toxicity","ml_app":"my-app","sampling_rate":1.0}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/custom-evaluator/get",
            200,
            body,
        )
        .await;

        let result = super::evals_get_evaluator(&cfg, "toxicity").await;
        assert!(
            result.is_ok(),
            "evals_get_evaluator failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_evals_get_evaluator_404() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/custom-evaluator/get",
            404,
            r#"{"errors":["not found"]}"#,
        )
        .await;

        let result = super::evals_get_evaluator(&cfg, "missing").await;
        assert!(result.is_err(), "should fail on 404");
        assert!(result.unwrap_err().to_string().contains("404"));
        cleanup_env();
    }

    // ---- evals_get_aggregate_stats ----

    #[tokio::test]
    async fn test_llm_obs_evals_get_aggregate_stats() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"stats":{"eval_name":"toxicity","pass_rate":0.85,"total":100,"by_value":{"pass":85,"fail":15}}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/eval/aggregate-stats",
            200,
            body,
        )
        .await;

        let result =
            super::evals_get_aggregate_stats(&cfg, "toxicity", None, "1h".into(), "now".into())
                .await;
        assert!(
            result.is_ok(),
            "evals_get_aggregate_stats failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_evals_get_aggregate_stats_with_ml_app() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body =
            r#"{"stats":{"eval_name":"toxicity","ml_app":"my-app","pass_rate":0.9,"total":50}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/eval/aggregate-stats",
            200,
            body,
        )
        .await;

        let result = super::evals_get_aggregate_stats(
            &cfg,
            "toxicity",
            Some("my-app".into()),
            "1h".into(),
            "now".into(),
        )
        .await;
        assert!(
            result.is_ok(),
            "evals_get_aggregate_stats with ml_app failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_evals_get_aggregate_stats_invalid_from() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let result = super::evals_get_aggregate_stats(
            &cfg,
            "toxicity",
            None,
            "not-a-time".into(),
            "now".into(),
        )
        .await;
        assert!(result.is_err(), "expected error for invalid --from value");
        cleanup_env();
    }

    // ---- evals_create_or_update ----

    #[tokio::test]
    async fn test_llm_obs_evals_create_or_update() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_eval_create.json",
            r#"{"prompt_template":"Rate: {{input}}","output_schema":{"type":"score"}}"#,
        );
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/custom-evaluator/create-or-update",
            200,
            r#"{"status":"ok"}"#,
        )
        .await;

        let result = super::evals_create_or_update(&cfg, "toxicity", tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "evals_create_or_update failed: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_evals_create_or_update_400() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json("pup_test_eval_create_400.json", r#"{"invalid":"body"}"#);
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/custom-evaluator/create-or-update",
            400,
            r#"{"errors":["bad request"]}"#,
        )
        .await;

        let result = super::evals_create_or_update(&cfg, "toxicity", tmp.to_str().unwrap()).await;
        assert!(result.is_err(), "should fail on 400");
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
    }

    // ---- evals_delete ----

    #[tokio::test]
    async fn test_llm_obs_evals_delete() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/custom-evaluator/delete",
            200,
            r#"{"status":"ok"}"#,
        )
        .await;

        let result = super::evals_delete(&cfg, "toxicity").await;
        assert!(result.is_ok(), "evals_delete failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_evals_delete_404() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/custom-evaluator/delete",
            404,
            r#"{"errors":["not found"]}"#,
        )
        .await;

        let result = super::evals_delete(&cfg, "missing").await;
        assert!(result.is_err(), "should fail on 404");
        assert!(result.unwrap_err().to_string().contains("404"));
        cleanup_env();
    }

    // ---- spans_search --summary ----

    #[tokio::test]
    async fn test_llm_obs_spans_search_summary_drops_verbose_fields() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"spans":[{"span_id":"s-1","trace_id":"t-1","name":"llm-call","span_kind":"llm","ml_app":"my-app","service":"svc","status":"ok","duration_ms":42.0,"start_ms":1000000,"parent_id":"undefined","tags":["env:prod"],"llm_info":{"model_name":"gpt-4","input_tokens":100},"input":{"preview":"hello"}}]}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/search-spans",
            200,
            body,
        )
        .await;

        let result = super::spans_search(
            &cfg,
            None,
            None,
            None,
            None,
            None,
            None,
            false,
            "1h".into(),
            "now".into(),
            10,
            None,
            true,
        )
        .await;
        assert!(
            result.is_ok(),
            "spans_search --summary failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_search_no_summary() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"spans":[{"span_id":"s-1","trace_id":"t-1","name":"llm-call","span_kind":"llm","ml_app":"my-app","status":"ok","duration_ms":42.0,"start_ms":1000000}]}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/trace/search-spans",
            200,
            body,
        )
        .await;

        let result = super::spans_search(
            &cfg,
            None,
            None,
            None,
            None,
            None,
            None,
            false,
            "1h".into(),
            "now".into(),
            10,
            None,
            false,
        )
        .await;
        assert!(result.is_ok(), "spans_search failed: {:?}", result.err());
        cleanup_env();
    }

    // ---- datasets_batch_update ----

    #[tokio::test]
    async fn test_llm_obs_datasets_batch_update() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_ds_batch_update.json",
            r#"{"data":{"id":"ds-1","type":"datasets","attributes":{"insert_records":[],"delete_records":[]}}}"#,
        );
        let resp_body = r#"{"data":[]}"#;
        let _mock = mock_any(&mut server, "POST", resp_body).await;

        let result =
            super::datasets_batch_update(&cfg, "proj-1", "ds-1", tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "datasets_batch_update failed: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_batch_update_400() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_ds_batch_update_400.json",
            r#"{"data":{"id":"ds-1","type":"datasets","attributes":{"insert_records":[],"delete_records":[]}}}"#,
        );
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["bad request"]}"#)
            .create_async()
            .await;

        let result =
            super::datasets_batch_update(&cfg, "proj-1", "ds-1", tmp.to_str().unwrap()).await;
        assert!(result.is_err(), "should fail on 400");
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    // ---- datasets_clone ----

    #[tokio::test]
    async fn test_llm_obs_datasets_clone() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_ds_clone.json",
            r#"{"data":{"id":"ds-1","type":"datasets","attributes":{"name":"cloned-dataset"}}}"#,
        );
        let resp_body = r#"{"data":{"id":"ds-2","type":"datasets","attributes":{"name":"cloned-dataset","description":null,"metadata":null,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","current_version":1}}}"#;
        let _mock = mock_any(&mut server, "POST", resp_body).await;

        let result = super::datasets_clone(&cfg, "proj-1", "ds-1", tmp.to_str().unwrap()).await;
        assert!(result.is_ok(), "datasets_clone failed: {:?}", result.err());
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_clone_404() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_ds_clone_404.json",
            r#"{"data":{"id":"ds-1","type":"datasets","attributes":{"name":"cloned-dataset"}}}"#,
        );
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["not found"]}"#)
            .create_async()
            .await;

        let result =
            super::datasets_clone(&cfg, "proj-1", "ds-missing", tmp.to_str().unwrap()).await;
        assert!(result.is_err(), "should fail on 404");
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    // ---- datasets_restore ----

    #[tokio::test]
    async fn test_llm_obs_datasets_restore() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_ds_restore.json",
            r#"{"data":{"id":"ds-1","type":"datasets","attributes":{"dataset_version":2}}}"#,
        );
        let resp_body = r#"{"data":{"id":"ds-1","type":"datasets","attributes":{"name":"my-dataset","description":null,"metadata":null,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z","current_version":2}}}"#;
        let _mock = mock_any(&mut server, "POST", resp_body).await;

        let result = super::datasets_restore(&cfg, "proj-1", "ds-1", tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "datasets_restore failed: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_restore_400() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_ds_restore_400.json",
            r#"{"data":{"id":"ds-1","type":"datasets","attributes":{"dataset_version":99}}}"#,
        );
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["invalid version"]}"#)
            .create_async()
            .await;

        let result = super::datasets_restore(&cfg, "proj-1", "ds-1", tmp.to_str().unwrap()).await;
        assert!(result.is_err(), "should fail on 400");
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }
}

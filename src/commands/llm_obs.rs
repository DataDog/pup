use anyhow::Result;
use datadog_api_client::datadogV2::api_llm_observability::{
    LLMObservabilityAPI, ListLLMObsAnnotationQueuesOptionalParams,
};
use datadog_api_client::datadogV2::model::{
    LLMObsAnnotationQueueInteractionsRequest, LLMObsAnnotationQueueRequest,
    LLMObsAnnotationQueueUpdateRequest, LLMObsCustomEvalConfigUpdateRequest,
    LLMObsDatasetBatchUpdateRequest, LLMObsDatasetCloneRequest, LLMObsDatasetRequest,
    LLMObsDatasetRestoreVersionRequest, LLMObsDeleteAnnotationQueueInteractionsRequest,
    LLMObsDeleteExperimentsRequest, LLMObsProjectRequest,
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

pub async fn projects_list(
    cfg: &Config,
    filter_id: Option<String>,
    filter_name: Option<String>,
    limit: Option<u32>,
    cursor: Option<String>,
) -> Result<()> {
    let mut query: Vec<(&str, String)> = Vec::new();
    if let Some(ref id) = filter_id {
        query.push(("filter[id]", id.clone()));
    }
    if let Some(ref name) = filter_name {
        query.push(("filter[name]", name.clone()));
    }
    if let Some(l) = limit {
        query.push(("page[limit]", l.to_string()));
    }
    if let Some(ref c) = cursor {
        query.push(("page[cursor]", c.clone()));
    }
    let query_refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let resp = raw_client::raw_get(cfg, "/api/v2/llm-obs/v1/projects", &query_refs)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list LLM obs projects: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Experiments ----

/// Create an experiment.
///
/// Uses the raw client rather than the typed one: the API's response omits `config`, which the
/// generated `LLMObsExperimentResponse` model requires, so the typed call fails to deserialize a
/// 200 and reports an error **after the experiment has already been created**. The request shape is
/// unchanged — same route, same body, `project_id` still required by the API.
pub async fn experiments_create(cfg: &Config, file: &str) -> Result<()> {
    let body: serde_json::Value = util::read_json_file(file)?;
    let resp = raw_client::raw_post(cfg, "/api/v2/llm-obs/v1/experiments", body)
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

/// Update an experiment's mutable fields.
///
/// Uses the raw client rather than the typed one: this endpoint answers a successful PATCH with
/// **HTTP 200 and a zero-byte body**, which the generated client tries to deserialize into
/// `LLMObsExperimentResponse` and fails on with "EOF while parsing a value" — reporting an error
/// on a write that has already landed. Callers scripting against pup would treat that exit code as
/// failure and retry, double-writing. The request shape is unchanged.
pub async fn experiments_update(cfg: &Config, experiment_id: &str, file: &str) -> Result<()> {
    let body: serde_json::Value = util::read_json_file(file)?;
    let path = format!("/api/v2/llm-obs/v1/experiments/{experiment_id}");
    let resp = raw_client::raw_patch(cfg, &path, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update LLM obs experiment: {e:?}"))?;
    // Empty body on success: report the update rather than printing a bare `null`.
    let out = if resp.is_null() {
        serde_json::json!({ "experiment_id": experiment_id, "status": "updated" })
    } else {
        resp
    };
    formatter::output(cfg, &out)
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

pub async fn datasets_list(
    cfg: &Config,
    project_id: &str,
    filter_id: Option<String>,
    filter_name: Option<String>,
    limit: Option<u32>,
    cursor: Option<String>,
) -> Result<()> {
    let path = format!("/api/v2/llm-obs/v1/{project_id}/datasets");
    let mut query: Vec<(&str, String)> = Vec::new();
    if let Some(ref id) = filter_id {
        query.push(("filter[id]", id.clone()));
    }
    if let Some(ref name) = filter_name {
        query.push(("filter[name]", name.clone()));
    }
    if let Some(l) = limit {
        query.push(("page[limit]", l.to_string()));
    }
    if let Some(ref c) = cursor {
        query.push(("page[cursor]", c.clone()));
    }
    let query_refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let resp = raw_client::raw_get(cfg, &path, &query_refs)
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

// ---- Dataset records (no typed equivalent — unstable MCP endpoints) ----

#[allow(clippy::too_many_arguments)]
pub async fn datasets_records(
    cfg: &Config,
    project_id: &str,
    dataset_id: &str,
    record_ids: Option<Vec<String>>,
    tags: Option<Vec<String>>,
    canonical_id: Option<String>,
    dataset_version: Option<i64>,
    limit: u32,
    cursor: Option<String>,
    compute_schema: Option<bool>,
) -> Result<()> {
    let mut body = serde_json::json!({
        "project_id": project_id,
        "dataset_id": dataset_id,
        "limit": limit,
    });
    if let Some(ids) = record_ids {
        body["record_ids"] = serde_json::json!(ids);
    }
    if let Some(t) = tags {
        body["tags"] = serde_json::json!(t);
    }
    if let Some(c) = canonical_id {
        body["canonical_id"] = serde_json::json!(c);
    }
    if let Some(v) = dataset_version {
        body["dataset_version"] = serde_json::json!(v);
    }
    if let Some(c) = cursor {
        body["cursor"] = serde_json::json!(c);
    }
    if let Some(cs) = compute_schema {
        body["compute_schema"] = serde_json::json!(cs);
    }
    let resp = raw_client::raw_post(cfg, "/api/unstable/llm-obs-mcp/v1/dataset/records", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get dataset records: {e:?}"))?;
    formatter::output(cfg, &resp)
}

/// Hard stop on page count so a misbehaving cursor cannot loop forever.
const RECORDS_ALL_MAX_PAGES: u32 = 200;

/// Read EVERY record in a dataset by paging the REST records endpoint.
///
/// `datasets records` posts to `llm-obs-mcp/v1/dataset/records`, which trims its response to a
/// size budget (~19 records on a dataset with sizeable inputs), reports `truncated: true`, and
/// returns **no cursor** — so there is no way to reach the rest of a large dataset through it.
/// This pages `GET /api/unstable/llm-obs/v1/datasets/{id}/records` using `meta.after`, which has
/// no such cap, and emits the aggregated records under the same `records`/`returned` keys.
///
/// The REST route needs no project_id, so this takes only the dataset id.
pub async fn datasets_records_all(
    cfg: &Config,
    dataset_id: &str,
    limit: Option<u32>,
) -> Result<()> {
    let path = format!("/api/unstable/llm-obs/v1/datasets/{dataset_id}/records");
    let page_limit = limit.unwrap_or(100).to_string();
    let mut records: Vec<serde_json::Value> = Vec::new();
    let mut cursor = String::new();
    let mut pages: u32 = 0;

    loop {
        let mut query: Vec<(&str, &str)> = vec![("page[limit]", page_limit.as_str())];
        if !cursor.is_empty() {
            query.push(("page[cursor]", cursor.as_str()));
        }
        let resp = raw_client::raw_get(cfg, &path, &query)
            .await
            .map_err(|e| anyhow::anyhow!("failed to list dataset records: {e:?}"))?;
        pages += 1;

        match resp["data"].as_array() {
            Some(page) if !page.is_empty() => records.extend(page.iter().cloned()),
            _ => break,
        }

        let after = resp["meta"]["after"].as_str().unwrap_or_default();
        // Empty or unchanged cursor both mean "no further pages".
        if after.is_empty() || after == cursor {
            break;
        }
        cursor = after.to_string();

        if pages >= RECORDS_ALL_MAX_PAGES {
            eprintln!(
                "warning: stopped after {pages} pages ({} records); the dataset may have more",
                records.len()
            );
            break;
        }
    }

    let out = serde_json::json!({
        "dataset_id": dataset_id,
        "kind": "full_list",
        "records": records,
        "returned": records.len(),
        "truncated": false,
        "pages_fetched": pages,
    });
    formatter::output(cfg, &out)
}

pub async fn datasets_records_full(
    cfg: &Config,
    project_id: &str,
    dataset_id: &str,
    record_ids: Vec<String>,
) -> Result<()> {
    let body = serde_json::json!({
        "project_id": project_id,
        "dataset_id": dataset_id,
        "record_ids": record_ids,
    });
    let resp = raw_client::raw_post(
        cfg,
        "/api/unstable/llm-obs-mcp/v1/dataset/records-full",
        body,
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to get full dataset records: {e:?}"))?;
    formatter::output(cfg, &resp)
}

/// Adds records to a dataset (mirrors the add_llmobs_dataset_records MCP tool).
///
/// The endpoint is two-step: without `confirm` it returns a preview (resolved IDs,
/// planned record count, tag union) and writes nothing; with `confirm` it inserts.
pub async fn datasets_records_add(
    cfg: &Config,
    project_id: &str,
    dataset_id: &str,
    file: &str,
    confirm: bool,
    create_new_version: Option<bool>,
) -> Result<()> {
    let records: serde_json::Value = util::read_json_file(file)?;
    match records.as_array() {
        Some(a) if !a.is_empty() => {}
        _ => anyhow::bail!("--file must contain a non-empty JSON array of records"),
    }
    let mut body = serde_json::json!({
        "project_id": project_id,
        "dataset_id": dataset_id,
        "records": records,
        "confirmed": confirm,
    });
    if let Some(v) = create_new_version {
        body["create_new_version"] = serde_json::json!(v);
    }
    let resp = raw_client::raw_post(
        cfg,
        "/api/unstable/llm-obs-mcp/v1/dataset/records-add",
        body,
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to add dataset records: {e:?}"))?;
    formatter::output(cfg, &resp)
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

pub async fn experiments_events_submit(
    cfg: &Config,
    experiment_id: &str,
    metrics: &str,
    tags: Option<Vec<String>>,
) -> Result<()> {
    // Mirrors the submit_llmobs_experiment_events MCP tool: experiment_id + metrics
    // (array) + optional tags. metrics is passed through as-is; the server validates it.
    let metrics: serde_json::Value = serde_json::from_str(metrics).map_err(|e| {
        anyhow::anyhow!("--metrics must be a JSON array of eval-metric events: {e}")
    })?;
    let mut body = serde_json::json!({
        "experiment_id": experiment_id,
        "metrics": metrics,
    });
    if let Some(t) = tags {
        body["tags"] = serde_json::json!(t);
    }
    let resp = raw_client::raw_post(
        cfg,
        "/api/unstable/llm-obs-mcp/v1/experiment/ingest-events",
        body,
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to submit experiment events: {e:?}"))?;
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

/// Parses `key:value` tag filters into the JSON object the search-spans endpoint expects.
/// Splits on the first colon only, so values may themselves contain colons.
fn parse_tag_filters(tags: &[String]) -> Result<serde_json::Map<String, serde_json::Value>> {
    let mut map = serde_json::Map::new();
    for tag in tags {
        let (key, value) = tag
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("--tags entries must be \"key:value\", got '{tag}'"))?;
        if key.is_empty() || value.is_empty() {
            anyhow::bail!("--tags entries must have a non-empty key and value, got '{tag}'");
        }
        map.insert(key.to_string(), serde_json::json!(value));
    }
    Ok(map)
}

#[allow(clippy::too_many_arguments)]
pub async fn spans_search(
    cfg: &Config,
    query: Option<String>,
    tags: Option<Vec<String>>,
    trace_id: Option<String>,
    apm_trace_id: Option<String>,
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
    if let Some(t) = tags {
        body["tags"] = serde_json::Value::Object(parse_tag_filters(&t)?);
    }
    if let Some(t) = trace_id {
        body["trace_id"] = serde_json::json!(t);
    }
    if let Some(a) = apm_trace_id {
        body["apm_trace_id"] = serde_json::json!(a);
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

// ---- Topic Discovery / Patterns (no typed equivalent — unstable MCP endpoints) ----
//
// Topic Discovery clusters LLM Obs spans into a topic hierarchy. A config defines
// what to cluster; a run is one clustering of those spans at a point in time; a
// completed run yields topics, each backed by clustering points (spans). The usual
// read flow is:
//
//   patterns configs list -> patterns runs status -> patterns topics
//     -> patterns points

const PATTERNS_BASE: &str = "/api/unstable/llm-obs-mcp/v1/topic-discovery";

/// POSTs `body` to a topic-discovery endpoint and prints the response.
async fn patterns_post(
    cfg: &Config,
    endpoint: &str,
    body: serde_json::Value,
    what: &str,
) -> Result<()> {
    let path = format!("{PATTERNS_BASE}/{endpoint}");
    let resp = raw_client::raw_post(cfg, &path, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to {what}: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn patterns_configs_list(cfg: &Config) -> Result<()> {
    patterns_post(
        cfg,
        "configs/list",
        serde_json::json!({}),
        "list pattern configs",
    )
    .await
}

/// Gets the most-recently-modified pattern config for the org. Takes no arguments —
/// use `patterns configs list` to see all configs or resolve a specific config_id.
pub async fn patterns_configs_get(cfg: &Config) -> Result<()> {
    patterns_post(
        cfg,
        "config/get",
        serde_json::json!({}),
        "get pattern config",
    )
    .await
}

pub async fn patterns_runs_list(cfg: &Config, config_id: &str) -> Result<()> {
    patterns_post(
        cfg,
        "runs/list",
        serde_json::json!({ "config_id": config_id }),
        "list pattern runs",
    )
    .await
}

pub async fn patterns_runs_status(cfg: &Config, config_id: &str) -> Result<()> {
    patterns_post(
        cfg,
        "run-status",
        serde_json::json!({ "config_id": config_id }),
        "get pattern run status",
    )
    .await
}

pub async fn patterns_topics(cfg: &Config, config_id: &str, run_id: Option<String>) -> Result<()> {
    let mut body = serde_json::json!({ "config_id": config_id });
    if let Some(r) = run_id {
        body["run_id"] = serde_json::json!(r);
    }
    patterns_post(cfg, "topics", body, "get patterns").await
}

pub async fn patterns_topics_with_points(
    cfg: &Config,
    config_id: &str,
    run_id: Option<String>,
    include_metrics: bool,
) -> Result<()> {
    let mut body = serde_json::json!({ "config_id": config_id });
    if let Some(r) = run_id {
        body["run_id"] = serde_json::json!(r);
    }
    if include_metrics {
        body["include_metrics"] = serde_json::json!(true);
    }
    patterns_post(cfg, "topics-with-points", body, "get patterns with points").await
}

pub async fn patterns_points(
    cfg: &Config,
    topic_id: &str,
    page_size: Option<u32>,
    page_token: Option<String>,
) -> Result<()> {
    let mut body = serde_json::json!({ "topic_id": topic_id });
    if let Some(s) = page_size {
        body["page_size"] = serde_json::json!(s);
    }
    if let Some(t) = page_token {
        body["page_token"] = serde_json::json!(t);
    }
    patterns_post(cfg, "clustered-points", body, "get pattern points").await
}

// ---- Agent Insights (no typed equivalent — unstable MCP endpoints) ----
//
// Agent Insights are server-generated findings about an ML app (for example a
// tool-call retry loop) that move through a review lifecycle. The usual flow is:
//
//   agent-insights list -> agent-insights get -> agent-insights update-status
//     -> agent-insights submit-feedback
//
// `list` and `get` return `feedback_targets`; a `submit-feedback` target key must
// come from one of those responses.

const AGENT_INSIGHTS_BASE: &str = "/api/unstable/llm-obs-mcp/v1/agent-insights";

/// Lifecycle statuses accepted by `--status`, in the server's declared order.
const AGENT_INSIGHT_STATUSES: [&str; 4] = ["for_review", "in_progress", "completed", "ignored"];

/// Usefulness verdicts accepted for a feedback target.
const AGENT_INSIGHT_USEFULNESS: [&str; 3] = ["useful", "somewhat_useful", "not_useful"];

/// Feedback items the endpoint accepts per submission. Rejecting locally keeps an
/// oversized batch from being sent and partially applied.
const MAX_AGENT_INSIGHT_FEEDBACK_ITEMS: usize = 25;

/// Rejects a value outside `allowed`, naming the flag and the valid values. The
/// server enforces these too; failing here gives a usable message instead of a 400.
fn validate_choice(flag: &str, value: &str, allowed: &[&str]) -> Result<()> {
    if !allowed.contains(&value) {
        anyhow::bail!(
            "{flag} must be one of {}, got '{value}'",
            allowed.join(", ")
        );
    }
    Ok(())
}

/// POSTs `body` to an agent-insights endpoint and prints the response.
async fn agent_insights_post(
    cfg: &Config,
    endpoint: &str,
    body: serde_json::Value,
    what: &str,
) -> Result<()> {
    let path = format!("{AGENT_INSIGHTS_BASE}/{endpoint}");
    let resp = raw_client::raw_post(cfg, &path, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to {what}: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn agent_insights_list(
    cfg: &Config,
    ml_app: Option<String>,
    status: Option<String>,
    insight_type: Option<String>,
    limit: Option<u32>,
    cursor: Option<String>,
) -> Result<()> {
    let mut body = serde_json::json!({});
    if let Some(a) = ml_app {
        body["ml_app"] = serde_json::json!(a);
    }
    if let Some(s) = status {
        validate_choice("--status", &s, &AGENT_INSIGHT_STATUSES)?;
        body["status"] = serde_json::json!(s);
    }
    if let Some(t) = insight_type {
        body["insight_type"] = serde_json::json!(t);
    }
    if let Some(l) = limit {
        body["limit"] = serde_json::json!(l);
    }
    if let Some(c) = cursor {
        body["cursor"] = serde_json::json!(c);
    }
    agent_insights_post(cfg, "list", body, "list agent insights").await
}

pub async fn agent_insights_get(cfg: &Config, insight_id: &str) -> Result<()> {
    agent_insights_post(
        cfg,
        "get",
        serde_json::json!({ "insight_id": insight_id }),
        "get agent insight",
    )
    .await
}

pub async fn agent_insights_update_status(
    cfg: &Config,
    insight_id: &str,
    status: &str,
) -> Result<()> {
    validate_choice("--status", status, &AGENT_INSIGHT_STATUSES)?;
    agent_insights_post(
        cfg,
        "status",
        serde_json::json!({ "insight_id": insight_id, "status": status }),
        "update agent insight status",
    )
    .await
}

/// Parses `--feedback` entries into the `feedback_items` array the endpoint expects.
///
/// Each entry is `target_key=usefulness[=reasoning]`. The separator is `=` rather than
/// `:` because real target keys embed colons — `suggested_evaluator:<eval_name>` — while
/// neither a target key nor a usefulness value contains `=`. Splitting into at most three
/// parts also leaves any `=` in the free-text reasoning intact.
fn parse_feedback_items(feedback: &[String]) -> Result<Vec<serde_json::Value>> {
    if feedback.is_empty() {
        anyhow::bail!("--feedback requires at least one entry");
    }
    if feedback.len() > MAX_AGENT_INSIGHT_FEEDBACK_ITEMS {
        anyhow::bail!(
            "--feedback accepts at most {MAX_AGENT_INSIGHT_FEEDBACK_ITEMS} entries, got {}",
            feedback.len()
        );
    }
    let mut items = Vec::with_capacity(feedback.len());
    for entry in feedback {
        let mut parts = entry.splitn(3, '=');
        let target_key = parts.next().unwrap_or_default();
        let usefulness = parts.next().unwrap_or_default();
        if target_key.is_empty() || usefulness.is_empty() {
            anyhow::bail!(
                "--feedback entries must be \"target_key=usefulness[=reasoning]\", got '{entry}'"
            );
        }
        validate_choice(
            "--feedback usefulness",
            usefulness,
            &AGENT_INSIGHT_USEFULNESS,
        )?;
        let mut item = serde_json::json!({
            "target_key": target_key,
            "usefulness": usefulness,
        });
        if let Some(reasoning) = parts.next().filter(|r| !r.is_empty()) {
            item["reasoning"] = serde_json::json!(reasoning);
        }
        items.push(item);
    }
    Ok(items)
}

pub async fn agent_insights_submit_feedback(
    cfg: &Config,
    insight_id: &str,
    feedback: Vec<String>,
) -> Result<()> {
    let items = parse_feedback_items(&feedback)?;
    agent_insights_post(
        cfg,
        "feedback",
        serde_json::json!({ "insight_id": insight_id, "feedback_items": items }),
        "submit agent insight feedback",
    )
    .await
}

// ---- Model pricing (no typed equivalent — unstable MCP endpoint) ----

/// Gets canonical model pricing rate cards, in USD per million tokens.
///
/// At least one of `provider`/`model` must be given: with neither, the endpoint has
/// nothing to match on. A `provider` alone scopes to that provider's catalog; a
/// `model` alone searches every provider, so results carry their own `provider`.
pub async fn model_pricing(
    cfg: &Config,
    provider: Option<String>,
    model: Option<String>,
    limit: Option<u32>,
    cursor: Option<String>,
) -> Result<()> {
    if provider.is_none() && model.is_none() {
        anyhow::bail!("at least one of --provider or --model is required");
    }
    let mut body = serde_json::json!({});
    if let Some(p) = provider {
        body["provider"] = serde_json::json!(p);
    }
    if let Some(m) = model {
        body["model"] = serde_json::json!(m);
    }
    if let Some(l) = limit {
        body["limit"] = serde_json::json!(l);
    }
    if let Some(c) = cursor {
        body["cursor"] = serde_json::json!(c);
    }
    let resp = raw_client::raw_post(cfg, "/api/unstable/llm-obs-mcp/v1/pricing/model", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get model pricing: {e:?}"))?;
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

        let result = super::projects_list(&cfg, None, None, None, None).await;
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

        let result = super::projects_list(&cfg, None, None, None, None).await;
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
        let result = super::projects_list(&cfg, None, None, None, None).await;
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
        let result = super::projects_list(&cfg, None, None, None, None).await;
        assert!(
            result.is_ok(),
            "should tolerate missing nullable fields: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_projects_list_with_filters() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Strict query param check: name/id filters and pagination are sent through.
        let _mock = server
            .mock("GET", "/api/v2/llm-obs/v1/projects")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("filter[id]".into(), "proj-1".into()),
                mockito::Matcher::UrlEncoded("filter[name]".into(), "my-project".into()),
                mockito::Matcher::UrlEncoded("page[limit]".into(), "5".into()),
                mockito::Matcher::UrlEncoded("page[cursor]".into(), "cursor-abc".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        let result = super::projects_list(
            &cfg,
            Some("proj-1".into()),
            Some("my-project".into()),
            Some(5),
            Some("cursor-abc".into()),
        )
        .await;
        assert!(
            result.is_ok(),
            "projects_list with filters failed: {:?}",
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
    async fn test_llm_obs_datasets_records_all_pages_until_cursor_empty() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Page 1: two records plus a cursor. Matched by the ABSENCE of page[cursor].
        let page1 = server
            .mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::UrlEncoded(
                "page[limit]".into(),
                "2".into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[{"id":"rec-1"},{"id":"rec-2"}],"meta":{"after":"CURSOR2"}}"#)
            .expect(1)
            .create_async()
            .await;

        // Page 2: one record and an empty cursor, which terminates the loop.
        let page2 = server
            .mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::UrlEncoded(
                "page[cursor]".into(),
                "CURSOR2".into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[{"id":"rec-3"}],"meta":{"after":""}}"#)
            .expect(1)
            .create_async()
            .await;

        let result = super::datasets_records_all(&cfg, "ds-1", Some(2)).await;
        assert!(
            result.is_ok(),
            "datasets_records_all failed: {:?}",
            result.err()
        );
        // Both pages must have been requested — proves the cursor was followed.
        page1.assert_async().await;
        page2.assert_async().await;

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_records_all_stops_on_repeated_cursor() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Always returns the same cursor it was given: a server bug that would loop forever
        // if the guard were missing.
        let _mock = mock_any(
            &mut server,
            "GET",
            r#"{"data":[{"id":"rec-1"}],"meta":{"after":"SAME"}}"#,
        )
        .await;

        let result = super::datasets_records_all(&cfg, "ds-1", None).await;
        assert!(result.is_ok(), "expected ok, got {:?}", result.err());

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_records_all_500() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["internal error"]}"#)
            .create_async()
            .await;

        let result = super::datasets_records_all(&cfg, "ds-1", None).await;
        assert!(
            result.is_err(),
            "expected error but got ok: {:?}",
            result.ok()
        );

        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_update_empty_200_body() {
        // The real endpoint answers a successful PATCH with HTTP 200 and zero bytes. The typed
        // client failed this with "EOF while parsing a value" AFTER the write had landed.
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let tmp = write_temp_json(
            "pup_test_exp_update_empty.json",
            r#"{"data":{"id":"exp-1","type":"experiments","attributes":{"status":"completed"}}}"#,
        );
        let _mock = server
            .mock("PATCH", mockito::Matcher::Any)
            .with_status(200)
            .with_body("")
            .create_async()
            .await;

        let result = super::experiments_update(&cfg, "exp-1", tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "empty 200 body must not be an error: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_update_json_body() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let tmp = write_temp_json(
            "pup_test_exp_update_json.json",
            r#"{"data":{"id":"exp-1","type":"experiments","attributes":{"status":"running"}}}"#,
        );
        let _mock = mock_any(
            &mut server,
            "PATCH",
            r#"{"data":{"id":"exp-1","type":"experiments","attributes":{"status":"running"}}}"#,
        )
        .await;

        let result = super::experiments_update(&cfg, "exp-1", tmp.to_str().unwrap()).await;
        assert!(result.is_ok(), "update failed: {:?}", result.err());
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_update_500() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let tmp = write_temp_json(
            "pup_test_exp_update_500.json",
            r#"{"data":{"id":"exp-1","type":"experiments","attributes":{"status":"running"}}}"#,
        );
        let _mock = server
            .mock("PATCH", mockito::Matcher::Any)
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["internal error"]}"#)
            .create_async()
            .await;

        let result = super::experiments_update(&cfg, "exp-1", tmp.to_str().unwrap()).await;
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
    async fn test_llm_obs_experiments_create_response_without_config() {
        // The API's create response omits `config`, which the typed model requires — the typed
        // client errored on a 200 after the experiment had already been created.
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let tmp = write_temp_json(
            "pup_test_exp_create_noconfig.json",
            r#"{"data":{"type":"experiments","attributes":{"name":"x","project_id":"proj-1"}}}"#,
        );
        let _mock = mock_any(
            &mut server,
            "POST",
            r#"{"data":{"id":"exp-1","type":"experiments","attributes":{"name":"x","project_id":"proj-1","is_auto_experiment":true,"created_at":"2024-01-01T00:00:00Z"}}}"#,
        )
        .await;

        let result = super::experiments_create(&cfg, tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "create must tolerate a response without `config`: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(tmp);
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

        let result = super::datasets_list(&cfg, "proj-1", None, None, None, None).await;
        assert!(result.is_ok(), "datasets_list failed: {:?}", result.err());
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_list_with_filters() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Strict path + query check: project-scoped path, name/id filters, pagination.
        let _mock = server
            .mock("GET", "/api/v2/llm-obs/v1/proj-1/datasets")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("filter[id]".into(), "ds-1".into()),
                mockito::Matcher::UrlEncoded("filter[name]".into(), "my-dataset".into()),
                mockito::Matcher::UrlEncoded("page[limit]".into(), "25".into()),
                mockito::Matcher::UrlEncoded("page[cursor]".into(), "cursor-xyz".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        let result = super::datasets_list(
            &cfg,
            "proj-1",
            Some("ds-1".into()),
            Some("my-dataset".into()),
            Some(25),
            Some("cursor-xyz".into()),
        )
        .await;
        assert!(
            result.is_ok(),
            "datasets_list with filters failed: {:?}",
            result.err()
        );
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

        let result = super::datasets_list(&cfg, "proj-1", None, None, None, None).await;
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
    async fn test_llm_obs_spans_search_tags_and_apm_trace_id() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Strict body check: tags go up as a JSON object map, apm_trace_id as a string.
        let _mock = server
            .mock("POST", "/api/unstable/llm-obs-mcp/v1/trace/search-spans")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "tags": {"env": "prod", "version": "1.2:3"},
                "apm_trace_id": "apm-9",
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"spans":[]}"#)
            .create_async()
            .await;

        let result = super::spans_search(
            &cfg,
            None,
            // The second tag's value contains a colon — only the first colon splits.
            Some(vec!["env:prod".into(), "version:1.2:3".into()]),
            None,
            Some("apm-9".into()),
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
        assert!(
            result.is_ok(),
            "spans_search with tags failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_spans_search_invalid_tags() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Malformed --tags must fail locally, before any request is made.
        for bad in ["envprod", ":prod", "env:"] {
            let result = super::spans_search(
                &cfg,
                None,
                Some(vec![bad.to_string()]),
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
            assert!(result.is_err(), "expected error for --tags value '{bad}'");
        }
        cleanup_env();
    }

    #[test]
    fn test_parse_tag_filters() {
        let map = super::parse_tag_filters(&["env:prod".into(), "svc:api:v2".into()]).unwrap();
        assert_eq!(map["env"], serde_json::json!("prod"));
        // Only the first colon splits, so colons survive inside values.
        assert_eq!(map["svc"], serde_json::json!("api:v2"));
        assert!(super::parse_tag_filters(&[]).unwrap().is_empty());
        assert!(super::parse_tag_filters(&["nocolon".into()]).is_err());
        assert!(super::parse_tag_filters(&[":novalue".into()]).is_err());
        assert!(super::parse_tag_filters(&["nokey:".into()]).is_err());
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

    #[tokio::test]
    async fn test_llm_obs_datasets_records() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"records":[{"id":"rec-1"}],"schema_summary":{},"returned":1,"truncated":false,"next_cursor":null}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/dataset/records",
            200,
            body,
        )
        .await;

        let result = super::datasets_records(
            &cfg, "proj-1", "ds-1", None, None, None, None, 10, None, None,
        )
        .await;
        assert!(
            result.is_ok(),
            "datasets_records failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_records_filtered() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"records":[],"returned":0,"truncated":false,"next_cursor":null}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/dataset/records",
            200,
            body,
        )
        .await;

        let result = super::datasets_records(
            &cfg,
            "proj-1",
            "ds-1",
            Some(vec!["rec-1".into(), "rec-2".into()]),
            Some(vec!["env:prod".into()]),
            Some("canon-1".into()),
            Some(3),
            5,
            Some("cursor-abc".into()),
            Some(false),
        )
        .await;
        assert!(
            result.is_ok(),
            "datasets_records filtered failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_records_500() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/dataset/records",
            500,
            r#"{"errors":["internal server error"]}"#,
        )
        .await;

        let result = super::datasets_records(
            &cfg, "proj-1", "ds-1", None, None, None, None, 10, None, None,
        )
        .await;
        assert!(result.is_err(), "should fail on 500");
        assert!(result.unwrap_err().to_string().contains("500"));
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_records_full() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body = r#"{"status":"success","data":{"records":[{"id":"rec-1","input":{"prompt":"hello"},"expected_output":"world"}]}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/dataset/records-full",
            200,
            body,
        )
        .await;

        let result =
            super::datasets_records_full(&cfg, "proj-1", "ds-1", vec!["rec-1".into()]).await;
        assert!(
            result.is_ok(),
            "datasets_records_full failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_records_add_preview() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_ds_records_add_preview.json",
            r#"[{"input":{"question":"hi"},"expected_output":"hello","tags":["env:prod"]}]"#,
        );
        // Without --confirm the endpoint must be told confirmed=false so it only previews.
        let _mock = server
            .mock("POST", "/api/unstable/llm-obs-mcp/v1/dataset/records-add")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "project_id": "proj-1",
                "dataset_id": "ds-1",
                "confirmed": false,
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":"success","data":{"planned_record_count":1}}"#)
            .create_async()
            .await;

        let result =
            super::datasets_records_add(&cfg, "proj-1", "ds-1", tmp.to_str().unwrap(), false, None)
                .await;
        assert!(
            result.is_ok(),
            "datasets_records_add preview failed: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_records_add_confirmed() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json(
            "pup_test_ds_records_add_confirmed.json",
            r#"[{"input":"a"},{"input":"b"}]"#,
        );
        let _mock = server
            .mock("POST", "/api/unstable/llm-obs-mcp/v1/dataset/records-add")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "confirmed": true,
                "create_new_version": false,
                "records": [{"input": "a"}, {"input": "b"}],
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":"success","data":{"inserted":2}}"#)
            .create_async()
            .await;

        let result = super::datasets_records_add(
            &cfg,
            "proj-1",
            "ds-1",
            tmp.to_str().unwrap(),
            true,
            Some(false),
        )
        .await;
        assert!(
            result.is_ok(),
            "datasets_records_add confirmed failed: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_records_add_invalid_file() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // An object, an empty array, and a missing file must all fail locally
        // before any request is made.
        let not_array =
            write_temp_json("pup_test_ds_records_add_not_array.json", r#"{"input":"a"}"#);
        let empty = write_temp_json("pup_test_ds_records_add_empty.json", r#"[]"#);
        for path in [not_array.to_str().unwrap(), empty.to_str().unwrap()] {
            let result =
                super::datasets_records_add(&cfg, "proj-1", "ds-1", path, false, None).await;
            assert!(result.is_err(), "expected error for records file '{path}'");
        }
        let result = super::datasets_records_add(
            &cfg,
            "proj-1",
            "ds-1",
            "/nonexistent/pup_records.json",
            false,
            None,
        )
        .await;
        assert!(result.is_err(), "expected error for missing records file");
        let _ = std::fs::remove_file(not_array);
        let _ = std::fs::remove_file(empty);
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_datasets_records_add_400() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let tmp = write_temp_json("pup_test_ds_records_add_400.json", r#"[{"input":"a"}]"#);
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/dataset/records-add",
            400,
            r#"{"reason":"unknown_dataset"}"#,
        )
        .await;

        let result =
            super::datasets_records_add(&cfg, "proj-1", "ds-1", tmp.to_str().unwrap(), true, None)
                .await;
        assert!(result.is_err(), "should fail on 400");
        let _ = std::fs::remove_file(tmp);
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_events_submit() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let resp_body = r#"{"status":"success","data":{"accepted":1}}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/experiment/ingest-events",
            200,
            resp_body,
        )
        .await;

        let result = super::experiments_events_submit(
            &cfg,
            "exp-1",
            r#"[{"label":"accuracy","metric_type":"score","score_value":0.9}]"#,
            Some(vec!["run:1".to_string()]),
        )
        .await;
        assert!(
            result.is_ok(),
            "experiments_events_submit failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_events_submit_400() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["bad request"]}"#)
            .create_async()
            .await;

        let result = super::experiments_events_submit(
            &cfg,
            "exp-1",
            r#"[{"label":"accuracy","metric_type":"score","score_value":0.9}]"#,
            None,
        )
        .await;
        assert!(result.is_err(), "should fail on 400");
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_llm_obs_experiments_events_submit_invalid_json() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Malformed --metrics should fail locally before any request is made.
        let result = super::experiments_events_submit(&cfg, "exp-1", "not-json", None).await;
        assert!(result.is_err(), "should fail on invalid metrics JSON");
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    // ---- Topic Discovery / Patterns ----

    #[tokio::test]
    async fn test_llm_obs_patterns_configs_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let body =
            r#"{"configs":[{"id":"cfg-1","name":"prod spans","evp_query":"@ml_app:my-app"}]}"#;
        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/topic-discovery/configs/list",
            200,
            body,
        )
        .await;

        let result = super::patterns_configs_list(&cfg).await;
        assert!(
            result.is_ok(),
            "patterns_configs_list failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_patterns_configs_get() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/topic-discovery/config/get",
            200,
            r#"{"config":{"id":"cfg-1","name":"prod spans"}}"#,
        )
        .await;

        let result = super::patterns_configs_get(&cfg).await;
        assert!(
            result.is_ok(),
            "patterns_configs_get failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_patterns_configs_get_404() {
        // The org may have no config yet; the endpoint 404s rather than returning empty.
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/topic-discovery/config/get",
            404,
            r#"{"reason":"not_found"}"#,
        )
        .await;

        let result = super::patterns_configs_get(&cfg).await;
        assert!(result.is_err(), "should fail on 404");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_patterns_runs_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Strict body check: config_id must reach the endpoint.
        let _mock = server
            .mock(
                "POST",
                "/api/unstable/llm-obs-mcp/v1/topic-discovery/runs/list",
            )
            .match_body(mockito::Matcher::Json(
                serde_json::json!({"config_id": "cfg-1"}),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"runs":[{"id":"run-1","status":"completed"}]}"#)
            .create_async()
            .await;

        let result = super::patterns_runs_list(&cfg, "cfg-1").await;
        assert!(
            result.is_ok(),
            "patterns_runs_list failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_patterns_runs_status() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock(
                "POST",
                "/api/unstable/llm-obs-mcp/v1/topic-discovery/run-status",
            )
            .match_body(mockito::Matcher::Json(
                serde_json::json!({"config_id": "cfg-1"}),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"run-1","status":"running","step":"clustering","progress":[]}"#)
            .create_async()
            .await;

        let result = super::patterns_runs_status(&cfg, "cfg-1").await;
        assert!(
            result.is_ok(),
            "patterns_runs_status failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_patterns_runs_status_500() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/topic-discovery/run-status",
            500,
            r#"{"errors":["internal server error"]}"#,
        )
        .await;

        let result = super::patterns_runs_status(&cfg, "cfg-1").await;
        assert!(result.is_err(), "should fail on 500");
        assert!(result.unwrap_err().to_string().contains("500"));
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_patterns_topics_latest_run() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Omitting run_id must leave the key out entirely so the server picks the latest run.
        let _mock = server
            .mock(
                "POST",
                "/api/unstable/llm-obs-mcp/v1/topic-discovery/topics",
            )
            .match_body(mockito::Matcher::Json(
                serde_json::json!({"config_id": "cfg-1"}),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"topics":[{"id":"topic-1","name":"billing questions","point_count":42}]}"#,
            )
            .create_async()
            .await;

        let result = super::patterns_topics(&cfg, "cfg-1", None).await;
        assert!(result.is_ok(), "patterns_topics failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_patterns_topics_specific_run() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock(
                "POST",
                "/api/unstable/llm-obs-mcp/v1/topic-discovery/topics",
            )
            .match_body(mockito::Matcher::Json(
                serde_json::json!({"config_id": "cfg-1", "run_id": "run-7"}),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"topics":[]}"#)
            .create_async()
            .await;

        let result = super::patterns_topics(&cfg, "cfg-1", Some("run-7".into())).await;
        assert!(
            result.is_ok(),
            "patterns_topics with run_id failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_patterns_topics_with_points() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // include_metrics is only sent when the flag is passed.
        let _mock = server
            .mock(
                "POST",
                "/api/unstable/llm-obs-mcp/v1/topic-discovery/topics-with-points",
            )
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "config_id": "cfg-1",
                "run_id": "run-7",
                "include_metrics": true,
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"topics":[{"id":"topic-1","points":[{"span_id":"s-1"}]}]}"#)
            .create_async()
            .await;

        let result =
            super::patterns_topics_with_points(&cfg, "cfg-1", Some("run-7".into()), true).await;
        assert!(
            result.is_ok(),
            "patterns_topics_with_points failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_patterns_topics_with_points_omits_metrics() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Without the flag, include_metrics must be absent (not false) so the
        // server default applies — matching the MCP tool's behavior.
        let _mock = server
            .mock(
                "POST",
                "/api/unstable/llm-obs-mcp/v1/topic-discovery/topics-with-points",
            )
            .match_body(mockito::Matcher::Json(
                serde_json::json!({"config_id": "cfg-1"}),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"topics":[]}"#)
            .create_async()
            .await;

        let result = super::patterns_topics_with_points(&cfg, "cfg-1", None, false).await;
        assert!(
            result.is_ok(),
            "patterns_topics_with_points without metrics failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_patterns_points_paginated() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock(
                "POST",
                "/api/unstable/llm-obs-mcp/v1/topic-discovery/clustered-points",
            )
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "topic_id": "topic-1",
                "page_size": 50,
                "page_token": "tok-abc",
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"points":[{"span_id":"s-1","session_id":"sess-1"}],"next_page_token":"tok-def"}"#)
            .create_async()
            .await;

        let result =
            super::patterns_points(&cfg, "topic-1", Some(50), Some("tok-abc".into())).await;
        assert!(result.is_ok(), "patterns_points failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_patterns_points_defaults() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Omitted paging args must not appear in the body.
        let _mock = server
            .mock(
                "POST",
                "/api/unstable/llm-obs-mcp/v1/topic-discovery/clustered-points",
            )
            .match_body(mockito::Matcher::Json(
                serde_json::json!({"topic_id": "topic-1"}),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"points":[]}"#)
            .create_async()
            .await;

        let result = super::patterns_points(&cfg, "topic-1", None, None).await;
        assert!(
            result.is_ok(),
            "patterns_points defaults failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_patterns_points_404() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/topic-discovery/clustered-points",
            404,
            r#"{"reason":"unknown_topic"}"#,
        )
        .await;

        let result = super::patterns_points(&cfg, "missing", None, None).await;
        assert!(result.is_err(), "should fail on 404");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_patterns_no_auth() {
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
        let result = super::patterns_configs_list(&cfg).await;
        assert!(result.is_err(), "should fail without auth");
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    // ---- Agent Insights ----

    #[tokio::test]
    async fn test_llm_obs_agent_insights_list_with_filters() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock("POST", "/api/unstable/llm-obs-mcp/v1/agent-insights/list")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "ml_app": "my-agent",
                "status": "in_progress",
                "insight_type": "tool_call_retry_loop",
                "limit": 5,
                "cursor": "tok-abc",
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"insights":[{"id":"ins-1","insight_type":"tool_call_retry_loop"}]}"#)
            .create_async()
            .await;

        let result = super::agent_insights_list(
            &cfg,
            Some("my-agent".into()),
            Some("in_progress".into()),
            Some("tool_call_retry_loop".into()),
            Some(5),
            Some("tok-abc".into()),
        )
        .await;
        assert!(
            result.is_ok(),
            "agent_insights_list failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_agent_insights_list_defaults() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Omitted filters must be absent from the body so the server's own
        // defaults (status=for_review, limit=25) apply.
        let _mock = server
            .mock("POST", "/api/unstable/llm-obs-mcp/v1/agent-insights/list")
            .match_body(mockito::Matcher::Json(serde_json::json!({})))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"insights":[]}"#)
            .create_async()
            .await;

        let result = super::agent_insights_list(&cfg, None, None, None, None, None).await;
        assert!(
            result.is_ok(),
            "agent_insights_list defaults failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_agent_insights_list_invalid_status() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        // No mock: validation must reject before any request is made.
        let result =
            super::agent_insights_list(&cfg, None, Some("archived".into()), None, None, None).await;
        let err = result
            .expect_err("should reject an unknown status")
            .to_string();
        assert!(err.contains("--status must be one of"), "unexpected: {err}");
        assert!(
            err.contains("for_review"),
            "should list valid values: {err}"
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_agent_insights_get() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock("POST", "/api/unstable/llm-obs-mcp/v1/agent-insights/get")
            .match_body(mockito::Matcher::Json(
                serde_json::json!({"insight_id": "ins-1"}),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"insight":{"id":"ins-1"},"feedback_targets":["insight"]}"#)
            .create_async()
            .await;

        let result = super::agent_insights_get(&cfg, "ins-1").await;
        assert!(
            result.is_ok(),
            "agent_insights_get failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_agent_insights_get_404() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/agent-insights/get",
            404,
            r#"{"errors":["insight not found"]}"#,
        )
        .await;

        let result = super::agent_insights_get(&cfg, "missing").await;
        assert!(result.is_err(), "should fail on 404");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_agent_insights_update_status() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock("POST", "/api/unstable/llm-obs-mcp/v1/agent-insights/status")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "insight_id": "ins-1",
                "status": "completed",
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"insight":{"id":"ins-1","status":"completed"}}"#)
            .create_async()
            .await;

        let result = super::agent_insights_update_status(&cfg, "ins-1", "completed").await;
        assert!(
            result.is_ok(),
            "agent_insights_update_status failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_agent_insights_update_status_invalid() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let result = super::agent_insights_update_status(&cfg, "ins-1", "done").await;
        assert!(result.is_err(), "should reject an unknown status");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_agent_insights_update_status_403() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/agent-insights/status",
            403,
            r#"{"errors":["forbidden"]}"#,
        )
        .await;

        let result = super::agent_insights_update_status(&cfg, "ins-1", "ignored").await;
        assert!(result.is_err(), "should fail on 403");
        cleanup_env();
    }

    #[test]
    fn test_parse_feedback_items_with_and_without_reasoning() {
        let items = super::parse_feedback_items(&[
            "insight=useful=helped narrow the retry loop".to_string(),
            "fix_with_agent=not_useful".to_string(),
        ])
        .expect("valid entries should parse");
        assert_eq!(
            items,
            vec![
                serde_json::json!({
                    "target_key": "insight",
                    "usefulness": "useful",
                    "reasoning": "helped narrow the retry loop",
                }),
                serde_json::json!({
                    "target_key": "fix_with_agent",
                    "usefulness": "not_useful",
                }),
            ]
        );
    }

    #[test]
    fn test_parse_feedback_items_keeps_colons_in_target_key() {
        // Real target keys embed a colon, e.g. the suggested_evaluator:<eval_name> keys
        // returned in feedback_targets. They must reach the server intact, and `=` in
        // free-text reasoning must not be mistaken for a separator.
        let items = super::parse_feedback_items(&[
            "suggested_evaluator:change-ranker-temporal-gate=somewhat_useful=ok, but a=b was off"
                .to_string(),
        ])
        .expect("colon-bearing target key should parse");
        assert_eq!(
            items,
            vec![serde_json::json!({
                "target_key": "suggested_evaluator:change-ranker-temporal-gate",
                "usefulness": "somewhat_useful",
                "reasoning": "ok, but a=b was off",
            })]
        );
    }

    #[test]
    fn test_parse_feedback_items_rejects_bad_input() {
        assert!(
            super::parse_feedback_items(&[]).is_err(),
            "empty feedback should be rejected"
        );
        assert!(
            super::parse_feedback_items(&["insight".to_string()]).is_err(),
            "missing usefulness should be rejected"
        );
        assert!(
            super::parse_feedback_items(&["=useful".to_string()]).is_err(),
            "empty target key should be rejected"
        );
        assert!(
            super::parse_feedback_items(&["insight=very_useful".to_string()]).is_err(),
            "unknown usefulness should be rejected"
        );
        assert!(
            super::parse_feedback_items(&["insight:useful".to_string()]).is_err(),
            "colon instead of = should be rejected, not silently mis-parsed"
        );
        let too_many: Vec<String> = (0..26).map(|i| format!("target-{i}=useful")).collect();
        assert!(
            super::parse_feedback_items(&too_many).is_err(),
            "more than 25 entries should be rejected"
        );
        assert!(
            super::parse_feedback_items(&too_many[..25]).is_ok(),
            "exactly 25 entries should be accepted"
        );
    }

    #[tokio::test]
    async fn test_llm_obs_agent_insights_submit_feedback() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock(
                "POST",
                "/api/unstable/llm-obs-mcp/v1/agent-insights/feedback",
            )
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "insight_id": "ins-1",
                "feedback_items": [
                    {"target_key": "insight", "usefulness": "useful", "reasoning": "spot on"},
                    {"target_key": "fix_with_agent", "usefulness": "somewhat_useful"},
                ],
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"accepted":2}"#)
            .create_async()
            .await;

        let result = super::agent_insights_submit_feedback(
            &cfg,
            "ins-1",
            vec![
                "insight=useful=spot on".to_string(),
                "fix_with_agent=somewhat_useful".to_string(),
            ],
        )
        .await;
        assert!(
            result.is_ok(),
            "agent_insights_submit_feedback failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_agent_insights_submit_feedback_400() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/agent-insights/feedback",
            400,
            r#"{"errors":["unknown target_key"]}"#,
        )
        .await;

        let result = super::agent_insights_submit_feedback(
            &cfg,
            "ins-1",
            vec!["not_a_target=useful".to_string()],
        )
        .await;
        assert!(result.is_err(), "should fail on 400");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_agent_insights_no_auth() {
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
        let result = super::agent_insights_list(&cfg, None, None, None, None, None).await;
        assert!(result.is_err(), "should fail without auth");
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    // ---- Model pricing ----

    #[tokio::test]
    async fn test_llm_obs_model_pricing_provider_and_model() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = server
            .mock("POST", "/api/unstable/llm-obs-mcp/v1/pricing/model")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "provider": "anthropic",
                "model": "claude-sonnet-4-20250514",
                "limit": 10,
                "cursor": "tok-abc",
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"status":"ok","unit":"usd_per_million_tokens","models":[{"provider":"anthropic","model":"claude-sonnet-4-20250514","match_type":"exact","tiers":[]}]}"#,
            )
            .create_async()
            .await;

        let result = super::model_pricing(
            &cfg,
            Some("anthropic".into()),
            Some("claude-sonnet-4-20250514".into()),
            Some(10),
            Some("tok-abc".into()),
        )
        .await;
        assert!(result.is_ok(), "model_pricing failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_model_pricing_model_only_searches_all_providers() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Provider omitted is meaningful, not a default: the endpoint then searches
        // the whole catalog, so it must not appear in the body.
        let _mock = server
            .mock("POST", "/api/unstable/llm-obs-mcp/v1/pricing/model")
            .match_body(mockito::Matcher::Json(
                serde_json::json!({"model": "claude"}),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":"ok","unit":"usd_per_million_tokens","models":[]}"#)
            .create_async()
            .await;

        let result = super::model_pricing(&cfg, None, Some("claude".into()), None, None).await;
        assert!(
            result.is_ok(),
            "model_pricing with model only failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_model_pricing_requires_provider_or_model() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        // No mock: validation must reject before any request is made.
        let result = super::model_pricing(&cfg, None, None, Some(10), None).await;
        let err = result
            .expect_err("should reject with neither provider nor model")
            .to_string();
        assert!(
            err.contains("--provider") && err.contains("--model"),
            "unexpected: {err}"
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_llm_obs_model_pricing_500() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let _mock = mock_post(
            &mut server,
            "/api/unstable/llm-obs-mcp/v1/pricing/model",
            500,
            r#"{"errors":["internal error"]}"#,
        )
        .await;

        let result = super::model_pricing(&cfg, Some("openai".into()), None, None, None).await;
        assert!(result.is_err(), "should fail on 500");
        cleanup_env();
    }
}

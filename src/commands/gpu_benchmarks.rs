//! GPU Monitoring inference benchmarks: load tests of an OpenAI-compatible
//! inference endpoint, run with AIPerf from Datadog's cluster or the customer's.
//!
//! The `/api/v2/gpu/benchmarks` routes are JSON:API: requests are sent as
//! `application/vnd.api+json` documents and responses are printed as returned.

use std::time::Duration;

use anyhow::{bail, Result};
use serde_json::{json, Value};

use crate::config::Config;
use crate::formatter;
use crate::raw_client;
use crate::util;
use crate::util_ext;

const BASE: &str = "/api/v2/gpu/benchmarks";
const JSONAPI: &str = "application/vnd.api+json";
const IDEMPOTENCY_KEY_HEADER: &str = "Idempotency-Key";

/// A run in any of these states will not change again.
const RUN_FINAL_STATUSES: &[&str] = &["completed", "cancelled", "invalid", "timeout", "failed"];

pub const LIST_PAGING: crate::pagination::PageSpec = crate::pagination::PageSpec {
    size: crate::pagination::PageSizeSpec {
        default: Some(50),
        max: Some(200),
    },
    base: crate::pagination::IndexBase::Zero,
};

pub const RUNS_PAGING: crate::pagination::PageSpec = crate::pagination::PageSpec {
    size: crate::pagination::PageSizeSpec {
        default: Some(20),
        max: Some(200),
    },
    base: crate::pagination::IndexBase::Zero,
};

/// Filters of `benchmarks list`, matched against the linked model deployment.
#[derive(Default)]
pub struct ListFilter {
    pub name: Option<String>,
    pub deployment_name: Option<String>,
    pub deployment_env: Option<String>,
    pub deployment_namespace: Option<String>,
}

/// Body of `sweeps start`.
pub struct SweepInput {
    pub kind: String,
    pub min_concurrency: Option<u32>,
    pub max_concurrency: Option<u32>,
    pub concurrency_levels: Vec<u32>,
    pub goodput_threshold: Option<f64>,
}

/// Sends a JSON:API request and returns the parsed response, or `Null` for an
/// empty body.
async fn request(
    cfg: &Config,
    method: &str,
    path: &str,
    query: &[(&str, &str)],
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> Result<Value> {
    let body = body.map(|b| serde_json::to_vec(&b)).transpose()?;
    let resp = raw_client::raw_request(
        cfg,
        method,
        path,
        query,
        body,
        Some(JSONAPI),
        JSONAPI,
        headers,
    )
    .await?;
    if resp.bytes.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&resp.bytes)
        .map_err(|e| anyhow::anyhow!("failed to parse {method} {path} response: {e}"))
}

/// Wraps attributes in a JSON:API document. A file that is already a document
/// (has a top-level `data`) is sent unchanged.
fn document(resource_type: &str, id: Option<&str>, attributes: Value) -> Value {
    if attributes.get("data").is_some() {
        return attributes;
    }
    let mut data = json!({ "type": resource_type, "attributes": attributes });
    if let Some(id) = id {
        data["id"] = json!(id);
    }
    json!({ "data": data })
}

fn idempotency_headers(key: &Option<String>) -> Vec<(&str, &str)> {
    key.as_deref()
        .map(|k| vec![(IDEMPOTENCY_KEY_HEADER, k)])
        .unwrap_or_default()
}

fn list_meta(data: &Value, page_size: u64, offset: u64) -> formatter::Metadata {
    let count = data["data"].as_array().map(|a| a.len());
    let total = data["meta"]["count"].as_u64();
    let next = offset + page_size;
    let truncated = total.is_some_and(|t| t > next);
    formatter::Metadata {
        count,
        truncated,
        next_offset: truncated.then_some(next as usize),
        ..Default::default()
    }
}

fn enc(id: &str) -> String {
    util_ext::percent_encode(id)
}

// ---- Benchmarks ----

pub async fn list(cfg: &Config, filter: ListFilter, page_size: u64, offset: u64) -> Result<()> {
    let limit = page_size.to_string();
    let off = offset.to_string();
    let mut query: Vec<(&str, &str)> = vec![("page[limit]", &limit), ("page[offset]", &off)];
    for (key, value) in [
        ("filter[name]", &filter.name),
        ("filter[deployment.name]", &filter.deployment_name),
        ("filter[deployment.env]", &filter.deployment_env),
        ("filter[deployment.namespace]", &filter.deployment_namespace),
    ] {
        if let Some(v) = value {
            query.push((key, v));
        }
    }
    let data = request(cfg, "GET", BASE, &query, None, &[]).await?;
    formatter::output_with_meta(cfg, &data, &list_meta(&data, page_size, offset))
}

pub async fn get(cfg: &Config, benchmark_id: &str) -> Result<()> {
    let path = format!("{BASE}/{}", enc(benchmark_id));
    let data = request(cfg, "GET", &path, &[], None, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let attributes: Value = util::read_json_file(file)?;
    let body = document("gpu_benchmarks", None, attributes);
    let data = request(cfg, "POST", BASE, &[], Some(body), &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn update(cfg: &Config, benchmark_id: &str, file: &str) -> Result<()> {
    let attributes: Value = util::read_json_file(file)?;
    let body = document("gpu_benchmarks", Some(benchmark_id), attributes);
    let path = format!("{BASE}/{}", enc(benchmark_id));
    let data = request(cfg, "PATCH", &path, &[], Some(body), &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn delete(cfg: &Config, benchmark_id: &str) -> Result<()> {
    if !util_ext::confirm_destructive(
        cfg,
        &format!("Delete GPU benchmark {benchmark_id} and its runs?"),
    )? {
        return Ok(());
    }
    let path = format!("{BASE}/{}", enc(benchmark_id));
    request(cfg, "DELETE", &path, &[], None, &[]).await?;
    eprintln!("Deleted GPU benchmark {benchmark_id}");
    Ok(())
}

/// Sends one chat completion to an endpoint, without saving anything.
pub async fn test_connection(cfg: &Config, file: &str) -> Result<()> {
    let endpoint: Value = util::read_json_file(file)?;
    let body = document(
        "gpu_benchmark_connection_tests",
        None,
        json!({ "endpoint": endpoint }),
    );
    let path = format!("{BASE}/test-connection");
    let data = request(cfg, "POST", &path, &[], Some(body), &[]).await?;
    formatter::output(cfg, &data)
}

// ---- Runs ----

pub async fn runs_list(
    cfg: &Config,
    benchmark_id: &str,
    sweep_id: Option<String>,
    page_size: u64,
    offset: u64,
) -> Result<()> {
    let limit = page_size.to_string();
    let off = offset.to_string();
    let mut query: Vec<(&str, &str)> = vec![("page[limit]", &limit), ("page[offset]", &off)];
    if let Some(s) = &sweep_id {
        query.push(("filter[sweep_id]", s));
    }
    let path = format!("{BASE}/{}/runs", enc(benchmark_id));
    let data = request(cfg, "GET", &path, &query, None, &[]).await?;
    formatter::output_with_meta(cfg, &data, &list_meta(&data, page_size, offset))
}

async fn fetch_run(cfg: &Config, run_id: &str) -> Result<Value> {
    let path = format!("{BASE}/runs/{}", enc(run_id));
    request(cfg, "GET", &path, &[], None, &[]).await
}

pub async fn runs_get(cfg: &Config, run_id: &str) -> Result<()> {
    let data = fetch_run(cfg, run_id).await?;
    formatter::output(cfg, &data)
}

pub async fn runs_start(
    cfg: &Config,
    benchmark_id: &str,
    idempotency_key: Option<String>,
) -> Result<()> {
    let path = format!("{BASE}/{}/runs", enc(benchmark_id));
    let headers = idempotency_headers(&idempotency_key);
    let data = request(cfg, "POST", &path, &[], None, &headers).await?;
    formatter::output(cfg, &data)
}

pub async fn runs_cancel(cfg: &Config, run_id: &str) -> Result<()> {
    let path = format!("{BASE}/runs/{}/cancel", enc(run_id));
    let data = request(cfg, "POST", &path, &[], None, &[]).await?;
    formatter::output(cfg, &data)
}

fn run_status(run: &Value) -> Option<&str> {
    run["data"]["attributes"]["status"].as_str()
}

/// Polls a run until it reaches a final status, then prints it. The API has no
/// long-poll, so this polls `GET /runs/{id}` every `interval`.
pub async fn runs_wait(cfg: &Config, run_id: &str, timeout: &str, interval: &str) -> Result<()> {
    let timeout = Duration::from_millis(util_ext::parse_duration_to_millis(timeout)? as u64);
    let interval = Duration::from_millis(util_ext::parse_duration_to_millis(interval)? as u64);
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let run = fetch_run(cfg, run_id).await?;
        let status = run_status(&run).unwrap_or_default();
        if RUN_FINAL_STATUSES.contains(&status) {
            return formatter::output(cfg, &run);
        }
        if tokio::time::Instant::now() + interval > deadline {
            bail!("run {run_id} still {status} after the --timeout; wait again or check it with `pup gpu benchmarks runs get {run_id}`");
        }
        tokio::time::sleep(interval).await;
    }
}

// ---- Sweeps ----

pub async fn sweeps_list(cfg: &Config, benchmark_id: &str) -> Result<()> {
    let path = format!("{BASE}/{}/sweeps", enc(benchmark_id));
    let data = request(cfg, "GET", &path, &[], None, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn sweeps_get(cfg: &Config, sweep_id: &str) -> Result<()> {
    let path = format!("{BASE}/sweeps/{}", enc(sweep_id));
    let data = request(cfg, "GET", &path, &[], None, &[]).await?;
    formatter::output(cfg, &data)
}

fn sweep_attributes(input: SweepInput) -> Result<Value> {
    if input.kind != "search" && input.kind != "curve" {
        bail!("--kind must be search or curve, got {:?}", input.kind);
    }
    let mut attributes = json!({ "kind": input.kind });
    if let Some(v) = input.min_concurrency {
        attributes["min_concurrency"] = json!(v);
    }
    if let Some(v) = input.max_concurrency {
        attributes["max_concurrency"] = json!(v);
    }
    if !input.concurrency_levels.is_empty() {
        attributes["concurrency_levels"] = json!(input.concurrency_levels);
    }
    if let Some(v) = input.goodput_threshold {
        attributes["goodput_threshold"] = json!(v);
    }
    Ok(attributes)
}

pub async fn sweeps_start(
    cfg: &Config,
    benchmark_id: &str,
    input: SweepInput,
    idempotency_key: Option<String>,
) -> Result<()> {
    let body = document("gpu_benchmark_sweeps", None, sweep_attributes(input)?);
    let path = format!("{BASE}/{}/sweeps", enc(benchmark_id));
    let headers = idempotency_headers(&idempotency_key);
    let data = request(cfg, "POST", &path, &[], Some(body), &headers).await?;
    formatter::output(cfg, &data)
}

pub async fn sweeps_cancel(cfg: &Config, sweep_id: &str) -> Result<()> {
    let path = format!("{BASE}/sweeps/{}/cancel", enc(sweep_id));
    let data = request(cfg, "POST", &path, &[], None, &[]).await?;
    formatter::output(cfg, &data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use mockito::Matcher;

    const ID: &str = "8b0e5c1a-4d2f-4b6e-9a3c-1f7d2e6b9c40";

    fn run_body(status: &str) -> String {
        json!({ "data": { "id": ID, "type": "gpu_benchmark_runs", "attributes": { "status": status } } })
            .to_string()
    }

    fn write_json(name: &str, value: Value) -> String {
        let path = std::env::temp_dir().join(format!(
            "pup-gpu-benchmarks-{name}-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, value.to_string()).unwrap();
        path.to_str().unwrap().to_string()
    }

    fn sweep(kind: &str) -> SweepInput {
        SweepInput {
            kind: kind.into(),
            min_concurrency: None,
            max_concurrency: Some(64),
            concurrency_levels: vec![],
            goodput_threshold: Some(0.9),
        }
    }

    #[test]
    fn test_document_wraps_attributes() {
        let doc = document("gpu_benchmarks", Some(ID), json!({ "name": "b" }));
        assert_eq!(
            doc,
            json!({ "data": { "type": "gpu_benchmarks", "id": ID, "attributes": { "name": "b" } } })
        );
    }

    #[test]
    fn test_document_keeps_a_document() {
        let given = json!({ "data": { "type": "gpu_benchmarks", "attributes": {} } });
        assert_eq!(document("gpu_benchmarks", None, given.clone()), given);
    }

    #[test]
    fn test_list_meta_reports_next_offset() {
        let data = json!({ "data": [{}, {}], "meta": { "count": 5 } });
        let meta = list_meta(&data, 2, 0);
        assert_eq!(meta.count, Some(2));
        assert!(meta.truncated);
        assert_eq!(meta.next_offset, Some(2));

        let last = list_meta(&data, 2, 4);
        assert!(!last.truncated);
        assert_eq!(last.next_offset, None);
    }

    #[test]
    fn test_sweep_attributes() {
        let attributes = sweep_attributes(sweep("search")).unwrap();
        assert_eq!(
            attributes,
            json!({ "kind": "search", "max_concurrency": 64, "goodput_threshold": 0.9 })
        );
    }

    #[test]
    fn test_sweep_attributes_rejects_unknown_kind() {
        let err = sweep_attributes(sweep("grid")).unwrap_err();
        assert!(err.to_string().contains("search or curve"));
    }

    #[tokio::test]
    async fn test_list_sends_paging_and_filters() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", BASE)
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("page[limit]".into(), "50".into()),
                Matcher::UrlEncoded("page[offset]".into(), "0".into()),
                Matcher::UrlEncoded("filter[deployment.name]".into(), "llama".into()),
            ]))
            .match_header("accept", JSONAPI)
            .with_status(200)
            .with_body(r#"{"data":[],"meta":{"count":0}}"#)
            .create_async()
            .await;
        let filter = ListFilter {
            deployment_name: Some("llama".into()),
            ..Default::default()
        };
        let result = list(&cfg, filter, 50, 0).await;
        assert!(result.is_ok(), "list failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", format!("{BASE}/{ID}").as_str())
            .with_status(404)
            .with_body(r#"{"errors":[{"title":"Not Found"}]}"#)
            .create_async()
            .await;
        assert!(get(&cfg, ID).await.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_create_sends_a_jsonapi_document() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let file = write_json(
            "create",
            json!({ "name": "llama", "endpoint": { "url": "https://llm.example.com/v1", "model": "llama" } }),
        );
        let mock = server
            .mock("POST", BASE)
            .match_header("content-type", JSONAPI)
            .match_body(Matcher::PartialJson(
                json!({ "data": { "type": "gpu_benchmarks", "attributes": { "name": "llama" } } }),
            ))
            .with_status(201)
            .with_body(r#"{"data":{"id":"b1","type":"gpu_benchmarks","attributes":{}}}"#)
            .create_async()
            .await;
        let result = create(&cfg, &file).await;
        assert!(result.is_ok(), "create failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_create_missing_file() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        assert!(create(&cfg, "/nonexistent/benchmark.json").await.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_update_sends_the_id() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let file = write_json("update", json!({ "name": "renamed" }));
        let mock = server
            .mock("PATCH", format!("{BASE}/{ID}").as_str())
            .match_body(Matcher::PartialJson(
                json!({ "data": { "type": "gpu_benchmarks", "id": ID } }),
            ))
            .with_status(200)
            .with_body(r#"{"data":{"id":"b1","type":"gpu_benchmarks","attributes":{}}}"#)
            .create_async()
            .await;
        let result = update(&cfg, ID, &file).await;
        assert!(result.is_ok(), "update failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_delete_with_auto_approve() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        cfg.auto_approve = true;
        let mock = server
            .mock("DELETE", format!("{BASE}/{ID}").as_str())
            .with_status(204)
            .create_async()
            .await;
        let result = delete(&cfg, ID).await;
        assert!(result.is_ok(), "delete failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_delete_active_run_conflict() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        cfg.auto_approve = true;
        let _mock = server
            .mock("DELETE", format!("{BASE}/{ID}").as_str())
            .with_status(409)
            .with_body(r#"{"errors":[{"title":"a run is active"}]}"#)
            .create_async()
            .await;
        assert!(delete(&cfg, ID).await.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_test_connection_wraps_the_endpoint() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let file = write_json(
            "test-connection",
            json!({ "url": "https://llm.example.com/v1", "model": "llama" }),
        );
        let mock = server
            .mock("POST", format!("{BASE}/test-connection").as_str())
            .match_body(Matcher::PartialJson(json!({ "data": {
                "type": "gpu_benchmark_connection_tests",
                "attributes": { "endpoint": { "model": "llama" } },
            } })))
            .with_status(200)
            .with_body(
                r#"{"data":{"type":"gpu_benchmark_connection_tests","attributes":{"ok":true}}}"#,
            )
            .create_async()
            .await;
        let result = test_connection(&cfg, &file).await;
        assert!(result.is_ok(), "test-connection failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_runs_list_filters_by_sweep() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", format!("{BASE}/{ID}/runs").as_str())
            .match_query(Matcher::UrlEncoded("filter[sweep_id]".into(), "s1".into()))
            .with_status(200)
            .with_body(r#"{"data":[],"meta":{"count":0}}"#)
            .create_async()
            .await;
        let result = runs_list(&cfg, ID, Some("s1".into()), 20, 0).await;
        assert!(result.is_ok(), "runs list failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_runs_start_sends_the_idempotency_key() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("POST", format!("{BASE}/{ID}/runs").as_str())
            .match_header("idempotency-key", "retry-1")
            .with_status(202)
            .with_body(run_body("queued"))
            .create_async()
            .await;
        let result = runs_start(&cfg, ID, Some("retry-1".into())).await;
        assert!(result.is_ok(), "runs start failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_runs_start_active_run_conflict() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("POST", format!("{BASE}/{ID}/runs").as_str())
            .with_status(409)
            .with_body(r#"{"errors":[{"title":"the benchmark already has an active run"}]}"#)
            .create_async()
            .await;
        assert!(runs_start(&cfg, ID, None).await.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_runs_cancel() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("POST", format!("{BASE}/runs/{ID}/cancel").as_str())
            .with_status(202)
            .with_body(run_body("running"))
            .create_async()
            .await;
        let result = runs_cancel(&cfg, ID).await;
        assert!(result.is_ok(), "runs cancel failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_runs_wait_returns_a_final_run() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", format!("{BASE}/runs/{ID}").as_str())
            .with_status(200)
            .with_body(run_body("completed"))
            .expect(1)
            .create_async()
            .await;
        let result = runs_wait(&cfg, ID, "1m", "1s").await;
        assert!(result.is_ok(), "runs wait failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_runs_wait_times_out() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", format!("{BASE}/runs/{ID}").as_str())
            .with_status(200)
            .with_body(run_body("running"))
            .create_async()
            .await;
        // A timeout shorter than the interval gives up after the first poll.
        let err = runs_wait(&cfg, ID, "1s", "1m").await.unwrap_err();
        assert!(err.to_string().contains("still running"), "{err}");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_runs_wait_rejects_a_bad_duration() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        assert!(runs_wait(&cfg, ID, "soon", "1s").await.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_sweeps_start() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("POST", format!("{BASE}/{ID}/sweeps").as_str())
            .match_body(Matcher::PartialJson(json!({ "data": {
                "type": "gpu_benchmark_sweeps",
                "attributes": { "kind": "search", "max_concurrency": 64 },
            } })))
            .with_status(202)
            .with_body(r#"{"data":{"id":"s1","type":"gpu_benchmark_sweeps","attributes":{}}}"#)
            .create_async()
            .await;
        let result = sweeps_start(&cfg, ID, sweep("search"), None).await;
        assert!(result.is_ok(), "sweeps start failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_sweeps_get_and_cancel() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let body = r#"{"data":{"id":"s1","type":"gpu_benchmark_sweeps","attributes":{}}}"#;
        let get_mock = server
            .mock("GET", "/api/v2/gpu/benchmarks/sweeps/s1")
            .with_status(200)
            .with_body(body)
            .create_async()
            .await;
        let cancel_mock = server
            .mock("POST", "/api/v2/gpu/benchmarks/sweeps/s1/cancel")
            .with_status(202)
            .with_body(body)
            .create_async()
            .await;
        assert!(sweeps_get(&cfg, "s1").await.is_ok());
        assert!(sweeps_cancel(&cfg, "s1").await.is_ok());
        get_mock.assert_async().await;
        cancel_mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_sweeps_list_forbidden() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", format!("{BASE}/{ID}/sweeps").as_str())
            .with_status(403)
            .with_body(r#"{"errors":[{"title":"Forbidden"}]}"#)
            .create_async()
            .await;
        assert!(sweeps_list(&cfg, ID).await.is_err());
        cleanup_env();
    }
}

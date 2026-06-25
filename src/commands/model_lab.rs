use anyhow::Result;
use datadog_api_client::datadogV2::api_model_lab_api::{
    ListModelLabProjectsOptionalParams, ListModelLabRunArtifactsOptionalParams,
    ListModelLabRunsOptionalParams, ModelLabAPIAPI,
};
use datadog_api_client::datadogV2::model::{
    ModelLabFacetType, ModelLabProjectFacetType, ModelLabRunStatus,
};

use crate::config::Config;
use crate::formatter;

fn make_api(cfg: &Config) -> ModelLabAPIAPI {
    crate::make_api!(ModelLabAPIAPI, cfg)
}

// --- Projects ---

pub async fn projects_list(
    cfg: &Config,
    filter: Option<String>,
    filter_tags: Option<String>,
    sort: Option<String>,
    page_size: Option<i64>,
    page_number: Option<i64>,
) -> Result<()> {
    let api = make_api(cfg);
    let mut params = ListModelLabProjectsOptionalParams::default();
    if let Some(f) = filter {
        params = params.filter(f);
    }
    if let Some(t) = filter_tags {
        params = params.filter_tags(t);
    }
    if let Some(s) = sort {
        params = params.sort(s);
    }
    if let Some(n) = page_size {
        params = params.page_size(n);
    }
    if let Some(n) = page_number {
        params = params.page_number(n);
    }
    let resp = api
        .list_model_lab_projects(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list model lab projects: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn projects_get(cfg: &Config, project_id: i64) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_model_lab_project(project_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get model lab project: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn projects_star(cfg: &Config, project_id: i64) -> Result<()> {
    let api = make_api(cfg);
    api.star_model_lab_project(project_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to star model lab project: {e:?}"))?;
    println!("Project {project_id} starred.");
    Ok(())
}

pub async fn projects_unstar(cfg: &Config, project_id: i64) -> Result<()> {
    let api = make_api(cfg);
    api.unstar_model_lab_project(project_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to unstar model lab project: {e:?}"))?;
    println!("Project {project_id} unstarred.");
    Ok(())
}

pub async fn projects_artifacts(cfg: &Config, project_id: i64) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .list_model_lab_project_artifacts(project_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list model lab project artifacts: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn projects_facet_keys(cfg: &Config) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .list_model_lab_project_facet_keys()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list model lab project facet keys: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn projects_facet_values(
    cfg: &Config,
    facet_type: &str,
    facet_name: String,
) -> Result<()> {
    let api = make_api(cfg);
    let ft = match facet_type {
        "tag" => ModelLabProjectFacetType::TAG,
        other => anyhow::bail!("unknown facet type '{other}'; valid values: tag"),
    };
    let resp = api
        .list_model_lab_project_facet_values(ft, facet_name)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list model lab project facet values: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// --- Runs ---

#[allow(clippy::too_many_arguments)]
pub async fn runs_list(
    cfg: &Config,
    filter: Option<String>,
    filter_project_id: Option<i64>,
    filter_status: Option<String>,
    filter_tags: Option<String>,
    filter_params: Option<String>,
    filter_parent_run_id: Option<String>,
    pinned_first: bool,
    include_pinned: bool,
    sort: Option<String>,
    page_size: Option<i64>,
    page_number: Option<i64>,
) -> Result<()> {
    let api = make_api(cfg);
    let mut params = ListModelLabRunsOptionalParams::default();
    if let Some(f) = filter {
        params = params.filter(f);
    }
    if let Some(id) = filter_project_id {
        params = params.filter_project_id(id);
    }
    if let Some(s) = filter_status {
        let status = parse_run_status(&s)?;
        params = params.filter_status(status);
    }
    if let Some(t) = filter_tags {
        params = params.filter_tags(t);
    }
    if let Some(p) = filter_params {
        params = params.filter_params(p);
    }
    if let Some(r) = filter_parent_run_id {
        params = params.filter_parent_run_id(r);
    }
    if pinned_first {
        params = params.pinned_first(true);
    }
    if include_pinned {
        params = params.include_pinned(true);
    }
    if let Some(s) = sort {
        params = params.sort(s);
    }
    if let Some(n) = page_size {
        params = params.page_size(n);
    }
    if let Some(n) = page_number {
        params = params.page_number(n);
    }
    let resp = api
        .list_model_lab_runs(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list model lab runs: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn runs_get(cfg: &Config, run_id: i64) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_model_lab_run(run_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get model lab run: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn runs_delete(cfg: &Config, run_id: i64) -> Result<()> {
    let api = make_api(cfg);
    api.delete_model_lab_run(run_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete model lab run: {e:?}"))?;
    println!("Run {run_id} deleted.");
    Ok(())
}

pub async fn runs_pin(cfg: &Config, run_id: i64) -> Result<()> {
    let api = make_api(cfg);
    api.pin_model_lab_run(run_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to pin model lab run: {e:?}"))?;
    println!("Run {run_id} pinned.");
    Ok(())
}

pub async fn runs_unpin(cfg: &Config, run_id: i64) -> Result<()> {
    let api = make_api(cfg);
    api.unpin_model_lab_run(run_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to unpin model lab run: {e:?}"))?;
    println!("Run {run_id} unpinned.");
    Ok(())
}

pub async fn runs_artifacts(cfg: &Config, run_id: i64, path: Option<String>) -> Result<()> {
    let api = make_api(cfg);
    let mut params = ListModelLabRunArtifactsOptionalParams::default();
    if let Some(p) = path {
        params = params.path(p);
    }
    let resp = api
        .list_model_lab_run_artifacts(run_id, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list model lab run artifacts: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn runs_facet_keys(cfg: &Config, filter_project_id: i64) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .list_model_lab_run_facet_keys(filter_project_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list model lab run facet keys: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn runs_facet_values(
    cfg: &Config,
    filter_project_id: i64,
    facet_type: &str,
    facet_name: String,
) -> Result<()> {
    let api = make_api(cfg);
    let ft = parse_facet_type(facet_type)?;
    let resp = api
        .list_model_lab_run_facet_values(filter_project_id, ft, facet_name)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list model lab run facet values: {e:?}"))?;
    formatter::output(cfg, &resp)
}

fn parse_run_status(s: &str) -> Result<ModelLabRunStatus> {
    match s {
        "pending" => Ok(ModelLabRunStatus::PENDING),
        "running" => Ok(ModelLabRunStatus::RUNNING),
        "completed" => Ok(ModelLabRunStatus::COMPLETED),
        "failed" => Ok(ModelLabRunStatus::FAILED),
        "killed" => Ok(ModelLabRunStatus::KILLED),
        "unresponsive" => Ok(ModelLabRunStatus::UNRESPONSIVE),
        "paused" => Ok(ModelLabRunStatus::PAUSED),
        other => anyhow::bail!(
            "unknown status '{other}'; valid values: pending, running, completed, failed, killed, unresponsive, paused"
        ),
    }
}

fn parse_facet_type(s: &str) -> Result<ModelLabFacetType> {
    match s {
        "parameter" => Ok(ModelLabFacetType::PARAMETER),
        "attribute" => Ok(ModelLabFacetType::ATTRIBUTE),
        "tag" => Ok(ModelLabFacetType::TAG),
        "metric" => Ok(ModelLabFacetType::METRIC),
        other => anyhow::bail!(
            "unknown facet type '{other}'; valid values: parameter, attribute, tag, metric"
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    // --- Projects ---

    #[tokio::test]
    async fn test_model_lab_projects_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(
            &mut s,
            r#"{"data":[],"meta":{"page":{"number":0,"size":10,"total":0}}}"#,
        )
        .await;
        let result = super::projects_list(&cfg, None, None, None, None, None).await;
        assert!(result.is_ok(), "projects list failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_model_lab_projects_list_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .with_status(403)
            .with_body(r#"{"errors":["Forbidden"]}"#)
            .create_async()
            .await;
        let result = super::projects_list(&cfg, None, None, None, None, None).await;
        assert!(result.is_err(), "projects list should fail on 403");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_model_lab_projects_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(
            &mut s,
            r#"{"data":{"id":"1","type":"projects","attributes":{"artifact_storage_location":"s3://bucket","created_at":"2024-01-01T00:00:00Z","description":"test","is_starred":false,"name":"my-project","tags":[],"updated_at":"2024-01-01T00:00:00Z"}}}"#,
        )
        .await;
        let result = super::projects_get(&cfg, 1).await;
        assert!(result.is_ok(), "projects get failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_model_lab_projects_get_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .with_body(r#"{"errors":["not found"]}"#)
            .create_async()
            .await;
        let result = super::projects_get(&cfg, 999).await;
        assert!(result.is_err(), "projects get should fail on 404");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_model_lab_projects_star() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, "").await;
        let result = super::projects_star(&cfg, 1).await;
        assert!(result.is_ok(), "projects star failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_model_lab_projects_unstar() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, "").await;
        let result = super::projects_unstar(&cfg, 1).await;
        assert!(result.is_ok(), "projects unstar failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_model_lab_projects_artifacts() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(
            &mut s,
            r#"{"data":{"id":"1","type":"project_files","attributes":{"files":[]}}}"#,
        )
        .await;
        let result = super::projects_artifacts(&cfg, 1).await;
        assert!(
            result.is_ok(),
            "projects artifacts failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    // --- Runs ---

    #[tokio::test]
    async fn test_model_lab_runs_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(
            &mut s,
            r#"{"data":[],"meta":{"page":{"number":0,"size":10,"total":0}}}"#,
        )
        .await;
        let result = super::runs_list(
            &cfg, None, None, None, None, None, None, false, false, None, None, None,
        )
        .await;
        assert!(result.is_ok(), "runs list failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_model_lab_runs_list_invalid_status() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data":[]}"#).await;
        let result = super::runs_list(
            &cfg,
            None,
            None,
            Some("bogus".to_string()),
            None,
            None,
            None,
            false,
            false,
            None,
            None,
            None,
        )
        .await;
        assert!(result.is_err(), "invalid status should fail");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_model_lab_runs_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(
            &mut s,
            r#"{"data":{"id":"42","type":"runs","attributes":{"created_at":"2024-01-01T00:00:00Z","descendant_match":false,"description":"test run","has_children":false,"is_pinned":false,"metric_summaries":[],"mlflow_artifact_location":"s3://bucket/run","name":"run-1","params":null,"project_id":1,"started_at":"2024-01-01T00:00:00Z","status":"completed","tags":[],"updated_at":"2024-01-01T00:00:00Z"}}}"#,
        )
        .await;
        let result = super::runs_get(&cfg, 42).await;
        assert!(result.is_ok(), "runs get failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_model_lab_runs_get_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .with_body(r#"{"errors":["not found"]}"#)
            .create_async()
            .await;
        let result = super::runs_get(&cfg, 999).await;
        assert!(result.is_err(), "runs get should fail on 404");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_model_lab_runs_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, "").await;
        let result = super::runs_delete(&cfg, 42).await;
        assert!(result.is_ok(), "runs delete failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_model_lab_runs_delete_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("DELETE", mockito::Matcher::Any)
            .with_status(404)
            .with_body(r#"{"errors":["not found"]}"#)
            .create_async()
            .await;
        let result = super::runs_delete(&cfg, 999).await;
        assert!(result.is_err(), "runs delete should fail on 404");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_model_lab_runs_pin() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, "").await;
        let result = super::runs_pin(&cfg, 42).await;
        assert!(result.is_ok(), "runs pin failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_model_lab_runs_unpin() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, "").await;
        let result = super::runs_unpin(&cfg, 42).await;
        assert!(result.is_ok(), "runs unpin failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_model_lab_runs_artifacts() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data":{"id":"42","type":"artifacts","attributes":{"files":[],"path_in_project":""}}}"#).await;
        let result = super::runs_artifacts(&cfg, 42, None).await;
        assert!(result.is_ok(), "runs artifacts failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_model_lab_parse_run_status_valid() {
        for s in &[
            "pending",
            "running",
            "completed",
            "failed",
            "killed",
            "unresponsive",
            "paused",
        ] {
            assert!(
                super::parse_run_status(s).is_ok(),
                "expected '{s}' to be valid"
            );
        }
    }

    #[tokio::test]
    async fn test_model_lab_parse_run_status_invalid() {
        assert!(super::parse_run_status("bogus").is_err());
    }
}

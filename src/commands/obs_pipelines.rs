use anyhow::Result;
use datadog_api_client::datadogV2::api_observability_pipelines::{
    ListPipelinesOptionalParams, ObservabilityPipelinesAPI,
};
use datadog_api_client::datadogV2::model::{ObservabilityPipeline, ObservabilityPipelineSpec};

use crate::config::Config;
use crate::formatter;
use crate::raw_client;
use crate::util;
use crate::util_ext;

fn make_api(cfg: &Config) -> ObservabilityPipelinesAPI {
    crate::make_api!(ObservabilityPipelinesAPI, cfg)
}

pub async fn list(cfg: &Config, limit: i64) -> Result<()> {
    let api = make_api(cfg);
    let params = ListPipelinesOptionalParams::default().page_size(limit);
    let resp = api
        .list_pipelines(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list pipelines: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get(cfg: &Config, pipeline_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_pipeline(pipeline_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get pipeline: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let body: ObservabilityPipelineSpec = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_pipeline(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create pipeline: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, pipeline_id: &str, file: &str) -> Result<()> {
    let body: ObservabilityPipeline = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .update_pipeline(pipeline_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update pipeline: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn diff(
    cfg: &Config,
    pipeline_id: &str,
    file: &str,
    only: &[String],
    ignore: &[String],
) -> Result<()> {
    let candidate: serde_json::Value = util::read_json_file(file)?;
    let live = raw_client::raw_get(
        cfg,
        &format!("/api/v2/obs-pipelines/pipelines/{pipeline_id}"),
        &[],
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to get pipeline: {e:?}"))?;

    let mut options = util_ext::ResourceDiffOptions::new("pipeline", pipeline_id);
    options.readonly_paths = util_ext::READONLY_OBS_PIPELINE_FIELDS;
    options.only = only;
    options.ignore = ignore;
    options.no_changes_message = Some(format!("No changes - pipeline {pipeline_id} is in sync."));
    util_ext::format_resource_diff(cfg, &live, &candidate, &options)
}

pub async fn delete(cfg: &Config, pipeline_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.delete_pipeline(pipeline_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete pipeline: {e:?}"))?;
    eprintln!("Pipeline {pipeline_id} deleted.");
    Ok(())
}

pub async fn validate(cfg: &Config, file: &str) -> Result<()> {
    let body: ObservabilityPipelineSpec = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .validate_pipeline(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to validate pipeline: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    #[tokio::test]
    async fn test_obs_pipelines_diff_detects_changes() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/v2/obs-pipelines/pipelines/pipeline-123")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "data": {
                        "id": "pipeline-123",
                        "type": "pipelines",
                        "attributes": {
                            "name": "Old Pipeline",
                            "config": {"sources": [{"id": "source", "type": "datadog_agent"}]},
                            "updated_at": "2024-01-01T00:00:00Z"
                        }
                    }
                }"#,
            )
            .create_async()
            .await;
        let path = write_temp_json(
            "pup_obs_pipeline_diff_detects_changes.json",
            r#"{
                "data": {
                    "attributes": {
                        "name": "New Pipeline",
                        "config": {"sources": [{"id": "source", "type": "datadog_agent"}]}
                    }
                }
            }"#,
        );

        let result = super::diff(&cfg, "pipeline-123", path.to_str().unwrap(), &[], &[]).await;
        let _ = std::fs::remove_file(path);
        assert!(
            result.is_ok(),
            "obs-pipelines diff failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_obs_pipelines_diff_invalid_json() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused.local");
        let path = write_temp_json(
            "pup_obs_pipeline_diff_invalid_json.json",
            "not valid json {{{",
        );

        let result = super::diff(&cfg, "pipeline-123", path.to_str().unwrap(), &[], &[]).await;
        let _ = std::fs::remove_file(path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to parse JSON"));
        cleanup_env();
    }
}

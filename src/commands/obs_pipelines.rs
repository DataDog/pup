use anyhow::Result;
use datadog_api_client::datadogV2::api_observability_pipelines::{
    ListPipelinesOptionalParams, ObservabilityPipelinesAPI,
};
use datadog_api_client::datadogV2::model::{ObservabilityPipeline, ObservabilityPipelineSpec};

use crate::config::Config;
use crate::formatter;
use crate::util;

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

use anyhow::Result;
use datadog_api_client::datadogV2::api_feature_flags::{
    FeatureFlagsAPI, ListFeatureFlagsEnvironmentsOptionalParams, ListFeatureFlagsOptionalParams,
};

use crate::config::Config;
use crate::formatter;
use crate::util;

// ---------------------------------------------------------------------------
// Helper: build a FeatureFlagsAPI with OAuth2 support
// ---------------------------------------------------------------------------

fn make_api(cfg: &Config) -> FeatureFlagsAPI {
    crate::make_api!(FeatureFlagsAPI, cfg)
}

// ---------------------------------------------------------------------------
// Feature Flags (flags subgroup)
// ---------------------------------------------------------------------------

pub async fn flags_list(
    cfg: &Config,
    key: Option<String>,
    is_archived: Option<bool>,
    limit: Option<i32>,
    offset: Option<i32>,
) -> Result<()> {
    let api = make_api(cfg);
    let mut params = ListFeatureFlagsOptionalParams::default();
    if let Some(v) = key {
        params = params.key(v);
    }
    if let Some(v) = is_archived {
        params = params.is_archived(v);
    }
    if let Some(v) = limit {
        params = params.limit(v as i64);
    }
    if let Some(v) = offset {
        params = params.offset(v as i64);
    }
    let resp = api
        .list_feature_flags(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list feature flags: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn flags_get(cfg: &Config, feature_flag_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let id = feature_flag_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid feature flag ID: {e:?}"))?;
    let resp = api
        .get_feature_flag(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get feature flag: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn flags_create(cfg: &Config, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::CreateFeatureFlagRequest =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_feature_flag(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create feature flag: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn flags_update(cfg: &Config, feature_flag_id: &str, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::UpdateFeatureFlagRequest =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let id = feature_flag_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid feature flag ID: {e:?}"))?;
    let resp = api
        .update_feature_flag(id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update feature flag: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn flags_archive(cfg: &Config, feature_flag_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let id = feature_flag_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid feature flag ID: {e:?}"))?;
    let resp = api
        .archive_feature_flag(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to archive feature flag: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn flags_unarchive(cfg: &Config, feature_flag_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let id = feature_flag_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid feature flag ID: {e:?}"))?;
    let resp = api
        .unarchive_feature_flag(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to unarchive feature flag: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Environments (environments subgroup)
// ---------------------------------------------------------------------------

pub async fn envs_list(
    cfg: &Config,
    name: Option<String>,
    key: Option<String>,
    limit: Option<i32>,
    offset: Option<i32>,
) -> Result<()> {
    let api = make_api(cfg);
    let mut params = ListFeatureFlagsEnvironmentsOptionalParams::default();
    if let Some(v) = name {
        params = params.name(v);
    }
    if let Some(v) = key {
        params = params.key(v);
    }
    if let Some(v) = limit {
        params = params.limit(v as i64);
    }
    if let Some(v) = offset {
        params = params.offset(v as i64);
    }
    let resp = api
        .list_feature_flags_environments(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list feature flag environments: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn envs_get(cfg: &Config, feature_flags_env_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let id = feature_flags_env_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid environment ID: {e:?}"))?;
    let resp = api
        .get_feature_flags_environment(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get feature flag environment: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn envs_create(cfg: &Config, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::CreateEnvironmentRequest =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_feature_flags_environment(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create feature flag environment: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn envs_update(cfg: &Config, feature_flags_env_id: &str, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::UpdateEnvironmentRequest =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let id = feature_flags_env_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid environment ID: {e:?}"))?;
    let resp = api
        .update_feature_flags_environment(id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update feature flag environment: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn envs_delete(cfg: &Config, feature_flags_env_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let id = feature_flags_env_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid environment ID: {e:?}"))?;
    api.delete_feature_flags_environment(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete feature flag environment: {e:?}"))?;
    eprintln!("Environment {feature_flags_env_id} deleted.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Enable / Disable flag in environment
// ---------------------------------------------------------------------------

pub async fn enable(cfg: &Config, feature_flag_id: &str, feature_flags_env_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let flag_id = feature_flag_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid feature flag ID: {e:?}"))?;
    let env_id = feature_flags_env_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid environment ID: {e:?}"))?;
    api.enable_feature_flag_environment(flag_id, env_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to enable feature flag in environment: {e:?}"))?;
    eprintln!("Feature flag {feature_flag_id} enabled in environment {feature_flags_env_id}.");
    Ok(())
}

pub async fn disable(
    cfg: &Config,
    feature_flag_id: &str,
    feature_flags_env_id: &str,
) -> Result<()> {
    let api = make_api(cfg);
    let flag_id = feature_flag_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid feature flag ID: {e:?}"))?;
    let env_id = feature_flags_env_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid environment ID: {e:?}"))?;
    api.disable_feature_flag_environment(flag_id, env_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to disable feature flag in environment: {e:?}"))?;
    eprintln!("Feature flag {feature_flag_id} disabled in environment {feature_flags_env_id}.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Allocations
// ---------------------------------------------------------------------------

pub async fn allocations_create(
    cfg: &Config,
    feature_flag_id: &str,
    environment_id: &str,
    file: &str,
) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::CreateAllocationsRequest =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let flag_id = feature_flag_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid feature flag ID: {e:?}"))?;
    let env_id = environment_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid environment ID: {e:?}"))?;
    let resp = api
        .create_allocations_for_feature_flag_in_environment(flag_id, env_id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create allocations: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn allocations_update(
    cfg: &Config,
    feature_flag_id: &str,
    environment_id: &str,
    file: &str,
) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::OverwriteAllocationsRequest =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let flag_id = feature_flag_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid feature flag ID: {e:?}"))?;
    let env_id = environment_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid environment ID: {e:?}"))?;
    let resp = api
        .update_allocations_for_feature_flag_in_environment(flag_id, env_id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update allocations: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Exposure schedules
// ---------------------------------------------------------------------------

pub async fn exposure_start(cfg: &Config, exposure_schedule_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let id = exposure_schedule_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid exposure schedule ID: {e:?}"))?;
    let resp = api
        .start_exposure_schedule(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to start exposure schedule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn exposure_stop(cfg: &Config, exposure_schedule_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let id = exposure_schedule_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid exposure schedule ID: {e:?}"))?;
    let resp = api
        .stop_exposure_schedule(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to stop exposure schedule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn exposure_pause(cfg: &Config, exposure_schedule_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let id = exposure_schedule_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid exposure schedule ID: {e:?}"))?;
    let resp = api
        .pause_exposure_schedule(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to pause exposure schedule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn exposure_resume(cfg: &Config, exposure_schedule_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let id = exposure_schedule_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid exposure schedule ID: {e:?}"))?;
    let resp = api
        .resume_exposure_schedule(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to resume exposure schedule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

use anyhow::Result;
use datadog_api_client::datadogV2::api_app_builder::{AppBuilderAPI, ListAppsOptionalParams};
use datadog_api_client::datadogV2::model::{
    AppDefinitionType, CreateAppRequest, DeleteAppsRequest, DeleteAppsRequestDataItems,
    UpdateAppRequest,
};

use crate::config::Config;
use crate::formatter;
use crate::util;
use crate::util_ext;

pub async fn list(cfg: &Config, query: Option<&str>) -> Result<()> {
    let api = crate::make_api!(AppBuilderAPI, cfg);
    let mut params = ListAppsOptionalParams::default();
    if let Some(q) = query {
        params = params.filter_query(q.to_string());
    }
    let resp = api
        .list_apps(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list apps: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get(cfg: &Config, app_id: &str) -> Result<()> {
    let api = crate::make_api!(AppBuilderAPI, cfg);
    let uuid = util_ext::parse_uuid(app_id, "app")?;
    let resp = api
        .get_app(uuid, Default::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get app: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(AppBuilderAPI, cfg);
    let body: CreateAppRequest = util::read_json_file(file)?;
    let resp = api
        .create_app(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create app: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, app_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(AppBuilderAPI, cfg);
    let uuid = util_ext::parse_uuid(app_id, "app")?;
    let body: UpdateAppRequest = util::read_json_file(file)?;
    let resp = api
        .update_app(uuid, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update app: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn delete(cfg: &Config, app_id: &str) -> Result<()> {
    let api = crate::make_api!(AppBuilderAPI, cfg);
    let uuid = util_ext::parse_uuid(app_id, "app")?;
    api.delete_app(uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete app: {e:?}"))?;
    println!("Successfully deleted app {app_id}");
    Ok(())
}

pub async fn delete_batch(cfg: &Config, app_ids: &[String]) -> Result<()> {
    let api = crate::make_api!(AppBuilderAPI, cfg);
    let items: Result<Vec<_>> = app_ids
        .iter()
        .map(|id| {
            let uuid = util_ext::parse_uuid(id, "app")?;
            Ok(DeleteAppsRequestDataItems::new(
                uuid,
                AppDefinitionType::APPDEFINITIONS,
            ))
        })
        .collect();
    let body = DeleteAppsRequest::new().data(items?);
    let resp = api
        .delete_apps(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete apps: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn publish(cfg: &Config, app_id: &str) -> Result<()> {
    let api = crate::make_api!(AppBuilderAPI, cfg);
    let uuid = util_ext::parse_uuid(app_id, "app")?;
    let resp = api
        .publish_app(uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to publish app: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn unpublish(cfg: &Config, app_id: &str) -> Result<()> {
    let api = crate::make_api!(AppBuilderAPI, cfg);
    let uuid = util_ext::parse_uuid(app_id, "app")?;
    api.unpublish_app(uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to unpublish app: {e:?}"))?;
    println!("Successfully unpublished app {app_id}");
    Ok(())
}

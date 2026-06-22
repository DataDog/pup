use anyhow::Result;
use datadog_api_client::datadogV2::api_widgets::{SearchWidgetsOptionalParams, WidgetsAPI};
use datadog_api_client::datadogV2::model::WidgetExperienceType;

use crate::config::Config;
use crate::formatter;
use crate::util;

fn parse_experience_type(s: &str) -> Result<WidgetExperienceType> {
    match s {
        "ccm_reports" => Ok(WidgetExperienceType::CCM_REPORTS),
        "logs_reports" => Ok(WidgetExperienceType::LOGS_REPORTS),
        "csv_reports" => Ok(WidgetExperienceType::CSV_REPORTS),
        "product_analytics" => Ok(WidgetExperienceType::PRODUCT_ANALYTICS),
        other => Err(anyhow::anyhow!(
            "unknown experience type: {other:?}; valid values: ccm_reports, logs_reports, csv_reports, product_analytics"
        )),
    }
}

fn make_api(cfg: &Config) -> WidgetsAPI {
    crate::make_api!(WidgetsAPI, cfg)
}

#[allow(clippy::too_many_arguments)]
pub async fn list(
    cfg: &Config,
    experience_type: &str,
    filter_widget_type: Option<String>,
    filter_creator_handle: Option<String>,
    filter_is_favorited: Option<bool>,
    filter_title: Option<String>,
    filter_tags: Option<String>,
    sort: Option<String>,
    page_number: Option<i32>,
    page_size: Option<i32>,
) -> Result<()> {
    let exp_type = parse_experience_type(experience_type)?;
    let api = make_api(cfg);

    let mut params = SearchWidgetsOptionalParams::default();
    if let Some(wt) = filter_widget_type {
        use datadog_api_client::datadog::UnparsedObject;
        use datadog_api_client::datadogV2::model::WidgetType;
        let widget_type = WidgetType::UnparsedObject(UnparsedObject {
            value: serde_json::Value::String(wt),
        });
        params = params.filter_widget_type(widget_type);
    }
    if let Some(handle) = filter_creator_handle {
        params = params.filter_creator_handle(handle);
    }
    if let Some(favorited) = filter_is_favorited {
        params = params.filter_is_favorited(favorited);
    }
    if let Some(title) = filter_title {
        params = params.filter_title(title);
    }
    if let Some(tags) = filter_tags {
        params = params.filter_tags(tags);
    }
    if let Some(s) = sort {
        params = params.sort(s);
    }
    if let Some(n) = page_number {
        params = params.page_number(n as i64);
    }
    if let Some(n) = page_size {
        params = params.page_size(n as i64);
    }

    let resp = api
        .search_widgets(exp_type, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list widgets: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get(cfg: &Config, experience_type: &str, widget_id: &str) -> Result<()> {
    let exp_type = parse_experience_type(experience_type)?;
    let uuid: uuid::Uuid = widget_id
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid widget UUID {widget_id:?}: {e}"))?;
    let api = make_api(cfg);
    let resp = api
        .get_widget(exp_type, uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get widget: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn create(cfg: &Config, experience_type: &str, file: &str) -> Result<()> {
    let exp_type = parse_experience_type(experience_type)?;
    let body: datadog_api_client::datadogV2::model::CreateOrUpdateWidgetRequest =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_widget(exp_type, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create widget: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update(
    cfg: &Config,
    experience_type: &str,
    widget_id: &str,
    file: &str,
) -> Result<()> {
    let exp_type = parse_experience_type(experience_type)?;
    let uuid: uuid::Uuid = widget_id
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid widget UUID {widget_id:?}: {e}"))?;
    let body: datadog_api_client::datadogV2::model::CreateOrUpdateWidgetRequest =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .update_widget(exp_type, uuid, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update widget: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn delete(cfg: &Config, experience_type: &str, widget_id: &str) -> Result<()> {
    let exp_type = parse_experience_type(experience_type)?;
    let uuid: uuid::Uuid = widget_id
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid widget UUID {widget_id:?}: {e}"))?;
    let api = make_api(cfg);
    api.delete_widget(exp_type, uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete widget: {e:?}"))?;
    eprintln!("Widget {widget_id} deleted.");
    Ok(())
}

use anyhow::Result;
use datadog_api_client::datadogV1::api_monitors::{
    DeleteMonitorOptionalParams, GetMonitorOptionalParams, ListMonitorsOptionalParams, MonitorsAPI,
};
use datadog_api_client::datadogV1::model::{Monitor, MonitorUpdateRequest};

use crate::config::Config;
use crate::formatter;
use crate::util;

#[derive(clap::Subcommand)]
pub enum Command {
    /// Create a monitor
    Create {
        #[arg(long)]
        file: String,
    },
    /// Delete a monitor
    Delete {
        monitor_id: i64,
        #[arg(long)]
        force: Option<String>,
    },
    /// Get a monitor's details
    Get {
        monitor_id: i64,
        #[arg(long)]
        group_states: Option<String>,
        #[arg(long)]
        with_downtimes: Option<bool>,
        #[arg(long)]
        with_assets: Option<bool>,
    },
    /// Get all monitors
    List {
        #[arg(long)]
        group_states: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        tags: Option<String>,
        #[arg(long)]
        monitor_tags: Option<String>,
        #[arg(long)]
        with_downtimes: Option<bool>,
        #[arg(long)]
        id_offset: Option<i64>,
        #[arg(long)]
        page: Option<i64>,
        #[arg(long)]
        page_size: Option<i32>,
    },
    /// Edit a monitor
    Update {
        monitor_id: i64,
        #[arg(long)]
        file: String,
    },
}

pub async fn run(cfg: &Config, command: Command) -> Result<()> {
    match command {
        Command::Create { file } => create(cfg, &file).await,
        Command::Delete { monitor_id, force } => delete(cfg, monitor_id, force).await,
        Command::Get {
            monitor_id,
            group_states,
            with_downtimes,
            with_assets,
        } => get(cfg, monitor_id, group_states, with_downtimes, with_assets).await,
        Command::List {
            group_states,
            name,
            tags,
            monitor_tags,
            with_downtimes,
            id_offset,
            page,
            page_size,
        } => {
            list(
                cfg,
                group_states,
                name,
                tags,
                monitor_tags,
                with_downtimes,
                id_offset,
                page,
                page_size,
            )
            .await
        }
        Command::Update { monitor_id, file } => update(cfg, monitor_id, &file).await,
    }
}

/// Create a monitor
pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let body: Monitor = util::read_json_file(file)?;
    let api = crate::make_api!(MonitorsAPI, cfg);
    let resp = api
        .create_monitor(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create_monitor: {:?}", e))?;
    formatter::output(cfg, &resp)
}

/// Delete a monitor
pub async fn delete(cfg: &Config, monitor_id: i64, force: Option<String>) -> Result<()> {
    let api = crate::make_api!(MonitorsAPI, cfg);
    let mut params = DeleteMonitorOptionalParams::default();
    if let Some(v) = force {
        params = params.force(v);
    }
    let resp = api
        .delete_monitor(monitor_id, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete_monitor: {:?}", e))?;
    formatter::output(cfg, &resp)
}

/// Get a monitor's details
pub async fn get(
    cfg: &Config,
    monitor_id: i64,
    group_states: Option<String>,
    with_downtimes: Option<bool>,
    with_assets: Option<bool>,
) -> Result<()> {
    let api = crate::make_api!(MonitorsAPI, cfg);
    let mut params = GetMonitorOptionalParams::default();
    if let Some(v) = group_states {
        params = params.group_states(v);
    }
    if let Some(v) = with_downtimes {
        params = params.with_downtimes(v);
    }
    if let Some(v) = with_assets {
        params = params.with_assets(v);
    }
    let resp = api
        .get_monitor(monitor_id, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get_monitor: {:?}", e))?;
    formatter::output(cfg, &resp)
}

/// Get all monitors
pub async fn list(
    cfg: &Config,
    group_states: Option<String>,
    name: Option<String>,
    tags: Option<String>,
    monitor_tags: Option<String>,
    with_downtimes: Option<bool>,
    id_offset: Option<i64>,
    page: Option<i64>,
    page_size: Option<i32>,
) -> Result<()> {
    let api = crate::make_api!(MonitorsAPI, cfg);
    let mut params = ListMonitorsOptionalParams::default();
    if let Some(v) = group_states {
        params = params.group_states(v);
    }
    if let Some(v) = name {
        params = params.name(v);
    }
    if let Some(v) = tags {
        params = params.tags(v);
    }
    if let Some(v) = monitor_tags {
        params = params.monitor_tags(v);
    }
    if let Some(v) = with_downtimes {
        params = params.with_downtimes(v);
    }
    if let Some(v) = id_offset {
        params = params.id_offset(v);
    }
    if let Some(v) = page {
        params = params.page(v);
    }
    if let Some(v) = page_size {
        params = params.page_size(v);
    }
    let resp = api
        .list_monitors(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list_monitors: {:?}", e))?;
    let resp: Vec<serde_json::Value> = resp
        .iter()
        .map(|item| {
            let mut value = serde_json::to_value(item).unwrap_or(serde_json::Value::Null);
            if let Some(object) = value.as_object_mut() {
                object.retain(|key, _| {
                    matches!(
                        key.as_str(),
                        "id" | "name" | "type" | "overall_state" | "tags"
                    )
                });
            }
            value
        })
        .collect();
    let meta = formatter::Metadata {
        count: Some(resp.len()),
        truncated: false,
        command: Some("monitors alerting list".to_string()),
        next_action: None,
    };
    formatter::format_and_print(&resp, &cfg.output_format, cfg.agent_mode, Some(&meta))
}

/// Edit a monitor
pub async fn update(cfg: &Config, monitor_id: i64, file: &str) -> Result<()> {
    let body: MonitorUpdateRequest = util::read_json_file(file)?;
    let api = crate::make_api!(MonitorsAPI, cfg);
    let resp = api
        .update_monitor(monitor_id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update_monitor: {:?}", e))?;
    formatter::output(cfg, &resp)
}

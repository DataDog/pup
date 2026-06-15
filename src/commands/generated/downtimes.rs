use anyhow::Result;
use datadog_api_client::datadogV1::api_downtimes::{DowntimesAPI, ListDowntimesOptionalParams};

use crate::config::Config;
use crate::formatter;

#[derive(clap::Subcommand)]
pub enum Command {
    /// Cancel a downtime
    Cancel { downtime_id: i64 },
    /// Get a downtime
    Get { downtime_id: i64 },
    /// Get all downtimes
    List {
        #[arg(long)]
        current_only: Option<bool>,
        #[arg(long)]
        with_creator: Option<bool>,
    },
}

pub async fn run(cfg: &Config, command: Command) -> Result<()> {
    match command {
        Command::Cancel { downtime_id } => cancel(cfg, downtime_id).await,
        Command::Get { downtime_id } => get(cfg, downtime_id).await,
        Command::List {
            current_only,
            with_creator,
        } => list(cfg, current_only, with_creator).await,
    }
}

/// Cancel a downtime
pub async fn cancel(cfg: &Config, downtime_id: i64) -> Result<()> {
    let api = crate::make_api!(DowntimesAPI, cfg);
    let resp = api
        .cancel_downtime(downtime_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to cancel_downtime: {:?}", e))?;
    formatter::output(cfg, &resp)
}

/// Get a downtime
pub async fn get(cfg: &Config, downtime_id: i64) -> Result<()> {
    let api = crate::make_api!(DowntimesAPI, cfg);
    let resp = api
        .get_downtime(downtime_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get_downtime: {:?}", e))?;
    formatter::output(cfg, &resp)
}

/// Get all downtimes
pub async fn list(
    cfg: &Config,
    current_only: Option<bool>,
    with_creator: Option<bool>,
) -> Result<()> {
    let api = crate::make_api!(DowntimesAPI, cfg);
    let mut params = ListDowntimesOptionalParams::default();
    if let Some(v) = current_only {
        params = params.current_only(v);
    }
    if let Some(v) = with_creator {
        params = params.with_creator(v);
    }
    let resp = api
        .list_downtimes(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list_downtimes: {:?}", e))?;
    let meta = formatter::Metadata {
        count: Some(resp.len()),
        truncated: false,
        command: Some("downtimes list".to_string()),
        next_action: None,
    };
    formatter::format_and_print(&resp, &cfg.output_format, cfg.agent_mode, Some(&meta))
}

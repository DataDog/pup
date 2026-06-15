pub mod alerting;

use anyhow::Result;
use datadog_api_client::datadogV1::api_monitors::{MonitorsAPI, SearchMonitorsOptionalParams};

use crate::config::Config;
use crate::formatter;

#[derive(clap::Subcommand)]
pub enum Command {
    /// Search monitors
    Search {
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        page: Option<i64>,
        #[arg(long)]
        per_page: Option<i64>,
        #[arg(long)]
        sort: Option<String>,
    },
    /// alerting commands
    Alerting {
        #[command(subcommand)]
        action: alerting::Command,
    },
}

pub async fn run(cfg: &Config, command: Command) -> Result<()> {
    match command {
        Command::Search {
            query,
            page,
            per_page,
            sort,
        } => search(cfg, query, page, per_page, sort).await,
        Command::Alerting { action } => alerting::run(cfg, action).await,
    }
}

/// Search monitors
pub async fn search(
    cfg: &Config,
    query: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
    sort: Option<String>,
) -> Result<()> {
    let api = crate::make_api!(MonitorsAPI, cfg);
    let mut params = SearchMonitorsOptionalParams::default();
    if let Some(v) = query {
        params = params.query(v);
    }
    if let Some(v) = page {
        params = params.page(v);
    }
    if let Some(v) = per_page {
        params = params.per_page(v);
    }
    if let Some(v) = sort {
        params = params.sort(v);
    }
    let resp = api
        .search_monitors(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search_monitors: {:?}", e))?;
    formatter::output(cfg, &resp)
}

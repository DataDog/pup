pub mod downtimes;
pub mod monitors;

use anyhow::Result;

use crate::config::Config;

/// All spec-generated pup commands.
///
/// This file is generated — do not edit by hand. Re-run the generator to add
/// or remove products; pup never needs manual changes for new commands.
#[derive(clap::Subcommand)]
pub enum GeneratedCommand {
    /// Manage downtimes resources
    Downtimes {
        #[command(subcommand)]
        action: downtimes::Command,
    },
    /// Manage monitors resources
    Monitors {
        #[command(subcommand)]
        action: monitors::Command,
    },
}

pub async fn run(cfg: &Config, command: GeneratedCommand) -> Result<()> {
    match command {
        GeneratedCommand::Downtimes { action } => downtimes::run(cfg, action).await,
        GeneratedCommand::Monitors { action } => monitors::run(cfg, action).await,
    }
}

// Stable integration point for openapi-transformer-generated pup commands.
//
// Future regenerations add variants to `GeneratedCommand` (and new modules
// alongside this file) in place; main.rs never needs to change again.

use anyhow::Result;

use crate::config::Config;

#[derive(clap::Subcommand)]
pub enum GeneratedCommand {}

pub async fn run(_cfg: &Config, command: GeneratedCommand) -> Result<()> {
    match command {}
}

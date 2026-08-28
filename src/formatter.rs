use anyhow::Result;
use serde::Serialize;

use crate::config::OutputFormat;

pub use crate::formatter_ext::{Metadata, TableOptions};

/// Format and print data to stdout.
pub fn format_and_print<T: Serialize>(
    data: &T,
    format: &OutputFormat,
    agent_mode: bool,
    meta: Option<&Metadata>,
    jq: Option<&str>,
) -> Result<()> {
    crate::formatter_ext::format_and_print(data, format, agent_mode, meta, jq)
}

/// Format and print data with command-provided table guidance.
pub fn format_and_print_with_table<T: Serialize>(
    data: &T,
    format: &OutputFormat,
    agent_mode: bool,
    meta: Option<&Metadata>,
    jq: Option<&str>,
    table: TableOptions<'_>,
) -> Result<()> {
    crate::formatter_ext::format_and_print_with_table(
        data, format, agent_mode, meta, jq, table,
    )
}

/// Convenience: format and print using config settings (respects -o flag, agent mode, and --jq).
pub fn output<T: Serialize>(cfg: &crate::config::Config, data: &T) -> Result<()> {
    format_and_print(
        data,
        &cfg.output_format,
        cfg.agent_mode,
        None,
        cfg.jq.as_deref(),
    )
}

/// Convenience wrapper for commands that know their useful table rows and columns.
pub fn output_with_table<T: Serialize>(
    cfg: &crate::config::Config,
    data: &T,
    table: TableOptions<'_>,
) -> Result<()> {
    format_and_print_with_table(
        data,
        &cfg.output_format,
        cfg.agent_mode,
        None,
        cfg.jq.as_deref(),
        table,
    )
}

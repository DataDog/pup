use anyhow::Result;
use serde::Serialize;

use crate::config::OutputFormat;

/// Agent mode metadata envelope.
#[derive(Serialize)]
pub struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
}

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

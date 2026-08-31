use anyhow::{Context, Result};
use serde_json::Value;
use std::io::Read;

use crate::config::Config;
use crate::formatter;

/// Render JSON through pup's formatter.
///
/// Reads a JSON document from stdin (default) or `--input FILE`, then prints it
/// using the configured output format (`--output`, `$DD_OUTPUT`/`$PUP_OUTPUT`).
/// This lets an extension in any language produce JSON and reuse pup's
/// table/yaml/csv/tsv rendering instead of reimplementing them.
pub fn run(cfg: &Config, input: Option<&str>) -> Result<()> {
    let raw = read_input(input, std::io::stdin().lock())?;
    render(cfg, &raw)
}

/// Read the JSON document from a file (`Some(path)` other than `"-"`) or from the
/// provided reader (`None` or `"-"`, i.e. stdin). The reader is a parameter so the
/// stdin path can be tested without touching the process's real stdin.
fn read_input(input: Option<&str>, reader: impl Read) -> Result<String> {
    match input {
        Some(path) if path != "-" => std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("failed to read --input {path:?}: {e}")),
        _ => crate::util_ext::read_to_string(reader, "failed to read JSON from stdin"),
    }
}

/// Parse `raw` as JSON and print it through the shared formatter.
fn render(cfg: &Config, raw: &str) -> Result<()> {
    if raw.trim().is_empty() {
        anyhow::bail!("no JSON input provided (pipe JSON to stdin or pass --input FILE)");
    }

    let value: Value = serde_json::from_str(raw).context("input is not valid JSON")?;
    formatter::output(cfg, &value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OutputFormat;
    use crate::test_support::write_temp_json;
    use std::io::Cursor;

    #[test]
    fn test_read_input_from_stdin_when_none() {
        let got = read_input(None, Cursor::new(b"[1,2,3]".to_vec())).unwrap();
        assert_eq!(got, "[1,2,3]");
    }

    #[test]
    fn test_read_input_from_stdin_when_dash() {
        let got = read_input(Some("-"), Cursor::new(b"{\"a\":1}".to_vec())).unwrap();
        assert_eq!(got, "{\"a\":1}");
    }

    #[test]
    fn test_read_input_from_file_ignores_reader() {
        let path = write_temp_json("pup_format_read_input.json", r#"{"from":"file"}"#);
        let got = read_input(
            path.to_str(),
            Cursor::new(b"{\"from\":\"reader\"}".to_vec()),
        );
        std::fs::remove_file(&path).ok();
        assert_eq!(got.unwrap(), r#"{"from":"file"}"#);
    }

    #[test]
    fn test_render_stdin_table() {
        let cfg = cfg_with(OutputFormat::Table);
        let raw = read_input(None, Cursor::new(b"[{\"id\":1}]".to_vec())).unwrap();
        let result = render(&cfg, &raw);
        assert!(result.is_ok(), "stdin render failed: {:?}", result.err());
    }

    fn cfg_with(format: OutputFormat) -> Config {
        Config {
            api_key: None,
            app_key: None,
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: format,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        }
    }

    #[test]
    fn test_run_reads_file_input_json() {
        let path = write_temp_json("pup_format_input.json", r#"[{"id":1,"name":"x"}]"#);
        let cfg = cfg_with(OutputFormat::Json);
        let result = run(&cfg, path.to_str());
        std::fs::remove_file(&path).ok();
        assert!(
            result.is_ok(),
            "format from file failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_run_table_format_from_file() {
        let path = write_temp_json("pup_format_table.json", r#"[{"id":1,"name":"x"}]"#);
        let cfg = cfg_with(OutputFormat::Table);
        let result = run(&cfg, path.to_str());
        std::fs::remove_file(&path).ok();
        assert!(result.is_ok(), "table format failed: {:?}", result.err());
    }

    #[test]
    fn test_run_json_is_raw_payload() {
        let path = write_temp_json("pup_format_raw.json", r#"{"data":[]}"#);
        let cfg = cfg_with(OutputFormat::Json);
        let result = run(&cfg, path.to_str());
        std::fs::remove_file(&path).ok();
        assert!(result.is_ok(), "json format failed: {:?}", result.err());
    }

    #[test]
    fn test_run_invalid_json_errors() {
        let path = write_temp_json("pup_format_bad.json", "{not json");
        let cfg = cfg_with(OutputFormat::Json);
        let result = run(&cfg, path.to_str());
        std::fs::remove_file(&path).ok();
        assert!(result.is_err(), "expected error for invalid JSON");
    }

    #[test]
    fn test_run_empty_input_errors() {
        let path = write_temp_json("pup_format_empty.json", "   \n");
        let cfg = cfg_with(OutputFormat::Json);
        let result = run(&cfg, path.to_str());
        std::fs::remove_file(&path).ok();
        assert!(result.is_err(), "expected error for empty input");
    }

    #[test]
    fn test_run_missing_file_errors() {
        let cfg = cfg_with(OutputFormat::Json);
        let result = run(&cfg, Some("/nonexistent/pup-format/x.json"));
        assert!(result.is_err(), "expected error for missing file");
    }
}

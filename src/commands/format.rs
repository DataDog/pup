use anyhow::{Context, Result};
use serde_json::Value;
use std::io::Read;

use crate::config::Config;
use crate::formatter;

#[derive(Default)]
pub struct FormatOptions<'a> {
    pub count: Option<usize>,
    pub command: Option<String>,
    pub next_action: Option<String>,
    pub rows_at: Option<&'a str>,
    pub row_at: Option<&'a str>,
    pub columns: &'a [String],
}

/// Render JSON through pup's formatter.
///
/// Reads a JSON document from stdin (default) or `--input FILE`, then prints it
/// using the configured output format (`--output`, `$DD_OUTPUT`/`$PUP_OUTPUT`). This
/// lets an extension in any language produce JSON and reuse pup's JSON envelope and
/// table/yaml/csv/tsv rendering instead of reimplementing them.
///
/// `rows_at`, `row_at`, and `columns` select the rows and columns for table, CSV, and
/// TSV output. They do not project or otherwise change JSON or YAML output.
///
/// `count`, `command`, and `next_action` override the envelope's metadata; unset
/// fields are derived from the document as for any other command.
pub fn run(cfg: &Config, input: Option<&str>, options: FormatOptions<'_>) -> Result<()> {
    let raw = read_input(input, std::io::stdin().lock())?;
    render(cfg, &raw, options)
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
fn render(cfg: &Config, raw: &str, options: FormatOptions<'_>) -> Result<()> {
    if raw.trim().is_empty() {
        anyhow::bail!("no JSON input provided (pipe JSON to stdin or pass --input FILE)");
    }

    let value: Value = serde_json::from_str(raw).context("input is not valid JSON")?;
    let piped = crate::output::unwrap_envelope(&value);
    let FormatOptions {
        count,
        command,
        next_action,
        rows_at,
        row_at,
        columns,
    } = options;

    let columns: Vec<&str> = columns
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|column| !column.is_empty())
        .collect();
    let mut table = formatter::TableOptions::new(&columns);
    if let Some(pointer) = rows_at {
        table = table.rows_at(pointer);
    }
    if let Some(pointer) = row_at {
        table = table.row_at(pointer);
    }
    let Some((body, piped_meta)) = piped else {
        let meta = formatter::Metadata {
            count,
            command,
            next_action,
            ..Default::default()
        };
        return formatter::format_and_print_with(
            &value,
            &cfg.output_format,
            cfg.jq.as_deref(),
            Some(&meta),
            Some(table),
        );
    };
    let meta = formatter::Metadata {
        count: count.or(piped_meta.count),
        command: command.or(piped_meta.command),
        next_action: next_action.or(piped_meta.next_action),
        ..piped_meta
    };
    // Re-render pup's own envelope (e.g. `pup api ... | pup format`) rather
    // than wrapping it again. Rebuilding it from the unwrapped body would be
    // lossy (`[1]` and `{"data": [1]}` produce the same envelope), so JSON and
    // YAML print the piped envelope as-is, with any metadata overrides.
    if crate::output::metadata_visible(&cfg.output_format) && cfg.jq.is_none() {
        let mut envelope = value;
        envelope["metadata"] = serde_json::to_value(&meta)?;
        return crate::output::print_envelope(&envelope, &cfg.output_format);
    }
    // Rows (table/CSV/TSV), `--jq`, and `--no-envelope` work on the body.
    formatter::format_and_print_with(
        &body,
        &cfg.output_format,
        cfg.jq.as_deref(),
        Some(&meta),
        Some(table),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OutputFormat;
    use crate::test_support::write_temp_json;
    use std::io::Cursor;

    #[test]
    fn test_read_input_from_stdin_when_none() {
        // input None → read from the provided reader (stdin in production).
        let got = read_input(None, Cursor::new(b"[1,2,3]".to_vec())).unwrap();
        assert_eq!(got, "[1,2,3]");
    }

    #[test]
    fn test_read_input_from_stdin_when_dash() {
        // input "-" → read from the provided reader (stdin in production).
        let got = read_input(Some("-"), Cursor::new(b"{\"a\":1}".to_vec())).unwrap();
        assert_eq!(got, "{\"a\":1}");
    }

    #[test]
    fn test_read_input_from_file_ignores_reader() {
        let path = write_temp_json("pup_format_read_input.json", r#"{"from":"file"}"#);
        // A non-"-" path reads the file, not the reader.
        let got = read_input(
            path.to_str(),
            Cursor::new(b"{\"from\":\"reader\"}".to_vec()),
        );
        std::fs::remove_file(&path).ok();
        assert_eq!(got.unwrap(), r#"{"from":"file"}"#);
    }

    #[test]
    fn test_render_stdin_table() {
        // The primary documented path: JSON piped via stdin, rendered as a table.
        let cfg = cfg_with(OutputFormat::Table);
        let raw = read_input(None, Cursor::new(b"[{\"id\":1}]".to_vec())).unwrap();
        let result = render(&cfg, &raw, FormatOptions::default());
        assert!(result.is_ok(), "stdin render failed: {:?}", result.err());
    }

    fn cfg_with(format: OutputFormat) -> Config {
        Config {
            #[cfg(not(target_arch = "wasm32"))]
            aws_wif_auth: None,
            api_key: None,
            app_key: None,
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: format,
            auto_approve: false,
            read_only: false,
            jq: None,
        }
    }

    #[test]
    fn test_run_reads_file_input_json() {
        let path = write_temp_json("pup_format_input.json", r#"[{"id":1,"name":"x"}]"#);
        let cfg = cfg_with(OutputFormat::Json);
        let result = run(&cfg, path.to_str(), FormatOptions::default());
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
        let result = run(&cfg, path.to_str(), FormatOptions::default());
        std::fs::remove_file(&path).ok();
        assert!(result.is_ok(), "table format failed: {:?}", result.err());
    }

    #[test]
    fn test_render_table_with_explicit_rows_and_columns() {
        let cfg = cfg_with(OutputFormat::Table);
        let columns = vec!["name".to_string(), "id".to_string()];
        let result = render(
            &cfg,
            r#"{"results":[{"data":{"id":1,"name":"alpha","details":"omit"}}]}"#,
            FormatOptions {
                rows_at: Some("/results"),
                row_at: Some("/data"),
                columns: &columns,
                ..FormatOptions::default()
            },
        );
        assert!(result.is_ok(), "table hints failed: {:?}", result.err());
    }

    #[test]
    fn test_render_table_rejects_missing_rows_path() {
        let cfg = cfg_with(OutputFormat::Table);
        let result = render(
            &cfg,
            r#"{"results":[]}"#,
            FormatOptions {
                rows_at: Some("/missing"),
                ..FormatOptions::default()
            },
        );
        assert!(result.is_err(), "missing table row path should fail");
    }

    #[test]
    fn test_non_table_output_ignores_table_hints() {
        let cfg = cfg_with(OutputFormat::Json);
        let result = render(
            &cfg,
            r#"{"results":[]}"#,
            FormatOptions {
                rows_at: Some("/missing"),
                ..FormatOptions::default()
            },
        );
        assert!(result.is_ok(), "JSON should ignore table hints");
    }

    #[test]
    fn test_render_json_with_metadata_flags() {
        let cfg = cfg_with(OutputFormat::Json);
        let options = FormatOptions {
            count: Some(0),
            command: Some("format".into()),
            next_action: Some("next".into()),
            ..FormatOptions::default()
        };
        assert!(render(&cfg, "[]", options).is_ok());
    }

    #[test]
    fn test_run_invalid_json_errors() {
        let path = write_temp_json("pup_format_bad.json", "{not json");
        let cfg = cfg_with(OutputFormat::Json);
        let result = run(&cfg, path.to_str(), FormatOptions::default());
        std::fs::remove_file(&path).ok();
        assert!(result.is_err(), "expected error for invalid JSON");
    }

    #[test]
    fn test_run_empty_input_errors() {
        let path = write_temp_json("pup_format_empty.json", "   \n");
        let cfg = cfg_with(OutputFormat::Json);
        let result = run(&cfg, path.to_str(), FormatOptions::default());
        std::fs::remove_file(&path).ok();
        assert!(result.is_err(), "expected error for empty input");
    }

    #[test]
    fn test_run_missing_file_errors() {
        let cfg = cfg_with(OutputFormat::Json);
        let result = run(
            &cfg,
            Some("/nonexistent/pup-format/x.json"),
            FormatOptions::default(),
        );
        assert!(result.is_err(), "expected error for missing file");
    }
}

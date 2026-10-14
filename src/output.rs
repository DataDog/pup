use anyhow::Result;
use serde::Serialize;
#[cfg(not(target_arch = "wasm32"))]
use std::io::IsTerminal;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use crate::config::OutputFormat;
use crate::filter;

#[derive(Clone, Copy, PartialEq)]
enum OutputOrder {
    Default,
    Preserve,
}

/// Command-provided guidance for rendering a concise table.
///
/// These options affect table output only. Other output formats continue to
/// serialize the complete response.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TableOptions<'a> {
    rows_at: Option<&'a str>,
    row_at: Option<&'a str>,
    columns: &'a [&'a str],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableInput<'a> {
    Generic,
    Explicit(TableOptions<'a>),
    AsIs,
}

impl<'a> From<TableOptions<'a>> for TableInput<'a> {
    fn from(options: TableOptions<'a>) -> Self {
        if options.rows_at.is_none() && options.row_at.is_none() && options.columns.is_empty() {
            Self::Generic
        } else {
            Self::Explicit(options)
        }
    }
}

impl<'a> TableInput<'a> {
    fn options(self) -> Option<TableOptions<'a>> {
        match self {
            Self::Explicit(options) => Some(options),
            Self::Generic | Self::AsIs => None,
        }
    }

    fn has_row_hints(self) -> bool {
        self.options()
            .is_some_and(|options| options.row_at.is_some() || !options.columns.is_empty())
    }
}

impl<'a> TableOptions<'a> {
    pub const fn new(columns: &'a [&'a str]) -> Self {
        Self {
            rows_at: None,
            row_at: None,
            columns,
        }
    }

    pub const fn rows_at(mut self, pointer: &'a str) -> Self {
        self.rows_at = Some(pointer);
        self
    }

    pub const fn row_at(mut self, pointer: &'a str) -> Self {
        self.row_at = Some(pointer);
        self
    }
}

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD_CYAN: &str = "\x1b[1;36m";
const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_MAGENTA: &str = "\x1b[35m";
const ANSI_DIM: &str = "\x1b[2m";
const ANSI_GRAY: &str = "\x1b[90m";
const OSC8_OPEN: &str = "\x1b]8;;";
const OSC8_CLOSE: &str = "\x1b]8;;\x1b\\";
const OSC_TERMINATOR: &str = "\x1b\\";
const NO_RESULTS: &str = "No results found";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TerminalCapabilities {
    colors: bool,
    hyperlinks: bool,
}

fn color_enabled(
    is_terminal: bool,
    no_color: Option<&std::ffi::OsStr>,
    clicolor: Option<&str>,
    term: Option<&str>,
) -> bool {
    is_terminal
        && no_color.is_none_or(|value| value.is_empty())
        && clicolor != Some("0")
        && !term.is_some_and(|value| value.eq_ignore_ascii_case("dumb"))
}

fn hyperlinks_enabled(terminal_styling: bool, pup_hyperlinks: Option<&str>) -> bool {
    terminal_styling && pup_hyperlinks != Some("0")
}

fn stdout_terminal_capabilities() -> TerminalCapabilities {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let colors = color_enabled(
            std::io::stdout().is_terminal(),
            std::env::var_os("NO_COLOR").as_deref(),
            std::env::var("CLICOLOR").ok().as_deref(),
            std::env::var("TERM").ok().as_deref(),
        );
        TerminalCapabilities {
            colors,
            hyperlinks: hyperlinks_enabled(colors, std::env::var("PUP_HYPERLINKS").ok().as_deref()),
        }
    }

    #[cfg(target_arch = "wasm32")]
    TerminalCapabilities::default()
}

/// Recursively sort all JSON object keys alphabetically.
fn sort_json_value(v: serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut sorted: std::collections::BTreeMap<String, serde_json::Value> =
                std::collections::BTreeMap::new();
            for (k, val) in map {
                sorted.insert(k, sort_json_value(val));
            }
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(sort_json_value).collect())
        }
        other => other,
    }
}

fn order_json_value(value: &serde_json::Value, output_order: OutputOrder) -> serde_json::Value {
    match output_order {
        OutputOrder::Default => sort_json_value(value.clone()),
        OutputOrder::Preserve => value.clone(),
    }
}

/// Go's encoding/json escapes <, >, and & for HTML safety.
/// Apply the same escaping to match Go output exactly.
fn go_html_escape(json: &str) -> String {
    json.replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
}

/// Metadata in the `metadata` field of the default JSON envelope.
///
/// Commands set only what the response cannot tell pup on its own;
/// `resolve_metadata` fills in the count, command path, continuation cursor,
/// and truncation flag from the response itself.
#[derive(Clone, Debug, Default, PartialEq, Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    /// True when pup can tell more results exist than were returned; false
    /// means "not known to be truncated", since not every API reports it.
    /// Always serialized so a known partial result is never hidden.
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Cursor for the next page; pass it back with the command's cursor flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Offset for the next page; pass it back with `--offset`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
    /// Page number of the next page; pass it back with the command's page flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Response locations that hold a continuation cursor, checked in order.
/// Only fields that always mean "the next page" belong here: a field that can
/// hold the current page's cursor (such as deployment gates' `meta.page.cursor`)
/// would mark every last page as truncated.
const CURSOR_POINTERS: &[&str] = &[
    "/meta/page/after",
    "/meta/page/next_cursor",
    "/meta/after",
    "/meta/next_cursor",
    "/meta/pagination/next_page",
    "/page/next_cursor",
    "/data/next_cursor",
    "/next_cursor",
    "/next_page_token",
];

/// Response locations whose `true` value marks a partial result.
const TRUNCATION_POINTERS: &[&str] = &[
    "/truncated",
    "/is_truncated",
    "/meta/truncated",
    "/meta/page/truncated",
    "/page/truncated",
    "/data/attributes/is_truncated",
    "/data/truncated",
];

/// `(total, items)` pointer pairs: the result is truncated when the response's
/// total exceeds the number of items it returned.
const TOTAL_COUNT_POINTERS: &[(&str, &str)] = &[("/meta/page/total_filtered_count", "/data")];

/// `(page, page_count)` pointer pairs for 0-indexed page pagination (e.g.
/// monitors search's own top-level `metadata`): another page exists when
/// `page + 1 < page_count`.
const PAGE_POINTERS: &[(&str, &str)] = &[("/metadata/page", "/metadata/page_count")];

/// `--no-envelope` (or `PUP_NO_ENVELOPE`): print the raw response body.
static RAW_OUTPUT: AtomicBool = AtomicBool::new(false);
/// The running command, reported as `metadata.command`.
static COMMAND: OnceLock<CommandInfo> = OnceLock::new();

struct CommandInfo {
    path: String,
    flags: ContinuationFlags,
}

/// The flags a command takes to fetch the next page, read from its clap
/// definition so hints name the real flag.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContinuationFlags {
    /// Resumes from a cursor (e.g. `--cursor`, `--page-cursor`, `--page-token`).
    pub cursor: Option<String>,
    /// Selects a page number (e.g. `--page`).
    pub page: Option<String>,
}

/// Print raw response bodies instead of the envelope (`--no-envelope`).
pub fn set_raw_output(raw: bool) {
    RAW_OUTPUT.store(raw, Ordering::Relaxed);
}

/// `--no-envelope` was passed, or `PUP_NO_ENVELOPE` is truthy (it is injected
/// into extension subprocesses so a child `pup` inherits the parent's choice).
#[cfg(not(feature = "browser"))]
pub fn raw_output_requested(flag: bool) -> bool {
    flag || crate::settings::env_bool("PUP_NO_ENVELOPE")
}

pub fn raw_output_enabled() -> bool {
    RAW_OUTPUT.load(Ordering::Relaxed)
}

/// Record the running command's path (e.g. `logs search`) for
/// `metadata.command`, and the flags it takes to fetch the next page.
pub fn set_command(path: String, flags: ContinuationFlags) {
    let _ = COMMAND.set(CommandInfo { path, flags });
}

/// Outside a parsed command (library use, unit tests) no flags are known, so
/// hints fall back to the most common ones.
fn continuation_flags() -> ContinuationFlags {
    match COMMAND.get() {
        Some(info) => info.flags.clone(),
        None => ContinuationFlags {
            cursor: Some("--cursor".into()),
            page: Some("--page".into()),
        },
    }
}

/// True when `format` prints the envelope (JSON and YAML), so its metadata is
/// part of stdout. Table, CSV, and TSV print only rows; their metadata goes to
/// stderr instead (see `stderr_block`).
pub fn metadata_visible(format: &OutputFormat) -> bool {
    matches!(format, OutputFormat::Json | OutputFormat::Yaml) && !raw_output_enabled()
}

fn derive_cursor(raw: &serde_json::Value) -> Option<String> {
    CURSOR_POINTERS
        .iter()
        .filter_map(|pointer| raw.pointer(pointer)?.as_str())
        .find(|cursor| !cursor.is_empty())
        .map(str::to_string)
}

fn derive_next_page(raw: &serde_json::Value) -> Option<u64> {
    PAGE_POINTERS.iter().find_map(|(page, count)| {
        let page = raw.pointer(page)?.as_u64()?;
        let count = raw.pointer(count)?.as_u64()?;
        (page + 1 < count).then_some(page + 1)
    })
}

fn derive_truncated(raw: &serde_json::Value) -> bool {
    let flagged = TRUNCATION_POINTERS
        .iter()
        .any(|pointer| raw.pointer(pointer) == Some(&serde_json::Value::Bool(true)));
    flagged
        || TOTAL_COUNT_POINTERS.iter().any(|(total, items)| {
            let total = raw.pointer(total).and_then(serde_json::Value::as_u64);
            let returned = raw
                .pointer(items)
                .and_then(serde_json::Value::as_array)
                .map(|items| items.len() as u64);
            matches!((total, returned), (Some(total), Some(returned)) if total > returned)
        })
}

/// Fill in metadata the caller did not set. `raw` is the response before
/// `--jq`; `data` is what the envelope will carry. The count always describes
/// `data`, while truncation and the cursor describe the underlying results, so
/// they survive a `--jq` filter that drops the response's own paging fields.
fn resolve_metadata(
    raw: &serde_json::Value,
    data: &serde_json::Value,
    explicit: Option<&Metadata>,
    filtered: bool,
) -> Metadata {
    let mut meta = explicit.cloned().unwrap_or_default();
    if filtered || meta.count.is_none() {
        meta.count = data.as_array().map(Vec::len);
    }
    if meta.command.is_none() {
        meta.command = COMMAND.get().map(|info| info.path.clone());
    }
    if meta.next_cursor.is_none() {
        meta.next_cursor = derive_cursor(raw);
    }
    if meta.next_page.is_none() {
        meta.next_page = derive_next_page(raw);
    }
    meta.truncated = meta.truncated
        || derive_truncated(raw)
        || meta.next_cursor.is_some()
        || meta.next_offset.is_some()
        || meta.next_page.is_some();
    if meta.next_action.is_none() {
        meta.next_action = continuation_hint(&meta);
    }
    meta
}

/// How to fetch the next page, when there is one, using the running command's
/// own cursor or page flag.
pub fn continuation_hint(meta: &Metadata) -> Option<String> {
    continuation_hint_with(meta, &continuation_flags())
}

fn continuation_hint_with(meta: &Metadata, flags: &ContinuationFlags) -> Option<String> {
    if let Some(cursor) = &meta.next_cursor {
        return Some(match &flags.cursor {
            Some(flag) => format!("More results available; rerun with {flag}=\"{cursor}\""),
            None => format!(
                "More results available; next cursor: \"{cursor}\" (this command has no cursor flag)"
            ),
        });
    }
    if let Some(offset) = meta.next_offset {
        return Some(format!(
            "More results available; rerun with --offset={offset}"
        ));
    }
    meta.next_page.map(|page| match &flags.page {
        Some(flag) => format!("More results available; rerun with {flag}={page}"),
        None => {
            format!("More results available; next page: {page} (this command has no page flag)")
        }
    })
}

/// The metadata block printed to stderr when stdout has no envelope.
///
/// With the envelope enabled, table, CSV, and TSV output print only rows on
/// stdout, so their metadata always goes to stderr as one block:
///
/// ```text
/// metadata: command="logs search" count=2 truncated=true next_cursor="abc"
/// next_action: More results available; rerun with --cursor="abc"
/// warning: <one line per warning>
/// ```
///
/// The `metadata:` line always appears; its `key=value` pairs keep this order
/// and omit absent keys, with strings JSON-quoted. `next_action:` and
/// `warning:` lines appear only when set. With `--no-envelope` pup adds no
/// metadata anywhere, so only `warning:` lines (problems) are printed.
fn stderr_block(meta: &Metadata, envelope: bool) -> Vec<String> {
    let mut lines = Vec::new();
    if envelope {
        let mut pairs = Vec::new();
        let quote = |s: &str| serde_json::Value::String(s.to_string()).to_string();
        if let Some(command) = &meta.command {
            pairs.push(format!("command={}", quote(command)));
        }
        if let Some(count) = meta.count {
            pairs.push(format!("count={count}"));
        }
        pairs.push(format!("truncated={}", meta.truncated));
        if let Some(cursor) = &meta.next_cursor {
            pairs.push(format!("next_cursor={}", quote(cursor)));
        }
        if let Some(offset) = meta.next_offset {
            pairs.push(format!("next_offset={offset}"));
        }
        if let Some(page) = meta.next_page {
            pairs.push(format!("next_page={page}"));
        }
        lines.push(format!("metadata: {}", pairs.join(" ")));
        if let Some(action) = &meta.next_action {
            lines.push(format!("next_action: {action}"));
        }
    }
    lines.extend(meta.warnings.iter().map(|w| format!("warning: {w}")));
    lines
}

/// Split a document that is already a pup envelope (for example `pup api`
/// output piped into `pup format`) back into the response and its metadata.
/// Returns `None` for any other document. pup always writes a boolean
/// `metadata.truncated` and only `Metadata`'s own keys, so a look-alike
/// document with other metadata is left whole rather than losing those keys.
pub fn unwrap_envelope(value: &serde_json::Value) -> Option<(serde_json::Value, Metadata)> {
    let obj = value.as_object()?;
    if obj.get("status")?.as_str()? != "success" || !obj.contains_key("data") {
        return None;
    }
    let metadata = obj.get("metadata")?;
    metadata.get("truncated")?.as_bool()?;
    let meta: Metadata = serde_json::from_value(metadata.clone()).ok()?;
    let mut rest = obj.clone();
    rest.remove("status");
    rest.remove("metadata");
    let body = if rest.len() == 1 {
        rest.remove("data").unwrap_or_default()
    } else {
        serde_json::Value::Object(rest)
    };
    Some((body, meta))
}

/// Wrap a response in the default `{status, data, metadata}` envelope.
///
/// A JSON:API-style body (an object with a top-level `data`) keeps all of its
/// keys: `data` stays where it is, and siblings such as `meta`, `links`, and
/// `included` are preserved next to `status` and `metadata`. Any other body,
/// and any `--jq` result, becomes the envelope's `data`. `unfiltered` is the
/// response before `--jq`, set only when a filter produced `value`.
pub fn build_envelope(
    value: serde_json::Value,
    unfiltered: Option<&serde_json::Value>,
    explicit: Option<&Metadata>,
) -> Result<serde_json::Value> {
    let filtered = unfiltered.is_some();
    let merge_in_place = !filtered
        && value.as_object().is_some_and(|obj| {
            obj.contains_key("data") && !obj.contains_key("status") && !obj.contains_key("metadata")
        });
    let data = if merge_in_place {
        &value["data"]
    } else {
        &value
    };
    let raw = unfiltered.unwrap_or(&value);
    let meta = serde_json::to_value(resolve_metadata(raw, data, explicit, filtered))?;
    if merge_in_place {
        let mut envelope = value;
        if let Some(obj) = envelope.as_object_mut() {
            obj.insert("status".into(), "success".into());
            obj.insert("metadata".into(), meta);
        }
        return Ok(envelope);
    }
    Ok(serde_json::json!({ "status": "success", "data": value, "metadata": meta }))
}

/// Format and print data to stdout.
///
/// JSON output is wrapped in the default envelope (see `build_envelope`)
/// unless `--no-envelope` is set. The `jq` parameter, when `Some`, applies a
/// jq expression to the serialized response **before** wrapping or rendering,
/// so the same expression works across all output formats; its result becomes
/// the envelope's `data`.
pub fn format_and_print<T: Serialize>(
    data: &T,
    format: &OutputFormat,
    jq: Option<&str>,
) -> Result<()> {
    format_and_print_with(data, format, jq, None, None)
}

/// Format and print data with command-provided table guidance.
pub fn format_and_print_with_table<T: Serialize>(
    data: &T,
    format: &OutputFormat,
    jq: Option<&str>,
    table: TableOptions<'_>,
) -> Result<()> {
    format_and_print_with(data, format, jq, None, Some(table))
}

/// Format and print data with command-provided metadata and/or table guidance.
pub fn format_and_print_with<T: Serialize>(
    data: &T,
    format: &OutputFormat,
    jq: Option<&str>,
    meta: Option<&Metadata>,
    table: Option<TableOptions<'_>>,
) -> Result<()> {
    let table_input = table.map_or(TableInput::Generic, Into::into);
    format_and_print_with_order(data, format, jq, meta, table_input, OutputOrder::Default)
}

fn format_and_print_with_order<T: Serialize>(
    data: &T,
    format: &OutputFormat,
    jq: Option<&str>,
    meta: Option<&Metadata>,
    table_input: TableInput<'_>,
    output_order: OutputOrder,
) -> Result<()> {
    // Serialize once; all renderers and the filter operate on this Value. The
    // pre-filter response is kept only when `--jq` replaced it.
    let response = serde_json::to_value(data)?;
    let (mut value, unfiltered) = match jq {
        Some(expr) => (filter::apply_jq(response.clone(), expr)?, Some(response)),
        None => (response, None),
    };
    let output_order = effective_output_order(format, jq, output_order);
    let table_input = effective_table_input(format, jq, table_input);
    if metadata_visible(format) {
        value = build_envelope(value, unfiltered.as_ref(), meta)?;
    } else {
        // Table, CSV, and TSV print only rows (and `--no-envelope` prints the
        // raw body), so stdout stays parseable and the metadata goes to stderr.
        let raw = unfiltered.as_ref().unwrap_or(&value);
        let rows = select_table_data(&value, table_input).unwrap_or(&value);
        let resolved = resolve_metadata(raw, rows, meta, jq.is_some());
        for line in stderr_block(&resolved, !raw_output_enabled()) {
            eprintln!("{line}");
        }
    }

    print_value(&value, format, output_order, table_input)
}

/// Print an already-built pup envelope (for example one piped into
/// `pup format`) as-is, without re-deriving it, so re-rendering JSON or YAML
/// is lossless.
pub fn print_envelope(envelope: &serde_json::Value, format: &OutputFormat) -> Result<()> {
    print_value(envelope, format, OutputOrder::Default, TableInput::Generic)
}

/// Render a raw payload produced by another pup process (a runbook step runs
/// pup with `--no-envelope`) the way `format_and_print` would print it in
/// `format`: the envelope for JSON/YAML (unless `--no-envelope`), or rows for
/// table/CSV/TSV. Returns the stdout text and the stderr metadata lines.
/// `filtered` marks a payload that is a `--jq` result, so it is wrapped whole.
pub fn render_payload(
    payload: serde_json::Value,
    format: &OutputFormat,
    meta: Option<&Metadata>,
    filtered: bool,
) -> Result<(String, Vec<String>)> {
    let table_input = if filtered {
        TableInput::AsIs
    } else {
        TableInput::Generic
    };
    if metadata_visible(format) {
        let unfiltered = filtered.then(|| payload.clone());
        let envelope = build_envelope(payload, unfiltered.as_ref(), meta)?;
        return Ok((format_value_to_string(&envelope, format)?, Vec::new()));
    }
    let rows = select_table_data(&payload, table_input).unwrap_or(&payload);
    let resolved = resolve_metadata(&payload, rows, meta, filtered);
    let notes = stderr_block(&resolved, !raw_output_enabled());
    let text =
        format_value_to_string_with_options(&payload, format, OutputOrder::Default, table_input)?;
    Ok((text, notes))
}

/// Render `value` to stdout in `format` (with colors on a terminal).
fn print_value(
    value: &serde_json::Value,
    format: &OutputFormat,
    output_order: OutputOrder,
    table_input: TableInput<'_>,
) -> Result<()> {
    let capabilities = stdout_terminal_capabilities();
    if capabilities.colors
        && matches!(
            format,
            OutputFormat::Json | OutputFormat::Yaml | OutputFormat::Table
        )
    {
        print_formatted(value, format, capabilities, output_order, table_input)?;
    } else {
        match output_order {
            OutputOrder::Default => match format {
                OutputFormat::Json => print_json(value),
                OutputFormat::Yaml => print_yaml(value),
                OutputFormat::Table => print_table_with_options(value, table_input),
                OutputFormat::Csv => print_csv(value, table_input),
                OutputFormat::Tsv => print_tsv(value, table_input),
            }?,
            OutputOrder::Preserve => {
                let rendered = format_value_to_string_with_options(
                    value,
                    format,
                    OutputOrder::Preserve,
                    table_input,
                )?;
                if !rendered.is_empty() {
                    print!("{rendered}");
                    if !rendered.ends_with('\n') {
                        println!();
                    }
                }
            }
        }
    }

    #[cfg(not(feature = "browser"))]
    if crate::rate_limit::verbose_enabled() {
        crate::rate_limit::eprint_verbose_response(format)?;
    }

    Ok(())
}

fn effective_output_order(
    format: &OutputFormat,
    jq: Option<&str>,
    output_order: OutputOrder,
) -> OutputOrder {
    if jq.is_some() && *format == OutputFormat::Table {
        OutputOrder::Preserve
    } else {
        output_order
    }
}

fn effective_table_input<'a>(
    format: &OutputFormat,
    jq: Option<&str>,
    table_input: TableInput<'a>,
) -> TableInput<'a> {
    let row_format = matches!(
        format,
        OutputFormat::Table | OutputFormat::Csv | OutputFormat::Tsv
    );
    if jq.is_some() && row_format {
        // A jq expression explicitly reshapes the response. Render that result as-is
        // instead of applying the command's row hints to a now-different shape.
        TableInput::AsIs
    } else {
        table_input
    }
}

/// Format query results without changing the caller's object-key order.
pub fn output_preserving_order<T: Serialize>(cfg: &crate::config::Config, data: &T) -> Result<()> {
    format_and_print_with_order(
        data,
        &cfg.output_format,
        cfg.jq.as_deref(),
        None,
        TableInput::AsIs,
        OutputOrder::Preserve,
    )
}

pub fn print_json(data: &serde_json::Value) -> Result<()> {
    let sorted_data = sort_json_value(data.clone());
    let json = go_html_escape(&serde_json::to_string_pretty(&sorted_data)?);
    println!("{json}");
    Ok(())
}

/// Render a JSON value to a string using the selected output format.
pub fn format_value_to_string(data: &serde_json::Value, format: &OutputFormat) -> Result<String> {
    format_value_to_string_with_options(data, format, OutputOrder::Default, TableInput::Generic)
}

fn format_value_to_string_with_options(
    data: &serde_json::Value,
    format: &OutputFormat,
    output_order: OutputOrder,
    table_input: TableInput<'_>,
) -> Result<String> {
    match format {
        OutputFormat::Json => {
            let ordered_data = order_json_value(data, output_order);
            Ok(go_html_escape(&serde_json::to_string_pretty(
                &ordered_data,
            )?))
        }
        OutputFormat::Yaml => {
            let ordered_data = order_json_value(data, output_order);
            Ok(serde_norway::to_string(&ordered_data)?)
        }
        OutputFormat::Table => format_table_to_string_with_options(data, output_order, table_input),
        OutputFormat::Csv => format_delimited(data, output_order, table_input, Delimited::Csv),
        OutputFormat::Tsv => format_delimited(data, output_order, table_input, Delimited::Tsv),
    }
}

/// Format and print a JSON value to stderr (same renderers as stdout).
pub fn eprint_formatted(data: &serde_json::Value, format: &OutputFormat) -> Result<()> {
    let rendered = format_value_to_string(data, format)?;
    eprintln!("{rendered}");
    Ok(())
}

fn print_formatted(
    data: &serde_json::Value,
    format: &OutputFormat,
    capabilities: TerminalCapabilities,
    output_order: OutputOrder,
    table_input: TableInput<'_>,
) -> Result<()> {
    let rendered = if *format == OutputFormat::Table {
        format_table_with_options(data, output_order, table_input, capabilities)?
    } else {
        format_value_to_string_with_options(data, format, output_order, table_input)?
    };
    let Some(rendered) = visible_output(rendered, format) else {
        return Ok(());
    };

    let rendered = highlight_output(rendered, format, capabilities);
    if *format == OutputFormat::Yaml {
        print!("{rendered}");
    } else {
        println!("{rendered}");
    }
    Ok(())
}

fn visible_output(rendered: String, format: &OutputFormat) -> Option<String> {
    if rendered.is_empty() {
        return (*format == OutputFormat::Table).then(|| NO_RESULTS.to_string());
    }
    Some(rendered)
}

fn highlight_output(
    rendered: String,
    format: &OutputFormat,
    capabilities: TerminalCapabilities,
) -> String {
    if !capabilities.colors {
        return rendered;
    }

    match format {
        OutputFormat::Json => highlight_json(&rendered),
        OutputFormat::Yaml => highlight_yaml(&rendered),
        OutputFormat::Table | OutputFormat::Csv | OutputFormat::Tsv => rendered,
    }
}

fn push_styled(output: &mut String, style: &str, value: &str) {
    output.push_str(style);
    output.push_str(value);
    output.push_str(ANSI_RESET);
}

// Pup only highlights output it generated itself. These scanners intentionally
// recognize the common structures emitted by serde_json and serde_norway rather
// than implementing general-purpose JSON and YAML parsers.
fn highlight_json(json: &str) -> String {
    let bytes = json.as_bytes();
    let mut output = String::with_capacity(json.len() + json.len() / 4);
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let start = index;
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => index = (index + 2).min(bytes.len()),
                        b'"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
                let style = if json[index..].trim_start().starts_with(':') {
                    ANSI_BOLD_CYAN
                } else {
                    ANSI_GREEN
                };
                push_styled(&mut output, style, &json[start..index]);
            }
            b'-' | b'0'..=b'9' => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && matches!(bytes[index], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    index += 1;
                }
                push_styled(&mut output, ANSI_YELLOW, &json[start..index]);
            }
            b't' if json[index..].starts_with("true") => {
                push_styled(&mut output, ANSI_MAGENTA, "true");
                index += 4;
            }
            b'f' if json[index..].starts_with("false") => {
                push_styled(&mut output, ANSI_MAGENTA, "false");
                index += 5;
            }
            b'n' if json[index..].starts_with("null") => {
                push_styled(&mut output, ANSI_GRAY, "null");
                index += 4;
            }
            b'{' | b'}' | b'[' | b']' | b',' | b':' => {
                push_styled(&mut output, ANSI_DIM, &json[index..index + 1]);
                index += 1;
            }
            _ => {
                let character = json[index..]
                    .chars()
                    .next()
                    .expect("index is within the string");
                output.push(character);
                index += character.len_utf8();
            }
        }
    }
    output
}

fn highlight_yaml(yaml: &str) -> String {
    let mut output = String::with_capacity(yaml.len() + yaml.len() / 4);
    let mut block_scalar_indent = None;
    for line in yaml.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let body = content.trim_start();
        let indent = content.len() - body.len();
        if block_scalar_indent
            .is_some_and(|parent_indent| body.is_empty() || indent > parent_indent)
        {
            output.push_str(content);
        } else {
            block_scalar_indent = highlight_yaml_line(&mut output, content).then_some(indent);
        }
        if line.ends_with('\n') {
            output.push('\n');
        }
    }
    output
}

fn highlight_yaml_line(output: &mut String, line: &str) -> bool {
    let body = line.trim_start();
    output.push_str(&line[..line.len() - body.len()]);

    let body = if let Some(rest) = body.strip_prefix("- ") {
        push_styled(output, ANSI_DIM, "-");
        output.push(' ');
        rest
    } else {
        body
    };

    if matches!(body, "---" | "...") {
        push_styled(output, ANSI_DIM, body);
        return false;
    }

    if let Some(colon) = find_yaml_mapping_colon(body) {
        push_styled(output, ANSI_BOLD_CYAN, &body[..colon]);
        push_styled(output, ANSI_DIM, ":");
        highlight_yaml_scalar(output, &body[colon + 1..])
    } else {
        highlight_yaml_scalar(output, body)
    }
}

fn find_yaml_mapping_colon(value: &str) -> Option<usize> {
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut escaped = false;
    for (index, character) in value.char_indices() {
        if double_quoted && character == '\\' && !escaped {
            escaped = true;
            continue;
        }
        if character == '"' && !single_quoted && !escaped {
            double_quoted = !double_quoted;
        } else if character == '\'' && !double_quoted {
            single_quoted = !single_quoted;
        } else if character == ':' && !single_quoted && !double_quoted {
            let rest = &value[index + 1..];
            if rest.is_empty() || rest.starts_with(char::is_whitespace) {
                return Some(index);
            }
        }
        escaped = false;
    }
    None
}

fn highlight_yaml_scalar(output: &mut String, scalar: &str) -> bool {
    let value = scalar.trim_start();
    output.push_str(&scalar[..scalar.len() - value.len()]);
    if value.is_empty() {
        return false;
    }

    let is_block_scalar = is_yaml_block_scalar(value);
    let style = match value {
        "null" | "Null" | "NULL" | "~" => ANSI_GRAY,
        "true" | "True" | "TRUE" | "false" | "False" | "FALSE" => ANSI_MAGENTA,
        _ if value.parse::<f64>().is_ok_and(|number| number.is_finite()) => ANSI_YELLOW,
        _ if is_block_scalar => ANSI_MAGENTA,
        _ => ANSI_GREEN,
    };
    push_styled(output, style, value);
    is_block_scalar
}

fn is_yaml_block_scalar(value: &str) -> bool {
    let Some(indicator) = value.split_ascii_whitespace().next() else {
        return false;
    };
    let mut characters = indicator.chars();
    if !matches!(characters.next(), Some('|' | '>')) {
        return false;
    }

    let mut has_chomping = false;
    let mut has_indent = false;
    characters.all(|character| match character {
        '+' | '-' if !has_chomping => {
            has_chomping = true;
            true
        }
        '1'..='9' if !has_indent => {
            has_indent = true;
            true
        }
        _ => false,
    })
}

fn print_yaml(data: &serde_json::Value) -> Result<()> {
    let sorted_data = sort_json_value(data.clone());
    let yaml = serde_norway::to_string(&sorted_data)?;
    print!("{yaml}");
    Ok(())
}

#[cfg(test)]
fn format_table_to_string(data: &serde_json::Value) -> Result<String> {
    format_table_to_string_with_options(data, OutputOrder::Default, TableInput::Generic)
}

fn format_table_to_string_with_options(
    data: &serde_json::Value,
    output_order: OutputOrder,
    table_input: TableInput<'_>,
) -> Result<String> {
    format_table_with_options(
        data,
        output_order,
        table_input,
        TerminalCapabilities::default(),
    )
}

fn format_table_with_options(
    data: &serde_json::Value,
    output_order: OutputOrder,
    table_input: TableInput<'_>,
    capabilities: TerminalCapabilities,
) -> Result<String> {
    let table_data = select_table_data(data, table_input)?;
    let has_row_hints = table_input.has_row_hints();
    match table_data {
        serde_json::Value::Array(_) if has_row_hints => {
            format_horizontal_table(table_data, output_order, table_input, capabilities)
        }
        serde_json::Value::Array(values)
            if values
                .iter()
                .all(|value| !matches!(value, serde_json::Value::Object(_))) =>
        {
            format_scalar_table(values.iter(), capabilities)
        }
        serde_json::Value::Array(_) => {
            format_horizontal_table(table_data, output_order, table_input, capabilities)
        }
        serde_json::Value::Object(_) if has_row_hints => {
            format_horizontal_table(table_data, output_order, table_input, capabilities)
        }
        serde_json::Value::Object(_) => format_vertical_table(table_data, capabilities),
        _ if has_row_hints => {
            anyhow::bail!("table row and column hints require an array or object response")
        }
        value => format_scalar_table(std::iter::once(value), capabilities),
    }
}

fn select_table_data<'a>(
    data: &'a serde_json::Value,
    table_input: TableInput<'_>,
) -> Result<&'a serde_json::Value> {
    match table_input {
        TableInput::Generic => Ok(unwrap_data(data)),
        TableInput::Explicit(options) => match options.rows_at {
            Some(pointer) => data.pointer(pointer).ok_or_else(|| {
                anyhow::anyhow!("table row path {pointer:?} was not found in the response")
            }),
            None => Ok(data),
        },
        TableInput::AsIs => Ok(data),
    }
}

fn format_horizontal_table(
    data: &serde_json::Value,
    output_order: OutputOrder,
    table_input: TableInput<'_>,
    capabilities: TerminalCapabilities,
) -> Result<String> {
    let raw_rows = match data {
        serde_json::Value::Array(rows) => rows.iter().collect(),
        serde_json::Value::Object(_) => vec![data],
        _ => Vec::new(),
    };
    let selected_rows = select_table_rows(raw_rows, table_input)?;
    if selected_rows.is_empty() {
        return Ok(NO_RESULTS.to_string());
    }

    let requested_columns = table_input
        .options()
        .map(|options| options.columns)
        .unwrap_or_default();
    if !requested_columns.is_empty() {
        let headers = select_requested_headers(&selected_rows, requested_columns)?;
        return Ok(render_horizontal_rows(
            &selected_rows,
            &headers,
            flattened_value,
            capabilities,
        ));
    }

    let owned_rows: Vec<serde_json::Value> =
        selected_rows.iter().map(|row| flatten_row(row)).collect();
    let rows: Vec<&serde_json::Value> = owned_rows.iter().collect();
    let final_headers = if output_order == OutputOrder::Default {
        select_list_headers(&rows, 12)
    } else {
        collect_headers(&rows).0.into_iter().take(12).collect()
    };
    if final_headers.is_empty() {
        return format_scalar_table(selected_rows, capabilities);
    }

    Ok(render_horizontal_rows(
        &rows,
        &final_headers,
        object_value,
        capabilities,
    ))
}

fn render_horizontal_rows(
    rows: &[&serde_json::Value],
    headers: &[String],
    value_at: for<'a> fn(&'a serde_json::Value, &str) -> Option<&'a serde_json::Value>,
    capabilities: TerminalCapabilities,
) -> String {
    let mut table = comfy_table::Table::new();
    table.set_header(
        headers
            .iter()
            .map(|header| table_header_cell(header, capabilities.colors))
            .collect::<Vec<_>>(),
    );

    for row in rows {
        let cells: Vec<comfy_table::Cell> = headers
            .iter()
            .map(|header| table_cell(value_at(row, header), header, capabilities))
            .collect();
        table.add_row(cells);
    }

    table.to_string()
}

fn select_table_rows<'a>(
    rows: Vec<&'a serde_json::Value>,
    table_input: TableInput<'_>,
) -> Result<Vec<&'a serde_json::Value>> {
    let Some(pointer) = table_input.options().and_then(|options| options.row_at) else {
        return Ok(rows);
    };
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            row.pointer(pointer).ok_or_else(|| {
                anyhow::anyhow!("table row path {pointer:?} was not found in result row {index}")
            })
        })
        .collect()
}

fn select_requested_headers(
    rows: &[&serde_json::Value],
    requested: &[&str],
) -> Result<Vec<String>> {
    let mut selected = Vec::new();
    for column in requested {
        if rows
            .iter()
            .any(|row| flattened_value(row, column).is_some())
            && !selected.iter().any(|item| item == column)
        {
            selected.push((*column).to_string());
        }
    }
    if selected.is_empty() {
        anyhow::bail!(
            "none of the requested table columns are present in the response: {}",
            requested.join(", ")
        );
    }
    Ok(selected)
}

fn flattened_value<'a>(row: &'a serde_json::Value, column: &str) -> Option<&'a serde_json::Value> {
    let object = row.as_object()?;
    if let Some(value) = object.get(column) {
        return Some(value);
    }

    let mut value = row;
    for part in column.split('.') {
        value = value.as_object()?.get(part)?;
    }
    Some(value)
}

fn object_value<'a>(row: &'a serde_json::Value, column: &str) -> Option<&'a serde_json::Value> {
    row.as_object()?.get(column)
}

fn format_vertical_table(
    data: &serde_json::Value,
    capabilities: TerminalCapabilities,
) -> Result<String> {
    let flat = flatten_row(data);
    let Some(fields) = flat.as_object() else {
        return Ok("No results found".to_string());
    };
    if fields.is_empty() {
        return Ok("No results found".to_string());
    }

    let mut table = comfy_table::Table::new();
    table.set_header([
        table_header_cell("FIELD", capabilities.colors),
        table_header_cell("VALUE", capabilities.colors),
    ]);
    for (field, value) in fields {
        table.add_row([
            comfy_table::Cell::new(field),
            table_cell(Some(value), field, capabilities),
        ]);
    }
    Ok(table.to_string())
}

fn format_scalar_table<'a>(
    values: impl IntoIterator<Item = &'a serde_json::Value>,
    capabilities: TerminalCapabilities,
) -> Result<String> {
    let mut table = comfy_table::Table::new();
    table.set_header([table_header_cell("VALUE", capabilities.colors)]);
    let mut has_values = false;
    for value in values {
        table.add_row([table_cell(Some(value), "", capabilities)]);
        has_values = true;
    }
    if !has_values {
        return Ok("No results found".to_string());
    }
    Ok(table.to_string())
}

fn collect_headers(
    rows: &[&serde_json::Value],
) -> (Vec<String>, std::collections::HashSet<String>) {
    let mut headers = Vec::new();
    let mut header_set = std::collections::HashSet::new();
    for row in rows {
        if let serde_json::Value::Object(map) = row {
            for key in map.keys() {
                if header_set.insert(key.clone()) {
                    headers.push(key.clone());
                }
            }
        }
    }
    (headers, header_set)
}

fn select_list_headers(rows: &[&serde_json::Value], max: usize) -> Vec<String> {
    let (headers, header_set) = collect_headers(rows);
    let priority = [
        "id",
        "public_id",
        "title",
        "name",
        "type",
        "overall_state",
        "status",
        "state",
        "severity",
        "created_at",
        "updated_at",
        "created",
        "modified",
        "attributes.timestamp",
        "attributes.service",
        "attributes.host",
        "attributes.status",
        "attributes.message",
    ];
    let mut final_headers = Vec::new();
    for &p in &priority {
        if final_headers.len() >= max {
            break;
        }
        if header_set.contains(p) {
            final_headers.push(p.to_string());
        }
    }
    for header in headers {
        if final_headers.len() >= max {
            break;
        }
        if !final_headers.contains(&header) {
            final_headers.push(header);
        }
    }
    final_headers
}

fn table_header_cell(header: &str, colors: bool) -> comfy_table::Cell {
    let cell = comfy_table::Cell::new(header);
    #[cfg(feature = "native")]
    if colors {
        return cell
            .fg(comfy_table::Color::Cyan)
            .add_attribute(comfy_table::Attribute::Bold);
    }
    #[cfg(not(feature = "native"))]
    let _ = colors;
    cell
}

// Table colors are best-effort: JSON value types are exact, while semantic
// states are recognized from a small set of conventional columns and values.
#[cfg(feature = "native")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableTone {
    Success,
    Warning,
    Error,
    Number,
    Boolean,
    Muted,
    Identifier,
}

fn table_cell(
    value: Option<&serde_json::Value>,
    header: &str,
    capabilities: TerminalCapabilities,
) -> comfy_table::Cell {
    let display = format_cell(value);
    #[cfg(feature = "native")]
    let tone = capabilities
        .colors
        .then(|| table_cell_tone(header, value, &display))
        .flatten();
    let display = hyperlink_table_cell(value, display, capabilities.hyperlinks);
    let cell = comfy_table::Cell::new_owned(display);

    #[cfg(feature = "native")]
    if let Some(tone) = tone {
        let color = match tone {
            TableTone::Success => comfy_table::Color::Green,
            TableTone::Warning | TableTone::Number => comfy_table::Color::Yellow,
            TableTone::Error => comfy_table::Color::Red,
            TableTone::Boolean => comfy_table::Color::Magenta,
            TableTone::Muted => comfy_table::Color::DarkGrey,
            TableTone::Identifier => comfy_table::Color::Cyan,
        };
        let mut cell = cell.fg(color);
        if matches!(
            tone,
            TableTone::Success | TableTone::Warning | TableTone::Error
        ) {
            cell = cell.add_attribute(comfy_table::Attribute::Bold);
        } else if tone == TableTone::Muted {
            cell = cell.add_attribute(comfy_table::Attribute::Dim);
        }
        return cell;
    }

    #[cfg(not(feature = "native"))]
    let _ = (header, capabilities);
    cell
}

fn hyperlink_table_cell(
    value: Option<&serde_json::Value>,
    display: String,
    enabled: bool,
) -> String {
    if !enabled {
        return display;
    }
    let Some(serde_json::Value::String(raw_target)) = value else {
        return display;
    };
    let Some(target) = safe_http_url(raw_target) else {
        return display;
    };
    // OSC 8 keeps the complete target separate from the displayed label.
    // comfy_table's custom_styling feature ignores these control bytes when
    // measuring the cell, so a long target cannot distort the table layout.
    format!(
        "{OSC8_OPEN}{}{OSC_TERMINATOR}{display}{OSC8_CLOSE}",
        target.as_str()
    )
}

fn safe_http_url(value: &str) -> Option<reqwest::Url> {
    let (scheme, _) = value.split_once(':')?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    if value
        .chars()
        .any(|character| character.is_control() || character.is_whitespace())
    {
        return None;
    }
    let Ok(parsed) = reqwest::Url::parse(value) else {
        return None;
    };
    // User info can hide the real host past a truncated, trusted-looking label.
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.has_host()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    Some(parsed)
}

#[cfg(feature = "native")]
fn table_cell_tone(
    header: &str,
    value: Option<&serde_json::Value>,
    display: &str,
) -> Option<TableTone> {
    let field = header.rsplit('.').next();
    if matches!(
        field,
        Some("overall_state" | "status" | "state" | "severity")
    ) {
        if let Some(tone) = semantic_table_tone(display) {
            return Some(tone);
        }
    }
    if matches!(field, Some("id" | "public_id"))
        && !matches!(value, None | Some(serde_json::Value::Null))
    {
        return Some(TableTone::Identifier);
    }

    match value {
        None | Some(serde_json::Value::Null) => Some(TableTone::Muted),
        Some(serde_json::Value::Number(_)) => Some(TableTone::Number),
        Some(serde_json::Value::Bool(_)) => Some(TableTone::Boolean),
        _ => None,
    }
}

#[cfg(feature = "native")]
fn semantic_table_tone(value: &str) -> Option<TableTone> {
    let normalized = value.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    match normalized.as_str() {
        "ok" | "success" | "healthy" | "active" | "enabled" | "resolved" | "completed"
        | "published" | "passing" | "up" => Some(TableTone::Success),
        "warn" | "warning" | "pending" | "degraded" | "unstable" | "muted" | "no_data"
        | "draft" | "skipped" => Some(TableTone::Warning),
        "error" | "failed" | "failure" | "alert" | "critical" | "down" | "unhealthy"
        | "triggered" => Some(TableTone::Error),
        "null" | "none" | "unknown" | "disabled" | "—" => Some(TableTone::Muted),
        _ => None,
    }
}

/// Flatten up to two levels of nested objects into dot-notation keys.
/// e.g. {"id": "x", "attributes": {"host": "foo", "tags": {"env": "prod"}}}
///   → {"id": "x", "attributes.host": "foo", "attributes.tags.env": "prod"}
fn flatten_row(value: &serde_json::Value) -> serde_json::Value {
    if let serde_json::Value::Object(map) = value {
        let mut flat = serde_json::Map::new();
        for (k, v) in map {
            if let serde_json::Value::Object(inner) = v {
                if inner.is_empty() {
                    flat.insert(k.clone(), v.clone());
                    continue;
                }
                for (ik, iv) in inner {
                    if let serde_json::Value::Object(inner2) = iv {
                        if inner2.is_empty() {
                            flat.insert(format!("{k}.{ik}"), iv.clone());
                            continue;
                        }
                        for (iik, iiv) in inner2 {
                            flat.insert(format!("{k}.{ik}.{iik}"), iiv.clone());
                        }
                    } else {
                        flat.insert(format!("{k}.{ik}"), iv.clone());
                    }
                }
            } else {
                flat.insert(k.clone(), v.clone());
            }
        }
        serde_json::Value::Object(flat)
    } else {
        value.clone()
    }
}

#[cfg(test)]
fn print_table(data: &serde_json::Value) -> Result<()> {
    print_table_with_options(data, TableInput::Generic)
}

fn print_table_with_options(data: &serde_json::Value, table_input: TableInput<'_>) -> Result<()> {
    println!(
        "{}",
        format_table_to_string_with_options(data, OutputOrder::Default, table_input)?
    );
    Ok(())
}

/// Recursively flatten a JSON object to dot-notation keys at any depth.
/// e.g. {"a": {"b": {"c": 1}}} → {"a.b.c": 1}
fn flatten_deep(
    value: &serde_json::Value,
    prefix: &str,
    out: &mut serde_json::Map<String, serde_json::Value>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten_deep(v, &key, out);
            }
        }
        _ => {
            out.insert(prefix.to_string(), value.clone());
        }
    }
}

/// Escape a single CSV field: wrap in quotes if it contains commas, quotes, or newlines.
/// Double any embedded double-quote characters.
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Render a JSON value as a plain string for CSV output (no truncation).
fn csv_cell(value: Option<&serde_json::Value>) -> String {
    match value {
        None | Some(serde_json::Value::Null) => String::new(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        Some(other) => other.to_string(),
    }
}

#[derive(Clone, Copy)]
enum Delimited {
    Csv,
    Tsv,
}

impl Delimited {
    fn separator(self) -> &'static str {
        match self {
            Self::Csv => ",",
            Self::Tsv => "\t",
        }
    }

    fn escape(self, s: &str) -> String {
        match self {
            Self::Csv => csv_escape(s),
            Self::Tsv => tsv_escape(s),
        }
    }
}

/// Escape a single TSV field: literal tab characters in values are replaced with \t.
/// No quoting is applied.
fn tsv_escape(s: &str) -> String {
    s.replace('\t', "\\t")
}

/// Rows for CSV/TSV, chosen exactly like the table: the command's `rows_at` /
/// `row_at` hints, the JSON:API `data` array for generic output, and the
/// `--jq` result as-is.
fn delimited_rows<'a>(
    data: &'a serde_json::Value,
    table_input: TableInput<'_>,
) -> Result<Vec<&'a serde_json::Value>> {
    let selected = select_table_data(data, table_input)?;
    let rows = match selected {
        serde_json::Value::Array(arr) => arr.iter().collect(),
        serde_json::Value::Object(_) => vec![selected],
        _ => vec![],
    };
    select_table_rows(rows, table_input)
}

/// Render rows as CSV or TSV. Without column hints every field is flattened to
/// a dot-notation column; with the command's columns (as for the table) only
/// those columns are printed, in that order.
fn format_delimited(
    data: &serde_json::Value,
    output_order: OutputOrder,
    table_input: TableInput<'_>,
    kind: Delimited,
) -> Result<String> {
    let raw_rows = delimited_rows(data, table_input)?;
    if raw_rows.is_empty() {
        return Ok(String::new());
    }

    let requested = table_input
        .options()
        .map(|options| options.columns)
        .unwrap_or_default();
    let (headers, cells): (Vec<String>, Vec<Vec<String>>) = if requested.is_empty() {
        let flat_rows: Vec<serde_json::Map<String, serde_json::Value>> = raw_rows
            .iter()
            .map(|r| {
                let mut out = serde_json::Map::new();
                flatten_deep(r, "", &mut out);
                out
            })
            .collect();
        let mut header_set = std::collections::HashSet::new();
        let mut headers: Vec<String> = Vec::new();
        for row in &flat_rows {
            for key in row.keys() {
                if header_set.insert(key.clone()) {
                    headers.push(key.clone());
                }
            }
        }
        if output_order == OutputOrder::Default {
            headers.sort();
        }
        let cells = flat_rows
            .iter()
            .map(|row| {
                headers
                    .iter()
                    .map(|h| csv_cell(row.get(h.as_str())))
                    .collect()
            })
            .collect();
        (headers, cells)
    } else {
        let headers = select_requested_headers(&raw_rows, requested)?;
        let cells = raw_rows
            .iter()
            .map(|row| {
                headers
                    .iter()
                    .map(|h| csv_cell(flattened_value(row, h)))
                    .collect()
            })
            .collect();
        (headers, cells)
    };

    let join = |fields: Vec<String>| {
        fields
            .iter()
            .map(|f| kind.escape(f))
            .collect::<Vec<_>>()
            .join(kind.separator())
    };
    let mut lines = vec![join(headers)];
    lines.extend(cells.into_iter().map(join));
    Ok(lines.join("\n"))
}

fn print_csv(data: &serde_json::Value, table_input: TableInput<'_>) -> Result<()> {
    let rendered = format_delimited(data, OutputOrder::Default, table_input, Delimited::Csv)?;
    if !rendered.is_empty() {
        println!("{rendered}");
    }
    Ok(())
}

fn print_tsv(data: &serde_json::Value, table_input: TableInput<'_>) -> Result<()> {
    let rendered = format_delimited(data, OutputOrder::Default, table_input, Delimited::Tsv)?;
    if !rendered.is_empty() {
        println!("{rendered}");
    }
    Ok(())
}

fn unwrap_data(value: &serde_json::Value) -> &serde_json::Value {
    match value {
        serde_json::Value::Object(map) => map.get("data").map(unwrap_data).unwrap_or(value),
        _ => value,
    }
}

/// Truncate `s` to at most `max` characters, appending an ellipsis when shortened.
/// Cuts on character boundaries so multi-byte UTF-8 text never panics.
fn truncate_ellipsis(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        if max == 0 {
            return String::new();
        }
        let keep: String = s.chars().take(max - 1).collect();
        format!("{keep}…")
    } else {
        s.to_string()
    }
}

/// Compact label for a single array element, used when previewing arrays in table cells.
/// For objects, prefers a recognizable label; falls back to a field count.
fn format_array_item(value: &serde_json::Value) -> String {
    if let serde_json::Value::Object(map) = value {
        for key in &["name", "title", "id", "type"] {
            if let Some(serde_json::Value::String(label)) = map.get(*key) {
                return truncate_ellipsis(label, 16);
            }
        }
        return field_count(map.len());
    }
    format_cell(Some(value))
}

fn format_cell(value: Option<&serde_json::Value>) -> String {
    match value {
        None => String::new(),
        Some(serde_json::Value::Null) => "—".to_string(),
        Some(serde_json::Value::String(s)) => truncate_ellipsis(s, 50),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        Some(serde_json::Value::Array(arr)) => {
            if arr.is_empty() {
                return "[]".to_string();
            }
            let mut parts: Vec<String> = arr.iter().take(4).map(format_array_item).collect();
            if arr.len() > 4 {
                parts.push(format!("+{} more", arr.len() - 4));
            }
            let result = format!("[{}]", parts.join(", "));
            truncate_ellipsis(&result, 50)
        }
        Some(serde_json::Value::Object(map)) => field_count(map.len()),
    }
}

fn field_count(count: usize) -> String {
    format!(
        "{{{count} {}}}",
        if count == 1 { "field" } else { "fields" }
    )
}

/// Format an API error with contextual guidance.
#[allow(dead_code)]
pub fn format_api_error(operation: &str, status: Option<u16>, body: Option<&str>) -> String {
    let mut msg = format!("failed to {operation}");

    if let Some(code) = status {
        msg.push_str(&format!(" (HTTP {code})"));
    }

    if let Some(body) = body {
        if !body.is_empty() {
            msg.push_str(&format!("\nAPI response: {body}"));
        }
    }

    if let Some(code) = status {
        let hint = match code {
            500.. => "API server error — try again later",
            429 => "rate limited — wait and retry",
            403 => "access denied — check permissions",
            401 => "authentication failed — check credentials or run 'pup auth login'",
            404 => "resource not found — verify the ID",
            400 => "invalid request — check parameters",
            _ => "",
        };
        if !hint.is_empty() {
            msg.push_str(&format!("\nHint: {hint}"));
        }
    }

    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_test_colors(value: &str) -> String {
        [
            ANSI_RESET,
            ANSI_BOLD_CYAN,
            ANSI_GREEN,
            ANSI_YELLOW,
            ANSI_MAGENTA,
            ANSI_DIM,
            ANSI_GRAY,
        ]
        .into_iter()
        .fold(value.to_string(), |plain, style| plain.replace(style, ""))
    }

    const COLORS: TerminalCapabilities = TerminalCapabilities {
        colors: true,
        hyperlinks: false,
    };
    const INTERACTIVE: TerminalCapabilities = TerminalCapabilities {
        colors: true,
        hyperlinks: true,
    };

    fn format_table_with_capabilities(
        data: &serde_json::Value,
        capabilities: TerminalCapabilities,
    ) -> String {
        format_table_with_options(
            data,
            OutputOrder::Default,
            TableInput::Generic,
            capabilities,
        )
        .unwrap()
    }

    fn strip_test_links(value: &str) -> String {
        let mut plain = String::with_capacity(value.len());
        let mut remainder = value;
        while let Some(open) = remainder.find(OSC8_OPEN) {
            plain.push_str(&remainder[..open]);
            let target = &remainder[open + OSC8_OPEN.len()..];
            let Some(label_start) = target.find(OSC_TERMINATOR) else {
                plain.push_str(&remainder[open..]);
                return plain;
            };
            let label = &target[label_start + OSC_TERMINATOR.len()..];
            let Some(close) = label.find(OSC8_CLOSE) else {
                plain.push_str(&remainder[open..]);
                return plain;
            };
            plain.push_str(&label[..close]);
            remainder = &label[close + OSC8_CLOSE.len()..];
        }
        plain.push_str(remainder);
        plain
    }

    #[test]
    fn test_color_enabled_only_for_capable_terminal() {
        let no_color = std::ffi::OsStr::new("1");
        let empty_no_color = std::ffi::OsStr::new("");

        assert!(color_enabled(true, None, None, Some("xterm-256color")));
        assert!(color_enabled(
            true,
            Some(empty_no_color),
            None,
            Some("xterm-256color")
        ));
        assert!(!color_enabled(false, None, None, Some("xterm-256color")));
        assert!(!color_enabled(
            true,
            Some(no_color),
            None,
            Some("xterm-256color")
        ));
        assert!(!color_enabled(true, None, Some("0"), None));
        assert!(!color_enabled(true, None, None, Some("dumb")));
    }

    #[test]
    fn test_hyperlinks_require_terminal_styling_and_allow_opt_out() {
        assert!(hyperlinks_enabled(true, None));
        assert!(!hyperlinks_enabled(false, None));
        assert!(!hyperlinks_enabled(true, Some("0")));
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_table_tones_use_types_and_common_field_names() {
        assert_eq!(semantic_table_tone("OK"), Some(TableTone::Success));
        assert_eq!(semantic_table_tone("No Data"), Some(TableTone::Warning));
        assert_eq!(semantic_table_tone("critical"), Some(TableTone::Error));
        assert_eq!(semantic_table_tone("custom"), None);

        assert_eq!(
            table_cell_tone("id", Some(&serde_json::json!("abc")), "abc"),
            Some(TableTone::Identifier)
        );
        assert_eq!(
            table_cell_tone("count", Some(&serde_json::json!(3)), "3"),
            Some(TableTone::Number)
        );
        assert_eq!(
            table_cell_tone("message", Some(&serde_json::json!("critical")), "critical"),
            None
        );
    }

    #[cfg(feature = "native")]
    #[test]
    fn test_table_cells_apply_styles_only_when_enabled() {
        assert_eq!(
            table_header_cell("status", COLORS.colors),
            comfy_table::Cell::new("status")
                .fg(comfy_table::Color::Cyan)
                .add_attribute(comfy_table::Attribute::Bold)
        );

        let value = serde_json::json!("Alert");
        assert_eq!(
            table_cell(Some(&value), "status", COLORS),
            comfy_table::Cell::new("Alert")
                .fg(comfy_table::Color::Red)
                .add_attribute(comfy_table::Attribute::Bold)
        );
        assert_eq!(
            table_cell(Some(&value), "status", TerminalCapabilities::default()),
            comfy_table::Cell::new("Alert")
        );
    }

    #[test]
    fn test_table_links_use_full_target_for_truncated_labels() {
        let target = "https://secure.gravatar.com/avatar/7cb8c2243893c5d246db0f80a5e9c834?size=128";
        let data = serde_json::json!({"author_icon": target});
        let plain = format_table_to_string(&data).unwrap();
        let linked = format_table_with_capabilities(&data, INTERACTIVE);

        assert!(!plain.contains(target));
        assert!(plain.contains('…'));
        assert!(linked.contains(&format!("{OSC8_OPEN}{target}{OSC_TERMINATOR}")));
        assert_eq!(strip_test_colors(&strip_test_links(&linked)), plain);
    }

    #[test]
    fn test_table_links_cover_scalar_vertical_and_horizontal_layouts() {
        let target = "https://example.com/resource?id=42";
        for data in [
            serde_json::json!(target),
            serde_json::json!({"url": target}),
            serde_json::json!([{"url": target}]),
        ] {
            let linked = format_table_with_capabilities(&data, INTERACTIVE);
            assert!(linked.contains(&format!("{OSC8_OPEN}{target}{OSC_TERMINATOR}")));
            assert_eq!(
                strip_test_colors(&strip_test_links(&linked)),
                format_table_to_string(&data).unwrap()
            );
        }
    }

    #[test]
    fn test_table_links_ignore_urls_embedded_in_other_values() {
        let target = "https://example.com/resource";
        let data = serde_json::json!({
            "description": format!("See {target} for details"),
            "links": [target]
        });

        assert!(!format_table_with_capabilities(&data, INTERACTIVE).contains(OSC8_OPEN));
    }

    #[test]
    fn test_table_links_reject_unsafe_or_non_http_targets() {
        for target in [
            "javascript:alert(1)",
            "https://",
            "https://example.com/bad value",
            "https://example.com/\u{1b}]8;;https://evil.example",
        ] {
            assert!(safe_http_url(target).is_none(), "accepted {target:?}");
        }
    }

    #[test]
    fn test_table_links_emit_normalized_targets() {
        for (target, normalized) in [
            ("https://example.com/café", "https://example.com/caf%C3%A9"),
            ("http://example.com\\path", "http://example.com/path"),
        ] {
            assert_eq!(safe_http_url(target).unwrap().as_str(), normalized);

            let data = serde_json::json!({"url": target});
            let linked = format_table_with_capabilities(&data, INTERACTIVE);

            assert!(linked.contains(&format!("{OSC8_OPEN}{normalized}{OSC_TERMINATOR}")));
            assert_eq!(
                strip_test_colors(&strip_test_links(&linked)),
                format_table_to_string(&data).unwrap()
            );
        }
    }

    #[test]
    fn test_table_links_reject_user_info_urls() {
        for target in [
            "https://app.datadoghq.com........................................@evil.example/path",
            "https://user:password@example.com/path",
            "https://:password@example.com/path",
        ] {
            let data = serde_json::json!({"url": target});
            let rendered = format_table_with_capabilities(&data, INTERACTIVE);

            assert!(!rendered.contains(OSC8_OPEN), "linked {target:?}");
            assert_eq!(rendered, format_table_to_string(&data).unwrap());
        }
    }

    #[test]
    fn test_table_links_are_not_added_when_disabled() {
        let data = serde_json::json!({"url": "https://example.com/resource"});
        let rendered = format_table_with_capabilities(&data, TerminalCapabilities::default());

        assert!(!rendered.contains(OSC8_OPEN));
        assert_eq!(rendered, format_table_to_string(&data).unwrap());
    }

    #[test]
    fn test_table_links_preserve_invisible_separators() {
        let separator_value = "before\u{2063}after";
        assert_eq!(
            format_cell(Some(&serde_json::json!(separator_value))),
            separator_value
        );

        let target = "https://example.com/before\u{2063}after";
        let data = serde_json::json!([{
            "field\u{2063}name": separator_value,
            "url": target
        }]);
        let plain = format_table_to_string(&data).unwrap();
        let linked = format_table_with_capabilities(&data, INTERACTIVE);
        let normalized_target = safe_http_url(target).unwrap();

        assert!(linked.contains(&format!(
            "{OSC8_OPEN}{}{OSC_TERMINATOR}",
            normalized_target.as_str()
        )));
        assert_eq!(strip_test_colors(&strip_test_links(&linked)), plain);
    }

    #[test]
    fn test_json_highlighting_preserves_content() {
        let json =
            "{\n  \"active\": true,\n  \"count\": 2,\n  \"name\": \"café\",\n  \"value\": null\n}";
        let highlighted = highlight_json(json);
        assert!(highlighted.contains(&format!("{ANSI_BOLD_CYAN}\"active\"")));
        assert!(highlighted.contains(&format!("{ANSI_GREEN}\"café\"")));
        assert!(highlighted.contains(&format!("{ANSI_YELLOW}2")));
        assert_eq!(strip_test_colors(&highlighted), json);
    }

    #[test]
    fn test_yaml_highlighting_preserves_content_and_quoted_colons() {
        let yaml = "name: api\nenabled: true\nurl: 'https://example.com:443'\n";
        let highlighted = highlight_yaml(yaml);
        assert!(highlighted.contains(&format!("{ANSI_BOLD_CYAN}name")));
        assert!(highlighted.contains(&format!("{ANSI_MAGENTA}true")));
        assert_eq!(strip_test_colors(&highlighted), yaml);
    }

    #[test]
    fn test_yaml_block_scalar_bodies_are_not_highlighted() {
        let yaml = "note: |-\n  status: error\n  https://example.com\nnext: true\nsummary: >2\n  name: api\n";
        let highlighted = highlight_yaml(yaml);

        assert!(highlighted.contains("\n  status: error\n  https://example.com\n"));
        assert!(highlighted.contains("\n  name: api\n"));
        assert!(highlighted.contains(&format!("{ANSI_BOLD_CYAN}next")));
        assert_eq!(strip_test_colors(&highlighted), yaml);
    }

    #[test]
    fn test_yaml_only_highlights_finite_numbers_as_numeric() {
        let highlighted = highlight_yaml("finite: 12.5\ninf: inf\nnegative_inf: -inf\nnan: nan\n");
        assert!(highlighted.contains(&format!("{ANSI_YELLOW}12.5")));
        for value in ["inf", "-inf", "nan"] {
            assert!(
                highlighted.contains(&format!("{ANSI_GREEN}{value}")),
                "expected string styling for {value}: {highlighted:?}"
            );
        }
    }

    #[test]
    fn test_empty_table_output_remains_visible() {
        assert_eq!(
            visible_output(String::new(), &OutputFormat::Table),
            Some(NO_RESULTS.to_string())
        );
        assert_eq!(visible_output(String::new(), &OutputFormat::Csv), None);
        assert_eq!(
            visible_output("value".to_string(), &OutputFormat::Table),
            Some("value".to_string())
        );
    }

    #[test]
    fn test_text_highlighting_does_not_modify_other_formats() {
        for format in [OutputFormat::Table, OutputFormat::Csv, OutputFormat::Tsv] {
            assert_eq!(
                highlight_output("value".to_string(), &format, COLORS),
                "value"
            );
        }
        assert_eq!(
            highlight_output(
                "{\"value\":1}".to_string(),
                &OutputFormat::Json,
                TerminalCapabilities::default()
            ),
            "{\"value\":1}"
        );
    }

    #[test]
    fn test_scalar_array_table_uses_value_column() {
        let rendered = format_table_to_string(&serde_json::json!(["one", "two"])).unwrap();
        assert!(rendered.contains("| VALUE |"));
        assert!(rendered.contains("| one   |"));
        assert!(rendered.contains("| two   |"));
    }

    #[test]
    fn test_data_wrapped_scalar_table_uses_value_column() {
        let rendered = format_table_to_string(&serde_json::json!({"data": 42})).unwrap();
        assert!(rendered.contains("| VALUE |"));
        assert!(rendered.contains("| 42    |"));
    }

    #[test]
    fn test_empty_scalar_array_table_has_no_results() {
        assert_eq!(
            format_table_to_string(&serde_json::json!([])).unwrap(),
            "No results found"
        );
    }

    #[test]
    fn test_generic_columns_do_not_guess_resource_shapes() {
        let row = serde_json::json!({
            "id": 123,
            "name": "API latency",
            "overall_state": "Alert",
            "type": "query alert",
            "priority": 1,
            "tags": ["service:api"],
            "modified": "2026-08-28T12:00:00Z",
            "query": "avg(last_5m):avg:latency{*} > 1",
            "message": "not useful in a list view"
        });
        let row = flatten_row(&row);
        let headers = select_list_headers(&[&row], 12);
        for expected in ["query", "message"] {
            assert!(
                headers.iter().any(|header| header == expected),
                "generic formatting should retain {expected}: {headers:?}"
            );
        }
    }

    #[test]
    fn test_command_table_options_select_rows_and_columns() {
        let data = serde_json::json!({
            "monitors": [{
                "id": 123,
                "name": "API latency",
                "overall_state": "Alert",
                "query": "avg(last_5m):avg:latency{*} > 1"
            }],
            "metadata": {"total_count": 1}
        });
        let table = TableOptions::new(&["name", "id", "priority"]).rows_at("/monitors");
        let rendered =
            format_table_to_string_with_options(&data, OutputOrder::Default, table.into()).unwrap();
        let header = rendered.lines().find(|line| line.contains("name")).unwrap();

        assert!(header.contains("| name        | id  |"), "{rendered}");
        assert!(!header.contains("priority"), "{rendered}");
        assert!(!rendered.contains("overall_state"), "{rendered}");
        assert!(!rendered.contains("query"), "{rendered}");
        assert!(rendered.contains("API latency"), "{rendered}");
    }

    #[test]
    fn test_command_table_options_select_nested_row_values() {
        let data = serde_json::json!({
            "data": {
                "attributes": {
                    "incidents": [{
                        "data": {
                            "id": "incident-1",
                            "attributes": {
                                "title": "API outage",
                                "state": "active"
                            }
                        }
                    }]
                }
            }
        });
        let table = TableOptions::new(&["attributes.title", "id"])
            .rows_at("/data/attributes/incidents")
            .row_at("/data");
        let rendered =
            format_table_to_string_with_options(&data, OutputOrder::Default, table.into()).unwrap();

        assert!(rendered.contains("attributes.title"), "{rendered}");
        assert!(rendered.contains("API outage"), "{rendered}");
        assert!(rendered.contains("incident-1"), "{rendered}");
        assert!(!rendered.contains("data.attributes"), "{rendered}");
    }

    #[test]
    fn test_command_table_options_reject_missing_rows() {
        let data = serde_json::json!({"items": []});
        let table = TableOptions::new(&["id"]).rows_at("/monitors");
        let error = format_table_to_string_with_options(&data, OutputOrder::Default, table.into())
            .unwrap_err()
            .to_string();
        assert!(error.contains("/monitors"), "{error}");
    }

    #[test]
    fn test_command_table_options_reject_missing_nested_row() {
        let data = serde_json::json!([{"attributes": {"title": "API outage"}}]);
        let table = TableOptions::new(&["attributes.title"]).row_at("/data");
        let error = format_table_to_string_with_options(&data, OutputOrder::Default, table.into())
            .unwrap_err()
            .to_string();
        assert!(error.contains("result row 0"), "{error}");
    }

    #[test]
    fn test_command_table_options_reject_unknown_columns() {
        let data = serde_json::json!([{"id": 123, "name": "API latency"}]);
        let table = TableOptions::new(&["missing"]);
        let error = format_table_to_string_with_options(&data, OutputOrder::Default, table.into())
            .unwrap_err()
            .to_string();
        assert!(error.contains("missing"), "{error}");
    }

    #[test]
    fn test_command_table_options_apply_to_single_object() {
        let data = serde_json::json!({
            "id": 123,
            "attributes": {"title": "API latency", "query": "omit"}
        });
        let table = TableOptions::new(&["attributes.title", "id"]);
        let rendered =
            format_table_to_string_with_options(&data, OutputOrder::Default, table.into()).unwrap();

        let header = rendered
            .lines()
            .find(|line| line.contains("attributes.title"))
            .unwrap();
        assert!(header.contains("| attributes.title | id  |"), "{rendered}");
        assert!(rendered.contains("API latency"), "{rendered}");
        assert!(!rendered.contains("query"), "{rendered}");
    }

    #[test]
    fn test_command_table_options_reject_scalar_input() {
        let table = TableOptions::new(&["id"]);
        let error = format_table_to_string_with_options(
            &serde_json::json!(42),
            OutputOrder::Default,
            table.into(),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("require an array or object"), "{error}");
    }

    #[test]
    fn test_empty_table_options_keep_generic_data_unwrapping() {
        let data = serde_json::json!({"data": [{"id": 123}]});
        let rendered = format_table_to_string_with_options(
            &data,
            OutputOrder::Default,
            TableOptions::default().into(),
        )
        .unwrap();

        assert!(rendered.contains("id"), "{rendered}");
        assert!(!rendered.contains("FIELD"), "{rendered}");
    }

    #[test]
    fn test_jq_table_projection_bypasses_command_table_options() {
        let data = serde_json::json!([{"value": 42, "name": "answer"}]);
        let jq = ".[] | {data: .value, label: .name}";
        let projected = filter::apply_jq(data, jq).unwrap();
        let output_order =
            effective_output_order(&OutputFormat::Table, Some(jq), OutputOrder::Default);
        let command_table = TableOptions::new(&["missing"]).rows_at("/missing");
        let table_input =
            effective_table_input(&OutputFormat::Table, Some(jq), command_table.into());
        let rendered =
            format_table_to_string_with_options(&projected, output_order, table_input).unwrap();
        for expected in ["data", "42", "label", "answer"] {
            assert!(
                rendered.contains(expected),
                "missing {expected}: {rendered}"
            );
        }
    }

    #[test]
    fn test_data_named_column_is_not_unwrapped() {
        let rows = serde_json::json!([{"data": 42}]);
        let rendered =
            format_table_to_string_with_options(&rows, OutputOrder::Preserve, TableInput::AsIs)
                .unwrap();
        let header = rendered.lines().find(|line| line.contains("data")).unwrap();
        assert!(header.contains("data"), "missing DDSQL column: {rendered}");
        assert!(rendered.contains("42"), "missing DDSQL value: {rendered}");
    }

    #[test]
    fn test_generic_column_selection_caps_fallback_fields() {
        let row = serde_json::json!({
            "extra_1": 1,
            "status": "ok",
            "name": "api",
            "id": 42,
            "extra_2": 2,
        });
        assert_eq!(
            select_list_headers(&[&row], 4),
            ["id", "name", "status", "extra_1"]
        );
        assert!(select_list_headers(&[], 4).is_empty());
    }

    #[test]
    fn test_column_budget_caps_priority_fields() {
        let priority = serde_json::json!({
            "id": 42,
            "title": "API",
            "name": "api",
            "status": "ok"
        });
        assert_eq!(select_list_headers(&[&priority], 2), ["id", "title"]);

        let monitor = serde_json::json!({
            "id": 42,
            "name": "API",
            "overall_state": "OK",
            "type": "query alert"
        });
        assert_eq!(select_list_headers(&[&monitor], 2), ["id", "name"]);
        assert!(select_list_headers(&[&monitor], 0).is_empty());
    }

    #[test]
    fn test_rows_without_columns_render_object_previews() {
        let rendered = format_table_to_string(&serde_json::json!([{}, {}])).unwrap();
        assert!(rendered.contains("| VALUE      |"));
        assert_eq!(rendered.matches("| {0 fields} |").count(), 2);
    }

    #[test]
    fn test_format_cell_string() {
        assert_eq!(format_cell(Some(&serde_json::json!("hello"))), "hello");
    }

    #[test]
    fn test_format_cell_long_string() {
        let long = "a".repeat(60);
        let result = format_cell(Some(&serde_json::json!(long)));
        assert_eq!(result.chars().count(), 50);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn test_truncate_ellipsis_handles_zero_width() {
        assert_eq!(truncate_ellipsis("value", 0), "");
    }

    #[test]
    fn test_format_cell_long_multibyte_string_char_boundary() {
        // Regression: truncating by byte index used to panic when the cut point
        // landed inside a multi-byte UTF-8 character (issue #676).
        let name = "Resx V4 ;-) (vérifier que c'est bien un problème de resx avant de recycler)";
        let result = format_cell(Some(&serde_json::json!(name)));
        // 49 kept chars + the ellipsis, counted by characters not bytes.
        let expected: String = name.chars().take(49).collect();
        assert_eq!(result, format!("{expected}…"));
        assert_eq!(result.chars().count(), 50, "got: {result}");
    }

    #[test]
    fn test_format_array_item_multibyte_no_panic() {
        // The array-preview path (16-char cap) is also char-boundary safe.
        let arr = serde_json::json!([{"name": "problème récurrent de résolution"}]);
        let result = format_cell(Some(&arr));
        assert!(result.contains('…'), "got: {result}");
        assert!(result.starts_with("[problème réc"), "got: {result}");
    }

    #[test]
    fn test_format_cell_number() {
        assert_eq!(format_cell(Some(&serde_json::json!(42))), "42");
        assert_eq!(format_cell(Some(&serde_json::json!(1.25))), "1.25");
    }

    #[test]
    fn test_format_cell_null() {
        assert_eq!(format_cell(Some(&serde_json::Value::Null)), "—");
        assert_eq!(format_cell(None), "");
    }

    #[test]
    fn test_format_cell_array() {
        assert_eq!(format_cell(Some(&serde_json::json!([]))), "[]");
        assert_eq!(format_cell(Some(&serde_json::json!([1, 2]))), "[1, 2]");
        assert_eq!(
            format_cell(Some(&serde_json::json!([1, 2, 3, 4, 5]))),
            "[1, 2, 3, 4, +1 more]"
        );
    }

    #[test]
    fn test_format_cell_array_of_objects_with_name() {
        // name takes priority over id; first 4 shown, 5th collapsed into "+1 more"
        let arr = serde_json::json!([
            {"id": "abc", "name": "API"},
            {"id": "def", "name": "Web"},
            {"id": "ghi", "name": "DB"},
            {"id": "jkl", "name": "Cache"},
            {"id": "mno", "name": "Queue"},
        ]);
        let result = format_cell(Some(&arr));
        assert!(result.contains("API"), "got: {result}");
        assert!(result.contains("Web"), "got: {result}");
        assert!(result.contains("DB"), "got: {result}");
        assert!(result.contains("Cache"), "got: {result}");
        assert!(result.contains("+1 more"), "got: {result}");
        assert!(!result.contains("Queue"), "got: {result}");
    }

    #[test]
    fn test_format_cell_array_of_name_only_objects() {
        // Objects with only name: shows name values
        let arr = serde_json::json!([
            {"name": "API"},
            {"name": "Web"},
            {"name": "DB"},
            {"name": "Cache"},
            {"name": "Queue"},
        ]);
        let result = format_cell(Some(&arr));
        assert!(result.contains("API"), "got: {result}");
        assert!(result.contains("Web"), "got: {result}");
        assert!(result.contains("+1 more"), "got: {result}");
        assert!(!result.contains("Queue"), "got: {result}");
    }

    #[test]
    fn test_format_cell_array_truncated() {
        // Array whose rendered form exceeds 50 chars should use one ellipsis character.
        let arr = serde_json::json!([
            {"name": "very-long-name-abc"},
            {"name": "very-long-name-def"},
            {"name": "very-long-name-ghi"},
            {"name": "very-long-name-jkl"},
        ]);
        let result = format_cell(Some(&arr));
        assert!(result.ends_with('…'), "expected truncation, got: {result}");
        assert_eq!(result.chars().count(), 50);
    }

    #[test]
    fn test_format_array_item_object_with_id() {
        let obj = serde_json::json!({"id": "abc123", "status": "ok"});
        assert_eq!(format_array_item(&obj), "abc123");
    }

    #[test]
    fn test_format_array_item_object_prefers_name_over_id() {
        let obj = serde_json::json!({"id": "abc", "name": "MyComp"});
        assert_eq!(format_array_item(&obj), "MyComp");
    }

    #[test]
    fn test_format_array_item_object_long_id() {
        let obj = serde_json::json!({"id": "32d06127-d03a-4da3-9ce6-41eb7bc8fd50"});
        let result = format_array_item(&obj);
        assert!(result.ends_with('…'));
        assert_eq!(result.chars().count(), 16);
    }

    #[test]
    fn test_format_array_item_object_no_known_key() {
        let obj = serde_json::json!({"foo": "bar", "baz": 1});
        assert_eq!(format_array_item(&obj), "{2 fields}");
    }

    #[test]
    fn test_format_array_item_primitive() {
        assert_eq!(format_array_item(&serde_json::json!(42)), "42");
        assert_eq!(format_array_item(&serde_json::json!("hello")), "hello");
    }

    #[test]
    fn test_format_cell_object() {
        assert_eq!(format_cell(Some(&serde_json::json!({"a": 1}))), "{1 field}");
        assert_eq!(
            format_cell(Some(&serde_json::json!({"a": 1, "b": 2}))),
            "{2 fields}"
        );
    }

    #[test]
    fn test_flatten_row_nested_object() {
        let row = serde_json::json!({
            "id": "abc",
            "type": "log",
            "attributes": {"host": "web-1", "status": "info"}
        });
        let flat = flatten_row(&row);
        let obj = flat.as_object().unwrap();
        assert_eq!(obj.get("id").unwrap(), "abc");
        assert_eq!(obj.get("type").unwrap(), "log");
        assert_eq!(obj.get("attributes.host").unwrap(), "web-1");
        assert_eq!(obj.get("attributes.status").unwrap(), "info");
        assert!(!obj.contains_key("attributes"));
    }

    #[test]
    fn test_flatten_row_two_levels_deep() {
        let row = serde_json::json!({
            "id": "abc",
            "attributes": {
                "host": "web-1",
                "tags": {"env": "prod", "service": "api"}
            }
        });
        let flat = flatten_row(&row);
        let obj = flat.as_object().unwrap();
        assert_eq!(obj.get("id").unwrap(), "abc");
        assert_eq!(obj.get("attributes.host").unwrap(), "web-1");
        assert_eq!(obj.get("attributes.tags.env").unwrap(), "prod");
        assert_eq!(obj.get("attributes.tags.service").unwrap(), "api");
        assert!(!obj.contains_key("attributes"));
        assert!(!obj.contains_key("attributes.tags"));
    }

    #[test]
    fn test_flatten_row_preserves_empty_objects() {
        let row = serde_json::json!({
            "attributes": {},
            "relationships": {"notebook": {}}
        });
        let flat = flatten_row(&row);
        let obj = flat.as_object().unwrap();
        assert_eq!(obj.get("attributes"), Some(&serde_json::json!({})));
        assert_eq!(
            obj.get("relationships.notebook"),
            Some(&serde_json::json!({}))
        );
    }

    #[test]
    fn test_flatten_row_no_nested() {
        let row = serde_json::json!({"id": "abc", "name": "foo"});
        let flat = flatten_row(&row);
        let obj = flat.as_object().unwrap();
        assert_eq!(obj.get("id").unwrap(), "abc");
        assert_eq!(obj.get("name").unwrap(), "foo");
    }

    #[test]
    fn test_flatten_row_non_object() {
        let val = serde_json::json!([1, 2, 3]);
        let flat = flatten_row(&val);
        assert_eq!(flat, val);
    }

    #[test]
    fn test_delimited_rows_array() {
        let val = serde_json::json!([{"id": 1}, {"id": 2}]);
        assert_eq!(delimited_rows(&val, TableInput::Generic).unwrap().len(), 2);
    }

    #[test]
    fn test_delimited_rows_data_wrapper() {
        let val = serde_json::json!({"data": [{"id": 1}], "meta": {}});
        assert_eq!(delimited_rows(&val, TableInput::Generic).unwrap().len(), 1);
    }

    #[test]
    fn test_delimited_rows_single_object() {
        let val = serde_json::json!({"id": 1, "name": "test"});
        assert_eq!(delimited_rows(&val, TableInput::Generic).unwrap().len(), 1);
    }

    #[test]
    fn test_format_api_error_basic() {
        let msg = format_api_error("list monitors", None, None);
        assert_eq!(msg, "failed to list monitors");
    }

    #[test]
    fn test_format_api_error_with_status() {
        let msg = format_api_error("list monitors", Some(403), None);
        assert!(msg.contains("HTTP 403"));
        assert!(msg.contains("access denied"));
    }

    #[test]
    fn test_format_api_error_with_body() {
        let msg = format_api_error("get user", Some(404), Some("not found"));
        assert!(msg.contains("not found"));
        assert!(msg.contains("resource not found"));
    }

    #[test]
    fn test_format_api_error_server_error() {
        let msg = format_api_error("query", Some(500), None);
        assert!(msg.contains("API server error"));
    }

    #[test]
    fn test_format_api_error_rate_limit() {
        let msg = format_api_error("query", Some(429), None);
        assert!(msg.contains("rate limited"));
    }

    #[test]
    fn test_format_api_error_401() {
        let msg = format_api_error("query", Some(401), None);
        assert!(msg.contains("authentication failed"));
    }

    #[test]
    fn test_format_api_error_400() {
        let msg = format_api_error("query", Some(400), None);
        assert!(msg.contains("invalid request"));
    }

    #[test]
    fn test_format_api_error_empty_body() {
        let msg = format_api_error("query", Some(500), Some(""));
        assert!(!msg.contains("API response:"));
    }

    #[test]
    fn test_sort_json_value_flat_object() {
        let val = serde_json::json!({"z": 1, "a": 2, "m": 3});
        let sorted = sort_json_value(val);
        let keys: Vec<_> = sorted.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["a", "m", "z"]);
    }

    #[test]
    fn test_sort_json_value_nested_object() {
        let val = serde_json::json!({"b": {"z": 1, "a": 2}, "a": 1});
        let sorted = sort_json_value(val);
        let outer_keys: Vec<_> = sorted.as_object().unwrap().keys().collect();
        assert_eq!(outer_keys, vec!["a", "b"]);
        let inner_keys: Vec<_> = sorted["b"].as_object().unwrap().keys().collect();
        assert_eq!(inner_keys, vec!["a", "z"]);
    }

    #[test]
    fn test_sort_json_value_array() {
        let val = serde_json::json!([{"z": 1, "a": 2}, {"b": 3}]);
        let sorted = sort_json_value(val);
        let first_keys: Vec<_> = sorted[0].as_object().unwrap().keys().collect();
        assert_eq!(first_keys, vec!["a", "z"]);
    }

    #[test]
    fn test_sort_json_value_primitives() {
        assert_eq!(
            sort_json_value(serde_json::json!(42)),
            serde_json::json!(42)
        );
        assert_eq!(
            sort_json_value(serde_json::json!("hello")),
            serde_json::json!("hello")
        );
        assert_eq!(
            sort_json_value(serde_json::json!(true)),
            serde_json::json!(true)
        );
        assert_eq!(
            sort_json_value(serde_json::json!(null)),
            serde_json::json!(null)
        );
    }

    #[test]
    fn test_preserving_order_renderer_keeps_query_column_order() {
        let data = serde_json::json!([{
            "zebra": 1,
            "alpha": 2,
            "middle": 3
        }]);

        for format in [OutputFormat::Json, OutputFormat::Yaml, OutputFormat::Table] {
            let rendered = format_value_to_string_with_options(
                &data,
                &format,
                OutputOrder::Preserve,
                TableInput::AsIs,
            )
            .unwrap();
            let zebra = rendered.find("zebra").unwrap();
            let alpha = rendered.find("alpha").unwrap();
            let middle = rendered.find("middle").unwrap();
            assert!(
                zebra < alpha && alpha < middle,
                "{format} changed column order: {rendered}"
            );
        }

        let csv = format_value_to_string_with_options(
            &data,
            &OutputFormat::Csv,
            OutputOrder::Preserve,
            TableInput::AsIs,
        )
        .unwrap();
        assert_eq!(csv.lines().next(), Some("zebra,alpha,middle"));

        let tsv = format_value_to_string_with_options(
            &data,
            &OutputFormat::Tsv,
            OutputOrder::Preserve,
            TableInput::AsIs,
        )
        .unwrap();
        assert_eq!(tsv.lines().next(), Some("zebra\talpha\tmiddle"));
    }

    #[test]
    fn test_default_renderer_still_sorts_json_keys() {
        let data = serde_json::json!({"zebra": 1, "alpha": 2, "middle": 3});
        let rendered = format_value_to_string(&data, &OutputFormat::Json).unwrap();

        let alpha = rendered.find("alpha").unwrap();
        let middle = rendered.find("middle").unwrap();
        let zebra = rendered.find("zebra").unwrap();
        assert!(alpha < middle && middle < zebra, "{rendered}");
    }

    #[test]
    fn test_go_html_escape_ampersand() {
        assert_eq!(go_html_escape("a&b"), r"a\u0026b");
    }

    #[test]
    fn test_go_html_escape_angle_brackets() {
        assert_eq!(go_html_escape("<div>"), r"\u003cdiv\u003e");
    }

    #[test]
    fn test_go_html_escape_no_change() {
        assert_eq!(go_html_escape("hello world"), "hello world");
    }

    #[test]
    fn test_go_html_escape_all_chars() {
        assert_eq!(
            go_html_escape("<a href=\"&\">"),
            r#"\u003ca href="\u0026"\u003e"#
        );
    }

    #[test]
    fn test_format_and_print_json() {
        let data = serde_json::json!({"name": "test"});
        let result = format_and_print(&data, &OutputFormat::Json, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_format_and_print_yaml() {
        let data = serde_json::json!({"name": "test"});
        let result = format_and_print(&data, &OutputFormat::Yaml, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_format_and_print_table() {
        let data = serde_json::json!([{"id": 1, "name": "test"}]);
        let result = format_and_print(&data, &OutputFormat::Table, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_single_object_table_uses_vertical_layout() {
        let data = serde_json::json!({"id": 42, "name": "api", "active": true});
        let rendered = format_table_to_string(&data).unwrap();
        assert!(rendered.contains("| FIELD  | VALUE |"));
        assert!(rendered.contains("| id     | 42    |"));
        assert!(rendered.contains("| name   | api   |"));
        assert!(rendered.contains("| active | true  |"));
    }

    #[test]
    fn test_data_wrapped_object_table_uses_vertical_layout() {
        let data = serde_json::json!({"data": {"id": 42, "name": "api"}, "meta": {}});
        let rendered = format_table_to_string(&data).unwrap();
        assert!(rendered.contains("| FIELD | VALUE |"));
        assert!(rendered.contains("| id    | 42    |"));
        assert!(!rendered.contains("meta"));
    }

    #[test]
    fn test_single_item_array_table_stays_horizontal() {
        let data = serde_json::json!([{"id": 42, "name": "api"}]);
        let rendered = format_table_to_string(&data).unwrap();
        assert!(rendered.contains("| id | name |"));
        assert!(!rendered.contains("| FIELD | VALUE |"));
    }

    #[test]
    fn test_empty_object_table_has_no_results() {
        assert_eq!(
            format_table_to_string(&serde_json::json!({})).unwrap(),
            "No results found"
        );
    }

    #[test]
    fn test_vertical_table_renders_empty_nested_object() {
        let rendered = format_table_to_string(&serde_json::json!({"attributes": {}})).unwrap();
        assert!(rendered.contains("| attributes | {0 fields} |"));
    }

    fn envelope(raw: serde_json::Value) -> serde_json::Value {
        build_envelope(raw, None, None).unwrap()
    }

    #[test]
    fn test_envelope_keeps_jsonapi_siblings_and_lifts_cursor() {
        let raw = serde_json::json!({
            "data": [{"id": 1}, {"id": 2}],
            "meta": {"page": {"after": "abc"}},
            "links": {"next": "x"}
        });
        let env = envelope(raw);
        assert_eq!(env["status"], "success");
        assert_eq!(env["data"], serde_json::json!([{"id": 1}, {"id": 2}]));
        // Siblings are not dropped, so the response's own cursor is still there.
        assert_eq!(env["meta"]["page"]["after"], "abc");
        assert_eq!(env["links"]["next"], "x");
        assert_eq!(env["metadata"]["count"], 2);
        assert_eq!(env["metadata"]["truncated"], true);
        assert_eq!(env["metadata"]["next_cursor"], "abc");
        assert!(env["metadata"]["next_action"]
            .as_str()
            .unwrap()
            .contains("--cursor=\"abc\""));
        assert!(env["metadata"].get("note").is_none());
    }

    #[test]
    fn test_envelope_wraps_non_jsonapi_bodies() {
        let env = envelope(serde_json::json!([{"id": 1}]));
        assert_eq!(env["data"], serde_json::json!([{"id": 1}]));
        assert_eq!(env["metadata"]["count"], 1);
        assert_eq!(env["metadata"]["truncated"], false);
        assert!(env["metadata"].get("next_cursor").is_none());
        assert!(env["metadata"].get("next_action").is_none());

        let env = envelope(serde_json::json!({"status": "ok", "series": []}));
        assert_eq!(env["status"], "success");
        assert_eq!(env["data"]["status"], "ok");
        assert!(env["metadata"].get("count").is_none());
    }

    #[test]
    fn test_envelope_wraps_body_whose_keys_would_collide() {
        let raw = serde_json::json!({"data": [1], "status": "done"});
        let env = envelope(raw.clone());
        assert_eq!(env["data"], raw);
        assert_eq!(env["metadata"]["count"], serde_json::Value::Null);
    }

    #[test]
    fn test_envelope_keeps_cursor_and_truncation_after_jq() {
        let raw = serde_json::json!({
            "results": [{"id": 1}, {"id": 2}, {"id": 3}],
            "page": {"next_cursor": "c2", "truncated": true}
        });
        let filtered = filter::apply_jq(raw.clone(), ".results[0:1]").unwrap();
        let meta = Metadata {
            count: Some(3),
            ..Default::default()
        };
        let env = build_envelope(filtered, Some(&raw), Some(&meta)).unwrap();
        assert_eq!(env["data"], serde_json::json!([{"id": 1}]));
        // The count describes what was printed, not the pre-filter page.
        assert_eq!(env["metadata"]["count"], 1);
        assert_eq!(env["metadata"]["truncated"], true);
        assert_eq!(env["metadata"]["next_cursor"], "c2");
    }

    #[test]
    fn test_explicit_metadata_wins_over_derived() {
        let raw = serde_json::json!({"data": [], "meta": {"page": {"after": "abc"}}});
        let meta = Metadata {
            command: Some("logs search".into()),
            next_action: Some("custom".into()),
            ..Default::default()
        };
        let env = build_envelope(raw, None, Some(&meta)).unwrap();
        assert_eq!(env["metadata"]["command"], "logs search");
        assert_eq!(env["metadata"]["next_action"], "custom");
        assert_eq!(env["metadata"]["next_cursor"], "abc");
    }

    #[test]
    fn test_truncation_flags_are_derived() {
        for raw in [
            serde_json::json!({"truncated": true}),
            serde_json::json!({"meta": {"truncated": true}}),
            serde_json::json!({"page": {"truncated": true}}),
            serde_json::json!({"data": {"attributes": {"is_truncated": true}}}),
        ] {
            assert!(derive_truncated(&raw), "{raw}");
        }
        assert!(!derive_truncated(&serde_json::json!({"truncated": "true"})));
        assert!(!derive_truncated(&serde_json::json!({"meta": {}})));
    }

    #[test]
    fn test_continuation_hint_prefers_cursor_then_offset() {
        let mut meta = Metadata::default();
        assert!(continuation_hint(&meta).is_none());
        meta.next_offset = Some(10);
        assert_eq!(
            continuation_hint(&meta).as_deref(),
            Some("More results available; rerun with --offset=10")
        );
        meta.next_cursor = Some("abc".into());
        assert!(continuation_hint(&meta)
            .unwrap()
            .contains("--cursor=\"abc\""));
        assert!(derive_cursor(&serde_json::json!({"meta": {"page": {"after": ""}}})).is_none());
    }

    #[test]
    fn test_unwrap_envelope_round_trips_jsonapi_and_wrapped_bodies() {
        let jsonapi = serde_json::json!({"data": [{"id": 1}], "meta": {"page": {"after": "abc"}}});
        let (body, meta) = unwrap_envelope(&envelope(jsonapi.clone())).unwrap();
        assert_eq!(body, jsonapi);
        assert_eq!(meta.next_cursor.as_deref(), Some("abc"));

        let list = serde_json::json!([1, 2]);
        let (body, meta) = unwrap_envelope(&envelope(list.clone())).unwrap();
        assert_eq!(body, list);
        assert_eq!(meta.count, Some(2));
    }

    #[test]
    fn test_unwrap_envelope_ignores_other_documents() {
        assert!(unwrap_envelope(&serde_json::json!({"data": [1]})).is_none());
        assert!(
            unwrap_envelope(&serde_json::json!({"status": "ok", "data": 1, "metadata": {}}))
                .is_none()
        );
        assert!(unwrap_envelope(&serde_json::json!({"status": "success", "data": 1})).is_none());
        assert!(unwrap_envelope(&serde_json::json!([1])).is_none());
    }

    #[test]
    fn test_unwrap_envelope_leaves_foreign_success_documents_whole() {
        // Another tool's `{status, data, metadata}` must not be mistaken for a
        // pup envelope: unwrapping it would silently drop `total` and `foo`.
        let foreign = serde_json::json!({
            "status": "success", "data": [1], "metadata": {"total": 9, "foo": "bar"}
        });
        assert!(unwrap_envelope(&foreign).is_none());
        let unknown_key = serde_json::json!({
            "status": "success", "data": [1], "metadata": {"truncated": false, "total": 9}
        });
        assert!(unwrap_envelope(&unknown_key).is_none());
        let not_bool = serde_json::json!({
            "status": "success", "data": [1], "metadata": {"truncated": "false"}
        });
        assert!(unwrap_envelope(&not_bool).is_none());
    }

    #[test]
    fn test_continuation_hint_uses_the_commands_cursor_flag() {
        let meta = Metadata {
            next_cursor: Some("G2".into()),
            ..Default::default()
        };
        assert_eq!(
            continuation_hint_with(
                &meta,
                &ContinuationFlags {
                    cursor: Some("--page-cursor".into()),
                    page: None
                }
            )
            .as_deref(),
            Some("More results available; rerun with --page-cursor=\"G2\"")
        );
        assert_eq!(
            continuation_hint_with(&meta, &ContinuationFlags::default()).as_deref(),
            Some("More results available; next cursor: \"G2\" (this command has no cursor flag)")
        );
    }

    #[test]
    fn test_new_continuation_locations_are_detected() {
        let points = serde_json::json!({"points": [], "next_page_token": "tok"});
        assert_eq!(derive_cursor(&points).as_deref(), Some("tok"));
        let flaky = serde_json::json!({"data": [], "meta": {"pagination": {"next_page": "p2"}}});
        assert_eq!(derive_cursor(&flaky).as_deref(), Some("p2"));
        let mcp = serde_json::json!({"status": "success", "data": {"next_cursor": "m2", "truncated": true}});
        assert_eq!(derive_cursor(&mcp).as_deref(), Some("m2"));
        assert!(derive_truncated(&mcp));
        // Deployment gates report the *current* page cursor; it is not a next page.
        let gates_last = serde_json::json!({"data": [], "meta": {"page": {"cursor": "cur"}}});
        assert!(derive_cursor(&gates_last).is_none());
        assert!(!derive_truncated(&gates_last));
    }

    #[test]
    fn test_truncation_from_total_filtered_count() {
        let partial =
            serde_json::json!({"data": [1, 2], "meta": {"page": {"total_filtered_count": 5}}});
        assert!(derive_truncated(&partial));
        let complete =
            serde_json::json!({"data": [1, 2], "meta": {"page": {"total_filtered_count": 2}}});
        assert!(!derive_truncated(&complete));
    }

    #[test]
    fn test_stderr_block_always_has_a_metadata_line() {
        assert_eq!(
            stderr_block(&Metadata::default(), true),
            ["metadata: truncated=false"]
        );
        let meta = Metadata {
            command: Some("logs search".into()),
            count: Some(2),
            truncated: true,
            next_cursor: Some("a\"b".into()),
            next_offset: Some(10),
            next_page: Some(3),
            next_action: Some("rerun with --cursor".into()),
            warnings: vec!["rate limited".into()],
        };
        assert_eq!(
            stderr_block(&meta, true),
            [
                r#"metadata: command="logs search" count=2 truncated=true next_cursor="a\"b" next_offset=10 next_page=3"#,
                "next_action: rerun with --cursor",
                "warning: rate limited",
            ]
        );
    }

    #[test]
    fn test_stderr_block_without_envelope_keeps_only_warnings() {
        let meta = Metadata {
            next_cursor: Some("CUR".into()),
            next_action: Some("more".into()),
            warnings: vec!["rate limited".into()],
            ..Default::default()
        };
        assert_eq!(stderr_block(&meta, false), ["warning: rate limited"]);
        assert!(stderr_block(&Metadata::default(), false).is_empty());
    }

    #[test]
    fn test_page_pagination_in_a_bodys_own_metadata() {
        // monitors search: `metadata` is the API's own pagination block.
        let more = serde_json::json!({
            "monitors": [], "metadata": {"page": 0, "page_count": 3, "per_page": 30, "total_count": 70}
        });
        assert_eq!(derive_next_page(&more), Some(1));
        let last = serde_json::json!({
            "monitors": [], "metadata": {"page": 2, "page_count": 3, "per_page": 30, "total_count": 70}
        });
        assert_eq!(derive_next_page(&last), None);
        assert!(derive_next_page(&serde_json::json!({"metadata": {"page": 0}})).is_none());

        // The body has its own `metadata`, so the envelope wraps it whole and
        // the API's pagination stays findable under data.metadata.
        let env = envelope(more);
        assert_eq!(env["data"]["metadata"]["page_count"], 3);
        assert_eq!(env["metadata"]["next_page"], 1);
        assert_eq!(env["metadata"]["truncated"], true);
        assert!(env["metadata"]["next_action"]
            .as_str()
            .unwrap()
            .contains("--page=1"));
    }

    #[test]
    fn test_page_hint_uses_the_commands_page_flag() {
        let meta = Metadata {
            next_page: Some(2),
            ..Default::default()
        };
        let flags = ContinuationFlags {
            cursor: None,
            page: Some("--page".into()),
        };
        assert_eq!(
            continuation_hint_with(&meta, &flags).as_deref(),
            Some("More results available; rerun with --page=2")
        );
        assert_eq!(
            continuation_hint_with(&meta, &ContinuationFlags::default()).as_deref(),
            Some("More results available; next page: 2 (this command has no page flag)")
        );
    }

    #[test]
    fn test_delimited_output_follows_table_row_hints() {
        let body = serde_json::json!({
            "counts": {"status": []},
            "metadata": {"page": 0, "page_count": 1},
            "monitors": [{"id": 1, "name": "cpu", "status": "OK"}, {"id": 2, "name": "disk", "status": "Alert"}]
        });
        let table = TableInput::Explicit(TableOptions::new(&["id", "name"]).rows_at("/monitors"));
        let csv = format_delimited(&body, OutputOrder::Default, table, Delimited::Csv).unwrap();
        assert_eq!(csv, "id,name\n1,cpu\n2,disk");
        let tsv = format_delimited(&body, OutputOrder::Default, table, Delimited::Tsv).unwrap();
        assert_eq!(tsv, "id\tname\n1\tcpu\n2\tdisk");
        // Without hints the whole body is one row, which is what the hints fix.
        let generic = format_delimited(
            &body,
            OutputOrder::Default,
            TableInput::Generic,
            Delimited::Csv,
        )
        .unwrap();
        assert_eq!(generic.lines().count(), 2);
    }

    #[test]
    fn test_delimited_output_honors_row_at_and_rejects_missing_rows_path() {
        let body = serde_json::json!({"results": [{"data": {"id": 1}}, {"data": {"id": 2}}]});
        let table =
            TableInput::Explicit(TableOptions::new(&[]).rows_at("/results").row_at("/data"));
        let csv = format_delimited(&body, OutputOrder::Default, table, Delimited::Csv).unwrap();
        assert_eq!(csv, "id\n1\n2");
        let missing = TableInput::Explicit(TableOptions::new(&["id"]).rows_at("/nope"));
        assert!(format_delimited(&body, OutputOrder::Default, missing, Delimited::Csv).is_err());
    }

    #[test]
    fn test_metadata_visible_for_json_and_yaml_only() {
        assert!(metadata_visible(&OutputFormat::Json));
        assert!(metadata_visible(&OutputFormat::Yaml));
        for format in [OutputFormat::Table, OutputFormat::Csv, OutputFormat::Tsv] {
            assert!(!metadata_visible(&format));
        }
    }

    #[test]
    fn test_format_and_print_yaml_and_table() {
        let data = serde_json::json!([{"id": 1, "name": "test"}]);
        assert!(format_and_print(&data, &OutputFormat::Yaml, None).is_ok());
        assert!(format_and_print(&data, &OutputFormat::Table, None).is_ok());
    }

    #[test]
    fn test_format_and_print_rejects_invalid_jq() {
        let data = serde_json::json!({"name": "test"});
        assert!(format_and_print(&data, &OutputFormat::Json, Some(".[")).is_err());
    }

    #[test]
    fn test_print_json_sorted() {
        let data = serde_json::json!({"z": 1, "a": 2});
        assert!(print_json(&data).is_ok());
    }

    #[test]
    fn test_print_table_empty() {
        let data = serde_json::json!([]);
        assert!(print_table(&data).is_ok());
    }

    #[test]
    fn test_print_table_no_rows() {
        let data = serde_json::json!(42);
        assert!(print_table(&data).is_ok());
    }

    #[test]
    fn test_delimited_rows_primitive() {
        assert!(delimited_rows(&serde_json::json!(42), TableInput::Generic)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_format_cell_bool() {
        assert_eq!(format_cell(Some(&serde_json::json!(true))), "true");
        assert_eq!(format_cell(Some(&serde_json::json!(false))), "false");
    }

    #[test]
    fn test_format_cell_three_item_array() {
        // Three primitives: still shown in full (≤4 items, fits in 50 chars)
        assert_eq!(
            format_cell(Some(&serde_json::json!([1, 2, 3]))),
            "[1, 2, 3]"
        );
    }

    #[test]
    fn test_csv_escape_plain() {
        assert_eq!(csv_escape("hello"), "hello");
    }

    #[test]
    fn test_csv_escape_with_comma() {
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
    }

    #[test]
    fn test_csv_escape_with_quotes() {
        assert_eq!(csv_escape("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn test_csv_escape_with_newline() {
        assert_eq!(csv_escape("a\nb"), "\"a\nb\"");
    }

    #[test]
    fn test_csv_cell_string() {
        assert_eq!(csv_cell(Some(&serde_json::json!("hello"))), "hello");
    }

    #[test]
    fn test_csv_cell_null() {
        assert_eq!(csv_cell(None), "");
        assert_eq!(csv_cell(Some(&serde_json::Value::Null)), "");
    }

    #[test]
    fn test_csv_cell_number() {
        assert_eq!(csv_cell(Some(&serde_json::json!(42))), "42");
    }

    #[test]
    fn test_csv_cell_bool() {
        assert_eq!(csv_cell(Some(&serde_json::json!(true))), "true");
    }

    #[test]
    fn test_flatten_deep_simple() {
        let val = serde_json::json!({"id": "x", "name": "foo"});
        let mut out = serde_json::Map::new();
        flatten_deep(&val, "", &mut out);
        assert_eq!(out.get("id").unwrap(), "x");
        assert_eq!(out.get("name").unwrap(), "foo");
    }

    #[test]
    fn test_flatten_deep_nested() {
        let val = serde_json::json!({"a": {"b": {"c": 1}}});
        let mut out = serde_json::Map::new();
        flatten_deep(&val, "", &mut out);
        assert_eq!(out.get("a.b.c").unwrap(), 1);
        assert!(!out.contains_key("a"));
        assert!(!out.contains_key("a.b"));
    }

    #[test]
    fn test_flatten_deep_mixed() {
        let val = serde_json::json!({"id": "x", "attrs": {"host": "web", "tags": {"env": "prod"}}});
        let mut out = serde_json::Map::new();
        flatten_deep(&val, "", &mut out);
        assert_eq!(out.get("id").unwrap(), "x");
        assert_eq!(out.get("attrs.host").unwrap(), "web");
        assert_eq!(out.get("attrs.tags.env").unwrap(), "prod");
    }

    #[test]
    fn test_print_csv_basic() {
        let data = serde_json::json!([{"id": 1, "name": "test"}]);
        assert!(print_csv(&data, TableInput::Generic).is_ok());
    }

    #[test]
    fn test_print_csv_empty() {
        let data = serde_json::json!([]);
        assert!(print_csv(&data, TableInput::Generic).is_ok());
    }

    #[test]
    fn test_print_csv_nested() {
        let data = serde_json::json!([{"id": 1, "attrs": {"host": "web", "env": "prod"}}]);
        assert!(print_csv(&data, TableInput::Generic).is_ok());
    }

    #[test]
    fn test_format_and_print_csv() {
        let data = serde_json::json!([{"id": 1, "name": "test"}]);
        let result = format_and_print(&data, &OutputFormat::Csv, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_output_helper() {
        let cfg = crate::config::Config {
            api_key: None,
            app_key: None,
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            read_only: false,
            jq: None,
        };
        let data = serde_json::json!({"hello": "world"});
        assert!(crate::formatter::output(&cfg, &data).is_ok());
    }

    #[test]
    fn test_print_table_with_priority_fields() {
        let data = serde_json::json!([
            {"id": 1, "name": "Test", "status": "ok", "type": "metric", "extra": "val"}
        ]);
        assert!(print_table(&data).is_ok());
    }

    #[test]
    fn test_print_table_many_columns() {
        let mut obj = serde_json::Map::new();
        for i in 0..15 {
            obj.insert(format!("col_{i}"), serde_json::json!(i));
        }
        let data = serde_json::json!([obj]);
        assert!(print_table(&data).is_ok());
    }

    #[test]
    fn test_tsv_escape_plain() {
        assert_eq!(tsv_escape("hello"), "hello");
    }

    #[test]
    fn test_tsv_escape_with_tab() {
        // Tab characters in values should be replaced with literal \t
        assert_eq!(tsv_escape("a\tb"), "a\\tb");
    }

    #[test]
    fn test_tsv_escape_no_quoting_for_comma() {
        // Commas are not special in TSV — no quoting applied.
        assert_eq!(tsv_escape("a,b"), "a,b");
    }

    #[test]
    fn test_print_tsv_basic() {
        let data = serde_json::json!([{"id": 1, "name": "test"}]);
        assert!(print_tsv(&data, TableInput::Generic).is_ok());
    }

    #[test]
    fn test_print_tsv_empty() {
        let data = serde_json::json!([]);
        assert!(print_tsv(&data, TableInput::Generic).is_ok());
    }

    #[test]
    fn test_print_tsv_nested() {
        let data = serde_json::json!([{"id": 1, "attrs": {"host": "web", "env": "prod"}}]);
        assert!(print_tsv(&data, TableInput::Generic).is_ok());
    }

    #[test]
    fn test_format_and_print_tsv() {
        let data = serde_json::json!([{"id": 1, "name": "test"}]);
        let result = format_and_print(&data, &OutputFormat::Tsv, None);
        assert!(result.is_ok());
    }
}

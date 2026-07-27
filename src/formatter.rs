use anyhow::Result;
use serde::Serialize;

use crate::config::OutputFormat;
use crate::filter;

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

/// Note injected into `metadata.note` of every agent-mode JSON envelope so
/// an LLM authoring a script for the user to run later is reminded that
/// this envelope only appears in agent mode — without `--no-agent` the
/// user will get raw JSON and any script depending on `.data` / `.status`
/// will silently break.
pub const AGENT_ENVELOPE_NOTE: &str = "This envelope (status/data/metadata) \
    only appears in agent mode. If you are writing a script the user will \
    run outside this agent session, append --no-agent so the output format \
    matches what they will see.";

/// Appended to `metadata.note` when `--jq` ran, so an agent reading the
/// enveloped output knows to write jq expressions against the raw payload
/// (the value under `.data`), not against the envelope itself.
pub const JQ_FILTER_NOTE: &str = "This output was filtered by --jq, which runs on \
    the response payload (the value shown under .data), not on this envelope. \
    Write jq expressions against the payload (e.g. .[]), not .data[].";

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

/// Go's encoding/json escapes <, >, and & for HTML safety.
/// Apply the same escaping to match Go output exactly.
fn go_html_escape(json: &str) -> String {
    json.replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
}

/// After a `--jq` filter rewrites the payload, the caller's `count`/`truncated`
/// describe the pre-filter data and would mislead agents. Drop them, keeping
/// `command`/`next_action`. `None` in → `None` out (envelope behaves as for a
/// command that supplies no metadata). `Metadata`'s `skip_serializing_if` on
/// both fields makes them disappear from the JSON.
fn strip_counts_after_filter(meta: Option<&Metadata>) -> Option<Metadata> {
    let m = meta?;
    Some(Metadata {
        count: None,
        truncated: false,
        command: m.command.clone(),
        next_action: m.next_action.clone(),
    })
}

/// Append `JQ_FILTER_NOTE` to the `metadata.note` field of an agent envelope.
/// Called only when `--jq` ran so agents learn the filter targets the payload,
/// not the envelope.
fn append_jq_note(envelope: &mut serde_json::Value) {
    if let Some(serde_json::Value::String(note)) = envelope.pointer_mut("/metadata/note") {
        note.push(' ');
        note.push_str(JQ_FILTER_NOTE);
    }
}

/// Build the agent-mode envelope as a JSON value. Always sets `status`,
/// `data`, and `metadata` — `metadata.note` is always present so an LLM
/// authoring a script for the user is reminded to pass `--no-agent`.
/// Extracted from `format_and_print` for unit-testability.
pub fn build_agent_envelope(
    data: &serde_json::Value,
    meta: Option<&Metadata>,
) -> Result<serde_json::Value> {
    let sorted_data = sort_json_value(data.clone());
    // Hoist: when the API wraps its list/object in a nested "data" key,
    // use that inner value directly so agents see .data[*] instead of .data.data[*].
    let effective_data = match &sorted_data {
        serde_json::Value::Object(obj) if obj.contains_key("data") => obj["data"].clone(),
        _ => sorted_data.clone(),
    };
    let mut metadata_value = match meta {
        Some(m) => serde_json::to_value(m)?,
        None => serde_json::Value::Object(serde_json::Map::new()),
    };
    // `Metadata` is a struct and serializes to an object; an empty map
    // is constructed above when `meta` is None. The branch is defensive
    // against future changes that might serialize a non-object type.
    if let serde_json::Value::Object(ref mut map) = metadata_value {
        map.insert(
            "note".to_string(),
            serde_json::Value::String(AGENT_ENVELOPE_NOTE.to_string()),
        );
    }
    Ok(serde_json::json!({
        "status": "success",
        "data": effective_data,
        "metadata": metadata_value,
    }))
}

/// Format and print data to stdout.
///
/// The `jq` parameter, when `Some`, applies a jq expression to the serialized
/// data **before** envelope wrapping or format rendering. The filter runs on
/// the raw API payload regardless of `--agent`/`-o`, so the same expression
/// works consistently across all output modes.
pub fn format_and_print<T: Serialize>(
    data: &T,
    format: &OutputFormat,
    agent_mode: bool,
    meta: Option<&Metadata>,
    jq: Option<&str>,
) -> Result<()> {
    // Serialize once; all renderers and the filter operate on this Value.
    let mut value = serde_json::to_value(data)?;
    if let Some(expr) = jq {
        value = filter::apply_jq(value, expr)?;
    }

    if agent_mode && *format == OutputFormat::Json {
        // A --jq filter rewrites the payload, so the caller's count/truncated
        // (computed on the pre-filter data) no longer describe .data. Drop them;
        // keep command/next_action.
        let stripped_meta;
        let meta = if jq.is_some() {
            stripped_meta = strip_counts_after_filter(meta);
            stripped_meta.as_ref()
        } else {
            meta
        };
        let mut envelope = build_agent_envelope(&value, meta)?;
        if jq.is_some() {
            // Extend the inline note so agents learn --jq targets the payload.
            append_jq_note(&mut envelope);
        }
        let json = go_html_escape(&serde_json::to_string_pretty(&envelope)?);
        println!("{json}");
        return Ok(());
    }

    match format {
        OutputFormat::Json => print_json(&value),
        OutputFormat::Yaml => print_yaml(&value),
        OutputFormat::Table => print_table(&value),
        OutputFormat::Csv => print_csv(&value),
        OutputFormat::Tsv => print_tsv(&value),
    }
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

pub fn print_json(data: &serde_json::Value) -> Result<()> {
    let sorted_data = sort_json_value(data.clone());
    let json = go_html_escape(&serde_json::to_string_pretty(&sorted_data)?);
    println!("{json}");
    Ok(())
}

fn print_yaml(data: &serde_json::Value) -> Result<()> {
    let sorted_data = sort_json_value(data.clone());
    let yaml = serde_norway::to_string(&sorted_data)?;
    print!("{yaml}");
    Ok(())
}

/// Flatten up to two levels of nested objects into dot-notation keys.
/// e.g. {"id": "x", "attributes": {"host": "foo", "tags": {"env": "prod"}}}
///   → {"id": "x", "attributes.host": "foo", "attributes.tags.env": "prod"}
fn flatten_row(value: &serde_json::Value) -> serde_json::Value {
    if let serde_json::Value::Object(map) = value {
        let mut flat = serde_json::Map::new();
        for (k, v) in map {
            if let serde_json::Value::Object(inner) = v {
                for (ik, iv) in inner {
                    if let serde_json::Value::Object(inner2) = iv {
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

fn print_table(data: &serde_json::Value) -> Result<()> {
    let raw_rows = extract_rows(data);
    let owned_rows: Vec<serde_json::Value> = raw_rows.iter().map(|r| flatten_row(r)).collect();
    let rows: Vec<&serde_json::Value> = owned_rows.iter().collect();

    if rows.is_empty() {
        println!("No results found");
        return Ok(());
    }

    // Collect headers from all rows
    let mut headers: Vec<String> = Vec::new();
    let mut header_set = std::collections::HashSet::new();
    for row in &rows {
        if let serde_json::Value::Object(map) = row {
            for key in map.keys() {
                if header_set.insert(key.clone()) {
                    headers.push(key.clone());
                }
            }
        }
    }

    // Prioritize common fields (including flattened log attribute fields)
    let priority = [
        "id",
        "title",
        "name",
        "type",
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
    let mut final_headers: Vec<String> = Vec::new();
    for &p in &priority {
        if header_set.contains(p) {
            final_headers.push(p.to_string());
        }
    }
    for h in &headers {
        if final_headers.len() >= 12 {
            break;
        }
        if !final_headers.contains(h) {
            final_headers.push(h.clone());
        }
    }

    let mut table = comfy_table::Table::new();
    table.set_header(&final_headers);

    for row in &rows {
        let cells: Vec<String> = final_headers
            .iter()
            .map(|h| {
                if let serde_json::Value::Object(map) = row {
                    format_cell(map.get(h.as_str()))
                } else {
                    String::new()
                }
            })
            .collect();
        table.add_row(cells);
    }

    println!("{table}");
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

fn print_csv(data: &serde_json::Value) -> Result<()> {
    let raw_rows = extract_rows(data);

    if raw_rows.is_empty() {
        return Ok(());
    }

    // Deep-flatten every row so all nested sub-fields become columns.
    let flat_rows: Vec<serde_json::Map<String, serde_json::Value>> = raw_rows
        .iter()
        .map(|r| {
            let mut out = serde_json::Map::new();
            flatten_deep(r, "", &mut out);
            out
        })
        .collect();

    // Collect all headers, preserving first-seen insertion order.
    let mut header_set = std::collections::HashSet::new();
    let mut headers: Vec<String> = Vec::new();
    for row in &flat_rows {
        for key in row.keys() {
            if header_set.insert(key.clone()) {
                headers.push(key.clone());
            }
        }
    }
    headers.sort();

    // Print header row.
    println!(
        "{}",
        headers
            .iter()
            .map(|h| csv_escape(h))
            .collect::<Vec<_>>()
            .join(",")
    );

    // Print data rows.
    for row in &flat_rows {
        let line = headers
            .iter()
            .map(|h| csv_escape(&csv_cell(row.get(h.as_str()))))
            .collect::<Vec<_>>()
            .join(",");
        println!("{line}");
    }

    Ok(())
}

/// Escape a single TSV field: literal tab characters in values are replaced with \t.
/// No quoting is applied.
fn tsv_escape(s: &str) -> String {
    s.replace('\t', "\\t")
}

fn print_tsv(data: &serde_json::Value) -> Result<()> {
    let raw_rows = extract_rows(data);

    if raw_rows.is_empty() {
        return Ok(());
    }

    // Deep-flatten every row so all nested sub-fields become columns.
    let flat_rows: Vec<serde_json::Map<String, serde_json::Value>> = raw_rows
        .iter()
        .map(|r| {
            let mut out = serde_json::Map::new();
            flatten_deep(r, "", &mut out);
            out
        })
        .collect();

    // Collect all headers, preserving first-seen insertion order.
    let mut header_set = std::collections::HashSet::new();
    let mut headers: Vec<String> = Vec::new();
    for row in &flat_rows {
        for key in row.keys() {
            if header_set.insert(key.clone()) {
                headers.push(key.clone());
            }
        }
    }
    headers.sort();

    // Print header row.
    println!(
        "{}",
        headers
            .iter()
            .map(|h| tsv_escape(h))
            .collect::<Vec<_>>()
            .join("\t")
    );

    // Print data rows.
    for row in &flat_rows {
        let line = headers
            .iter()
            .map(|h| tsv_escape(&csv_cell(row.get(h.as_str()))))
            .collect::<Vec<_>>()
            .join("\t");
        println!("{line}");
    }

    Ok(())
}

/// Extract displayable rows from a JSON value.
/// Handles: arrays, objects with "data" field, single objects.
fn extract_rows(value: &serde_json::Value) -> Vec<&serde_json::Value> {
    match value {
        serde_json::Value::Array(arr) => arr.iter().collect(),
        serde_json::Value::Object(map) => {
            // API responses often wrap data: { "data": [...], "meta": ... }
            if let Some(data) = map.get("data") {
                return extract_rows(data);
            }
            vec![value]
        }
        _ => vec![],
    }
}

/// Truncate `s` to at most `max` characters, appending "..." when shortened.
/// Cuts on character boundaries so multi-byte UTF-8 text never panics.
fn truncate_ellipsis(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let keep: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{keep}...")
    } else {
        s.to_string()
    }
}

/// Compact label for a single array element, used when previewing arrays in table cells.
/// For objects, tries id/name/title/type in order; falls back to format_cell for primitives.
fn format_array_item(value: &serde_json::Value) -> String {
    if let serde_json::Value::Object(map) = value {
        for key in &["name", "title", "id", "type"] {
            if let Some(serde_json::Value::String(s)) = map.get(*key) {
                return truncate_ellipsis(s, 16);
            }
        }
        return format!("{{{} fields}}", map.len());
    }
    format_cell(Some(value))
}

fn format_cell(value: Option<&serde_json::Value>) -> String {
    match value {
        None | Some(serde_json::Value::Null) => String::new(),
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
        Some(serde_json::Value::Object(map)) => format!("{{{} fields}}", map.len()),
    }
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

    #[test]
    fn test_format_cell_string() {
        assert_eq!(format_cell(Some(&serde_json::json!("hello"))), "hello");
    }

    #[test]
    fn test_format_cell_long_string() {
        let long = "a".repeat(60);
        let result = format_cell(Some(&serde_json::json!(long)));
        assert_eq!(result.len(), 50);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_format_cell_long_multibyte_string_char_boundary() {
        // Regression: truncating by byte index used to panic when the cut point
        // landed inside a multi-byte UTF-8 character (issue #676).
        let name = "Resx V4 ;-) (vérifier que c'est bien un problème de resx avant de recycler)";
        let result = format_cell(Some(&serde_json::json!(name)));
        // 47 kept chars + the ellipsis, counted by characters not bytes.
        let expected: String = name.chars().take(47).collect();
        assert_eq!(result, format!("{expected}..."));
        assert_eq!(result.chars().count(), 50, "got: {result}");
    }

    #[test]
    fn test_format_array_item_multibyte_no_panic() {
        // The array-preview path (16-char cap) is also char-boundary safe.
        let arr = serde_json::json!([{"name": "problème récurrent de résolution"}]);
        let result = format_cell(Some(&arr));
        assert!(result.contains("..."), "got: {result}");
        assert!(result.starts_with("[problème réc"), "got: {result}");
    }

    #[test]
    fn test_format_cell_number() {
        assert_eq!(format_cell(Some(&serde_json::json!(42))), "42");
        assert_eq!(format_cell(Some(&serde_json::json!(1.25))), "1.25");
    }

    #[test]
    fn test_format_cell_null() {
        assert_eq!(format_cell(Some(&serde_json::Value::Null)), "");
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
        // Array whose rendered form exceeds 50 chars should be truncated with "..."
        let arr = serde_json::json!([
            {"name": "very-long-name-abc"},
            {"name": "very-long-name-def"},
            {"name": "very-long-name-ghi"},
            {"name": "very-long-name-jkl"},
        ]);
        let result = format_cell(Some(&arr));
        assert!(
            result.ends_with("..."),
            "expected truncation, got: {result}"
        );
        assert!(result.len() == 50);
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
        assert!(result.ends_with("..."));
        assert_eq!(result.len(), 16);
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
    fn test_extract_rows_array() {
        let val = serde_json::json!([{"id": 1}, {"id": 2}]);
        assert_eq!(extract_rows(&val).len(), 2);
    }

    #[test]
    fn test_extract_rows_data_wrapper() {
        let val = serde_json::json!({"data": [{"id": 1}], "meta": {}});
        assert_eq!(extract_rows(&val).len(), 1);
    }

    #[test]
    fn test_extract_rows_single_object() {
        let val = serde_json::json!({"id": 1, "name": "test"});
        assert_eq!(extract_rows(&val).len(), 1);
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
        let result = format_and_print(&data, &OutputFormat::Json, false, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_format_and_print_yaml() {
        let data = serde_json::json!({"name": "test"});
        let result = format_and_print(&data, &OutputFormat::Yaml, false, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_format_and_print_table() {
        let data = serde_json::json!([{"id": 1, "name": "test"}]);
        let result = format_and_print(&data, &OutputFormat::Table, false, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_format_and_print_agent_mode() {
        let data = serde_json::json!({"name": "test"});
        let meta = Metadata {
            count: Some(1),
            truncated: false,
            command: Some("test".into()),
            next_action: None,
        };
        let result = format_and_print(&data, &OutputFormat::Json, true, Some(&meta), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_agent_envelope_injects_script_authoring_note_with_meta() {
        let data = serde_json::json!({"name": "test"});
        let meta = Metadata {
            count: Some(1),
            truncated: false,
            command: Some("monitors list".into()),
            next_action: None,
        };
        let envelope = build_agent_envelope(&data, Some(&meta)).unwrap();
        assert_eq!(envelope["status"], "success");
        assert_eq!(envelope["metadata"]["count"], 1);
        assert_eq!(envelope["metadata"]["command"], "monitors list");
        assert_eq!(envelope["metadata"]["note"], AGENT_ENVELOPE_NOTE);
        assert!(
            envelope["metadata"]["note"]
                .as_str()
                .unwrap()
                .contains("--no-agent"),
            "note must point agents at --no-agent so the gaslighting case is fixed: {envelope}"
        );
    }

    #[test]
    fn test_agent_envelope_injects_script_authoring_note_without_meta() {
        let data = serde_json::json!({"name": "test"});
        let envelope = build_agent_envelope(&data, None).unwrap();
        // Even when callers pass no Metadata, the note must still appear —
        // otherwise the "envelope only in agent mode" warning is invisible
        // for the many commands that don't construct a Metadata.
        assert_eq!(envelope["metadata"]["note"], AGENT_ENVELOPE_NOTE);
        assert_eq!(envelope["status"], "success");
        assert!(envelope["metadata"]["count"].is_null());
        assert!(
            envelope["metadata"]["note"]
                .as_str()
                .unwrap()
                .contains("--no-agent"),
            "note constant itself must mention --no-agent so the rule survives if the constant is rewritten"
        );
    }

    #[test]
    fn test_agent_envelope_hoists_inner_data_and_keeps_note() {
        // When the caller's payload is `{ "data": [...] }`, the envelope
        // hoists the inner array so agents see `.data[*]` instead of
        // `.data.data[*]`. Verify that the hoist and the metadata.note
        // injection don't interfere with each other — both must happen.
        let payload = serde_json::json!({"data": [{"id": 1}, {"id": 2}]});
        let envelope = build_agent_envelope(&payload, None).unwrap();
        assert_eq!(envelope["data"], serde_json::json!([{"id": 1}, {"id": 2}]));
        assert_eq!(envelope["metadata"]["note"], AGENT_ENVELOPE_NOTE);
    }

    #[test]
    fn test_format_and_print_agent_mode_no_meta() {
        let data = serde_json::json!({"name": "test"});
        let result = format_and_print(&data, &OutputFormat::Json, true, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_format_and_print_agent_mode_respects_yaml_flag() {
        // In agent mode, -o yaml should bypass the agent envelope and use YAML output.
        let data = serde_json::json!({"name": "test"});
        let result = format_and_print(&data, &OutputFormat::Yaml, true, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_format_and_print_agent_mode_respects_table_flag() {
        // In agent mode, -o table should bypass the agent envelope and use table output.
        let data = serde_json::json!([{"id": 1, "name": "test"}]);
        let result = format_and_print(&data, &OutputFormat::Table, true, None, None);
        assert!(result.is_ok());
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
    fn test_extract_rows_primitive() {
        assert!(extract_rows(&serde_json::json!(42)).is_empty());
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
        assert!(print_csv(&data).is_ok());
    }

    #[test]
    fn test_print_csv_empty() {
        let data = serde_json::json!([]);
        assert!(print_csv(&data).is_ok());
    }

    #[test]
    fn test_print_csv_nested() {
        let data = serde_json::json!([{"id": 1, "attrs": {"host": "web", "env": "prod"}}]);
        assert!(print_csv(&data).is_ok());
    }

    #[test]
    fn test_format_and_print_csv() {
        let data = serde_json::json!([{"id": 1, "name": "test"}]);
        let result = format_and_print(&data, &OutputFormat::Csv, false, None, None);
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
            agent_mode: false,
            read_only: false,
            jq: None,
        };
        let data = serde_json::json!({"hello": "world"});
        assert!(output(&cfg, &data).is_ok());
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
        assert!(print_tsv(&data).is_ok());
    }

    #[test]
    fn test_print_tsv_empty() {
        let data = serde_json::json!([]);
        assert!(print_tsv(&data).is_ok());
    }

    #[test]
    fn test_print_tsv_nested() {
        let data = serde_json::json!([{"id": 1, "attrs": {"host": "web", "env": "prod"}}]);
        assert!(print_tsv(&data).is_ok());
    }

    #[test]
    fn test_format_and_print_tsv() {
        let data = serde_json::json!([{"id": 1, "name": "test"}]);
        let result = format_and_print(&data, &OutputFormat::Tsv, false, None, None);
        assert!(result.is_ok());
    }

    // --- strip_counts_after_filter -------------------------------------------

    #[test]
    fn test_strip_counts_none_meta_returns_none() {
        assert!(strip_counts_after_filter(None).is_none());
    }

    #[test]
    fn test_strip_counts_drops_count_and_truncated() {
        let meta = Metadata {
            count: Some(10),
            truncated: true,
            command: Some("monitors list".into()),
            next_action: Some("next".into()),
        };
        let stripped = strip_counts_after_filter(Some(&meta)).unwrap();
        assert!(stripped.count.is_none(), "count should be dropped");
        assert!(!stripped.truncated, "truncated should be cleared");
        assert_eq!(stripped.command.as_deref(), Some("monitors list"));
        assert_eq!(stripped.next_action.as_deref(), Some("next"));
    }

    // --- append_jq_note ------------------------------------------------------

    #[test]
    fn test_append_jq_note_extends_note_field() {
        let data = serde_json::json!({"id": 1});
        let mut envelope = build_agent_envelope(&data, None).unwrap();
        // Before: note contains only AGENT_ENVELOPE_NOTE.
        let before = envelope["metadata"]["note"].as_str().unwrap().to_string();
        assert!(before.contains("agent mode"), "pre-condition: {before}");

        append_jq_note(&mut envelope);

        let after = envelope["metadata"]["note"].as_str().unwrap();
        assert!(
            after.contains(AGENT_ENVELOPE_NOTE),
            "original note must be preserved: {after}"
        );
        assert!(
            after.contains(JQ_FILTER_NOTE),
            "jq note must be appended: {after}"
        );
        assert!(
            after.contains(".data"),
            "jq note must mention .data: {after}"
        );
    }

    // --- integration: strip + append through the real builder ----------------

    #[test]
    fn test_jq_filter_path_drops_count_and_appends_note() {
        let filtered = serde_json::json!({"id": 1, "name": "foo"});
        let meta = Metadata {
            count: Some(10),
            truncated: false,
            command: Some("monitors list".into()),
            next_action: None,
        };

        let stripped = strip_counts_after_filter(Some(&meta));
        let mut env = build_agent_envelope(&filtered, stripped.as_ref()).unwrap();
        append_jq_note(&mut env);

        assert!(
            env["metadata"]["count"].is_null(),
            "count must be omitted after filter: {}",
            env["metadata"]["count"]
        );
        assert!(
            env["metadata"]["truncated"].is_null(),
            "truncated must be omitted after filter"
        );
        assert_eq!(
            env["metadata"]["command"],
            serde_json::json!("monitors list"),
            "command must be preserved"
        );
        let note = env["metadata"]["note"].as_str().unwrap();
        assert!(
            note.contains(AGENT_ENVELOPE_NOTE),
            "original note must survive: {note}"
        );
        assert!(
            note.contains(JQ_FILTER_NOTE),
            "jq note must be appended: {note}"
        );
        assert_eq!(env["status"], "success");
    }

    #[test]
    fn test_no_jq_path_keeps_count_and_note_unchanged() {
        // Regression: when --jq is NOT used, the envelope must be byte-for-byte
        // identical to pre-change behavior: count stays, only AGENT_ENVELOPE_NOTE.
        let data = serde_json::json!([{"id": 1}, {"id": 2}]);
        let meta = Metadata {
            count: Some(2),
            truncated: false,
            command: Some("monitors list".into()),
            next_action: None,
        };
        let env = build_agent_envelope(&data, Some(&meta)).unwrap();
        assert_eq!(
            env["metadata"]["count"],
            serde_json::json!(2),
            "count must survive without --jq"
        );
        let note = env["metadata"]["note"].as_str().unwrap();
        assert!(
            !note.contains(JQ_FILTER_NOTE),
            "jq note must NOT appear without --jq: {note}"
        );
        assert!(
            note.contains(AGENT_ENVELOPE_NOTE),
            "original note must be present: {note}"
        );
    }
}

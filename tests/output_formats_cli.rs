//! Every output format through the real binary: JSON and YAML carry the
//! envelope; table, CSV, and TSV print only rows on stdout, with the metadata
//! on stderr.
#[path = "support/idp_cli.rs"]
mod support;

use mockito::{Matcher, Mock, Server};
use serde_json::{json, Value};
use std::process::Output;
use support::{payload, pup};

/// A monitors search page: the body's own `metadata` is API pagination.
fn search_body() -> Value {
    json!({
        "counts": {"status": [{"count": 2, "name": "OK"}]},
        "metadata": {"page": 0, "page_count": 2, "per_page": 2, "total_count": 3},
        "monitors": [
            {"id": 1, "name": "cpu high", "status": "OK", "type": "metric alert", "tags": ["env:prod"]},
            {"id": 2, "name": "disk, full", "status": "Alert", "type": "metric alert", "tags": []}
        ]
    })
}

fn mock_search(server: &mut Server, body: Value) -> Mock {
    server
        .mock("GET", "/api/v1/monitor/search")
        .match_query(Matcher::Any)
        .with_header("content-type", "application/json")
        .with_body(body.to_string())
        .create()
}

fn run(server: &Server, args: &[&str]) -> (String, String) {
    let output: Output = pup(server, args);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(output.status.success(), "{stderr}");
    (stdout, stderr)
}

/// Split one CSV line into fields, honoring double-quoted fields.
fn csv_fields(line: &str) -> Vec<String> {
    let (mut fields, mut field, mut quoted) = (Vec::new(), String::new(), false);
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut field)),
            _ => field.push(c),
        }
    }
    fields.push(field);
    fields
}

#[test]
fn csv_prints_one_row_per_monitor_and_metadata_on_stderr() {
    let mut server = Server::new();
    let request = mock_search(&mut server, search_body());
    let (stdout, stderr) = run(&server, &["monitors", "search", "--output", "csv"]);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "header + one row per monitor: {stdout}");
    let header = csv_fields(lines[0]);
    assert_eq!(header, ["id", "name", "status", "type", "tags"]);
    assert_eq!(csv_fields(lines[1])[..3], ["1", "cpu high", "OK"]);
    assert_eq!(csv_fields(lines[2])[1], "disk, full", "quoted field");
    for leaked in ["metadata", "counts", "status\":", "page_count"] {
        assert!(
            !lines[0].contains(leaked),
            "{leaked} leaked into the header"
        );
    }
    assert!(
        stderr
            .contains(r#"metadata: command="monitors search" count=2 truncated=true next_page=1"#),
        "{stderr}"
    );
    assert!(
        stderr.contains("next_action: More results available; rerun with --page=1"),
        "{stderr}"
    );
    request.assert();
}

#[test]
fn tsv_and_table_select_the_same_monitor_rows() {
    let mut server = Server::new();
    let request = mock_search(&mut server, search_body()).expect(2);
    let (tsv, tsv_err) = run(&server, &["monitors", "search", "--output", "tsv"]);
    let rows: Vec<Vec<&str>> = tsv.lines().map(|l| l.split('\t').collect()).collect();
    assert_eq!(rows.len(), 3, "{tsv}");
    assert_eq!(rows[0], ["id", "name", "status", "type", "tags"]);
    assert_eq!(rows[2][1], "disk, full");
    assert!(tsv_err.starts_with("metadata: "), "{tsv_err}");

    let (table, table_err) = run(&server, &["monitors", "search", "--output", "table"]);
    assert!(
        table.contains("cpu high") && table.contains("disk, full"),
        "{table}"
    );
    assert!(!table.contains("page_count"), "{table}");
    assert!(table_err.contains("next_page=1"), "{table_err}");
    request.assert();
}

#[test]
fn yaml_carries_the_envelope() {
    let mut server = Server::new();
    let request = mock_search(&mut server, search_body());
    let (stdout, stderr) = run(&server, &["monitors", "search", "--output", "yaml"]);
    let doc: Value = serde_norway::from_str(&stdout).expect("YAML stdout");
    assert_eq!(doc["status"], "success");
    // The body has its own `metadata`, so it is wrapped whole.
    assert_eq!(doc["data"]["metadata"]["page_count"], 2);
    assert_eq!(doc["metadata"]["next_page"], 1);
    assert_eq!(doc["metadata"]["truncated"], true);
    assert!(stderr.is_empty(), "metadata is in the YAML body: {stderr}");
    request.assert();
}

#[test]
fn no_envelope_yaml_and_csv_add_no_metadata() {
    let mut server = Server::new();
    let request = mock_search(&mut server, search_body()).expect(2);
    let (yaml, yaml_err) = run(
        &server,
        &["--no-envelope", "monitors", "search", "--output", "yaml"],
    );
    let doc: Value = serde_norway::from_str(&yaml).expect("YAML stdout");
    assert_eq!(doc, search_body());
    assert!(yaml_err.is_empty(), "{yaml_err}");

    let (csv, csv_err) = run(
        &server,
        &["--no-envelope", "monitors", "search", "--output", "csv"],
    );
    assert_eq!(csv.lines().count(), 3, "{csv}");
    assert!(csv_err.is_empty(), "{csv_err}");
    request.assert();
}

#[test]
fn jq_result_is_rendered_as_is_in_csv() {
    let mut server = Server::new();
    let request = mock_search(&mut server, search_body());
    let (stdout, stderr) = run(
        &server,
        &[
            "monitors",
            "search",
            "--output",
            "csv",
            "--jq",
            "[.monitors[] | {id, name}]",
        ],
    );
    assert_eq!(stdout.trim_end(), "id,name\n1,cpu high\n2,\"disk, full\"");
    // count describes the filtered rows; continuation survives the filter.
    assert!(
        stderr.contains("count=2 truncated=true next_page=1"),
        "{stderr}"
    );
    request.assert();
}

#[test]
fn empty_result_prints_no_rows_and_complete_metadata() {
    let mut server = Server::new();
    let body = json!({
        "counts": {}, "metadata": {"page": 0, "page_count": 1, "per_page": 30, "total_count": 0},
        "monitors": []
    });
    let request = mock_search(&mut server, body);
    let (stdout, stderr) = run(&server, &["monitors", "search", "--output", "csv"]);
    assert!(stdout.is_empty(), "{stdout}");
    assert_eq!(
        stderr.trim_end(),
        r#"metadata: command="monitors search" count=0 truncated=false"#
    );
    request.assert();
}

#[test]
fn bare_v1_array_renders_rows_and_wraps_in_json() {
    let mut server = Server::new();
    let body = json!([
        {"id": 7, "name": "cpu", "type": "metric alert", "query": "avg(last_5m):avg:cpu{*} > 90", "overall_state": "OK"}
    ]);
    let request = server
        .mock("GET", "/api/v1/monitor")
        .match_query(Matcher::Any)
        .with_header("content-type", "application/json")
        .with_body(body.to_string())
        .expect(2)
        .create();
    let (csv, csv_err) = run(&server, &["monitors", "list", "--output", "csv"]);
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 2, "{csv}");
    assert_eq!(csv_fields(lines[0])[..2], ["id", "name"]);
    assert_eq!(csv_fields(lines[1])[..2], ["7", "cpu"]);
    assert!(csv_err.contains("count=1 truncated=false"), "{csv_err}");

    let output = pup(&server, &["monitors", "list"]);
    assert!(output.stderr.is_empty(), "metadata is in the JSON body");
    let doc = payload(output);
    assert_eq!(doc["data"][0]["id"], 7);
    assert_eq!(doc["metadata"]["count"], 1);
    request.assert();
}

/// Run `pup format` with `stdin`, returning stdout and stderr.
fn format(args: &[&str], stdin: &str) -> (String, String) {
    use std::io::Write;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_pup"))
        .arg("format")
        .args(args)
        .env("DD_TOKEN_STORAGE", "file")
        .env_remove("PUP_NO_ENVELOPE")
        .env_remove("PUP_OUTPUT")
        .env_remove("PUP_FILTER")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn pup format");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(output.status.success(), "{stderr}");
    (String::from_utf8_lossy(&output.stdout).to_string(), stderr)
}

#[test]
fn format_of_a_piped_envelope_is_idempotent() {
    for body in [
        json!([1, 2]),
        json!({"data": [1]}),
        json!({"data": [{"id": "a"}], "meta": {"page": {"after": "c2"}}, "links": {"next": "x"}}),
        // Keys already sorted: JSON output sorts keys, and a vertical table
        // follows the input's key order.
        json!({"metadata": {"page": 0, "page_count": 2}, "monitors": [1], "status": "ok"}),
    ] {
        let (once, _) = format(&[], &body.to_string());
        let (twice, twice_err) = format(&[], &once);
        assert_eq!(once, twice, "format | format changed {body}");
        assert!(twice_err.is_empty(), "{twice_err}");
        // Rows survive a round trip too.
        let (rows_once, _) = format(&["--output", "table"], &body.to_string());
        let (rows_twice, _) = format(&["--output", "table"], &once);
        assert_eq!(rows_once, rows_twice, "table rows changed for {body}");
    }
}

#[test]
fn piped_envelope_keeps_a_data_object_payload() {
    // Bits repro: `echo '[1]' | pup format --jq '{data: .}' | pup format`
    let (filtered, _) = format(&["--jq", "{data: .}"], "[1]");
    let first: Value = serde_json::from_str(&filtered).unwrap();
    assert_eq!(first["data"], json!({"data": [1]}));
    let (again, _) = format(&[], &filtered);
    let second: Value = serde_json::from_str(&again).unwrap();
    assert_eq!(second, first, "the data object must not be unwrapped");
}

#[test]
fn piped_look_alike_document_is_wrapped_not_unwrapped() {
    let foreign = json!({"status": "success", "data": [1], "metadata": {"total": 9}});
    let (out, _) = format(&[], &foreign.to_string());
    let doc: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(doc["data"], foreign, "a foreign document is a payload");
}

//! Continuation cursors must survive in what the CLI prints: in the JSON body
//! as-is, and as a stderr hint when the rendered format drops it.
#[path = "support/idp_cli.rs"]
mod support;

use mockito::{Matcher, Mock, Server};
use serde_json::json;
use support::{payload, pup};

const LOGS_SEARCH: [&str; 4] = ["logs", "search", "--query", "*"];

fn logs_page(server: &mut Server, after: Option<&str>) -> Mock {
    let meta = match after {
        Some(cursor) => json!({"page": {"after": cursor}}),
        None => json!({"page": {}}),
    };
    server
        .mock("POST", "/api/v2/logs/events/search")
        .match_query(Matcher::Any)
        .with_header("content-type", "application/json")
        .with_body(
            json!({"data": [{"id": "AQ", "type": "log", "attributes": {"message": "boom"}}], "meta": meta})
                .to_string(),
        )
        .create()
}

#[test]
fn json_output_keeps_the_cursor_in_the_body() {
    let mut server = Server::new();
    let request = logs_page(&mut server, Some("next-page"));
    let output = pup(&server, &LOGS_SEARCH);
    assert!(String::from_utf8_lossy(&output.stderr).is_empty());
    let body = payload(output);
    assert_eq!(body["meta"]["page"]["after"], "next-page");
    assert!(body.get("status").is_none(), "no envelope: {body}");
    request.assert();
}

#[test]
fn table_output_names_the_cursor_on_stderr() {
    let mut server = Server::new();
    let request = logs_page(&mut server, Some("next-page"));
    let output = pup(
        &server,
        &[&LOGS_SEARCH[..], &["--output", "table"]].concat(),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--cursor=\"next-page\""),
        "missing next-page hint: {stderr}"
    );
    request.assert();
}

#[test]
fn jq_output_names_the_cursor_on_stderr() {
    let mut server = Server::new();
    let request = logs_page(&mut server, Some("next-page"));
    let output = pup(&server, &[&LOGS_SEARCH[..], &["--jq", ".data"]].concat());
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert_eq!(
        payload(output),
        json!([{"id": "AQ", "type": "log", "attributes": {"message": "boom"}}])
    );
    assert!(stderr.contains("--cursor=\"next-page\""), "{stderr}");
    request.assert();
}

#[test]
fn last_page_prints_no_hint() {
    let mut server = Server::new();
    let request = logs_page(&mut server, None);
    let output = pup(
        &server,
        &[&LOGS_SEARCH[..], &["--output", "table"]].concat(),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("--cursor"));
    request.assert();
}

//! Continuation must never be silently lost: the default JSON envelope carries
//! it in `metadata` (and keeps the response's own `meta`), `--no-envelope`
//! prints the raw body, and formats that cannot show metadata say on stderr
//! where the next page starts.
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

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn default_envelope_carries_the_cursor_in_metadata() {
    let mut server = Server::new();
    let request = logs_page(&mut server, Some("next-page"));
    let output = pup(&server, &LOGS_SEARCH);
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
    let body = payload(output);
    assert_eq!(body["status"], "success");
    assert_eq!(body["data"][0]["id"], "AQ");
    assert_eq!(body["meta"]["page"]["after"], "next-page", "siblings kept");
    assert_eq!(body["metadata"]["command"], "logs search");
    assert_eq!(body["metadata"]["count"], 1);
    assert_eq!(body["metadata"]["truncated"], true);
    assert_eq!(body["metadata"]["next_cursor"], "next-page");
    assert!(body["metadata"]["next_action"]
        .as_str()
        .unwrap()
        .contains("--cursor=\"next-page\""));
    request.assert();
}

#[test]
fn jq_result_keeps_metadata_and_cursor() {
    let mut server = Server::new();
    let request = logs_page(&mut server, Some("next-page"));
    let body = payload(pup(
        &server,
        &[&LOGS_SEARCH[..], &["--jq", ".data"]].concat(),
    ));
    assert_eq!(body["data"][0]["attributes"]["message"], "boom");
    assert!(body.get("meta").is_none(), "--jq output replaces the body");
    assert_eq!(body["metadata"]["count"], 1);
    assert_eq!(body["metadata"]["next_cursor"], "next-page");
    request.assert();
}

#[test]
fn no_envelope_prints_the_raw_body() {
    for flag in ["--no-envelope", "--no-agent"] {
        let mut server = Server::new();
        let request = logs_page(&mut server, Some("next-page"));
        let body = payload(pup(&server, &[&[flag][..], &LOGS_SEARCH].concat()));
        assert!(body.get("status").is_none(), "{flag}: {body}");
        assert!(body.get("metadata").is_none(), "{flag}: {body}");
        assert_eq!(body["meta"]["page"]["after"], "next-page");
        request.assert();
    }
    // The raw body already shows its cursor, so stderr stays quiet.
    let mut server = Server::new();
    let request = logs_page(&mut server, Some("next-page"));
    let output = pup(&server, &[&["--no-envelope"][..], &LOGS_SEARCH].concat());
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
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
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("--cursor=\"next-page\""),
        "missing next-page hint: {}",
        stderr(&output)
    );
    request.assert();
}

#[test]
fn last_page_is_complete_and_prints_no_hint() {
    let mut server = Server::new();
    let request = logs_page(&mut server, None);
    let body = payload(pup(&server, &LOGS_SEARCH));
    assert_eq!(body["metadata"]["truncated"], false);
    assert!(body["metadata"].get("next_cursor").is_none());
    assert!(body["metadata"].get("next_action").is_none());
    request.assert();

    let mut server = Server::new();
    let request = logs_page(&mut server, None);
    let output = pup(
        &server,
        &[&LOGS_SEARCH[..], &["--output", "table"]].concat(),
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!stderr(&output).contains("--cursor"));
    request.assert();
}

#[test]
fn no_envelope_is_inherited_from_the_environment() {
    let mut server = Server::new();
    let request = logs_page(&mut server, None);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_pup"))
        .args(["--read-only", "--output", "json"])
        .args(LOGS_SEARCH)
        .env("PUP_NO_ENVELOPE", "1")
        .env("PUP_MOCK_SERVER", server.url())
        .env("DD_ACCESS_TOKEN", "local-fixture-token")
        .env("DD_SITE", "datadoghq.com")
        .env("DD_TOKEN_STORAGE", "file")
        .env(
            "PUP_CONFIG_DIR",
            std::env::temp_dir().join("pup-pagination-env-fixture"),
        )
        .env_remove("PUP_FILTER")
        .env_remove("DD_API_KEY")
        .env_remove("DD_APP_KEY")
        .output()
        .unwrap();
    let body = payload(output);
    assert!(body.get("metadata").is_none(), "{body}");
    request.assert();
}

#[test]
fn hint_uses_the_commands_own_cursor_flag() {
    // llm-obs patterns points resumes with --page-token, not --cursor.
    let mut server = Server::new();
    let request = server
        .mock(
            "POST",
            "/api/unstable/llm-obs-mcp/v1/topic-discovery/clustered-points",
        )
        .match_query(Matcher::Any)
        .with_header("content-type", "application/json")
        .with_body(
            json!({"points": [{"span_id": "s-1"}], "next_page_token": "tok-def"}).to_string(),
        )
        .create();
    let output = pup(
        &server,
        &[
            "llm-obs",
            "patterns",
            "points",
            "--topic-id",
            "t1",
            "--output",
            "table",
        ],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("--page-token=\"tok-def\""), "{stderr}");
    assert!(!stderr.contains("--cursor="), "{stderr}");
    request.assert();
}

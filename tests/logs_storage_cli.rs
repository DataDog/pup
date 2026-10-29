//! `logs list` and `logs query` share `logs search`'s storage default: with no
//! `--storage`, they try Flex and fall back to indexed logs when Flex is
//! unavailable. An explicit `--storage` is sent as given.
#[path = "support/idp_cli.rs"]
mod support;

use mockito::{Matcher, Mock, Server};
use support::{payload, pup};

const LOGS_SEARCH_PATH: &str = "/api/v2/logs/events/search";
const EMPTY_PAGE: &str = r#"{"data": [], "meta": {"page": {}}}"#;

fn search_mock(server: &mut Server, storage_tier: &str, status: usize, body: &str) -> Mock {
    server
        .mock("POST", LOGS_SEARCH_PATH)
        .match_query(Matcher::Any)
        .match_body(Matcher::Regex(format!(
            r#""storage_tier":"{storage_tier}""#
        )))
        .with_status(status)
        .with_header("content-type", "application/json")
        .with_body(body)
        .expect(1)
        .create()
}

fn logs_args<'a>(subcommand: &'a str, extra: &[&'a str]) -> Vec<&'a str> {
    [&["logs", subcommand, "--query", "*"][..], extra].concat()
}

#[test]
fn list_and_query_try_flex_by_default() {
    for subcommand in ["list", "query"] {
        let mut server = Server::new();
        let flex = search_mock(&mut server, "flex", 200, EMPTY_PAGE);
        payload(pup(&server, &logs_args(subcommand, &[])));
        flex.assert();
    }
}

#[test]
fn list_and_query_fall_back_to_indexed_logs_after_flex_403() {
    for subcommand in ["list", "query"] {
        let mut server = Server::new();
        let flex = search_mock(
            &mut server,
            "flex",
            403,
            r#"{"errors":["Flex storage forbidden"]}"#,
        );
        let fallback = server
            .mock("POST", LOGS_SEARCH_PATH)
            .match_query(Matcher::Any)
            .with_header("content-type", "application/json")
            .with_body(EMPTY_PAGE)
            .expect(1)
            .create();

        let output = pup(&server, &logs_args(subcommand, &[]));
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        payload(output);
        assert!(
            stderr.contains("retried with indexed logs"),
            "{subcommand}: missing fallback warning: {stderr}"
        );
        flex.assert();
        fallback.assert();
    }
}

#[test]
fn list_and_query_send_explicit_storage_as_given() {
    for subcommand in ["list", "query"] {
        let mut server = Server::new();
        let indexes = search_mock(&mut server, "indexes", 200, EMPTY_PAGE);
        payload(pup(
            &server,
            &logs_args(subcommand, &["--storage", "indexes"]),
        ));
        indexes.assert();
    }
}

#[test]
fn list_and_query_do_not_retry_explicit_flex_after_403() {
    for subcommand in ["list", "query"] {
        let mut server = Server::new();
        let flex = search_mock(
            &mut server,
            "flex",
            403,
            r#"{"errors":["Flex storage forbidden"]}"#,
        );
        let fallback = server
            .mock("POST", LOGS_SEARCH_PATH)
            .match_query(Matcher::Any)
            .with_header("content-type", "application/json")
            .with_body(EMPTY_PAGE)
            .expect(0)
            .create();
        let output = pup(&server, &logs_args(subcommand, &["--storage", "flex"]));
        assert!(
            !output.status.success(),
            "{subcommand}: explicit --storage flex must not fall back"
        );
        flex.assert();
        fallback.assert();
    }
}

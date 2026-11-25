//! Every API request names the running command and whether it is generated
//! from the API spec or hand-written, so traffic can be split by command source.
// Release executables compile out the PUP_MOCK_SERVER override used here.
#![cfg(debug_assertions)]

#[path = "support/idp_cli.rs"]
mod support;

use mockito::{Matcher, Server};
use support::{payload, pup};

#[test]
fn generated_command_request_carries_its_path_and_source() {
    let mut server = Server::new();
    let request = server
        .mock("GET", "/api/v2/domain_allowlist")
        .match_header(
            "user-agent",
            Matcher::Regex(
                r"^pup/\S+ \(.*; cmd domain-allowlist\.get; cmd-source generated\)$".into(),
            ),
        )
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"data": {"attributes": {"domains": ["@example.com"], "enabled": false}, "id": "00000000-0000-0000-0000-000000000002", "type": "domain_allowlist"}}"#,
        )
        .create();
    payload(pup(&server, &["domain-allowlist", "get"]));
    request.assert();
}

#[test]
fn hand_written_command_request_carries_its_path_and_source() {
    let mut server = Server::new();
    let request = server
        .mock("POST", "/api/v2/logs/events/search")
        .match_query(Matcher::Any)
        .match_header(
            "user-agent",
            Matcher::Regex(r"^pup/\S+ \(.*; cmd logs\.search; cmd-source hand_written\)$".into()),
        )
        .with_header("content-type", "application/json")
        .with_body(r#"{"data": [], "meta": {"page": {}}}"#)
        .create();
    payload(pup(&server, &["logs", "search", "--query", "*"]));
    request.assert();
}

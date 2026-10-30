//! `logs list` and `logs query` share `logs search`'s `auto` storage default.
#[path = "support/idp_cli.rs"]
mod support;

use mockito::{Matcher, Server};
use support::{payload, pup};

#[test]
fn list_and_query_try_flex_by_default() {
    for subcommand in ["list", "query"] {
        let mut server = Server::new();
        let flex = server
            .mock("POST", "/api/v2/logs/events/search")
            .match_query(Matcher::Any)
            .match_body(Matcher::Regex(r#""storage_tier":"flex""#.to_string()))
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": [], "meta": {"page": {}}}"#)
            .expect(1)
            .create();
        payload(pup(&server, &["logs", subcommand, "--query", "*"]));
        flex.assert();
    }
}

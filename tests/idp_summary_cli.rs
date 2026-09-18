//! Run the actual CLI against a local UEG contract fixture.
#[path = "support/idp_cli.rs"]
mod support;

use mockito::{Matcher, Server};
use serde_json::json;
use support::{payload, pup};

#[test]
fn facets_preserve_null_counts_and_continuation() {
    let mut server = Server::new();
    let request = server
        .mock("GET", "/api/v2/idp/entity_graph/facets")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("query".into(), "kind:service".into()),
            Matcher::UrlEncoded("facets".into(), "owner".into()),
            Matcher::UrlEncoded("page[limit]".into(), "1".into()),
        ]))
        .with_header("content-type", "application/json")
        .with_body(
            json!({"data": [{"id": "facet", "type": "facet", "attributes": {
            "facet_name": "owner", "null_count": 1, "values": [{"value": "payments", "count": 2}]
        }}], "meta": {"page": {"next_cursor": "next"}}})
            .to_string(),
        )
        .create();
    let result = payload(pup(
        &server,
        &[
            "--no-agent",
            "idp",
            "entities",
            "facets",
            "kind:service",
            "--facet",
            "owner",
            "--limit",
            "1",
        ],
    ));
    assert_eq!(result["results"][0]["null_count"], 1);
    assert_eq!(result["results"][0]["values"][0]["count"], 2);
    assert_eq!(result["page"]["next_cursor"], "next");
    assert_eq!(result["page"]["truncated"], true);
    request.assert();
}

#[test]
fn facets_bound_provider_responses_without_fabricating_a_cursor() {
    let mut server = Server::new();
    let response = json!({"data": [{"attributes": {
        "facet_name": "owner", "null_count": 9,
        "values": [{"value": "a", "count": 2}, {"value": "b", "count": 1}]
    }}]});
    let request = server
        .mock("GET", "/api/v2/idp/entity_graph/facets")
        .match_query(Matcher::Any)
        .with_header("content-type", "application/json")
        .with_body(response.to_string())
        .expect(2)
        .create();
    let args = [
        "idp",
        "entities",
        "facets",
        "kind:service",
        "--facet",
        "owner",
        "--limit",
        "1",
    ];
    let result = payload(pup(&server, &[&["--agent"][..], &args].concat()));
    let data = &result["data"];
    assert_eq!(data["results"][0]["values"].as_array().unwrap().len(), 1);
    assert_eq!(data["results"][0]["values_returned"], 2);
    assert_eq!(data["results"][0]["values_truncated"], true);
    assert_eq!(data["results"][0]["null_count"], 9);
    assert!(data["page"]["next_cursor"].is_null());
    assert_eq!(result["metadata"]["truncated"], true);
    assert!(data["warnings"][0]
        .as_str()
        .unwrap()
        .contains("sampled locally"));
    let raw = payload(pup(
        &server,
        &[&["--no-agent"][..], &args, &["--raw"]].concat(),
    ));
    assert_eq!(raw, response);
    request.assert();
}

#[test]
fn facets_reject_malformed_values_instead_of_reporting_empty_vocabulary() {
    let mut server = Server::new();
    let request = server
        .mock("GET", "/api/v2/idp/entity_graph/facets")
        .match_query(Matcher::Any)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":[{"attributes":{"facet_name":"owner"}}]}"#)
        .create();
    let output = pup(
        &server,
        &[
            "--no-agent",
            "idp",
            "entities",
            "facets",
            "kind:service",
            "--facet",
            "owner",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("expected a values array"));
    request.assert();
}

#[test]
fn grouped_counts_use_jsonapi_and_work_in_read_only_agent_mode() {
    let mut server = Server::new();
    let request = server.mock("POST", "/api/v2/idp/entity_graph/aggregate")
        .match_header("content-type", "application/json")
        .match_body(Matcher::Json(json!({"data": {"attributes": {
            "query": "kind:service", "group_by": ["owner"],
            "metrics": [{"name": "count", "func": "count", "filter": "*"},
                {"name": "alerting", "func": "count", "filter": "alert_monitors_count:>0"}],
            "order_by": ["alerting:desc"], "pagination": {"limit": 25, "cursor": ""}
        }}})))
        .with_header("content-type", "application/json")
        .with_body(json!({"data": [{"id": "payments", "type": "aggregate", "attributes": {
            "group": {"owner": "payments"}, "metrics": [{"name": "count", "value": 2}, {"name": "alerting", "value": 1}]
        }}, {"id": "missing", "type": "aggregate", "attributes": {
            "group": {"owner": null}, "metrics": [{"name": "count", "value": 1}, {"name": "alerting", "value": 0}]
        }}]}).to_string()).create();
    let result = payload(pup(
        &server,
        &[
            "--agent",
            "idp",
            "entities",
            "aggregate",
            "kind:service",
            "--group-by",
            "owner",
            "--count",
            "alerting=alert_monitors_count:>0",
            "--order-by",
            "alerting:desc",
        ],
    ));
    assert_eq!(result["data"]["results"][0]["metrics"][1]["value"], 1);
    assert!(result["data"]["results"][1]["group"]["owner"].is_null());
    assert_eq!(result["data"]["page"]["truncated"], false);
    request.assert();
}

#[test]
fn unsupported_summary_provider_is_an_error_not_an_empty_inventory() {
    let mut server = Server::new();
    let request = server
        .mock("GET", "/api/v2/idp/entity_graph/facets")
        .match_query(Matcher::Any)
        .with_status(400)
        .with_body("facets not supported for this provider")
        .create();
    let output = pup(
        &server,
        &[
            "--no-agent",
            "idp",
            "entities",
            "facets",
            "kind:github.repository",
            "--facet",
            "name",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not supported"));
    request.assert();
}

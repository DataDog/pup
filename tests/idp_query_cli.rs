//! Run the actual CLI against a local UEG contract fixture.
#[path = "support/idp_cli.rs"]
mod support;

use mockito::{Matcher, Server};
use serde_json::{json, Value};
use support::{payload, pup};

use std::process::Output;

fn service_response() -> Value {
    json!({"data": [{"type": "service", "id": "ref:service:checkout", "attributes": {
        "name": "checkout", "display_name": "Checkout API", "active_incidents_count": null
    }, "relationships": {"runtime_downstream_services": {"data": [{
        "type": "service", "id": "ref:service:payments",
        "meta": {"requests_count": 42, "error_rate": null}
    }]}}}], "meta": {"warnings": {"omitted_relations": [
        {"relation": "owner_teams", "target_kind": "team", "reason": "access denied"}
    ]}}})
}

fn service_schema(server: &mut Server) -> mockito::Mock {
    server.mock("GET", "/api/v2/idp/entity_graph/kinds/service")
        .with_header("content-type", "application/json")
        .with_body(json!({"data": {"id": "service", "attributes": {
            "attribute_types": {
                "name": {"dataType": "string"}, "display_name": {"dataType": "string"},
                "owner": {"dataType": "string"}, "active_incidents_count": {"dataType": "int"},
                "requests_per_second": {"dataType": "float", "scopes": [{"name": "env"}]}
            }, "relations": {"owner_teams": {"target_kind": "team"}, "runtime_downstream_services": {"target_kind": "service"}}
        }}}).to_string()).create()
}

#[test]
fn query_preserves_evidence_in_human_agent_jq_and_raw_modes() {
    let mut server = Server::new();
    let _schema = service_schema(&mut server);
    let response = service_response();
    let request = server
        .mock("GET", "/api/v2/idp/entity_graph/entities")
        .match_query(Matcher::UrlEncoded(
            "fields[service]".into(),
            "name,display_name,active_incidents_count".into(),
        ))
        .with_header("content-type", "application/json")
        .with_body(response.to_string())
        .expect(4)
        .create();
    let args = [
        "idp",
        "entities",
        "query",
        "kind:service",
        "--field",
        "name,display_name,active_incidents_count",
    ];
    let human = payload(pup(&server, &[&["--no-agent"][..], &args].concat()));
    assert_eq!(human["results"][0]["fields"]["name"], "checkout");
    assert_eq!(
        human["results"][0]["fields"]["display_name"],
        "Checkout API"
    );
    assert!(human["results"][0]["fields"]["active_incidents_count"].is_null());
    assert_eq!(
        human["results"][0]["relationships"]["runtime_downstream_services"]["sample"][0]
            ["edge_fields"]["requests_count"],
        42
    );
    assert_eq!(human["server_warnings"], response["meta"]["warnings"]);
    let agent = payload(pup(&server, &[&["--agent"][..], &args].concat()));
    assert_eq!(agent["data"], human);
    let filtered = payload(pup(
        &server,
        &[
            &["--no-agent", "--jq", ".results[0].fields.name"][..],
            &args,
        ]
        .concat(),
    ));
    assert_eq!(filtered, "checkout");
    let raw = payload(pup(
        &server,
        &[&["--no-agent"][..], &args, &["--raw"]].concat(),
    ));
    assert_eq!(raw, response);
    request.assert();
}

#[test]
fn related_fields_and_edge_measurements_use_the_ueg_wire_contract() {
    let mut server = Server::new();
    let _schema = service_schema(&mut server);
    let team = server
        .mock("GET", "/api/v2/idp/entity_graph/kinds/team")
        .with_header("content-type", "application/json")
        .with_body(
            json!({"data": {"id": "team", "attributes": {"attribute_types": {
                "handle": {"dataType": "string"}, "user_count": {"dataType": "int"}
            }}}})
            .to_string(),
        )
        .create();
    let request = server
        .mock("GET", "/api/v2/idp/entity_graph/entities")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("fields[service]".into(), "name,owner".into()),
            Matcher::UrlEncoded("fields[team]".into(), "handle,user_count".into()),
            Matcher::UrlEncoded(
                "fields[service.runtime_downstream_services]".into(),
                "requests_count,error_rate".into(),
            ),
        ]))
        .with_header("content-type", "application/json")
        .with_body(service_response().to_string())
        .create();
    let result = payload(pup(
        &server,
        &[
            "--no-agent",
            "idp",
            "entities",
            "query",
            "kind:service",
            "--field",
            "name,owner",
            "--include",
            "owner_teams,runtime_downstream_services",
            "--fields",
            "team=handle,user_count",
            "--edge-fields",
            "runtime_downstream_services=requests_count,error_rate",
        ],
    ));
    assert_eq!(
        result["query"]["fields_by_kind"]["team"],
        json!(["handle", "user_count"])
    );
    request.assert();
    team.assert();
}

#[test]
fn field_typos_fail_before_querying_entities() {
    let mut server = Server::new();
    let schema = service_schema(&mut server);
    let entities = server
        .mock("GET", "/api/v2/idp/entity_graph/entities")
        .match_query(Matcher::Any)
        .expect(0)
        .create();
    let output = pup(
        &server,
        &[
            "--no-agent",
            "idp",
            "entities",
            "query",
            "kind:service",
            "--field",
            "owenr",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field"));
    schema.assert();
    entities.assert();
}

#[test]
fn malformed_entity_response_exits_unsuccessfully() {
    let mut server = Server::new();
    let request = server
        .mock("GET", "/api/v2/idp/entity_graph/entities")
        .match_query(Matcher::Any)
        .with_header("content-type", "application/json")
        .with_body("{\"unexpected\": true}")
        .create();
    let output = pup(
        &server,
        &["--no-agent", "idp", "entities", "query", "kind:service"],
    );
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("missing field `data`"), "{error}");
    request.assert();
}

#[test]
fn continuation_replays_projections_absolute_time_scope_and_sorting() {
    let mut server = Server::new();
    let _schema = service_schema(&mut server);
    let common = vec![
        Matcher::UrlEncoded("fields[service]".into(), "name,requests_per_second".into()),
        Matcher::UrlEncoded("time[start]".into(), "1789639200000".into()),
        Matcher::UrlEncoded("time[end]".into(), "1789642800000".into()),
        Matcher::UrlEncoded("properties[service][scope][*][env]".into(), "prod".into()),
        Matcher::UrlEncoded("order_by".into(), "name:desc".into()),
        Matcher::UrlEncoded("free_text_match".into(), "partial".into()),
        Matcher::UrlEncoded("meta[fields]".into(), "total_count".into()),
    ];
    let first = server
        .mock("GET", "/api/v2/idp/entity_graph/entities")
        .match_query(Matcher::AllOf(common.clone()))
        .with_header("content-type", "application/json")
        .with_body(
            json!({"data": [], "meta": {"page": {"next_cursor": "opaque cursor"}}}).to_string(),
        )
        .create();
    let result = payload(pup(
        &server,
        &[
            "--no-agent",
            "--org",
            "fixture-org",
            "idp",
            "entities",
            "query",
            "kind:service",
            "--field",
            "name,requests_per_second",
            "--from",
            "2026-09-17T10:00:00Z",
            "--to",
            "2026-09-17T11:00:00Z",
            "--scope",
            "env=prod",
            "--order-by",
            "name:desc",
            "--free-text-match",
            "partial",
            "--include-total-count",
        ],
    ));
    first.assert();
    let continuation = result["next_request"]["args"].as_array().unwrap();
    assert!(continuation
        .windows(2)
        .any(|pair| pair == [json!("--org"), json!("fixture-org")]));
    let mut continued = common;
    continued.push(Matcher::UrlEncoded(
        "page[cursor]".into(),
        "opaque cursor".into(),
    ));
    let second = server
        .mock("GET", "/api/v2/idp/entity_graph/entities")
        .match_query(Matcher::AllOf(continued))
        .with_header("content-type", "application/json")
        .with_body("{\"data\":[]}")
        .create();
    let mut args = vec!["--no-agent"];
    args.extend(
        result["next_request"]["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap()),
    );
    let next = payload(pup(&server, &args));
    assert_eq!(next["page"]["truncated"], false);
    assert!(next.get("next_request").is_none());
    second.assert();
}

#[test]
fn undeclared_scope_is_rejected_before_the_entity_request() {
    let mut server = Server::new();
    let _schema = service_schema(&mut server);
    let output = pup(
        &server,
        &[
            "--no-agent",
            "idp",
            "entities",
            "query",
            "kind:service",
            "--scope",
            "invented=prod",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not declared"));
}

fn entity_page(
    server: &mut Server,
    cursor: &str,
    limit: usize,
    names: &[&str],
    next: &str,
) -> mockito::Mock {
    let rows: Vec<Value> = names.iter().map(|name| {
        json!({"type": "service", "id": format!("ref:service:{name}"), "attributes": {"name": name}})
    }).collect();
    server
        .mock("GET", "/api/v2/idp/entity_graph/entities")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("page[cursor]".into(), cursor.into()),
            Matcher::UrlEncoded("page[limit]".into(), limit.to_string()),
            Matcher::UrlEncoded("fields[service]".into(), "name".into()),
        ]))
        .with_header("content-type", "application/json")
        .with_body(
            json!({"data": rows, "meta": {
                "page": {"next_cursor": next}, "warnings": {"fixture_page": cursor}
            }})
            .to_string(),
        )
        .create()
}

fn bounded_query(server: &Server, budget: &str) -> Output {
    pup(
        server,
        &[
            "--no-agent",
            "idp",
            "entities",
            "query",
            "kind:service",
            "--field",
            "name",
            "--cursor",
            "start",
            "--limit",
            "2",
            "--max-results",
            budget,
        ],
    )
}

#[test]
fn bounded_inventory_sizes_the_last_request_and_replays_its_continuation() {
    let mut server = Server::new();
    let _schema = service_schema(&mut server);
    let first = entity_page(&mut server, "start", 2, &["a", "b"], "second");
    let second = entity_page(&mut server, "second", 1, &["c"], "third");
    let result = payload(bounded_query(&server, "3"));
    assert_eq!(result["count"], 3);
    assert_eq!(result["page"]["pages_fetched"], 2);
    assert_eq!(result["page"]["stop_reason"], "result_limit");
    assert_eq!(result["page"]["next_cursor"], "third");
    assert_eq!(result["additional_page_warnings"][0]["page"], 2);
    assert_eq!(
        result["additional_page_warnings"][0]["warnings"]["fixture_page"],
        "second"
    );
    first.assert();
    second.assert();

    let third = entity_page(&mut server, "third", 2, &["d"], "");
    let mut args = vec!["--no-agent"];
    args.extend(
        result["next_request"]["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap()),
    );
    let continued = payload(pup(&server, &args));
    assert_eq!(continued["page"]["stop_reason"], "end_of_results");
    assert_eq!(continued["page"]["max_results"], 3);
    assert_eq!(continued["count"], 1);
    third.assert();
}

#[test]
fn bounded_inventory_accepts_a_terminal_empty_page_without_stale_truncation() {
    let mut server = Server::new();
    let _schema = service_schema(&mut server);
    let first = entity_page(&mut server, "start", 2, &["a", "b"], "end");
    let last = entity_page(&mut server, "end", 2, &[], "");
    let result = payload(bounded_query(&server, "10"));
    assert_eq!(result["count"], 2);
    assert_eq!(result["page"]["pages_fetched"], 2);
    assert_eq!(result["page"]["stop_reason"], "end_of_results");
    assert_eq!(result["page"]["truncated"], false);
    assert!(result.get("next_request").is_none());
    assert!(result.get("warnings").is_none());
    first.assert();
    last.assert();
}

#[test]
fn bounded_inventory_rejects_nonprogress_and_oversized_responses() {
    for (names, next, error) in [
        (vec!["a"], "start", "repeated a pagination cursor"),
        (vec![], "next", "empty page with a continuation cursor"),
        (
            vec!["a", "b", "c"],
            "next",
            "more entities than the requested page limit",
        ),
    ] {
        let mut server = Server::new();
        let _schema = service_schema(&mut server);
        let page = entity_page(&mut server, "start", 2, &names, next);
        let output = bounded_query(&server, "10");
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(error));
        page.assert();
    }
}

#[test]
fn later_page_failure_does_not_print_a_successful_partial_inventory() {
    let mut server = Server::new();
    let _schema = service_schema(&mut server);
    let first = entity_page(&mut server, "start", 2, &["a", "b"], "failed");
    let failed = server
        .mock("GET", "/api/v2/idp/entity_graph/entities")
        .match_query(Matcher::UrlEncoded("page[cursor]".into(), "failed".into()))
        .with_status(503)
        .with_body("fixture backend unavailable")
        .create();
    let output = bounded_query(&server, "10");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("failed to query Datadog entities"));
    first.assert();
    failed.assert();
}

#[test]
fn bounded_inventory_rejects_invalid_budgets_and_raw_mode() {
    let server = Server::new();
    for budget in ["0", "10001"] {
        let output = bounded_query(&server, budget);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("max-results"));
    }
    let output = pup(
        &server,
        &[
            "idp",
            "entities",
            "query",
            "kind:service",
            "--max-results",
            "10",
            "--raw",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be used with"));
}

#[test]
fn bare_text_modes_preserve_the_query_and_warn_about_empty_fuzzy_results() {
    let mut server = Server::new();
    for (mode, response) in [
        ("partial", service_response()),
        ("fuzzy", json!({"data": []})),
    ] {
        let request = server
            .mock("GET", "/api/v2/idp/entity_graph/entities")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded(
                    "query".into(),
                    "kind:service AND owner:payments AND catalog".into(),
                ),
                Matcher::UrlEncoded("free_text_match".into(), mode.into()),
            ]))
            .with_header("content-type", "application/json")
            .with_body(response.to_string())
            .create();
        let result = payload(pup(
            &server,
            &[
                "--no-agent",
                "idp",
                "entities",
                "query",
                "kind:service AND owner:payments AND catalog",
                "--free-text-match",
                mode,
            ],
        ));
        if mode == "fuzzy" {
            assert_eq!(result["count"], 0);
            assert!(result["warnings"].as_array().unwrap().iter().any(|w| w
                .as_str()
                .unwrap()
                .contains("Verify with --free-text-match partial")));
        } else {
            assert_eq!(result["count"], 1);
        }
        request.assert();
    }
}

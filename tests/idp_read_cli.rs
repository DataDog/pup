//! Run the actual CLI against a local UEG contract fixture.
#[path = "support/idp_cli.rs"]
mod support;

use mockito::{Matcher, Server};
use serde_json::json;
use support::{payload, pup};

#[test]
fn assist_requests_health_counts_and_preserves_unknowns_and_enrichment_errors() {
    let mut server = Server::new();
    let entity = server.mock("GET", "/api/v2/idp/entity_graph/entities")
        .match_query(Matcher::Any)
        .match_request(|request| {
            let url = reqwest::Url::parse(&format!("http://fixture{}", request.path_and_query())).unwrap();
            let params: std::collections::HashMap<_, _> = url.query_pairs().collect();
            let service: Vec<_> = params.get("fields[service]").map(|s| s.split(',').collect()).unwrap_or_default();
            let team: Vec<_> = params.get("fields[team]").map(|s| s.split(',').collect()).unwrap_or_default();
            ["name", "contacts", "links", "active_incidents_count", "alert_monitors_count", "ok_slos_count", "breached_slos_count"]
                .iter().all(|field| service.contains(field)) && team.contains(&"id") && team.contains(&"user_count")
        })
        .with_header("content-type", "application/json")
        .with_body(json!({"data": [{"id":"ref:service:checkout", "type":"service", "attributes": {
            "name":"checkout", "active_incidents_count":0, "alert_monitors_count":2, "breached_slos_count":null
        }}], "included":[{"id":"ref:team:team-id", "type":"team", "attributes":{
            "id":"team-id", "name":"Payments", "handle":"payments", "user_count":3
        }}]}).to_string()).create();
    let oncall = server
        .mock("GET", "/api/v2/on-call/teams/team-id/on-call")
        .match_query(Matcher::Any)
        .with_status(403)
        .with_body("forbidden")
        .create();
    let result = payload(pup(&server, &["--no-agent", "idp", "assist", "checkout"]));
    assert_eq!(result["health"]["incidents"]["active"], 0);
    assert_eq!(result["health"]["monitors"]["alert"], 2);
    assert!(result["health"]["slos"]["breached"].is_null());
    assert_eq!(result["owner"]["member_count"], 3);
    assert!(result["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w.as_str().unwrap().contains("responders are unknown")));
    entity.assert();
    oncall.assert();
}

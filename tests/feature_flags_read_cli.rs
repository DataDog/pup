//! Regression coverage for reading feature flags with environment targeting.
#[path = "support/idp_cli.rs"]
mod support;

use mockito::Server;
use serde_json::{json, Value};
use support::{payload, pup};

const FLAG_ID: &str = "00000000-0000-4000-8000-000000000001";
const FLAG_PATH: &str = "/api/v2/feature-flags/00000000-0000-4000-8000-000000000001";

fn flag_response(allocations: Value) -> Value {
    json!({
        "data": {
            "id": FLAG_ID,
            "type": "feature-flags",
            "attributes": {
                "key": "targeting-regression",
                "name": "Targeting regression",
                "description": "Client-side boolean flag",
                "distribution_channel": "client",
                "value_type": "BOOLEAN",
                "variants": [],
                "feature_flag_environments": [{
                    "environment_id": "00000000-0000-4000-8000-000000000002",
                    "environment_name": "Development",
                    "status": "ENABLED",
                    "allocations": allocations
                }]
            }
        }
    })
}

#[test]
fn get_preserves_targeting_allocations() {
    let mut server = Server::new();
    // Captured from GET /api/v2/feature-flags/{id} in ddstaging; identifiers,
    // names, targeting values, and timestamps are sanitized, preserving structure.
    let expected: Value = serde_json::from_str(include_str!(
        "fixtures/feature_flags/flag_with_targeting.json"
    ))
    .unwrap();
    let mock = server
        .mock("GET", FLAG_PATH)
        .match_header("authorization", "Bearer local-fixture-token")
        .with_header("content-type", "application/json")
        .with_body(expected.to_string())
        .create();

    let actual = payload(pup(&server, &["feature-flags", "flags", "get", FLAG_ID]));
    assert_eq!(actual, expected);
    mock.assert();
}

#[test]
fn get_preserves_flags_without_targeting() {
    let mut server = Server::new();
    for allocations in [Value::Null, json!([]), json!({})] {
        let expected = flag_response(allocations);
        let mock = server
            .mock("GET", FLAG_PATH)
            .with_header("content-type", "application/json")
            .with_body(expected.to_string())
            .create();
        let actual = payload(pup(&server, &["feature-flags", "flags", "get", FLAG_ID]));
        assert_eq!(actual, expected);
        mock.assert();
        mock.remove();
    }
}

#[test]
fn get_rejects_invalid_id_without_requesting_api() {
    let mut server = Server::new();
    let mock = server.mock("GET", mockito::Matcher::Any).expect(0).create();
    let output = pup(&server, &["feature-flags", "flags", "get", "not-a-uuid"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid feature flag ID"));
    mock.assert();
}

#[test]
fn get_reports_api_errors() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", FLAG_PATH)
        .with_status(404)
        .with_body(r#"{"errors":["Feature flag not found"]}"#)
        .create();
    let output = pup(&server, &["feature-flags", "flags", "get", FLAG_ID]);
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("failed to get feature flag"), "{error}");
    assert!(error.contains("404"), "{error}");
    mock.assert();
}

#[test]
fn get_reports_invalid_json() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", FLAG_PATH)
        .with_header("content-type", "application/json")
        .with_body("{invalid json}")
        .create();
    let output = pup(&server, &["feature-flags", "flags", "get", FLAG_ID]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("failed to get feature flag"));
    mock.assert();
}

#[test]
fn get_preserves_rate_limit_errors() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", FLAG_PATH)
        .with_status(429)
        .with_header("x-ratelimit-name", "get_feature_flag")
        .with_body(r#"{"errors":["Too Many Requests"]}"#)
        .create();
    let output = pup(&server, &["feature-flags", "flags", "get", FLAG_ID]);
    let expected_exit_code = if cfg!(unix) { 429 % 256 } else { 429 };
    assert_eq!(output.status.code(), Some(expected_exit_code));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("failed to get feature flag"), "{error}");
    assert!(error.contains("rule: get_feature_flag"), "{error}");
    mock.assert();
}

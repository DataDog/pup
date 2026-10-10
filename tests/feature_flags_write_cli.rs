//! Write responses must preserve targeting arrays without SDK deserialization.
use mockito::{Matcher, Server};
use serde_json::{json, Value};
use std::process::{Command, Output};

const FLAG_ID: &str = "00000000-0000-4000-8000-000000000001";

struct Operation {
    action: &'static str,
    method: &'static str,
    path: String,
    request: Option<Value>,
}

fn operations() -> Vec<Operation> {
    let path = format!("/api/v2/feature-flags/{FLAG_ID}");
    vec![
        Operation {
            action: "create",
            method: "POST",
            path: "/api/v2/feature-flags".into(),
            request: Some(json!({"data":{"type":"feature-flags","attributes":{
                "key":"test-flag","name":"Test flag","value_type":"BOOLEAN",
                "variants":[{"key":"on","name":"On","value":"true"}]
            }}})),
        },
        Operation {
            action: "update",
            method: "PUT",
            path: path.clone(),
            request: Some(
                json!({"data":{"type":"feature-flags","attributes":{"name":"Updated flag"}}}),
            ),
        },
        Operation {
            action: "archive",
            method: "POST",
            path: format!("{path}/archive"),
            request: None,
        },
        Operation {
            action: "unarchive",
            method: "POST",
            path: format!("{path}/unarchive"),
            request: None,
        },
    ]
}

fn response() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/feature_flags/flag_with_targeting.json"
    ))
    .unwrap()
}

fn run(server: &Server, op: &Operation, input: Option<&str>, flags: &[&str], id: &str) -> Output {
    let dir = std::env::temp_dir().join(format!(
        "pup-flag-write-{}-{}",
        std::process::id(),
        server.host_with_port().replace(':', "-")
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pup"));
    cmd.args(["--yes", "--output", "json"])
        .args(flags)
        .args(["feature-flags", "flags", op.action]);
    if op.action != "create" {
        cmd.arg(id);
    }
    if let Some(input) = input {
        let file = dir.join("request.json");
        std::fs::write(&file, input).unwrap();
        cmd.arg("--file").arg(file);
    }
    let output = cmd
        .env("PUP_MOCK_SERVER", server.url())
        .env("DD_ACCESS_TOKEN", "local-fixture-token")
        .env("DD_SITE", "datadoghq.com")
        .env("PUP_CONFIG_DIR", dir.join("config"))
        .env_remove("DD_ORG")
        .env_remove("DD_API_KEY")
        .env_remove("DD_APP_KEY")
        .env_remove("PUP_FILTER")
        .env_remove("PUP_READ_ONLY")
        .env_remove("DD_READ_ONLY")
        .env_remove("DD_CLI_READ_ONLY")
        .output()
        .expect("execute pup");
    std::fs::remove_dir_all(dir).unwrap();
    output
}

#[test]
fn writes_preserve_targeting_and_request_contracts() {
    let mut server = Server::new();
    for op in operations() {
        for shape in ["array", "null", "empty-array", "legacy-map"] {
            let mut expected = response();
            if shape != "array" {
                for env in expected["data"]["attributes"]["feature_flag_environments"]
                    .as_array_mut()
                    .unwrap()
                {
                    env["allocations"] = match shape {
                        "null" => Value::Null,
                        "empty-array" => json!([]),
                        _ => json!({}),
                    };
                }
            }
            let body = op.request.as_ref().map(Value::to_string);
            let mock = server
                .mock(op.method, op.path.as_str())
                .match_header("authorization", "Bearer local-fixture-token")
                .match_header("accept", "application/json")
                .match_header(
                    "content-type",
                    if op.request.is_some() {
                        Matcher::Exact("application/json".into())
                    } else {
                        Matcher::Missing
                    },
                )
                .match_body(
                    op.request
                        .clone()
                        .map(Matcher::Json)
                        .unwrap_or_else(|| Matcher::Exact(String::new())),
                )
                .with_status(if op.action == "create" { 201 } else { 200 })
                .with_header("content-type", "application/json")
                .with_body(expected.to_string())
                .create();
            let output = run(&server, &op, body.as_deref(), &[], FLAG_ID);
            assert!(
                output.status.success(),
                "{} {shape}: {}",
                op.action,
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                serde_json::from_slice::<Value>(&output.stdout).unwrap(),
                expected
            );
            mock.assert();
            mock.remove();
        }
    }
}

#[test]
fn writes_preserve_http_errors_and_rate_limits() {
    let mut server = Server::new();
    for op in operations() {
        for status in [403, 404, 429] {
            let mock = server
                .mock(op.method, op.path.as_str())
                .with_status(status)
                .with_header("x-ratelimit-name", "feature_flag_write")
                .with_body(r#"{"errors":["Write rejected"]}"#)
                .create();
            let body = op.request.as_ref().map(Value::to_string);
            let output = run(&server, &op, body.as_deref(), &[], FLAG_ID);
            assert!(!output.status.success());
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(
                error.contains(&format!("failed to {} feature flag", op.action)),
                "{error}"
            );
            assert!(error.contains(&status.to_string()), "{error}");
            if status == 429 {
                assert_eq!(
                    output.status.code(),
                    Some(if cfg!(unix) { 429 % 256 } else { 429 })
                );
                assert!(error.contains("rule: feature_flag_write"), "{error}");
            }
            mock.assert();
            mock.remove();
        }
    }
}

#[test]
fn writes_report_malformed_success_responses_without_resending() {
    let mut server = Server::new();
    for op in operations() {
        let mock = server
            .mock(op.method, op.path.as_str())
            .expect(1)
            .with_status(200)
            .with_body("{bad json}")
            .create();
        let body = op.request.as_ref().map(Value::to_string);
        let output = run(&server, &op, body.as_deref(), &[], FLAG_ID);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr)
            .contains(&format!("failed to {} feature flag", op.action)));
        mock.assert();
        mock.remove();
    }
}

#[test]
fn invalid_request_files_are_rejected_before_sending() {
    let mut server = Server::new();
    let mocks = [
        server.mock("POST", Matcher::Any).expect(0).create(),
        server.mock("PUT", Matcher::Any).expect(0).create(),
    ];
    for op in operations().into_iter().filter(|op| op.request.is_some()) {
        for input in [
            "{bad json}",
            "{}",
            r#"{"data":{"type":"feature-flags","attributes":null}}"#,
        ] {
            let output = run(&server, &op, Some(input), &[], FLAG_ID);
            assert!(!output.status.success());
            assert!(!String::from_utf8_lossy(&output.stderr).contains("HTTP"));
        }
    }
    for mock in mocks {
        mock.assert();
    }
}

#[test]
fn invalid_ids_are_rejected_before_sending() {
    let mut server = Server::new();
    let mocks = [
        server.mock("POST", Matcher::Any).expect(0).create(),
        server.mock("PUT", Matcher::Any).expect(0).create(),
    ];
    for op in operations().into_iter().filter(|op| op.action != "create") {
        let body = op.request.as_ref().map(Value::to_string);
        let output = run(&server, &op, body.as_deref(), &[], "not-a-uuid");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("invalid feature flag ID"));
    }
    for mock in mocks {
        mock.assert();
    }
}

#[test]
fn read_only_blocks_writes_before_sending() {
    let mut server = Server::new();
    let mocks = [
        server.mock("POST", Matcher::Any).expect(0).create(),
        server.mock("PUT", Matcher::Any).expect(0).create(),
    ];
    for op in operations() {
        let body = op.request.as_ref().map(Value::to_string);
        let output = run(&server, &op, body.as_deref(), &["--read-only"], FLAG_ID);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("read-only"));
    }
    for mock in mocks {
        mock.assert();
    }
}

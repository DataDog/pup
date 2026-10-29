//! Governance safety and output contracts through the real CLI, without live writes.
use mockito::{Matcher, Server};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);
const CONTROL: &str = "/api/v2/governance/control/test";
const MITIGATE: &str = "/api/v2/governance/detections/mitigate";
const DETECTION: &str = "/api/v2/governance/detections/d1";

fn control() -> Value {
    json!({"data":{"attributes":{"mitigations":[{"id":"manual_fix","title":"Fix resource",
        "description":"Changes the resource","execution_modes":["manual"],"permissions":["dashboards_write"]}]}}})
}

fn current() -> Value {
    json!({"data":{"id":"d1","type":"governance_control_detection","attributes":{
        "state":"active","mitigate_after":"2999-11-30T00:00:00Z","assigned_to":null,"assigned_team":null}}})
}

fn body() -> Value {
    json!({"data":{"type":"governance_control_detection","attributes":{
        "detection_type":"test","detection_ids":["d1","d2"],"mitigation_type":"manual_fix"}}})
}

fn mitigate() -> Vec<&'static str> {
    vec![
        "governance",
        "detections",
        "mitigate",
        "test",
        "--ids",
        "d1,d2",
        "--mitigation-type",
        "manual_fix",
    ]
}

fn update() -> Vec<&'static str> {
    vec![
        "governance",
        "detections",
        "update",
        "d1",
        "--state",
        "exception",
    ]
}

fn run(url: &str, args: &[&str], stdin: &str, agent: bool) -> Output {
    let dir = std::env::temp_dir().join(format!(
        "pup-governance-{}-{}",
        std::process::id(),
        NEXT_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_pup"));
    // No inherited credentials, config, read-only settings, or agent markers.
    command
        .env_clear()
        .env("PUP_CONFIG_DIR", &dir)
        .env("DD_SITE", "datadoghq.com")
        .env("PUP_MOCK_SERVER", url)
        .env("DD_ACCESS_TOKEN", "local-governance-fixture-token")
        .args(["--output", "json"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Keep coverage instrumentation when cargo llvm-cov runs the binary tests.
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    if agent {
        command.env("CLAUDECODE", "1");
    }
    let mut child = command.spawn().expect("execute pup");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    output
}

fn success(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("standard JSON envelope")
}

#[test]
fn read_only_blocks_both_writes_even_dry_runs() {
    let mut server = Server::new();
    let mocks: Vec<_> = ["GET", "PATCH", "POST"]
        .iter()
        .map(|m| server.mock(*m, Matcher::Any).expect(0).create())
        .collect();
    for args in [mitigate(), update()] {
        for dry in [false, true] {
            let mut args = args.clone();
            args.push("--read-only");
            if dry {
                args.push("--dry-run");
            }
            let output = run(&server.url(), &args, "", false);
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("read-only"));
        }
    }
    for mock in mocks {
        mock.assert();
    }
}

#[test]
fn dry_run_previews_exact_requests_and_never_writes() {
    let mut server = Server::new();
    let control = server
        .mock("GET", CONTROL)
        .with_body(control().to_string())
        .create();
    let get = server
        .mock("GET", DETECTION)
        .with_body(current().to_string())
        .create();
    let post = server.mock("POST", Matcher::Any).expect(0).create();
    let patch = server.mock("PATCH", Matcher::Any).expect(0).create();
    let mut args = mitigate();
    args.push("--dry-run");
    let result = success(&run(&server.url(), &args, "", false));
    assert_eq!(result["status"], "success");
    assert_eq!(result["data"]["dry_run"], true);
    assert_eq!(
        result["data"]["request"],
        json!({"method":"POST","path":MITIGATE,"body":body()})
    );
    assert_eq!(result["data"]["mitigation"]["title"], "Fix resource");
    assert!(result["metadata"]["next_action"]
        .as_str()
        .unwrap()
        .contains("user's approval"));
    let mut args = update();
    args.push("--dry-run");
    let result = success(&run(&server.url(), &args, "", false));
    assert_eq!(result["data"]["dry_run"], true);
    assert_eq!(result["data"]["current"], current()["data"]);
    assert_eq!(result["data"]["request"]["method"], "PATCH");
    assert!(result["data"]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e.as_str().unwrap().contains("cleared")));
    for mock in [control, get, post, patch] {
        mock.assert();
    }
}

#[test]
fn no_and_eof_cancel_both_writes() {
    let mut server = Server::new();
    let control = server
        .mock("GET", CONTROL)
        .expect(2)
        .with_body(control().to_string())
        .create();
    let get = server
        .mock("GET", DETECTION)
        .expect(2)
        .with_body(current().to_string())
        .create();
    let post = server.mock("POST", Matcher::Any).expect(0).create();
    let patch = server.mock("PATCH", Matcher::Any).expect(0).create();
    for args in [mitigate(), update()] {
        for input in ["no\n", ""] {
            let output = run(&server.url(), &args, input, false);
            assert!(output.status.success());
            assert!(String::from_utf8_lossy(&output.stdout).contains("Operation cancelled."));
            assert!(String::from_utf8_lossy(&output.stderr).contains("Type 'yes' to confirm:"));
        }
    }
    for mock in [control, get, post, patch] {
        mock.assert();
    }
}

#[test]
fn yes_flag_agent_and_typed_yes_send_exactly_once() {
    let mut server = Server::new();
    let control = server
        .mock("GET", CONTROL)
        .expect(3)
        .with_body(control().to_string())
        .create();
    for (flags, input, agent, response) in [
        (vec!["--yes"], "", false, ""),
        (vec![], "", true, "Queued"),
        (vec![], "yes\n", false, "{}"),
    ] {
        let post = server
            .mock("POST", MITIGATE)
            .expect(1)
            .match_header("authorization", "Bearer local-governance-fixture-token")
            .match_body(Matcher::Json(body()))
            .with_status(202)
            .with_body(response)
            .create();
        let mut args = mitigate();
        args.extend(flags);
        let result = success(&run(&server.url(), &args, input, agent));
        assert_eq!(result["data"]["accepted"], true);
        assert!(result["metadata"]["next_action"]
            .as_str()
            .unwrap()
            .contains("asynchronously"));
        if response == "Queued" {
            assert_eq!(result["data"]["response"], "Queued");
        }
        if response.is_empty() {
            assert!(result["data"]["response"].is_null());
        }
        post.assert();
        post.remove();
    }
    control.assert();
}

#[test]
fn update_output_reports_preimage_and_effects() {
    let mut server = Server::new();
    let get = server
        .mock("GET", DETECTION)
        .with_body(current().to_string())
        .create();
    let mut after = current();
    after["data"]["attributes"]["state"] = json!("exception");
    after["data"]["attributes"]["mitigate_after"] = Value::Null;
    let patch = server.mock("PATCH", DETECTION).expect(1)
        .match_body(Matcher::Json(json!({"data":{"type":"governance_control_detection","attributes":{"state":"exception"}}})))
        .with_body(after.to_string()).create();
    let mut args = update();
    args.push("--yes");
    let result = success(&run(&server.url(), &args, "", false));
    assert_eq!(result["previous"], current()["data"]["attributes"]);
    assert_eq!(result["data"], after["data"]);
    assert!(result["effects"].to_string().contains("cleared"));
    get.assert();
    patch.assert();
}

#[test]
fn invalid_flags_and_local_update_fields_send_no_requests() {
    let mut server = Server::new();
    let mocks: Vec<_> = ["GET", "PATCH", "POST"]
        .iter()
        .map(|m| server.mock(*m, Matcher::Any).expect(0).create())
        .collect();
    for args in [
        vec![
            "governance",
            "detections",
            "list",
            "test",
            "--page-size",
            "0",
        ],
        vec![
            "governance",
            "detections",
            "list",
            "test",
            "--page-size",
            "10001",
        ],
        vec![
            "governance",
            "detections",
            "list",
            "test",
            "--state",
            "inactive",
        ],
        vec![
            "governance",
            "best-practices",
            "list",
            "--category",
            "unknown",
        ],
        vec![
            "governance",
            "detections",
            "mitigate",
            "test",
            "--ids",
            "d1",
        ],
        vec!["governance", "detections", "update", "d1"],
        vec![
            "governance",
            "detections",
            "update",
            "d1",
            "--state",
            "mitigated",
        ],
        vec![
            "governance",
            "detections",
            "update",
            "d1",
            "--mitigate-after",
            "bad",
        ],
        vec![
            "governance",
            "detections",
            "update",
            "d1",
            "--mitigate-after",
            "2000-01-01T00:00:00Z",
        ],
        vec![
            "governance",
            "detections",
            "update",
            "d1",
            "--state",
            "exception",
            "--mitigate-after",
            "2999-11-30T00:00:00Z",
        ],
        vec![
            "governance",
            "detections",
            "update",
            "d1",
            "--assigned-to",
            "invalid",
        ],
    ] {
        let output = run(&server.url(), &args, "", false);
        assert!(!output.status.success(), "{args:?}");
        if args.last() == Some(&"mitigated") {
            assert!(String::from_utf8_lossy(&output.stderr).contains("detections mitigate"));
        }
    }
    for mock in mocks {
        mock.assert();
    }
}

#[test]
fn detections_pagination_and_limits_merge_use_standard_envelope() {
    let mut server = Server::new();
    let list = server
        .mock("GET", "/api/v2/governance/control/test/detections")
        .match_query(Matcher::Any)
        .with_body(r#"{"data":[{"id":"d1"}]}"#)
        .create();
    let result = success(&run(
        &server.url(),
        &[
            "governance",
            "detections",
            "list",
            "test",
            "--page",
            "2",
            "--page-size",
            "1",
        ],
        "",
        false,
    ));
    assert_eq!(result["metadata"]["next_page"], 3);
    assert!(result["metadata"]["next_action"]
        .as_str()
        .unwrap()
        .contains("--page=3"));
    list.assert();
    let rate = server
        .mock("GET", "/api/v2/governance/limits")
        .with_body(r#"{"data":[{"type":"limit"}]}"#)
        .create();
    let resource = server
        .mock("GET", "/api/v2/governance/resource_limits")
        .with_body(r#"{"data":[{"type":"resource-limit"}]}"#)
        .create();
    let result = success(&run(
        &server.url(),
        &["governance", "limits", "list"],
        "",
        false,
    ));
    assert_eq!(
        result["data"],
        json!([{"type":"limit"},{"type":"resource-limit"}])
    );
    assert_eq!(result["metadata"]["count"], 2);
    rate.assert();
    resource.assert();
}

#[test]
fn write_failure_http_rejections_and_unknown_outcomes_never_resend() {
    let mut server = Server::new();
    let control = server
        .mock("GET", CONTROL)
        .expect(6)
        .with_body(control().to_string())
        .create();
    let get = server
        .mock("GET", DETECTION)
        .expect(6)
        .with_body(current().to_string())
        .create();
    for (method, path, mut args) in [
        ("POST", MITIGATE, mitigate()),
        ("PATCH", DETECTION, update()),
    ] {
        args.push("--yes");
        for status in [400, 403, 409, 429, 500, 503] {
            let mock = server
                .mock(method, path)
                .expect(1)
                .with_status(status)
                .with_header("x-ratelimit-name", "governance_write")
                .with_body("request failed")
                .create();
            let output = run(&server.url(), &args, "", false);
            assert!(!output.status.success());
            let error = String::from_utf8_lossy(&output.stderr);
            if status < 500 {
                assert!(error.contains("rejected; nothing was applied"), "{error}");
            } else {
                for expected in [
                    "Outcome unknown",
                    "Do not resubmit",
                    "pup governance detections",
                    "Audit Trail",
                    "new approval",
                ] {
                    assert!(error.contains(expected), "{error}");
                }
                assert!(
                    !error.contains("try again") && !error.contains("retry"),
                    "{error}"
                );
            }
            if status == 403 {
                assert!(
                    error.contains("governance_console_read")
                        && error.contains("pup auth login --extra-scopes"),
                    "{error}"
                );
                assert!(!error.contains("local-governance-fixture-token"));
            }
            if status == 429 {
                assert!(
                    error.contains("rule: governance_write") && error.contains("wait and retry"),
                    "{error}"
                );
                assert_eq!(
                    output.status.code(),
                    Some(if cfg!(unix) { 429 % 256 } else { 429 })
                );
            }
            mock.assert();
            mock.remove();
        }
    }
    control.assert();
    get.assert();
}

fn read_request(stream: &mut std::net::TcpStream) -> String {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0; 1024];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        assert_ne!(count, 0);
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]);
            let length = headers
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_string)
                })
                .map(|n| n.parse::<usize>().unwrap())
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                return String::from_utf8(bytes).unwrap();
            }
        }
    }
}

fn accept(listener: &std::net::TcpListener) -> std::net::TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                return stream;
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            result => panic!("expected a request within 10 seconds: {result:?}"),
        }
    }
}

#[test]
fn write_failure_connection_lost_after_send_and_refused_before_send() {
    for (method, mut args, preflight) in [
        ("POST", mitigate(), control()),
        ("PATCH", update(), current()),
    ] {
        args.push("--yes");
        for refused in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let preflight = preflight.to_string();
            let thread = std::thread::spawn(move || {
                let mut stream = accept(&listener);
                assert!(read_request(&mut stream).starts_with("GET "));
                // For refusal, close the listening socket before releasing the GET response.
                if refused {
                    drop(listener);
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{preflight}", preflight.len()).unwrap();
                    return;
                }
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{preflight}",
                    preflight.len()
                )
                .unwrap();
                drop(stream);
                let mut write = accept(&listener);
                assert!(read_request(&mut write).starts_with(method));
                drop(write); // Server received the write but lost the response.
                listener.set_nonblocking(true).unwrap();
                let until = std::time::Instant::now() + std::time::Duration::from_millis(300);
                while std::time::Instant::now() < until {
                    assert!(
                        matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
                        "write was resent"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            });
            let output = run(&format!("http://{address}"), &args, "", false);
            thread.join().unwrap();
            assert!(!output.status.success());
            let error = String::from_utf8_lossy(&output.stderr);
            if refused {
                assert!(error.contains("not sent"), "{error}");
            } else {
                assert!(
                    error.contains("Outcome unknown") && error.contains("Do not resubmit"),
                    "{error}"
                );
                assert!(
                    !error.contains("try again") && !error.contains("retry"),
                    "{error}"
                );
            }
        }
    }
}

#[test]
fn write_failure_malformed_update_response_keeps_preimage() {
    let mut server = Server::new();
    let get = server
        .mock("GET", DETECTION)
        .with_body(current().to_string())
        .create();
    let patch = server
        .mock("PATCH", DETECTION)
        .expect(1)
        .with_body("{not-json}")
        .create();
    let mut args = update();
    args.push("--yes");
    let output = run(&server.url(), &args, "", false);
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("Outcome unknown")
            && error.contains("Do not resubmit")
            && error.contains("previous"),
        "{error}"
    );
    assert!(
        !error.contains("try again") && !error.contains("retry"),
        "{error}"
    );
    get.assert();
    patch.assert();
}

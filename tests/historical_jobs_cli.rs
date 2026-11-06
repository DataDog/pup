//! Body-less historical-job writes print a parseable JSON envelope.
use mockito::Server;
use serde_json::Value;
use std::process::{Command, Output};

fn pup(server: &Server, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pup"))
        .args(["--yes", "--output", "json"])
        .args(args)
        .env("PUP_MOCK_SERVER", server.url())
        .env("DD_ACCESS_TOKEN", "local-fixture-token")
        .env("DD_SITE", "datadoghq.com")
        .env("DD_TOKEN_STORAGE", "file")
        .env(
            "PUP_CONFIG_DIR",
            std::env::temp_dir().join("pup-historical-jobs-fixture"),
        )
        .env_remove("PUP_NO_ENVELOPE")
        .env_remove("DD_READ_ONLY")
        .env_remove("DD_CLI_READ_ONLY")
        .env_remove("DD_API_KEY")
        .env_remove("DD_APP_KEY")
        .output()
        .unwrap()
}

fn assert_envelope(server: &Server, args: &[&str], command: &str) {
    let output = pup(server, args);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert!(json["data"].is_null(), "{json}");
    assert_eq!(json["metadata"]["command"], command, "{json}");
}

#[test]
fn cancel_delete_convert_print_json_envelope() {
    let mut server = Server::new();
    let base = "/api/v2/siem-historical-detections/jobs";
    let mocks = [
        server
            .mock("PATCH", format!("{base}/j1/cancel").as_str())
            .with_status(204)
            .create(),
        server
            .mock("DELETE", format!("{base}/j1").as_str())
            .with_status(204)
            .create(),
        server
            .mock("POST", format!("{base}/signal_convert").as_str())
            .with_status(204)
            .create(),
    ];
    assert_envelope(
        &server,
        &["security", "historical-jobs", "cancel", "j1"],
        "security historical-jobs cancel",
    );
    assert_envelope(
        &server,
        &["security", "historical-jobs", "delete", "j1"],
        "security historical-jobs delete",
    );
    assert_envelope(
        &server,
        &[
            "security",
            "historical-jobs",
            "convert",
            "--histsignal-id",
            "h1",
            "--severity",
            "low",
            "--message",
            "m",
        ],
        "security historical-jobs convert",
    );
    for m in mocks {
        m.assert();
    }
}

#[test]
fn cancel_error_exits_nonzero() {
    let mut server = Server::new();
    let _m = server
        .mock(
            "PATCH",
            "/api/v2/siem-historical-detections/jobs/missing/cancel",
        )
        .with_status(404)
        .with_body(r#"{"errors":["Not Found"]}"#)
        .create();
    let output = pup(
        &server,
        &["security", "historical-jobs", "cancel", "missing"],
    );
    assert!(!output.status.success());
}

//! Markdown preferences and explicit requests through the real CLI.
#![cfg(all(debug_assertions, feature = "native"))]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use mockito::{Matcher, Server};
use serde_json::{json, Value};

struct Fixture {
    config_dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let config_dir =
            std::env::temp_dir().join(format!("pup-markdown-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join("config.yaml"), "{}\n").unwrap();
        Self { config_dir }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pup"));
        command
            .env("PUP_CONFIG_DIR", &self.config_dir)
            .env("DD_ACCESS_TOKEN", "local-fixture-token")
            .env("DD_SITE", "datadoghq.com")
            .env("DD_TOKEN_STORAGE", "file")
            .env("PUP_NO_UPDATE_CHECK", "1")
            .env_remove("DD_ORG")
            .env_remove("DD_API_KEY")
            .env_remove("DD_APP_KEY")
            .env_remove("DD_OUTPUT")
            .env_remove("PUP_OUTPUT")
            .env_remove("PUP_FILTER")
            .env_remove("PUP_NO_ENVELOPE");
        command
    }

    fn ambient_markdown(&self, source: &str) -> Command {
        let mut command = self.command();
        if source == "config" {
            fs::write(self.config_dir.join("config.yaml"), "output: markdown\n").unwrap();
        } else {
            command.env(source, "markdown");
        }
        command
    }

    #[cfg(unix)]
    fn install_extension(&self) {
        use std::os::unix::fs::PermissionsExt;

        let directory = self.config_dir.join("extensions/pup-format-probe");
        fs::create_dir_all(&directory).unwrap();
        let executable = directory.join("pup-format-probe");
        fs::write(
            &executable,
            "#!/bin/sh\nprintf '%s' \"$PUP_OUTPUT\" > \"$PUP_CONFIG_DIR/extension-ran\"\nprintf '%s\\n' \"$PUP_OUTPUT\"\n",
        )
        .unwrap();
        fs::set_permissions(executable, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.config_dir);
    }
}

#[test]
fn ambient_markdown_falls_back_for_json_commands() {
    for source in ["DD_OUTPUT", "PUP_OUTPUT", "config"] {
        let fixture = Fixture::new();
        let mut server = Server::new();
        let request = server
            .mock("GET", "/api/v1/monitor/search")
            .match_query(Matcher::Any)
            .with_header("content-type", "application/json")
            .with_body(
                json!({
                    "counts": {},
                    "metadata": {"page": 0, "page_count": 1, "per_page": 30, "total_count": 0},
                    "monitors": []
                })
                .to_string(),
            )
            .create();
        let output = fixture
            .ambient_markdown(source)
            .args(["monitors", "search"])
            .env("PUP_MOCK_SERVER", server.url())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let body: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(body["status"], "success");
        request.assert();
    }
}

#[test]
fn explicit_markdown_is_rejected_before_a_remote_write() {
    let fixture = Fixture::new();
    let mut server = Server::new();
    let request = server.mock("POST", "/api/v1/monitor").expect(0).create();
    let file = fixture.config_dir.join("monitor.json");
    fs::write(
        &file,
        json!({
            "name": "Probe",
            "type": "metric alert",
            "query": "avg(last_5m):avg:system.cpu.user{*} > 90"
        })
        .to_string(),
    )
    .unwrap();
    let output = fixture
        .ambient_markdown("DD_OUTPUT")
        .args([
            "--yes", "--output", "markdown", "monitors", "create", "--file",
        ])
        .arg(file)
        .env("PUP_MOCK_SERVER", server.url())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("markdown output is only supported by commands with a Markdown representation"));
    request.assert();
}

#[cfg(unix)]
#[test]
fn extension_output_is_resolved_before_launch() {
    for source in ["DD_OUTPUT", "PUP_OUTPUT", "config"] {
        let fixture = Fixture::new();
        fixture.install_extension();
        let output = fixture
            .ambient_markdown(source)
            .arg("format-probe")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"json\n");
        assert_eq!(
            fs::read_to_string(fixture.config_dir.join("extension-ran")).unwrap(),
            "json"
        );
    }

    let fixture = Fixture::new();
    fixture.install_extension();
    for args in [
        vec!["--output", "markdown", "format-probe"],
        vec!["--output=markdown", "format-probe"],
        vec!["-omarkdown", "format-probe"],
    ] {
        let output = fixture
            .ambient_markdown("DD_OUTPUT")
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(
            "markdown output is only supported by commands with a Markdown representation"
        ));
        assert!(!fixture.config_dir.join("extension-ran").exists());
    }
    let output = fixture
        .ambient_markdown("DD_OUTPUT")
        .args(["--output", "yaml", "format-probe"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"yaml\n");
}

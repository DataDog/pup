//! Smoke-test real executable startup on every platform, including Windows.
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pup"))
        .arg("--no-envelope")
        .args(args)
        .env(
            "PUP_CONFIG_DIR",
            std::env::temp_dir().join(format!("pup-startup-fixture-{}", std::process::id())),
        )
        .env_remove("DD_ACCESS_TOKEN")
        .env_remove("DD_API_KEY")
        .env_remove("DD_APP_KEY")
        .env_remove("DD_ORG")
        .output()
        .expect("execute pup")
}

#[test]
fn help_starts_without_authentication() {
    let output = run(&["--help"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage:"));
}

#[test]
fn invalid_command_reports_a_cli_error() {
    let output = run(&["pup-ci-command-that-does-not-exist"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand"));
}

#[test]
fn api_help_points_to_live_documentation() {
    let output = run(&["api", "--help"]);
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("pup docs api monitors create-a-monitor")
    );
}

#[test]
fn docs_api_help_explains_live_discovery_without_auth() {
    let output = run(&["docs", "api", "--help"]);
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("--list"));
    assert!(help.contains("No authentication"));
    assert!(help.contains("Markdown"));
}

#[test]
fn docs_api_rejects_topic_with_list() {
    let output = run(&["docs", "api", "monitors", "--list"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be used with"));
}

#[test]
fn docs_api_operation_flags_require_topic_and_conflict_with_fetch() {
    for args in [
        vec!["docs", "api", "--operations"],
        vec![
            "docs",
            "api",
            "monitors",
            "create-a-monitor",
            "--operations",
        ],
        vec!["docs", "api", "monitors", "create-a-monitor", "--list"],
    ] {
        assert_eq!(run(&args).status.code(), Some(2));
    }
    let output = run(&["docs", "api", "--help"]);
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("--operations"));
    assert!(help.contains("create-a-monitor"));
}

#[test]
fn agent_surface_dispatches_versioned_projection() {
    let output = run(&["agent", "surface"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let document: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("surface output must be valid JSON");
    assert_eq!(document["format_version"], 1);
}

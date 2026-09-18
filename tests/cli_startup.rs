//! Smoke-test real executable startup on every platform, including Windows.
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pup"))
        .arg("--no-agent")
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

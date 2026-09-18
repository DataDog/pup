//! Shared real-binary IDP test harness. Credentials are local fixture values.
use mockito::Server;
use serde_json::Value;
use std::process::{Command, Output};

pub fn pup(server: &Server, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pup"));
    if !args.contains(&"--read-only") {
        command.arg("--read-only");
    }
    command
        .args(["--output", "json"])
        .args(args)
        .env("PUP_MOCK_SERVER", server.url())
        .env("DD_ACCESS_TOKEN", "local-fixture-token")
        .env("DD_SITE", "datadoghq.com")
        .env(
            "PUP_CONFIG_DIR",
            std::env::temp_dir().join(format!(
                "pup-idp-fixture-{}",
                server.host_with_port().replace(':', "-")
            )),
        )
        .env_remove("PUP_FILTER")
        .env_remove("DD_ORG")
        .env_remove("DD_API_KEY")
        .env_remove("DD_APP_KEY")
        .output()
        .expect("execute pup")
}

pub fn payload(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("CLI stdout must be JSON")
}

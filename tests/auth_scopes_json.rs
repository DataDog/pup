use serde_json::Value;
use std::process::Command;

fn run_auth_scopes(output: &str) -> std::process::Output {
    let config_dir =
        std::env::temp_dir().join(format!("pup-auth-scopes-json-{}", std::process::id()));
    std::fs::create_dir_all(&config_dir).expect("create isolated config dir");

    Command::new(env!("CARGO_BIN_EXE_pup"))
        .args(["auth", "scopes", &format!("--output={output}")])
        .env("PUP_CONFIG_DIR", config_dir)
        .env_remove("DD_ACCESS_TOKEN")
        .env_remove("DD_API_KEY")
        .env_remove("DD_APP_KEY")
        .env_remove("DD_SITE")
        .env_remove("DD_ORG")
        .output()
        .expect("run pup auth scopes")
}

#[test]
fn auth_scopes_json_emits_default_oauth_scope_contract_without_auth() {
    let output = run_auth_scopes("json");

    assert!(
        output.status.success(),
        "expected pup auth scopes to succeed without auth; status: {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "scope discovery should not start OAuth or print auth diagnostics: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value: Value = serde_json::from_slice(&output.stdout).expect("valid JSON scope contract");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["source"], "pup auth login");

    let scopes = value["scopes"]
        .as_array()
        .expect("scopes should be a JSON array");
    assert!(!scopes.is_empty(), "scopes should not be empty");
    assert!(
        scopes.iter().all(|scope| scope.as_str().is_some()),
        "all scopes should be strings: {scopes:?}"
    );

    let scope_strings: Vec<&str> = scopes.iter().filter_map(Value::as_str).collect();
    assert!(scope_strings.contains(&"metrics_read"));
    assert!(scope_strings.contains(&"timeseries_query"));
    assert!(scope_strings.contains(&"dashboards_write"));
    assert!(scope_strings.contains(&"org_management"));
}

#[test]
fn auth_scopes_json_rejects_non_json_output() {
    let output = run_auth_scopes("table");

    assert!(
        !output.status.success(),
        "expected pup auth scopes --output=table to fail"
    );
    assert!(
        output.stdout.is_empty(),
        "scope discovery should not emit table output: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("pup auth scopes only supports --output=json"),
        "expected non-json diagnostic, got: {stderr}"
    );
}

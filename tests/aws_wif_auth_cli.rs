//! Exercise real CLI configuration, AWS proof exchange and API authentication.
#![cfg(not(target_arch = "wasm32"))]

use mockito::{Matcher, Server};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const ORG: &str = "11111111-2222-3333-4444-555555555555";

fn pup(server: &Server) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pup"));
    cmd.env_clear()
        .env(
            "PUP_CONFIG_DIR",
            std::env::temp_dir().join(format!("pup-wif-cli-{}", std::process::id())),
        )
        .env("DD_TOKEN_STORAGE", "file")
        .env("PUP_MOCK_SERVER", server.url())
        .env("DD_SITE", "datadoghq.com")
        .env("DD_AUTH_PROVIDER", "aws")
        .env("DD_ORG_UUID", ORG)
        .env("AWS_ACCESS_KEY_ID", "test-access")
        .env("AWS_SECRET_ACCESS_KEY", "test-secret")
        .env("AWS_SESSION_TOKEN", "test-session")
        .env("AWS_REGION", "us-east-1")
        .env("AWS_EC2_METADATA_DISABLED", "true")
        // Explicit AWS mode must ignore these, including SDK environment loading.
        .env("DD_ACCESS_TOKEN", "unwanted-bearer")
        .env("DD_API_KEY", "unwanted-api-key")
        .env("DD_APP_KEY", "unwanted-app-key");
    // Windows needs its system directory to load networking providers even
    // when the subprocess otherwise has an isolated credential environment.
    #[cfg(windows)]
    for name in ["SystemRoot", "WINDIR"] {
        if let Some(value) = std::env::var_os(name) {
            cmd.env(name, value);
        }
    }
    cmd
}

fn exchange(server: &mut Server) -> mockito::Mock {
    let expires = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 900;
    server
        .mock("POST", "/api/v2/delegated-token")
        .match_header("authorization", Matcher::Regex("^Delegated ".into()))
        .match_header("dd-api-key", Matcher::Missing)
        .with_body(
            serde_json::json!({"data":{"attributes":{
                "access_token":"test-delegated-token","expires":expires.to_string()
            }}})
            .to_string(),
        )
        .create()
}

#[test]
fn sdk_and_passthrough_commands_exchange_aws_proof_without_static_dd_credentials() {
    let mut server = Server::new();
    for args in [vec!["monitors", "list"], vec!["api", "/api/v1/monitor"]] {
        let token = exchange(&mut server);
        let api = server
            .mock("GET", "/api/v1/monitor")
            .match_query(Matcher::Any)
            .match_header("authorization", "Bearer test-delegated-token")
            .match_header("dd-api-key", Matcher::Missing)
            .match_header("dd-application-key", Matcher::Missing)
            .with_header("content-type", "application/json")
            .with_body("[]")
            .create();
        let output = pup(&server).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("test-delegated-token"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("test-secret"));
        token.assert();
        api.assert();
        token.remove();
        api.remove();
    }
}

#[test]
fn failed_exchange_never_falls_back_or_calls_api() {
    let mut server = Server::new();
    let token = server
        .mock("POST", "/api/v2/delegated-token")
        .with_status(403)
        .with_body("echoed-secret-proof")
        .create();
    let api = server
        .mock("GET", "/api/v1/monitor")
        .match_query(Matcher::Any)
        .expect(0)
        .create();
    let output = pup(&server).args(["monitors", "list"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("HTTP 403"), "{stderr}");
    assert!(!stderr.contains("echoed-secret-proof"));
    assert!(!stderr.contains("unwanted-"));
    token.assert();
    api.assert();
}

#[test]
fn status_is_local_and_token_command_exports_only_the_delegated_token() {
    let mut server = Server::new();
    let no_exchange = server
        .mock("POST", "/api/v2/delegated-token")
        .expect(0)
        .create();
    let output = pup(&server).args(["auth", "status"]).output().unwrap();
    assert!(output.status.success());
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["auth_method"], "aws_workload_identity");
    assert_eq!(status["status"], "configured");
    assert_eq!(status["org_uuid"], ORG);
    no_exchange.assert();
    no_exchange.remove();
    let token = exchange(&mut server);
    let output = pup(&server).args(["auth", "token"]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"test-delegated-token\n");
    token.assert();
}

#[test]
fn config_file_selects_aws_without_using_saved_bearer_or_site() {
    let server = Server::new();
    let dir = std::env::temp_dir().join(format!(
        "pup-wif-config-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.yaml"), format!(
        "auth_provider: aws\norg_uuid: {ORG}\nsite: datadoghq.eu\naccess_token: ignored-file-token\napi_key: ignored-key\napp_key: ignored-app-key\n"
    )).unwrap();
    let output = pup(&server)
        .env("PUP_CONFIG_DIR", &dir)
        .env_remove("DD_AUTH_PROVIDER")
        .env_remove("DD_ORG_UUID")
        .env_remove("DD_SITE")
        .args(["auth", "status"])
        .output()
        .unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["auth_method"], "aws_workload_identity");
    assert_eq!(status["site"], "datadoghq.eu");
    assert_eq!(status["org_uuid"], ORG);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("ignored-"));
}

#[test]
fn rejects_invalid_config_and_ambiguous_org_without_network() {
    let mut server = Server::new();
    let no_exchange = server
        .mock("POST", "/api/v2/delegated-token")
        .expect(0)
        .create();
    for (key, value, expected) in [
        ("DD_ORG_UUID", "", "requires DD_ORG_UUID"),
        ("DD_ORG_UUID", "bad-uuid", "valid DD_ORG_UUID"),
        (
            "DD_AUTH_PROVIDER",
            "unknown",
            "unsupported DD_AUTH_PROVIDER",
        ),
        ("DD_ORG", "saved-session", "named OAuth session"),
    ] {
        let output = pup(&server)
            .env(key, value)
            .args(["monitors", "list"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = pup(&server)
        .args(["--org", "saved-session", "monitors", "list"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--org selects an OAuth session"));
    no_exchange.assert();
}

#[test]
fn missing_aws_credentials_and_untrusted_sites_fail_closed() {
    let mut server = Server::new();
    let no_exchange = server
        .mock("POST", "/api/v2/delegated-token")
        .expect(0)
        .create();
    let output = pup(&server)
        .env_remove("AWS_ACCESS_KEY_ID")
        .env_remove("AWS_SECRET_ACCESS_KEY")
        .env_remove("AWS_SESSION_TOKEN")
        .env(
            "AWS_SHARED_CREDENTIALS_FILE",
            "/nonexistent/pup-wif-credentials",
        )
        .env("AWS_CONFIG_FILE", "/nonexistent/pup-wif-config")
        .args(["monitors", "list"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not resolve AWS credentials"));
    for args in [vec!["monitors", "list"], vec!["auth", "token"]] {
        let output = pup(&server)
            .env_remove("PUP_MOCK_SERVER")
            .env("DD_SITE", "untrusted.example")
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("no terminal was available"));
    }
    no_exchange.assert();
}

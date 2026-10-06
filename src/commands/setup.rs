use anyhow::Result;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::commands::skills_remote;
use crate::config::Config;
use crate::extensions::exec;

const AI_SETUP_PACKAGE: &str = "@datadog/ai-setup-cli";
const AI_SETUP_BIN: &str = "ai-setup-cli";
const AI_SETUP_DEFAULT_VERSION: &str = "latest";
const AI_SETUP_PACKAGE_ENV: &str = "PUP_SETUP_AI_SETUP_PACKAGE";
const MIN_NODE_MAJOR: u32 = 22;
const NODE_DOWNLOAD_URL: &str = "https://nodejs.org/en/download";
const SESSION_SKILL_ID: &str = "orchestrator";
/// AI Setup writes the org's API key into the project so the Agent and tracers
/// can send data, which needs this scope on top of pup's defaults.
pub const LOGIN_EXTRA_SCOPES: &str = "api_keys_read";
const TELEMETRY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, PartialEq, Eq)]
enum NodeStatus {
    Ready(PathBuf),
    NpxNotFound,
    NodeNotFound,
    NodeTooOld(String),
}

impl NodeStatus {
    fn summary(&self) -> String {
        match self {
            NodeStatus::Ready(_) => "ready".to_string(),
            NodeStatus::NpxNotFound => "npx_not_found".to_string(),
            NodeStatus::NodeNotFound => "node_not_found".to_string(),
            NodeStatus::NodeTooOld(version) => format!("node_too_old:{version}"),
        }
    }

    fn message(&self) -> String {
        match self {
            NodeStatus::NodeTooOld(version) => format!(
                "pup setup needs Node.js {MIN_NODE_MAJOR} or newer (found {version}). \
                 Upgrade from {NODE_DOWNLOAD_URL} and rerun `pup setup`."
            ),
            _ => format!(
                "pup setup runs Datadog's AI Setup with npx, which needs Node.js \
                 {MIN_NODE_MAJOR} or newer. Install it from {NODE_DOWNLOAD_URL} and rerun \
                 `pup setup`."
            ),
        }
    }
}

/// Backs the built-in `pup setup` command: runs AI Setup from npm with `args`.
/// `login` runs pup's OAuth login for headless runs that have no session yet.
pub async fn run(
    cfg: &mut Config,
    args: &[String],
    login: impl AsyncFnOnce(&Config) -> Result<()>,
) -> Result<i32> {
    let status = probe_node();
    let NodeStatus::Ready(npx) = status else {
        return Ok(report_missing_node(cfg, &status).await);
    };
    let headless = wants_headless(
        cfg.agent_mode,
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
    );
    // A person at a terminal without a session gets AI Setup's own sign-in
    // flow, exactly as when running it standalone.
    if should_log_in(headless, cfg.access_token.is_some(), args) {
        let granted_scopes = stored_token_scopes(cfg);
        ensure_session(cfg, granted_scopes.as_deref(), login, |cfg| {
            crate::config::load_token_from_storage(&cfg.site, cfg.org.as_deref())
        })
        .await?;
    }
    let launch = Launch {
        package: package_spec(std::env::var(AI_SETUP_PACKAGE_ENV).ok().as_deref())?,
        headless,
        json_help: wants_json_help(cfg.agent_mode, std::env::var("PUP_OUTPUT").ok().as_deref()),
    };
    let cmd = build_command(&npx, &build_args(args, &cfg.site, &launch), cfg);
    exec::run_inherited(cmd, "npx")
}

#[derive(Debug)]
struct Launch {
    package: String,
    headless: bool,
    json_help: bool,
}

async fn report_missing_node(cfg: &Config, status: &NodeStatus) -> i32 {
    eprintln!("{}", status.message());
    record_failure(cfg, &status.summary()).await;
    1
}

/// Signs the user in when pup has no credentials, or when the saved session
/// lacks the scopes setup needs, so `pup setup` stays a single command.
async fn ensure_session(
    cfg: &mut Config,
    granted_scopes: Option<&str>,
    login: impl AsyncFnOnce(&Config) -> Result<()>,
    stored_token: impl FnOnce(&Config) -> Option<String>,
) -> Result<()> {
    if !needs_login(cfg, granted_scopes) {
        return Ok(());
    }
    login(cfg).await?;
    cfg.access_token = stored_token(cfg);
    if cfg.access_token.is_none() {
        anyhow::bail!("login finished but no pup session was found; run `pup auth login`");
    }
    Ok(())
}

fn has_credentials(cfg: &Config) -> bool {
    cfg.access_token.is_some() || (cfg.api_key.is_some() && cfg.app_key.is_some())
}

/// `granted_scopes` is `None` when pup can't see the token's scopes, such as a
/// token passed in `DD_ACCESS_TOKEN`; that token is used as is.
fn needs_login(cfg: &Config, granted_scopes: Option<&str>) -> bool {
    if cfg.access_token.is_none() {
        return !has_credentials(cfg);
    }
    granted_scopes.is_some_and(|scopes| !has_scopes(scopes, LOGIN_EXTRA_SCOPES))
}

fn has_scopes(granted: &str, required: &str) -> bool {
    let granted: Vec<&str> = granted.split_whitespace().collect();
    crate::config::parse_scopes(required)
        .iter()
        .all(|scope| granted.contains(&scope.as_str()))
}

fn stored_token_scopes(cfg: &Config) -> Option<String> {
    if std::env::var("DD_ACCESS_TOKEN").is_ok_and(|token| !token.is_empty()) {
        return None;
    }
    let guard = crate::auth::storage::get_storage().ok()?;
    let lock = guard.lock().ok()?;
    let tokens = lock
        .as_ref()?
        .load_tokens(&cfg.site, cfg.org.as_deref())
        .ok()??;
    Some(tokens.scope)
}

fn should_log_in(headless: bool, has_session: bool, args: &[String]) -> bool {
    (headless || has_session) && !asks_for_help(args)
}

fn asks_for_help(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--help" || arg == "-h")
}

/// Best effort: never fails, never blocks for long, and is skipped without credentials.
async fn record_failure(cfg: &Config, summary: &str) {
    if !has_credentials(cfg) {
        return;
    }
    let session_id = format!("pup-setup-{}", uuid::Uuid::new_v4());
    let _ = skills_remote::record_session(
        cfg,
        &session_id,
        &[SESSION_SKILL_ID.to_string()],
        summary,
        "failed",
        TELEMETRY_TIMEOUT,
    )
    .await;
}

fn wants_headless(agent_mode: bool, stdin_tty: bool, stdout_tty: bool) -> bool {
    agent_mode || !stdin_tty || !stdout_tty
}

/// pup's own default output is JSON, so only agent mode or an explicit
/// `PUP_OUTPUT=json` should switch AI Setup's help to JSON.
fn wants_json_help(agent_mode: bool, pup_output_env: Option<&str>) -> bool {
    agent_mode || pup_output_env == Some("json")
}

/// Resolves which AI Setup package npx runs. The override accepts a version or
/// dist-tag (`2.1.0`, `next`) or a full npm package spec (a local tarball path,
/// a `file:` spec, or `@datadog/ai-setup-cli@<version>`), so an unpublished
/// build can be tested end to end.
fn package_spec(override_value: Option<&str>) -> Result<String> {
    let Some(value) = override_value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(format!("{AI_SETUP_PACKAGE}@{AI_SETUP_DEFAULT_VERSION}"));
    };
    if value.starts_with('-') || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        anyhow::bail!(
            "{AI_SETUP_PACKAGE_ENV} must be a version, a dist-tag, or an npm package spec \
             such as a tarball path, without spaces or a leading '-'"
        );
    }
    let is_full_spec = value.contains('/') || value.contains(':') || value.ends_with(".tgz");
    if is_full_spec {
        Ok(value.to_string())
    } else {
        Ok(format!("{AI_SETUP_PACKAGE}@{value}"))
    }
}

fn build_args(user_args: &[String], site: &str, launch: &Launch) -> Vec<String> {
    let mut args = vec![
        "-y".to_string(),
        "--package".to_string(),
        launch.package.clone(),
        AI_SETUP_BIN.to_string(),
    ];
    if launch.headless && !has_flag(user_args, "--headless") {
        args.push("--headless".to_string());
    }
    if !has_flag(user_args, "--site") {
        args.push("--site".to_string());
        args.push(site.to_string());
    }
    args.extend(user_args.iter().cloned());
    if launch.json_help && asks_for_help(user_args) && !has_flag(user_args, "--json") {
        args.push("--json".to_string());
    }
    args
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter()
        .any(|arg| arg == flag || arg.starts_with(&format!("{flag}=")))
}

fn build_command(npx: &Path, args: &[String], cfg: &Config) -> Command {
    let mut cmd = Command::new(npx);
    cmd.args(args);
    exec::inject_auth_env(&mut cmd, cfg);
    // Keys from the shell can belong to a different org than the OAuth session,
    // and a lone key is not a session, so only a complete key pair is forwarded.
    if cfg.access_token.is_some() || !has_credentials(cfg) {
        cmd.env_remove("DD_API_KEY");
        cmd.env_remove("DD_APP_KEY");
    }
    cmd
}

fn probe_node() -> NodeStatus {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let npx = find_on_path("npx", &path);
    let node = find_on_path("node", &path);
    let version = node.and_then(|node| {
        let output = Command::new(node).arg("--version").output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
    });
    node_status(npx, version.as_deref())
}

fn node_status(npx: Option<PathBuf>, node_version: Option<&str>) -> NodeStatus {
    let Some(version) = node_version else {
        return NodeStatus::NodeNotFound;
    };
    match parse_node_major(version) {
        Some(major) if major >= MIN_NODE_MAJOR => {}
        _ => return NodeStatus::NodeTooOld(version.to_string()),
    }
    match npx {
        Some(npx) => NodeStatus::Ready(npx),
        None => NodeStatus::NpxNotFound,
    }
}

fn parse_node_major(version: &str) -> Option<u32> {
    version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()?
        .parse()
        .ok()
}

fn find_on_path(name: &str, path: &std::ffi::OsStr) -> Option<PathBuf> {
    let candidates: &[&str] = if cfg!(windows) {
        &["exe", "cmd"]
    } else {
        &[""]
    };
    std::env::split_paths(path).find_map(|dir| {
        candidates.iter().find_map(|ext| {
            let candidate = if ext.is_empty() {
                dir.join(name)
            } else {
                dir.join(format!("{name}.{ext}"))
            };
            candidate.is_file().then_some(candidate)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    fn env_value(cmd: &Command, name: &str) -> Option<Option<String>> {
        cmd.get_envs()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.map(|v| v.to_string_lossy().to_string()))
    }

    #[test]
    fn headless_for_agents_and_non_terminals_only() {
        assert!(wants_headless(true, true, true));
        assert!(wants_headless(false, false, true));
        assert!(wants_headless(false, true, false));
        assert!(!wants_headless(false, true, true));
    }

    fn launch(headless: bool, json_help: bool) -> Launch {
        Launch {
            package: package_spec(None).unwrap(),
            headless,
            json_help,
        }
    }

    #[test]
    fn build_args_injects_headless_and_site_before_user_args() {
        assert_eq!(
            build_args(
                &args(&["--product", "linux"]),
                "datadoghq.eu",
                &launch(true, false)
            ),
            args(&[
                "-y",
                "--package",
                "@datadog/ai-setup-cli@latest",
                "ai-setup-cli",
                "--headless",
                "--site",
                "datadoghq.eu",
                "--product",
                "linux",
            ])
        );
    }

    #[test]
    fn build_args_interactive_skips_headless() {
        let built = build_args(
            &args(&["--product", "rum"]),
            "datadoghq.com",
            &launch(false, false),
        );
        assert!(!built.contains(&"--headless".to_string()));
        assert!(built.contains(&"--site".to_string()));
    }

    #[test]
    fn build_args_respects_user_provided_flags() {
        for user in [
            args(&["--headless", "--site", "us5.datadoghq.com"]),
            args(&["--headless", "--site=us5.datadoghq.com"]),
        ] {
            let built = build_args(&user, "datadoghq.com", &launch(true, false));
            assert_eq!(built.iter().filter(|a| *a == "--headless").count(), 1);
            assert!(!built.contains(&"datadoghq.com".to_string()));
        }
    }

    #[test]
    fn build_args_passes_help_through() {
        let built = build_args(&args(&["--help"]), "datadoghq.com", &launch(false, false));
        assert_eq!(built.last().map(String::as_str), Some("--help"));
        assert!(!built.contains(&"--json".to_string()));
    }

    #[test]
    fn build_args_agent_help_asks_for_json() {
        let built = build_args(&args(&["--help"]), "datadoghq.com", &launch(true, true));
        assert_eq!(
            built[built.len() - 2..],
            ["--help".to_string(), "--json".to_string()]
        );
        let short = build_args(&args(&["-h"]), "datadoghq.com", &launch(true, true));
        assert_eq!(short.last().map(String::as_str), Some("--json"));
    }

    #[test]
    fn build_args_json_only_for_help_and_not_duplicated() {
        let no_help = build_args(
            &args(&["--product", "apm"]),
            "datadoghq.com",
            &launch(true, true),
        );
        assert!(!no_help.contains(&"--json".to_string()));
        let already = build_args(
            &args(&["--help", "--json"]),
            "datadoghq.com",
            &launch(true, true),
        );
        assert_eq!(already.iter().filter(|a| *a == "--json").count(), 1);
    }

    #[test]
    fn json_help_for_agents_or_explicit_json_output_only() {
        assert!(wants_json_help(true, None));
        assert!(wants_json_help(false, Some("json")));
        assert!(!wants_json_help(false, Some("table")));
        assert!(!wants_json_help(false, None));
    }

    #[test]
    fn package_spec_defaults_to_latest() {
        assert_eq!(package_spec(None).unwrap(), "@datadog/ai-setup-cli@latest");
        assert_eq!(
            package_spec(Some("  ")).unwrap(),
            "@datadog/ai-setup-cli@latest"
        );
    }

    #[test]
    fn package_spec_accepts_versions_tags_and_local_builds() {
        assert_eq!(
            package_spec(Some("2.0.25")).unwrap(),
            "@datadog/ai-setup-cli@2.0.25"
        );
        assert_eq!(
            package_spec(Some("next")).unwrap(),
            "@datadog/ai-setup-cli@next"
        );
        for spec in [
            "/tmp/datadog-ai-setup-cli-2.1.0.tgz",
            "./datadog-ai-setup-cli-2.1.0.tgz",
            "datadog-ai-setup-cli-2.1.0.tgz",
            "file:../ai-setup-cli",
            "@datadog/ai-setup-cli@2.1.0",
        ] {
            assert_eq!(package_spec(Some(spec)).unwrap(), spec);
        }
    }

    #[test]
    fn package_spec_rejects_flag_injection_and_whitespace() {
        for bad in ["--registry=https://evil.example", "-y", "2.0 --foo", "a\tb"] {
            let err = package_spec(Some(bad)).unwrap_err().to_string();
            assert!(err.contains("PUP_SETUP_AI_SETUP_PACKAGE"), "got: {err}");
        }
    }

    #[test]
    fn build_command_strips_keys_when_oauth_token_present() {
        let mut cfg = test_config("http://unused");
        cfg.access_token = Some("oauth-token".into());
        let cmd = build_command(Path::new("/usr/bin/npx"), &[], &cfg);
        assert_eq!(env_value(&cmd, "DD_API_KEY"), Some(None));
        assert_eq!(env_value(&cmd, "DD_APP_KEY"), Some(None));
        assert_eq!(
            env_value(&cmd, "DD_ACCESS_TOKEN"),
            Some(Some("oauth-token".into()))
        );
        assert_eq!(
            env_value(&cmd, "DD_SITE"),
            Some(Some("datadoghq.com".into()))
        );
        cleanup_env();
    }

    #[test]
    fn build_command_keeps_keys_without_oauth_token() {
        let cfg = test_config("http://unused");
        let cmd = build_command(Path::new("/usr/bin/npx"), &[], &cfg);
        assert_eq!(
            env_value(&cmd, "DD_API_KEY"),
            Some(Some("test-api-key".into()))
        );
        assert_eq!(env_value(&cmd, "DD_ACCESS_TOKEN"), Some(None));
        cleanup_env();
    }

    #[test]
    fn build_command_forwards_agent_mode_and_output() {
        let mut cfg = test_config("http://unused");
        cfg.agent_mode = true;
        let cmd = build_command(Path::new("/usr/bin/npx"), &args(&["--help"]), &cfg);
        assert_eq!(env_value(&cmd, "PUP_AGENT_MODE"), Some(Some("true".into())));
        assert_eq!(env_value(&cmd, "PUP_OUTPUT"), Some(Some("json".into())));
        assert!(cmd.get_args().any(|a| a == "--help"));
        cleanup_env();
    }

    #[test]
    fn parse_node_major_handles_versions() {
        assert_eq!(parse_node_major("v22.3.0\n"), Some(22));
        assert_eq!(parse_node_major("18.19.0"), Some(18));
        assert_eq!(parse_node_major("garbage"), None);
        assert_eq!(parse_node_major(""), None);
    }

    #[test]
    fn node_status_cases() {
        let npx = PathBuf::from("/bin/npx");
        assert_eq!(
            node_status(Some(npx.clone()), Some("v22.1.0")),
            NodeStatus::Ready(npx.clone())
        );
        assert_eq!(node_status(None, Some("v24.0.0")), NodeStatus::NpxNotFound);
        assert_eq!(
            node_status(Some(npx.clone()), None),
            NodeStatus::NodeNotFound
        );
        assert_eq!(
            node_status(Some(npx), Some("v18.19.0")),
            NodeStatus::NodeTooOld("v18.19.0".into())
        );
        assert_eq!(
            NodeStatus::NodeTooOld("v18.19.0".into()).summary(),
            "node_too_old:v18.19.0"
        );
    }

    fn no_credentials(cfg: &mut Config) {
        cfg.api_key = None;
        cfg.app_key = None;
        cfg.access_token = None;
    }

    #[tokio::test]
    async fn no_credentials_logs_in_then_uses_new_token() {
        let _lock = lock_env().await;
        let mut cfg = test_config("http://unused");
        no_credentials(&mut cfg);
        let called = std::cell::Cell::new(false);
        let result = ensure_session(
            &mut cfg,
            None,
            async |_: &Config| {
                called.set(true);
                Ok(())
            },
            |_| Some("new-token".into()),
        )
        .await;
        assert!(result.is_ok());
        assert!(called.get());
        assert_eq!(cfg.access_token.as_deref(), Some("new-token"));
        cleanup_env();
    }

    #[tokio::test]
    async fn session_login_decision_follows_granted_scopes() {
        let _lock = lock_env().await;
        let cases = [
            (None, false),
            (Some("apm_read api_keys_read dashboards_read"), false),
            (Some("apm_read dashboards_read"), true),
            (Some(""), true),
        ];
        for (granted_scopes, expect_login) in cases {
            let mut cfg = test_config("http://unused");
            cfg.access_token = Some("old-token".into());
            let called = std::cell::Cell::new(false);
            let result = ensure_session(
                &mut cfg,
                granted_scopes,
                async |_: &Config| {
                    called.set(true);
                    Ok(())
                },
                |_| Some("new-token".into()),
            )
            .await;
            assert!(result.is_ok(), "{granted_scopes:?}");
            assert_eq!(called.get(), expect_login, "{granted_scopes:?}");
            let expected_token = if expect_login {
                "new-token"
            } else {
                "old-token"
            };
            assert_eq!(cfg.access_token.as_deref(), Some(expected_token));
        }
        cleanup_env();
    }

    #[test]
    fn key_pair_without_session_skips_login() {
        let mut cfg = test_config("http://unused");
        cfg.access_token = None;
        cfg.api_key = Some("api".into());
        cfg.app_key = Some("app".into());
        assert!(!needs_login(&cfg, None));
    }

    #[tokio::test]
    async fn login_failure_stops_setup() {
        let _lock = lock_env().await;
        let mut cfg = test_config("http://unused");
        no_credentials(&mut cfg);
        let result = ensure_session(
            &mut cfg,
            None,
            async |_: &Config| Err(anyhow::anyhow!("login cancelled")),
            |_| Some("unused".into()),
        )
        .await;
        assert_eq!(result.unwrap_err().to_string(), "login cancelled");
        assert!(cfg.access_token.is_none());
        cleanup_env();
    }

    #[tokio::test]
    async fn login_without_saved_session_is_an_error() {
        let _lock = lock_env().await;
        let mut cfg = test_config("http://unused");
        no_credentials(&mut cfg);
        let result = ensure_session(&mut cfg, None, async |_: &Config| Ok(()), |_| None).await;
        assert!(result.unwrap_err().to_string().contains("no pup session"));
        cleanup_env();
    }

    #[test]
    fn logs_in_for_headless_runs_or_existing_sessions_but_not_help() {
        let setup = args(&["--product", "apm"]);
        assert!(should_log_in(true, false, &setup));
        assert!(should_log_in(false, true, &setup));
        assert!(!should_log_in(false, false, &setup));
        assert!(!should_log_in(true, true, &args(&["--help"])));
        assert!(!should_log_in(
            true,
            false,
            &args(&["--product", "apm", "-h"])
        ));
    }

    #[test]
    fn interactive_without_session_forwards_no_credentials() {
        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        let mut cfg = test_config("http://unused");
        no_credentials(&mut cfg);
        let built = build_args(
            &args(&["--product", "rum"]),
            &cfg.site,
            &launch(wants_headless(false, true, true), false),
        );
        assert!(!built.contains(&"--headless".to_string()));
        let cmd = build_command(Path::new("/usr/bin/npx"), &built, &cfg);
        for name in ["DD_ACCESS_TOKEN", "DD_API_KEY", "DD_APP_KEY"] {
            assert_eq!(env_value(&cmd, name), Some(None), "{name} must be removed");
        }
        cleanup_env();
    }

    #[test]
    fn lone_api_key_is_not_forwarded() {
        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        let mut cfg = test_config("http://unused");
        cfg.app_key = None;
        let cmd = build_command(Path::new("/usr/bin/npx"), &[], &cfg);
        assert_eq!(env_value(&cmd, "DD_API_KEY"), Some(None));
        assert_eq!(env_value(&cmd, "DD_APP_KEY"), Some(None));
        cleanup_env();
    }

    #[test]
    fn find_on_path_finds_files_only() {
        let dir = std::env::temp_dir().join(format!("pup-setup-path-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("npx-dir")).unwrap();
        let name = if cfg!(windows) { "node.exe" } else { "node" };
        std::fs::write(dir.join(name), "").unwrap();
        let path = std::env::join_paths([dir.clone()]).unwrap();
        assert_eq!(find_on_path("node", &path), Some(dir.join(name)));
        assert_eq!(find_on_path("npx-dir", &path), None);
        assert_eq!(find_on_path("npx", &path), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn missing_node_records_failed_session() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("POST", "/api/v2/onboarding/sessions")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "data": {
                    "type": "onboarding_session",
                    "attributes": {
                        "skill_ids": ["orchestrator"],
                        "summary": "node_not_found",
                        "status": "failed",
                    },
                },
            })))
            .with_status(201)
            .with_header("content-type", "application/vnd.api+json")
            .with_body(r#"{"data":{"id":"pup-setup"}}"#)
            .create_async()
            .await;
        assert_eq!(
            report_missing_node(&cfg, &NodeStatus::NodeNotFound).await,
            1
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn telemetry_api_error_keeps_exit_code() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("POST", "/api/v2/onboarding/sessions")
            .with_status(500)
            .with_body(r#"{"errors":["boom"]}"#)
            .create_async()
            .await;
        let status = NodeStatus::NodeTooOld("v18.0.0".into());
        assert_eq!(report_missing_node(&cfg, &status).await, 1);
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn telemetry_skipped_without_credentials() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        cfg.api_key = None;
        cfg.app_key = None;
        let mock = server
            .mock("POST", mockito::Matcher::Any)
            .expect(0)
            .create_async()
            .await;
        assert_eq!(report_missing_node(&cfg, &NodeStatus::NpxNotFound).await, 1);
        mock.assert_async().await;
        cleanup_env();
    }
}

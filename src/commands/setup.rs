use anyhow::Result;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::auth::storage::SessionEntry;
use crate::commands::setup_help;
use crate::config::Config;
use crate::extensions::exec;

const AI_SETUP_PACKAGE: &str = "@datadog/ai-setup-cli";
const AI_SETUP_BIN: &str = "ai-setup-cli";
const AI_SETUP_DEFAULT_VERSION: &str = "latest";
const AI_SETUP_PACKAGE_ENV: &str = "PUP_SETUP_AI_SETUP_PACKAGE";
const MIN_NODE_MAJOR: u32 = 22;
const NODE_DOWNLOAD_URL: &str = "https://nodejs.org/en/download";
pub const LOGIN_EXTRA_SCOPES: &str = "api_keys_read,product_analytics_apps_write";
const TELEMETRY_TIMEOUT: Duration = Duration::from_secs(2);
const TELEMETRY_EVENT: &str = "setup_prerequisite_missing";
const TELEMETRY_SERVICE: &str = "setup-cli";
const TELEMETRY_TAGS: &str = "team:agentic-onboarding,service:setup-cli,source:pup";

#[derive(Debug, PartialEq, Eq)]
enum NodeStatus {
    Ready(PathBuf),
    NpxNotFound,
    NodeNotFound,
    NodeTooOld(String),
}

impl NodeStatus {
    fn reason(&self) -> &'static str {
        match self {
            NodeStatus::Ready(_) => "ready",
            NodeStatus::NpxNotFound => "npx_not_found",
            NodeStatus::NodeNotFound => "node_not_found",
            NodeStatus::NodeTooOld(_) => "node_too_old",
        }
    }

    fn node_version(&self) -> Option<&str> {
        match self {
            NodeStatus::NodeTooOld(version) => Some(version),
            _ => None,
        }
    }

    fn message(&self) -> String {
        match self {
            NodeStatus::NodeTooOld(version) => format!(
                "pup setup needs Node.js {MIN_NODE_MAJOR} or newer (found {version}). \
                 Upgrade from {NODE_DOWNLOAD_URL} and rerun `pup setup`."
            ),
            NodeStatus::NpxNotFound => format!(
                "pup setup runs Datadog's AI Setup with npx, which wasn't found. npx comes \
                 with npm: install npm (for example `sudo apt install npm`) or reinstall \
                 Node.js from {NODE_DOWNLOAD_URL}, then rerun `pup setup`."
            ),
            _ => format!(
                "pup setup runs Datadog's AI Setup with npx, which needs Node.js \
                 {MIN_NODE_MAJOR} or newer. Install it from {NODE_DOWNLOAD_URL} and rerun \
                 `pup setup`."
            ),
        }
    }
}

pub async fn run(
    cfg: &mut Config,
    args: &[String],
    agent: bool,
    login: impl AsyncFnOnce(&Config) -> Result<SessionEntry>,
) -> Result<i32> {
    let args = &normalize_help_args(args);
    let status = probe_node();
    let NodeStatus::Ready(npx) = status else {
        return Ok(report_missing_node(cfg, &status).await);
    };
    let package = package_spec(std::env::var(AI_SETUP_PACKAGE_ENV).ok().as_deref())?;
    if asks_for_help(args) {
        let raw = fetch_ai_setup_help(&npx, &package, cfg);
        if has_flag(args, "--json") {
            let help = raw
                .as_deref()
                .and_then(setup_help::to_pup_json)
                .ok_or_else(|| anyhow::anyhow!("could not load AI Setup's help from {package}"))?;
            println!("{help}");
        } else {
            setup_help::print(raw.as_deref().and_then(setup_help::parse).as_ref())?;
        }
        return Ok(0);
    }
    let headless = asks_for_headless(args)
        || wants_headless(
            agent,
            std::io::stdin().is_terminal(),
            std::io::stdout().is_terminal(),
        );
    let had_session = cfg.access_token.is_some();
    let mut login = Some(login);
    if should_log_in(headless, had_session, args) {
        let granted_scopes = stored_token_scopes(cfg);
        if let Some(login) = login.take() {
            ensure_session(cfg, granted_scopes.as_deref(), login, |cfg| {
                crate::settings::load_token_from_storage(&cfg.site, cfg.org.as_deref())
            })
            .await?;
        }
    }
    let launch = Launch { package, headless };
    let cmd = build_command(&npx, &build_args(args, &cfg.site, &launch), cfg);
    let exit_code = exec::run_inherited(cmd, "npx")?;
    if should_offer_pup_login(headless, had_session, args, exit_code) {
        if let Some(login) = login.take() {
            offer_pup_login(cfg, login).await;
        }
    }
    Ok(exit_code)
}

pub fn apply_site_arg(cfg: &mut Config, args: &[String]) -> Result<()> {
    let site_flags = args
        .iter()
        .filter(|arg| *arg == "--site" || arg.starts_with("--site="))
        .count();
    if site_flags > 1 {
        anyhow::bail!("--site was given more than once");
    }
    let Some(site) = flag_value(args, "--site") else {
        return Ok(());
    };
    let previous = cfg.site.clone();
    cfg.set_site_explicit(site)?;
    let token_from_env = std::env::var("DD_ACCESS_TOKEN").is_ok_and(|token| !token.is_empty());
    if cfg.site != previous && !token_from_env {
        cfg.access_token = crate::settings::load_token_from_storage(&cfg.site, cfg.org.as_deref());
    }
    Ok(())
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let prefix = format!("{flag}=");
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == flag {
            return Some(iter.next().cloned().unwrap_or_default());
        }
        if let Some(value) = arg.strip_prefix(&prefix) {
            return Some(value.to_string());
        }
    }
    None
}

fn without_flag(args: &[String], flag: &str) -> Vec<String> {
    let prefix = format!("{flag}=");
    let mut kept = Vec::with_capacity(args.len());
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == flag {
            iter.next();
        } else if !arg.starts_with(&prefix) {
            kept.push(arg.clone());
        }
    }
    kept
}

fn should_offer_pup_login(
    headless: bool,
    had_session: bool,
    args: &[String],
    exit_code: i32,
) -> bool {
    !headless && !had_session && exit_code == 0 && !asks_for_help(args)
}

async fn offer_pup_login(cfg: &Config, login: impl AsyncFnOnce(&Config) -> Result<SessionEntry>) {
    eprint!(
        "\nLog in to pup on {} too, so you can use its other commands? [Y/n] ",
        cfg.site
    );
    std::io::Write::flush(&mut std::io::stderr()).ok();
    let mut answer = String::new();
    let read = std::io::stdin().read_line(&mut answer);
    if !accepts_login(read, &answer) {
        eprintln!("You can run `pup auth login` any time.");
        return;
    }
    if let Err(err) = login(cfg).await {
        eprintln!("pup login failed ({err}). You can run `pup auth login` any time.");
    }
}

fn accepts_login(read: std::io::Result<usize>, answer: &str) -> bool {
    matches!(read, Ok(bytes) if bytes > 0)
        && matches!(answer.trim().to_lowercase().as_str(), "" | "y" | "yes")
}

fn signup_hint(site: &str) -> String {
    format!(
        "pup setup needs a Datadog login. No account yet? Sign up at https://{}/signup, \
         then rerun `pup setup`.",
        crate::settings::auth_host_for(site)
    )
}

#[derive(Debug)]
struct Launch {
    package: String,
    headless: bool,
}

async fn report_missing_node(cfg: &Config, status: &NodeStatus) -> i32 {
    eprintln!("{}", status.message());
    report_prerequisite_missing(cfg, status).await;
    1
}

async fn ensure_session(
    cfg: &mut Config,
    granted_scopes: Option<&str>,
    login: impl AsyncFnOnce(&Config) -> Result<SessionEntry>,
    stored_token: impl FnOnce(&Config) -> Option<String>,
) -> Result<()> {
    if !needs_login(cfg, granted_scopes) {
        return Ok(());
    }
    if cfg.access_token.is_none() {
        eprintln!("{}", signup_hint(&cfg.site));
    }
    let session = login(cfg).await?;
    cfg.site = session.site;
    cfg.org = session.org;
    cfg.access_token = stored_token(cfg);
    if cfg.access_token.is_none() {
        anyhow::bail!("login finished but no pup session was found; run `pup auth login`");
    }
    Ok(())
}

fn has_credentials(cfg: &Config) -> bool {
    cfg.access_token.is_some() || (cfg.api_key.is_some() && cfg.app_key.is_some())
}

fn needs_login(cfg: &Config, granted_scopes: Option<&str>) -> bool {
    if cfg.access_token.is_none() {
        return !has_credentials(cfg);
    }
    granted_scopes.is_some_and(|scopes| !has_scopes(scopes, LOGIN_EXTRA_SCOPES))
}

fn has_scopes(granted: &str, required: &str) -> bool {
    let granted: Vec<&str> = granted.split_whitespace().collect();
    crate::settings::parse_scopes(required)
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
    active_token_scopes(cfg, tokens)
}

fn active_token_scopes(cfg: &Config, saved: crate::auth::types::TokenSet) -> Option<String> {
    (cfg.access_token.as_deref() == Some(saved.access_token.as_str())).then_some(saved.scope)
}

fn should_log_in(headless: bool, has_session: bool, args: &[String]) -> bool {
    (headless || has_session) && !asks_for_help(args)
}

fn normalize_help_args(args: &[String]) -> Vec<String> {
    let mut args = args.to_vec();
    if args.first().is_some_and(|arg| arg == "help") {
        args[0] = "--help".to_string();
    }
    args
}

fn fetch_ai_setup_help(npx: &Path, package: &str, cfg: &Config) -> Option<Vec<u8>> {
    let args = ["-y", "--package", package, AI_SETUP_BIN, "--help", "--json"].map(String::from);
    let output = build_command(npx, &args, cfg)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

fn asks_for_help(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--help" || arg == "-h")
}

pub fn is_help_request(args: &[String]) -> bool {
    asks_for_help(&normalize_help_args(args))
}

async fn report_prerequisite_missing(cfg: &Config, status: &NodeStatus) {
    let Some(token) = intake_client_token(&cfg.site) else {
        return;
    };
    let url = format!("https://http-intake.logs.{}/v1/input/{token}", cfg.site);
    let dd_oid = cfg
        .access_token
        .as_ref()
        .and_then(|_| crate::auth::storage::find_session(cfg.org.as_deref()))
        .filter(|session| session.site == cfg.site)
        .and_then(|session| session.org_uuid);
    send_intake_event(&url, &prerequisite_event(cfg, status, dd_oid.as_deref())).await;
}

fn intake_client_token(site: &str) -> Option<&'static str> {
    match site {
        "datad0g.com" => Some("pub9a1f0104f5632c5840cca5a99a43c32a"),
        "datadoghq.com" => Some("pub5b34c05a3abc178e4d2abddf11bb4f31"),
        "us3.datadoghq.com" => Some("pub82dbd99bf6299230cee3685535d4660d"),
        "us5.datadoghq.com" => Some("pubf9a4a5b93cc00e7f4d5628f05ba1e7f0"),
        "datadoghq.eu" => Some("pub471980dfdfb12f9cf2ffdf83551c9cb7"),
        "ap1.datadoghq.com" => Some("pubbbc10e19ab9fa2f7eb54250610e9bbad"),
        "ap2.datadoghq.com" => Some("pube391858a603cb55a0e72a8cb9f9fb627"),
        "uk1.datadoghq.com" => Some("pub90fe05f76003a728ed554865d39a88d1"),
        _ => None,
    }
}

fn prerequisite_event(
    cfg: &Config,
    status: &NodeStatus,
    dd_oid: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "message": TELEMETRY_EVENT,
        "dd_oid": dd_oid,
        "service": TELEMETRY_SERVICE,
        "source": "pup",
        "reason": status.reason(),
        "node_version": status.node_version(),
        "pup_version": crate::version::VERSION,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "site": cfg.site,
        "authenticated": has_credentials(cfg),
    })
}

async fn send_intake_event(url: &str, event: &serde_json::Value) {
    let Ok(client) = reqwest::Client::builder()
        .timeout(TELEMETRY_TIMEOUT)
        .build()
    else {
        return;
    };
    let _ = client
        .post(url)
        .query(&[("ddsource", "pup"), ("ddtags", TELEMETRY_TAGS)])
        .json(event)
        .send()
        .await;
}

fn wants_headless(agent: bool, stdin_tty: bool, stdout_tty: bool) -> bool {
    agent || !stdin_tty || !stdout_tty
}

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
    if launch.headless {
        args.push("--headless".to_string());
    }
    args.push("--site".to_string());
    args.push(site.to_string());
    args.extend(
        without_flag(user_args, "--site")
            .into_iter()
            .filter(|arg| !is_headless_flag(arg)),
    );
    args
}

fn asks_for_headless(args: &[String]) -> bool {
    args.iter()
        .any(|arg| arg == "--headless" || arg == "--headless=true")
}

fn is_headless_flag(arg: &str) -> bool {
    arg == "--headless" || arg == "--no-headless" || arg.starts_with("--headless=")
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter()
        .any(|arg| arg == flag || arg.starts_with(&format!("{flag}=")))
}

fn build_command(npx: &Path, args: &[String], cfg: &Config) -> Command {
    let mut cmd = Command::new(npx);
    cmd.args(args);
    cmd.env("AI_SETUP_CLI_LAUNCHED_BY", "pup");
    cmd.env("AI_SETUP_CLI_LAUNCHER_VERSION", crate::version::VERSION);
    exec::inject_auth_env(&mut cmd, cfg);
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

    fn launch(headless: bool) -> Launch {
        Launch {
            package: package_spec(None).unwrap(),
            headless,
        }
    }

    #[test]
    fn build_args_injects_headless_and_site_before_user_args() {
        assert_eq!(
            build_args(
                &args(&["--product", "linux"]),
                "datadoghq.eu",
                &launch(true)
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
            &launch(false),
        );
        assert!(!built.contains(&"--headless".to_string()));
        assert!(built.contains(&"--site".to_string()));
    }

    #[test]
    fn build_args_passes_pup_site_once_in_place_of_user_site() {
        for user in [
            args(&[
                "--headless",
                "--site",
                "https://app.us5.datadoghq.com",
                "--product",
                "linux",
            ]),
            args(&[
                "--headless",
                "--site=https://app.us5.datadoghq.com",
                "--product",
                "linux",
            ]),
        ] {
            let built = build_args(&user, "us5.datadoghq.com", &launch(true));
            assert_eq!(built.iter().filter(|a| *a == "--headless").count(), 1);
            assert_eq!(built.iter().filter(|a| a.starts_with("--site")).count(), 1);
            assert!(built.contains(&"us5.datadoghq.com".to_string()));
            assert!(!built.iter().any(|a| a.contains("https:")));
            assert!(built.ends_with(&args(&["--product", "linux"])));
        }
    }

    #[tokio::test]
    async fn site_arg_becomes_pup_site_and_keeps_env_token() {
        let _lock = lock_env().await;
        std::env::set_var("DD_ACCESS_TOKEN", "env-token");
        let mut cfg = test_config("http://unused");
        cfg.site = "datadoghq.com".into();
        cfg.access_token = Some("env-token".into());

        apply_site_arg(
            &mut cfg,
            &args(&["--product", "apm", "--site=https://app.datadoghq.eu/"]),
        )
        .unwrap();
        assert_eq!(cfg.site, "datadoghq.eu");
        assert!(cfg.site_explicit);
        assert_eq!(cfg.access_token.as_deref(), Some("env-token"));

        for missing in [
            args(&["--site", "   "]),
            args(&["--site="]),
            args(&["--product", "linux", "--site"]),
        ] {
            assert!(apply_site_arg(&mut cfg, &missing).is_err(), "{missing:?}");
        }
        cleanup_env();
    }

    #[test]
    fn site_arg_given_twice_is_rejected() {
        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        let mut cfg = test_config("http://unused");
        cfg.site = "datadoghq.com".into();
        for repeated in [
            args(&["--site", "datadoghq.com", "--site", "datadoghq.eu"]),
            args(&["--site=datadoghq.com", "--site", "datadoghq.eu"]),
            args(&["--site", "datadoghq.com", "--site"]),
        ] {
            assert!(apply_site_arg(&mut cfg, &repeated).is_err(), "{repeated:?}");
            assert_eq!(cfg.site, "datadoghq.com");
        }
        cleanup_env();
    }

    #[test]
    fn user_cannot_turn_off_required_headless() {
        for user in [
            args(&["--headless=false", "--product", "linux"]),
            args(&["--no-headless", "--product", "linux"]),
            args(&["--headless", "--no-headless", "--product", "linux"]),
        ] {
            let built = build_args(&user, "datadoghq.com", &launch(true));
            assert_eq!(built.iter().filter(|a| is_headless_flag(a)).count(), 1);
            assert!(built.contains(&"--headless".to_string()));
            assert!(built.ends_with(&args(&["--product", "linux"])));
        }
    }

    #[test]
    fn only_true_headless_flags_ask_for_headless() {
        assert!(asks_for_headless(&args(&["--headless"])));
        assert!(asks_for_headless(&args(&["--headless=true"])));
        assert!(!asks_for_headless(&args(&["--headless=false"])));
        assert!(!asks_for_headless(&args(&["--no-headless"])));
        let interactive = build_args(
            &args(&["--headless=false", "--product", "rum"]),
            "datadoghq.com",
            &launch(false),
        );
        assert!(!interactive.iter().any(|a| is_headless_flag(a)));
    }

    #[test]
    fn flag_value_reads_both_forms() {
        assert_eq!(
            flag_value(
                &args(&["--product", "apm", "--site", "datadoghq.eu"]),
                "--site"
            ),
            Some("datadoghq.eu".into())
        );
        assert_eq!(
            flag_value(&args(&["--site=datadoghq.eu"]), "--site"),
            Some("datadoghq.eu".into())
        );
        assert_eq!(flag_value(&args(&["--product", "apm"]), "--site"), None);
    }

    #[test]
    fn build_args_passes_help_through() {
        let built = build_args(&args(&["--help"]), "datadoghq.com", &launch(false));
        assert_eq!(built.last().map(String::as_str), Some("--help"));
        assert!(!built.contains(&"--json".to_string()));
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
        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
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
    fn build_command_tells_ai_setup_it_was_launched_by_pup() {
        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        let cfg = test_config("http://unused");
        let cmd = build_command(Path::new("/usr/bin/npx"), &[], &cfg);
        assert_eq!(
            env_value(&cmd, "AI_SETUP_CLI_LAUNCHED_BY"),
            Some(Some("pup".into()))
        );
        assert_eq!(
            env_value(&cmd, "AI_SETUP_CLI_LAUNCHER_VERSION"),
            Some(Some(crate::version::VERSION.into()))
        );
        cleanup_env();
    }

    #[test]
    fn build_command_keeps_keys_without_oauth_token() {
        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
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
    fn build_command_forwards_output_format() {
        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        let cfg = test_config("http://unused");
        let cmd = build_command(Path::new("/usr/bin/npx"), &args(&["--help"]), &cfg);
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
    fn missing_node_messages_match_the_cause() {
        let npx = NodeStatus::NpxNotFound.message();
        assert!(npx.contains("npx, which wasn't found"), "{npx}");
        assert!(npx.contains("install npm"), "{npx}");
        let missing = NodeStatus::NodeNotFound.message();
        assert!(missing.contains("needs Node.js 22 or newer"), "{missing}");
        assert!(!missing.contains("install npm"), "{missing}");
        let old = NodeStatus::NodeTooOld("v20.20.1".into()).message();
        assert!(old.contains("found v20.20.1"), "{old}");
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
        let too_old = NodeStatus::NodeTooOld("v18.19.0".into());
        assert_eq!(too_old.reason(), "node_too_old");
        assert_eq!(too_old.node_version(), Some("v18.19.0"));
        assert_eq!(NodeStatus::NpxNotFound.node_version(), None);
    }

    fn no_credentials(cfg: &mut Config) {
        cfg.api_key = None;
        cfg.app_key = None;
        cfg.access_token = None;
    }

    fn saved_session(cfg: &Config) -> SessionEntry {
        SessionEntry {
            site: cfg.site.clone(),
            org: cfg.org.clone(),
            org_uuid: None,
        }
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
            async |cfg: &Config| {
                called.set(true);
                Ok(saved_session(cfg))
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
            (
                Some("apm_read api_keys_read product_analytics_apps_write dashboards_read"),
                false,
            ),
            (Some("apm_read api_keys_read dashboards_read"), true),
            (Some("apm_read product_analytics_apps_write"), true),
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
                async |cfg: &Config| {
                    called.set(true);
                    Ok(saved_session(cfg))
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
        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        let mut cfg = test_config("http://unused");
        cfg.access_token = None;
        cfg.api_key = Some("api".into());
        cfg.app_key = Some("app".into());
        assert!(!needs_login(&cfg, None));
        cleanup_env();
    }

    #[test]
    fn saved_scopes_count_only_for_the_active_token() {
        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        let saved = |token: &str| crate::auth::types::TokenSet {
            access_token: token.into(),
            refresh_token: String::new(),
            token_type: "Bearer".into(),
            expires_in: 3600,
            issued_at: 0,
            scope: "apm_read".into(),
            client_id: String::new(),
        };
        let mut cfg = test_config("http://unused");
        cfg.access_token = Some("config-token".into());
        assert_eq!(active_token_scopes(&cfg, saved("stored-token")), None);
        assert_eq!(
            active_token_scopes(&cfg, saved("config-token")).as_deref(),
            Some("apm_read")
        );
        cleanup_env();
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
        let result = ensure_session(
            &mut cfg,
            None,
            async |cfg: &Config| Ok(saved_session(cfg)),
            |_| None,
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("no pup session"));
        cleanup_env();
    }

    #[tokio::test]
    async fn login_that_switches_region_or_org_loads_that_session() {
        let _lock = lock_env().await;
        let mut cfg = test_config("http://unused");
        no_credentials(&mut cfg);
        cfg.site = "datadoghq.com".into();
        let result = ensure_session(
            &mut cfg,
            None,
            async |_: &Config| {
                Ok(SessionEntry {
                    site: "us3.datadoghq.com".into(),
                    org: Some("acme".into()),
                    org_uuid: None,
                })
            },
            |cfg| {
                (cfg.site == "us3.datadoghq.com" && cfg.org.as_deref() == Some("acme"))
                    .then(|| "us3-token".to_string())
            },
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(cfg.site, "us3.datadoghq.com");
        assert_eq!(cfg.access_token.as_deref(), Some("us3-token"));
        cleanup_env();
    }

    #[test]
    fn leading_help_word_is_treated_as_help_flag() {
        assert_eq!(
            normalize_help_args(&args(&["help", "--product", "linux"])),
            args(&["--help", "--product", "linux"])
        );
        assert!(asks_for_help(&normalize_help_args(&args(&["help"]))));
        assert_eq!(
            normalize_help_args(&args(&["--product", "help"])),
            args(&["--product", "help"])
        );
    }

    #[test]
    fn offers_pup_login_only_after_successful_terminal_runs_without_a_session() {
        let setup = args(&["--product", "apm"]);
        assert!(should_offer_pup_login(false, false, &setup, 0));
        assert!(!should_offer_pup_login(true, false, &setup, 0));
        assert!(!should_offer_pup_login(false, true, &setup, 0));
        assert!(!should_offer_pup_login(false, false, &setup, 1));
        assert!(!should_offer_pup_login(false, false, &args(&["--help"]), 0));
    }

    #[test]
    fn pup_login_offer_defaults_to_yes() {
        for answer in ["\n", "y\n", "YES\n"] {
            assert!(accepts_login(Ok(answer.len()), answer), "{answer:?}");
        }
        for answer in ["n\n", "no\n", "later\n"] {
            assert!(!accepts_login(Ok(answer.len()), answer), "{answer:?}");
        }
    }

    #[test]
    fn pup_login_offer_declines_on_end_of_input_or_read_error() {
        assert!(!accepts_login(Ok(0), ""));
        assert!(!accepts_login(Err(std::io::Error::other("closed")), ""));
    }

    #[test]
    fn signup_hint_points_at_the_site_ui_host() {
        assert!(signup_hint("datadoghq.com").contains("https://app.datadoghq.com/signup"));
        assert!(signup_hint("us5.datadoghq.com").contains("https://us5.datadoghq.com/signup"));
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
            &launch(wants_headless(false, true, true)),
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

    #[test]
    fn intake_tokens_cover_datadog_sites_only() {
        for site in [
            "datad0g.com",
            "datadoghq.com",
            "us3.datadoghq.com",
            "us5.datadoghq.com",
            "datadoghq.eu",
            "ap1.datadoghq.com",
            "ap2.datadoghq.com",
            "uk1.datadoghq.com",
        ] {
            assert!(
                intake_client_token(site).is_some_and(|t| t.starts_with("pub")),
                "{site}"
            );
        }
        assert_eq!(intake_client_token("mygateway.example.com"), None);
    }

    #[test]
    fn prerequisite_event_describes_the_failure() {
        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        let mut cfg = test_config("http://unused");
        cfg.site = "datad0g.com".into();
        no_credentials(&mut cfg);
        let event = prerequisite_event(&cfg, &NodeStatus::NodeTooOld("v18.19.0".into()), None);
        assert_eq!(event["message"], "setup_prerequisite_missing");
        assert_eq!(event["service"], "setup-cli");
        assert_eq!(event["source"], "pup");
        assert_eq!(event["reason"], "node_too_old");
        assert_eq!(event["node_version"], "v18.19.0");
        assert_eq!(event["site"], "datad0g.com");
        assert_eq!(event["authenticated"], false);
        assert_eq!(event["pup_version"], crate::version::VERSION);
        assert!(event["dd_oid"].is_null());

        let org_uuid = "1dd437f2-ecb6-11f0-8e9a-0217575d1df7";
        let event = prerequisite_event(&cfg, &NodeStatus::NodeNotFound, Some(org_uuid));
        assert_eq!(event["dd_oid"], org_uuid);
        cleanup_env();
    }

    #[tokio::test]
    async fn intake_event_is_posted_with_setup_cli_tags() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/input/pub-test")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("ddsource".into(), "pup".into()),
                mockito::Matcher::UrlEncoded("ddtags".into(), TELEMETRY_TAGS.into()),
            ]))
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "message": "setup_prerequisite_missing",
                "reason": "node_not_found",
            })))
            .with_status(202)
            .create_async()
            .await;
        let cfg = test_config("http://unused");
        let event = prerequisite_event(&cfg, &NodeStatus::NodeNotFound, None);
        send_intake_event(&format!("{}/v1/input/pub-test", server.url()), &event).await;
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn intake_failure_is_ignored() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(500)
            .create_async()
            .await;
        let cfg = test_config("http://unused");
        let event = prerequisite_event(&cfg, &NodeStatus::NpxNotFound, None);
        send_intake_event(&format!("{}/v1/input/pub-test", server.url()), &event).await;
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn missing_node_exits_1_on_hosts_without_an_intake_token() {
        let mut cfg = test_config("http://unused");
        cfg.site = "mygateway.example.com".into();
        assert_eq!(
            report_missing_node(&cfg, &NodeStatus::NodeNotFound).await,
            1
        );
    }
}

use anyhow::Result;
use std::path::Path;

use crate::config::Config;

/// Spawn the extension executable with inherited stdio and auth environment.
/// Returns the extension's exit code.
pub fn exec_extension(ext_path: &Path, args: &[String], cfg: &Config) -> Result<i32> {
    let mut cmd = std::process::Command::new(ext_path);
    cmd.args(args);

    inject_auth_env(&mut cmd, cfg);

    let status = cmd
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| anyhow::anyhow!("failed to execute extension {}: {e}", ext_path.display()))?;

    // On Unix, if the process was killed by a signal, status.code() returns None.
    // Use the standard convention of 128 + signal_number.
    let exit_code = status.code().unwrap_or_else(|| {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            status.signal().map(|s| 128 + s).unwrap_or(1)
        }
        #[cfg(not(unix))]
        {
            1
        }
    });
    Ok(exit_code)
}

/// Set (or remove) auth and config environment variables on the child process command.
/// Variables not active in the current config are explicitly removed to prevent
/// stale credentials from leaking through the parent environment.
fn inject_auth_env(cmd: &mut std::process::Command, cfg: &Config) {
    // Always set site and output format.
    cmd.env("DD_SITE", &cfg.site);
    cmd.env("PUP_OUTPUT", cfg.output_format.to_string());

    // Set or unset auth variables based on current config.
    match &cfg.access_token {
        Some(token) => {
            cmd.env("DD_ACCESS_TOKEN", token);
        }
        None => {
            cmd.env_remove("DD_ACCESS_TOKEN");
        }
    }
    match &cfg.api_key {
        Some(key) => {
            cmd.env("DD_API_KEY", key);
        }
        None => {
            cmd.env_remove("DD_API_KEY");
        }
    }
    match &cfg.app_key {
        Some(key) => {
            cmd.env("DD_APP_KEY", key);
        }
        None => {
            cmd.env_remove("DD_APP_KEY");
        }
    }
    match &cfg.org {
        Some(org) => {
            cmd.env("DD_ORG", org);
        }
        None => {
            cmd.env_remove("DD_ORG");
        }
    }

    // GitHub tokens are used only by pup for extension install/list/upgrade.
    // Extensions receive Datadog auth, not repository access credentials.
    for name in ["GH_TOKEN", "GITHUB_TOKEN", "HOMEBREW_GITHUB_API_TOKEN"] {
        cmd.env_remove(name);
    }

    // Boolean mode flags - set when active, unset when not.
    if cfg.auto_approve {
        cmd.env("PUP_AUTO_APPROVE", "true");
    } else {
        cmd.env_remove("PUP_AUTO_APPROVE");
    }
    if cfg.read_only {
        cmd.env("PUP_READ_ONLY", "true");
    } else {
        cmd.env_remove("PUP_READ_ONLY");
    }
    if cfg.agent_mode {
        cmd.env("PUP_AGENT_MODE", "true");
    } else {
        cmd.env_remove("PUP_AGENT_MODE");
    }
    // Pass --jq expression to extension subprocesses so they can self-apply it.
    // Note: pup does not post-filter an extension's stdout; this only lets an
    // extension read the expression via PUP_FILTER if it chooses.
    match &cfg.jq {
        Some(expr) => {
            cmd.env("PUP_FILTER", expr);
        }
        None => {
            cmd.env_remove("PUP_FILTER");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OutputFormat;

    fn test_config() -> Config {
        Config {
            api_key: None,
            app_key: None,
            access_token: None,
            site: "datadoghq.com".to_string(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        }
    }

    fn removed_env(cmd: &std::process::Command, name: &str) -> bool {
        cmd.get_envs()
            .any(|(key, value)| key == name && value.is_none())
    }

    #[test]
    fn test_inject_auth_env_removes_github_tokens() {
        let cfg = test_config();
        let mut cmd = std::process::Command::new("pup-foo");

        inject_auth_env(&mut cmd, &cfg);

        assert!(removed_env(&cmd, "GH_TOKEN"));
        assert!(removed_env(&cmd, "GITHUB_TOKEN"));
        assert!(removed_env(&cmd, "HOMEBREW_GITHUB_API_TOKEN"));
    }
}

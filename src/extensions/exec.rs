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
    // Forward PAT/SAT under the env var name that matches `pat_kind` so the
    // child sees the same credential kind the parent resolved. If `pat_kind`
    // is None (no PAT configured), unset both to avoid leaking stale values
    // from the parent shell.
    match (&cfg.pat, cfg.pat_kind) {
        (Some(token), Some(crate::config::PatKind::Personal)) => {
            cmd.env("DD_PAT", token);
            cmd.env_remove("DD_SAT");
        }
        (Some(token), Some(crate::config::PatKind::Service)) => {
            cmd.env("DD_SAT", token);
            cmd.env_remove("DD_PAT");
        }
        _ => {
            cmd.env_remove("DD_PAT");
            cmd.env_remove("DD_SAT");
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
}

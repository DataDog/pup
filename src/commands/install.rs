//! `pup install <component> <platform>` — unified install entry point.
//!
//! **Status: scaffolding.** The CLI surface and module structure land in this PR;
//! the actual install flow (russh client, OCI registry fetch, signed package
//! install, `datadog-installer setup`) ships in follow-up PRs.
//!
//! Motivation lives in `docs/INSTALL.md` — short version: skills currently
//! embed `bash -c "$(curl …install_script_agent7.sh)"` to install the agent +
//! SSI on remote hosts, which scanners (correctly) flag as RCE-shaped and which
//! references a script that lives outside any repo we control. Moving the
//! install flow into pup turns the SKILL.md into a one-liner (`pup install …`)
//! and shifts the install logic into compiled Rust that already gets reviewed.
//!
//! Today this returns a structured "not yet implemented" error that lists the
//! planned steps. `--dry-run` prints the same plan without erroring so callers
//! can wire up the surface end-to-end.

use anyhow::{bail, Result};

use crate::config::Config;

/// Install Single Step Instrumentation on a remote Linux host.
///
/// The `--dry-run` flag returns successfully after printing the install plan.
/// Without it, returns an error today — the actual install lands in a follow-up
/// PR per the checklist in `docs/INSTALL.md`.
pub async fn ssi_linux(
    _cfg: &Config,
    host: String,
    user: String,
    key: Option<String>,
    port: u16,
    dry_run: bool,
) -> Result<()> {
    let plan = LinuxSsiPlan {
        host: &host,
        user: &user,
        key: key.as_deref(),
        port,
    };

    if dry_run {
        plan.print();
        return Ok(());
    }

    bail!(
        "`pup install ssi linux` is scaffolded but not yet executable in this build. \
         Re-run with --dry-run to see the planned steps for {user}@{host}:{port}, \
         or track the follow-up implementation work in docs/INSTALL.md.",
    )
}

/// Captured arguments for a single Linux-SSI install. Kept narrow on purpose —
/// the actual install code lives in follow-up PRs and will likely want a
/// richer config struct (e.g. proxy settings, sudo strategy, distro override).
struct LinuxSsiPlan<'a> {
    host: &'a str,
    user: &'a str,
    key: Option<&'a str>,
    port: u16,
}

impl LinuxSsiPlan<'_> {
    fn print(&self) {
        println!("# Planned `pup install ssi linux` steps");
        println!("#");
        println!(
            "# Target: {user}@{host}:{port}{key}",
            user = self.user,
            host = self.host,
            port = self.port,
            key = match self.key {
                Some(k) => format!("  (key: {k})"),
                None => "  (key: SSH agent / default identity)".to_string(),
            }
        );
        println!("#");
        println!("# 1. Open an SSH session via russh client.");
        println!("# 2. Detect distro family by reading /etc/os-release ($ID + $ID_LIKE).");
        println!("# 3. Configure Datadog package repo for the host:");
        println!("#      - Debian/Ubuntu → add apt repo signed by");
        println!("#        keys.datadoghq.com/DATADOG_APT_KEY_CURRENT.public");
        println!("#      - RHEL/CentOS/Amazon/Rocky/Alma → write /etc/yum.repos.d/datadog.repo");
        println!("# 4. Install signed packages: datadog-agent (+ datadog-signing-keys on apt).");
        println!("# 5. Download the `datadog-installer` binary from the OCI registry");
        println!("#      (gcr.io/datadoghq/installer-package), verify its SHA-256 against a");
        println!("#      pinned manifest, and execute it locally on the remote host.");
        println!("# 6. Run `datadog-installer setup` to install apm-inject + language");
        println!("#      libraries and to write /etc/ld.so.preload.");
        println!("# 7. Patch /etc/datadog-agent/datadog.yaml with DD_API_KEY + DD_SITE.");
        println!("# 8. Restart the agent and confirm `datadog-agent status` is healthy.");
        println!("#");
        println!("# None of these steps run today — this command is scaffolding so the pup team");
        println!("# can review the shape before the implementation lands. See docs/INSTALL.md.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_config;

    #[tokio::test]
    async fn dry_run_returns_ok_and_does_not_bail() {
        // ssi_linux's scaffolding doesn't read from cfg yet, so we just need
        // *any* Config to satisfy the signature.
        let cfg = test_config("http://unused.invalid");
        let result = ssi_linux(
            &cfg,
            "bastion.example.com".to_string(),
            "ec2-user".to_string(),
            Some("~/.ssh/id_ed25519".to_string()),
            22,
            /* dry_run */ true,
        )
        .await;
        assert!(
            result.is_ok(),
            "dry-run should succeed; got {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn non_dry_run_bails_with_actionable_message() {
        let cfg = test_config("http://unused.invalid");
        let err = ssi_linux(
            &cfg,
            "host.example.com".to_string(),
            "root".to_string(),
            None,
            22,
            /* dry_run */ false,
        )
        .await
        .expect_err("non-dry-run should bail until the real impl lands");
        let msg = format!("{err:#}");
        // The error message must tell the user what to do next, not just say
        // "not implemented" with no context.
        assert!(
            msg.contains("--dry-run"),
            "error should point at --dry-run as the working flow, got: {msg}"
        );
        assert!(
            msg.contains("docs/INSTALL.md"),
            "error should reference the design doc, got: {msg}"
        );
    }
}

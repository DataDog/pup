use anyhow::{bail, Result};
use std::io::Write;
use std::path::PathBuf;

use super::{discovery, install};
use crate::config::Config;

const FIRST_PARTY_EXTENSIONS: &[(&str, &str)] = &[("setup", "DataDog/pup-setup")];

/// Debug builds only: install first-party extensions from this local file
/// instead of their GitHub release, so the flow can be exercised before a
/// release exists.
const DEV_LOCAL_SOURCE_ENV: &str = "PUP_DEV_FIRST_PARTY_LOCAL_SOURCE";

pub(crate) fn first_party_source(name: &str) -> Option<&'static str> {
    FIRST_PARTY_EXTENSIONS
        .iter()
        .find(|(ext, _)| *ext == name)
        .map(|(_, source)| *source)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum InstallDecision {
    Install,
    Prompt,
    RefuseReadOnly,
    Hint,
}

pub(crate) fn decide(read_only: bool, auto_approve: bool, interactive: bool) -> InstallDecision {
    if read_only {
        InstallDecision::RefuseReadOnly
    } else if auto_approve {
        InstallDecision::Install
    } else if interactive {
        InstallDecision::Prompt
    } else {
        InstallDecision::Hint
    }
}

pub(crate) fn is_yes(answer: &str) -> bool {
    matches!(answer.trim().to_lowercase().as_str(), "y" | "yes")
}

/// Offers to install a first-party extension that is not installed yet.
/// Returns the installed executable path, or `None` when `name` is not a
/// first-party extension or the user declined.
pub(crate) fn offer_install(
    name: &str,
    cfg: &Config,
    interactive: bool,
) -> Result<Option<PathBuf>> {
    let Some(source) = first_party_source(name) else {
        return Ok(None);
    };

    match decide(cfg.read_only, cfg.auto_approve, interactive) {
        InstallDecision::RefuseReadOnly => bail!(
            "'{name}' is a first-party pup extension that is not installed, and installing \
             extensions is blocked in read-only mode. Run `pup extension install {source}` \
             without --read-only first."
        ),
        InstallDecision::Hint => bail!(
            "'{name}' is a first-party pup extension that is not installed. Install it with \
             `pup extension install {source}`, or rerun with --yes."
        ),
        InstallDecision::Install => {
            eprintln!(
                "pup: '{name}' is not installed; installing first-party extension from {source}"
            );
        }
        InstallDecision::Prompt => {
            eprint!(
                "'{name}' is a first-party pup extension from {source}. Install it now? [y/N]: "
            );
            std::io::stderr().flush().ok();
            let mut answer = String::new();
            std::io::stdin().read_line(&mut answer)?;
            if !is_yes(&answer) {
                return Ok(None);
            }
        }
    }

    match dev_local_source() {
        Some(path) => install::install_from_local(&path, name, false, false, None)?,
        None => {
            install::install_from_github(source, None, None, None, false, false, None)?;
        }
    }

    match discovery::extension_path(name) {
        Some(path) => Ok(Some(path)),
        None => bail!("installed '{name}' from {source} but could not find its executable"),
    }
}

fn dev_local_source() -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var_os(DEV_LOCAL_SOURCE_ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_party_source_known() {
        assert_eq!(first_party_source("setup"), Some("DataDog/pup-setup"));
    }

    #[test]
    fn test_first_party_source_unknown() {
        assert_eq!(first_party_source("not-a-first-party-ext"), None);
        assert_eq!(first_party_source(""), None);
        assert_eq!(first_party_source("Setup"), None);
    }

    #[test]
    fn test_decide_read_only_wins() {
        assert_eq!(decide(true, true, true), InstallDecision::RefuseReadOnly);
        assert_eq!(decide(true, false, false), InstallDecision::RefuseReadOnly);
    }

    #[test]
    fn test_decide_auto_approve_installs() {
        assert_eq!(decide(false, true, false), InstallDecision::Install);
        assert_eq!(decide(false, true, true), InstallDecision::Install);
    }

    #[test]
    fn test_decide_interactive_prompts() {
        assert_eq!(decide(false, false, true), InstallDecision::Prompt);
    }

    #[test]
    fn test_decide_non_interactive_without_approval_hints() {
        assert_eq!(decide(false, false, false), InstallDecision::Hint);
    }

    #[test]
    fn test_is_yes() {
        assert!(is_yes("y\n"));
        assert!(is_yes(" YES "));
        assert!(!is_yes("\n"));
        assert!(!is_yes("n"));
        assert!(!is_yes("yep"));
    }
}

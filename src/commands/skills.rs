use anyhow::{bail, Result};

use crate::skills;

/// Resolve the platform list from CLI input, validating each entry.
///
/// Validation runs in every mode — `--dir` may override the destination path
/// but does not let the caller pass a typo'd platform name silently. A
/// mistyped platform still has user-visible meaning (it's printed in success
/// messages, it controls extension-vs-skill routing, etc.), so we always
/// require it to be a recognized value.
fn resolve_or_bail(input: Option<&str>) -> Result<Vec<String>> {
    let auto_detected = input.map(|s| s.trim().is_empty()).unwrap_or(true);
    let platforms = skills::resolve_platform_list(input);
    if platforms.iter().any(|p| p.is_empty()) {
        bail!(
            "could not auto-detect AI assistant. Specify a platform: claude, \
             cursor, codex, opencode, windsurf, gemini, pi, devin, or `all`."
        );
    }
    for p in &platforms {
        if skills::lookup_platform(p).is_none() {
            if auto_detected {
                bail!(
                    "auto-detected '{p}' is not a supported platform. Specify \
                     one explicitly: claude, cursor, codex, opencode, windsurf, \
                     gemini, pi, devin, or `all`."
                );
            }
            bail!(
                "unknown platform: '{p}'. Supported: claude, cursor, codex, \
                 opencode, windsurf, gemini, pi, devin, or `all`."
            );
        }
    }
    Ok(platforms)
}

pub fn list(cfg: &crate::config::Config, entry_type: Option<String>) -> Result<()> {
    let entries: Vec<_> = skills::SKILLS
        .iter()
        .filter(|e| match &entry_type {
            Some(t) => e.entry_type == t.as_str(),
            None => true,
        })
        .collect();

    let items: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            let mut v = serde_json::json!({
                "name": e.name,
                "type": e.entry_type,
                "description": e.description,
            });
            if e.entry_type == "extension" {
                // serde_json::json!({}) always produces Value::Object, so as_object_mut()
                // is always Some here; the if-let is a safe defensive pattern.
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("platform".to_string(), serde_json::json!(e.platform));
                    obj.insert(
                        "files".to_string(),
                        serde_json::json!(e.files.iter().map(|(r, _)| *r).collect::<Vec<_>>()),
                    );
                }
            }
            v
        })
        .collect();

    crate::formatter::format_and_print(
        &items,
        &cfg.output_format,
        cfg.agent_mode,
        None,
        cfg.jq.as_deref(),
    )?;
    Ok(())
}

pub fn install(
    cfg: &crate::config::Config,
    platform: Option<String>,
    name: Option<String>,
    dir: Option<String>,
    entry_type: Option<String>,
    project: bool,
) -> Result<()> {
    let (project_root, _) = skills::project_root_or_cwd();
    let user_scope = !project;
    let platforms = resolve_or_bail(platform.as_deref())?;

    let mut entries: Vec<_> = skills::SKILLS
        .iter()
        .filter(|e| match &name {
            Some(n) => e.name == n.as_str(),
            None => true,
        })
        .filter(|e| match &entry_type {
            Some(t) => e.entry_type == t.as_str(),
            None => true,
        })
        .collect();

    if let Some(ref n) = name {
        if entries.is_empty() {
            bail!("skill not found: {n}");
        }
    }

    // When all platforms are selected, always include extension bundles for
    // extension-only platforms (e.g. dd-pup-pi for pi), even when a --name or
    // --type filter would otherwise exclude them. `all` means "full experience
    // on every platform", and pi-style platforms can only receive content via
    // their extension bundle, not individual skills. The --type filter is
    // intentionally bypassed for these entries; it applies to skill-capable
    // platforms only.
    let all_selected = platform
        .as_deref()
        .is_some_and(|p| p.trim().eq_ignore_ascii_case("all"));
    if all_selected {
        let already_included: std::collections::BTreeSet<&str> =
            entries.iter().map(|e| e.name).collect();
        for e in skills::SKILLS.iter() {
            if e.entry_type == "extension" && !already_included.contains(e.name) {
                entries.push(e);
            }
        }
        // No platform guard here: install_paths returns empty when entry.platform
        // doesn't match the current platform (unless --dir is set), so non-matching
        // extensions are naturally skipped. This also handles future extensions for
        // non-extension-only platforms without needing to update this loop.
    }

    let mut installed_files = 0usize;
    let mut dirs_used = std::collections::BTreeSet::new();
    let mut entry_hits = std::collections::BTreeSet::new();
    let mut platforms_hit = std::collections::BTreeSet::new();
    // Deduplicate write targets by path so that `--dir` installs across multiple
    // platforms don't write and count the same file more than once.
    let mut pending_writes: std::collections::BTreeMap<std::path::PathBuf, String> =
        std::collections::BTreeMap::new();
    for plat in &platforms {
        for entry in &entries {
            let targets =
                skills::install_paths(entry, plat, &project_root, dir.as_deref(), user_scope)?;
            if targets.is_empty() {
                continue;
            }
            for (path, content) in targets {
                use std::collections::btree_map::Entry;
                match pending_writes.entry(path) {
                    Entry::Vacant(v) => {
                        v.insert(content);
                    }
                    Entry::Occupied(o) if *o.get() != content => {
                        // Defensive guard: with install_paths enforcing the
                        // platform match for extensions before the --dir
                        // override, and with format_as_skill_md /
                        // format_as_agent_md currently producing identical
                        // output, this branch is unreachable today. It will
                        // fire if a future format diverges between platforms.
                        bail!(
                            "conflicting content for '{}': multiple platforms produced \
                             different output for the same --dir destination; use \
                             separate --dir values per platform",
                            o.key().display()
                        );
                    }
                    Entry::Occupied(_) => {} // identical content: dedup is correct
                }
            }
            // Record success only after all files for this entry are accepted
            // without conflict, so entry_hits/platforms_hit reflect what was
            // actually committed to pending_writes.
            entry_hits.insert(entry.name);
            platforms_hit.insert(plat.as_str());
        }
    }
    for (path, content) in &pending_writes {
        // install_paths always produces paths with at least one directory
        // component, so parent() is Some in practice. The if-let guards
        // against any future degenerate path without panicking.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            dirs_used.insert(parent.display().to_string());
        }
        std::fs::write(path, content)?;
        installed_files += 1;
    }

    // If the user filtered with --name or --type but nothing actually
    // installed across the selected platforms, the command succeeded by doing
    // nothing — surface that as an error so a typo doesn't silently no-op.
    if entry_hits.is_empty() && (name.is_some() || entry_type.is_some()) {
        let filter = match (&name, &entry_type) {
            (Some(n), Some(t)) => format!("name={n}, type={t}"),
            (Some(n), None) => format!("name={n}"),
            (None, Some(t)) => format!("type={t}"),
            (None, None) => unreachable!(),
        };

        // If the filter targets a skill/agent but all selected platforms are
        // extension-only (no skills dir), give an actionable hint.
        let ext_only: Vec<&str> = platforms
            .iter()
            .filter(|p| {
                // resolve_or_bail already validated every platform in `platforms`.
                skills::lookup_platform(p)
                    .expect("platform already validated by resolve_or_bail")
                    .is_extension_only()
            })
            .map(String::as_str)
            .collect();
        // Derive from the already-filtered `entries` list rather than rescanning
        // SKILLS — avoids contradictory messages when --name names an extension
        // while --type names a non-extension type simultaneously.
        let filter_is_skill_or_agent =
            !entries.is_empty() && entries.iter().all(|e| e.entry_type != "extension");
        if !ext_only.is_empty() && filter_is_skill_or_agent && ext_only.len() == platforms.len() {
            let available: Vec<&str> = skills::SKILLS
                .iter()
                .filter(|e| e.entry_type == "extension" && ext_only.contains(&e.platform))
                .map(|e| e.name)
                .collect();
            bail!(
                "no install target matched {filter} on the selected platform(s): {}. \
                 {} only support extensions, not skills or agents. \
                 Available extension(s): {}",
                platforms.join(", "),
                ext_only.join(", "),
                if available.is_empty() {
                    "(none)".to_string()
                } else {
                    available.join(", ")
                }
            );
        }

        bail!(
            "no install target matched {filter} on the selected platform(s): {}",
            platforms.join(", "),
        );
    }

    let installed_entries = entry_hits.len();
    if cfg.agent_mode {
        let directories: Vec<_> = dirs_used.into_iter().collect();
        let result = serde_json::json!({
            "installed": installed_entries,
            "files": installed_files,
            "directories": directories,
            "platforms": platforms_hit.iter().collect::<Vec<_>>(),
        });
        crate::formatter::format_and_print(
            &result,
            &cfg.output_format,
            cfg.agent_mode,
            None,
            cfg.jq.as_deref(),
        )?;
    } else {
        for d in &dirs_used {
            println!("  {d}");
        }
        println!(
            "Installed {} entry(ies), {} file(s) across {} platform(s)",
            installed_entries,
            installed_files,
            platforms_hit.len(),
        );
    }

    Ok(())
}

pub fn path(platform: Option<String>, project: bool) -> Result<()> {
    let (project_root, _) = skills::project_root_or_cwd();
    let user_scope = !project;
    let platforms = resolve_or_bail(platform.as_deref())?;
    let scope_label = if user_scope { "user" } else { "project" };

    for plat in &platforms {
        println!("platform: {plat} (scope: {scope_label})");
        let sd = skills::skills_dir(plat, &project_root, user_scope);
        if let Some(ref sd) = sd {
            println!("  skills:     {}", sd.display());
        }
        if let Some(ad) = skills::agents_dir(plat, &project_root, user_scope) {
            // Suppress redundant agents path when it matches skills (most
            // platforms share the dir; only Claude Code splits them).
            if sd.as_ref() != Some(&ad) {
                println!("  agents:     {}", ad.display());
            }
        }
        if let Some(ed) = skills::extensions_dir(plat, &project_root, user_scope) {
            println!("  extensions: {}", ed.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::test_support::TempDir;

    fn base_cfg() -> Config {
        Config {
            api_key: None,
            app_key: None,
            access_token: None,
            site: "datadoghq.com".to_string(),
            site_explicit: false,
            org: None,
            output_format: crate::config::OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        }
    }

    #[test]
    fn resolve_or_bail_normalizes_alias() {
        let p = resolve_or_bail(Some("claude")).unwrap();
        assert_eq!(p, vec!["claude-code".to_string()]);
    }

    #[test]
    fn resolve_or_bail_expands_all() {
        let p = resolve_or_bail(Some("all")).unwrap();
        // All known platforms — must include each canonical name.
        for expected in ["claude-code", "cursor", "codex", "opencode", "pi"] {
            assert!(p.iter().any(|x| x == expected), "missing {expected}");
        }
    }

    #[test]
    fn resolve_or_bail_rejects_unknown_platform() {
        let err = resolve_or_bail(Some("clood")).unwrap_err().to_string();
        assert!(err.contains("unknown platform"), "got: {err}");
        assert!(err.contains("clood"), "got: {err}");
    }

    #[test]
    fn resolve_or_bail_dir_override_still_validates() {
        // --dir does NOT exempt the platform name from validation — a typo
        // would otherwise silently write to the override dir with a
        // misleading "success" message.
        assert!(resolve_or_bail(Some("clood")).is_err());
    }

    #[test]
    fn install_dir_override_writes_named_skill() {
        let tmp = TempDir::new("install_dir_named");
        let cfg = base_cfg();
        install(
            &cfg,
            Some("claude".to_string()),
            Some("dd-pup".to_string()),
            Some(tmp.path().to_str().unwrap().to_string()),
            None,
            false,
        )
        .unwrap();
        let file = tmp.path().join("dd-pup").join("SKILL.md");
        assert!(file.exists(), "expected {} to exist", file.display());
        let body = std::fs::read_to_string(&file).unwrap();
        assert!(body.contains("name: dd-pup"));
    }

    #[test]
    fn install_bails_when_named_entry_does_not_apply_to_platform() {
        let cfg = base_cfg();
        // dd-pup-pi is a pi-only extension. Trying to install it on claude
        // (without --dir) must error rather than silently succeed.
        let err = install(
            &cfg,
            Some("claude".to_string()),
            Some("dd-pup-pi".to_string()),
            None,
            None,
            false,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("no install target"), "got: {err}");
        assert!(err.contains("dd-pup-pi"), "got: {err}");
    }

    #[test]
    fn install_pi_named_skill_succeeds() {
        // Regression test for https://github.com/DataDog/pup/issues/562 — pi
        // supports skills; installing dd-apm on pi must succeed.
        let tmp = TempDir::new("install_pi_skill");
        let cfg = base_cfg();
        install(
            &cfg,
            Some("pi".to_string()),
            Some("dd-apm".to_string()),
            Some(tmp.path().to_str().unwrap().to_string()),
            None,
            false,
        )
        .unwrap();
        assert!(
            tmp.path().join("dd-apm/SKILL.md").exists(),
            "dd-apm skill should be installed for pi"
        );
    }

    #[test]
    fn install_pi_type_skill_succeeds() {
        let tmp = TempDir::new("install_pi_type_skill");
        let cfg = base_cfg();
        install(
            &cfg,
            Some("pi".to_string()),
            None,
            Some(tmp.path().to_str().unwrap().to_string()),
            Some("skill".to_string()),
            false,
        )
        .unwrap();
        // At least one skill should have been written.
        let any_skill = std::fs::read_dir(tmp.path())
            .unwrap()
            .any(|e| e.unwrap().path().join("SKILL.md").exists());
        assert!(any_skill, "expected at least one SKILL.md installed for pi");
    }

    #[test]
    fn install_extension_on_wrong_platform_gives_generic_error_not_hint() {
        let cfg = base_cfg();
        // dd-pup-pi is a pi extension; installing it on claude must give the
        // generic error (not the extension-only hint, since claude is not
        // extension-only).
        let err = install(
            &cfg,
            Some("claude".to_string()),
            Some("dd-pup-pi".to_string()),
            None,
            None,
            false,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("no install target"), "got: {err}");
        assert!(!err.contains("only support extensions"), "got: {err}");
    }

    #[test]
    fn install_all_with_named_skill_also_installs_pi_extension() {
        let tmp = TempDir::new("install_all_pi");
        let cfg = base_cfg();
        // --name dd-apm would normally skip pi (it's a skill, not an extension),
        // but `all` means "full experience everywhere", so dd-pup-pi must also
        // be installed for pi.
        install(
            &cfg,
            Some("all".to_string()),
            Some("dd-apm".to_string()),
            Some(tmp.path().to_str().unwrap().to_string()),
            None,
            false,
        )
        .unwrap();
        assert!(
            tmp.path().join("dd-apm/SKILL.md").exists(),
            "dd-apm skill should be installed"
        );
        assert!(
            tmp.path().join("dd-pup-pi/index.ts").exists(),
            "dd-pup-pi extension should be installed when `all` is selected"
        );
        assert!(tmp.path().join("dd-pup-pi/package.json").exists());
        // With path dedup, --dir writes each unique destination exactly once:
        // dd-apm/SKILL.md + dd-pup-pi/index.ts + dd-pup-pi/package.json + dd-pup-pi/README.md
        let file_count = std::fs::read_dir(tmp.path())
            .unwrap()
            .flat_map(|d| {
                std::fs::read_dir(d.unwrap().path())
                    .unwrap()
                    .map(|f| f.unwrap().path())
            })
            .count();
        assert_eq!(
            file_count, 4,
            "expected 4 unique files (1 skill + 3 extension)"
        );
    }

    #[test]
    fn install_all_with_type_filter_also_installs_pi_extension() {
        let tmp = TempDir::new("install_all_type_pi");
        let cfg = base_cfg();
        // --type skill would normally skip pi, but `all` forces dd-pup-pi.
        install(
            &cfg,
            Some("all".to_string()),
            None,
            Some(tmp.path().to_str().unwrap().to_string()),
            Some("skill".to_string()),
            false,
        )
        .unwrap();
        assert!(
            tmp.path().join("dd-pup-pi/index.ts").exists(),
            "dd-pup-pi extension should be installed even with --type skill when `all` is used"
        );
    }

    #[test]
    fn install_bails_when_named_entry_does_not_exist() {
        let cfg = base_cfg();
        let err = install(
            &cfg,
            Some("claude".to_string()),
            Some("nonexistent-skill".to_string()),
            None,
            None,
            false,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("skill not found"), "got: {err}");
    }
}

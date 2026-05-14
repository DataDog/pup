use anyhow::{bail, Result};

use crate::skills;

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

    crate::formatter::format_and_print(&items, &cfg.output_format, cfg.agent_mode, None)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn install(
    cfg: &crate::config::Config,
    name: Option<String>,
    target_agent: Option<String>,
    dir: Option<String>,
    entry_type: Option<String>,
    platform: Option<String>,
    user_scope: bool,
) -> Result<()> {
    let (project_root, in_project) = skills::project_root_or_cwd();
    let agent = skills::resolve_agent(target_agent.as_deref());
    let platform_slug = skills::resolve_platform(platform.as_deref(), &agent);

    // Default scope for extensions:
    //   - explicit --user wins
    //   - else: project-local if a project root (.git) was found, else user-global
    let extensions_user_scope = user_scope || !in_project;

    let entries: Vec<_> = skills::SKILLS
        .iter()
        .filter(|e| match &name {
            Some(n) => e.name == n.as_str(),
            None => true,
        })
        .filter(|e| match &entry_type {
            Some(t) => e.entry_type == t.as_str(),
            None => true,
        })
        // When --platform is set, scope extensions to that platform.
        // Skills and agents are unaffected by --platform.
        .filter(|e| {
            if e.entry_type != "extension" {
                return true;
            }
            if platform_slug.is_empty() {
                // No platform context — only install extensions when the user
                // asked for them explicitly (by --type=extension or by name).
                return entry_type.as_deref() == Some("extension") || name.is_some();
            }
            e.platform == platform_slug
        })
        .collect();

    if let Some(ref n) = name {
        if entries.is_empty() {
            bail!("skill not found: {n}");
        }
    }

    let mut installed_files = 0usize;
    let mut installed_entries = 0usize;
    let mut dirs_used = std::collections::BTreeSet::new();
    for entry in &entries {
        let targets = skills::install_paths(
            entry,
            &agent,
            &platform_slug,
            &project_root,
            dir.as_deref(),
            extensions_user_scope,
        )?;
        installed_entries += 1;
        for (path, content) in targets {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
                dirs_used.insert(parent.display().to_string());
            }
            std::fs::write(&path, &content)?;
            installed_files += 1;
        }
    }

    if cfg.agent_mode {
        let directories: Vec<_> = dirs_used.into_iter().collect();
        let result = serde_json::json!({
            "installed": installed_entries,
            "files": installed_files,
            "directories": directories,
        });
        crate::formatter::format_and_print(&result, &cfg.output_format, cfg.agent_mode, None)?;
    } else {
        for d in &dirs_used {
            println!("  {d}");
        }
        println!(
            "Installed {} entry(ies), {} file(s)",
            installed_entries, installed_files
        );
    }

    Ok(())
}

pub fn path(
    target_agent: Option<String>,
    platform: Option<String>,
    user_scope: bool,
) -> Result<()> {
    let (project_root, in_project) = skills::project_root_or_cwd();
    let agent = skills::resolve_agent(target_agent.as_deref());
    let sd = skills::skills_dir(&agent, &project_root);
    let ad = skills::agents_dir(&agent, &project_root);
    if sd == ad {
        println!("skills:     {}", sd.display());
    } else {
        println!("skills:     {}", sd.display());
        println!("agents:     {}", ad.display());
    }

    let platform_slug = skills::resolve_platform(platform.as_deref(), &agent);
    if !platform_slug.is_empty() {
        let scope = user_scope || !in_project;
        if let Some(ed) = skills::extensions_dir(&platform_slug, &project_root, scope) {
            println!(
                "extensions: {}  (platform: {}, scope: {})",
                ed.display(),
                platform_slug,
                if scope { "user" } else { "project" },
            );
        }
    }
    Ok(())
}

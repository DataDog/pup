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
            serde_json::json!({
                "name": e.name,
                "type": e.entry_type,
                "description": e.description,
            })
        })
        .collect();

    crate::formatter::format_and_print(&items, &cfg.output_format, cfg.agent_mode, None)?;
    Ok(())
}

pub fn install(
    cfg: &crate::config::Config,
    name: Option<String>,
    target_agent: Option<String>,
    dir: Option<String>,
    entry_type: Option<String>,
) -> Result<()> {
    let project_root =
        skills::find_project_root().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let agent = skills::resolve_agent(target_agent.as_deref());

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
        .collect();

    if let Some(ref n) = name {
        if entries.is_empty() {
            bail!("skill not found: {n}");
        }
    }

    let mut installed = 0;
    let mut dirs_used = std::collections::BTreeSet::new();
    for entry in &entries {
        let (path, fmt) = skills::install_path(entry, &agent, &project_root, dir.as_deref());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            dirs_used.insert(parent.display().to_string());
        }
        let content = skills::format_content(entry, &fmt);
        std::fs::write(&path, &content)?;
        installed += 1;

        // Write any nested sub-skill files under the parent skill's directory.
        // Only applies when the parent installs as a skill directory
        // (`<skills_dir>/<name>/SKILL.md`), not as a single subagent .md file.
        if fmt == skills::InstallFormat::SkillMd {
            if let Some(parent_dir) = path.parent() {
                for sub in skills::SUB_SKILLS.iter().filter(|s| s.parent == entry.name) {
                    let sub_path = parent_dir.join(sub.rel_path);
                    if let Some(sub_parent) = sub_path.parent() {
                        std::fs::create_dir_all(sub_parent)?;
                    }
                    std::fs::write(&sub_path, sub.content)?;
                    installed += 1;
                }
            }
        }
    }

    if cfg.agent_mode {
        let directories: Vec<_> = dirs_used.into_iter().collect();
        let result = serde_json::json!({
            "installed": installed,
            "directories": directories,
        });
        crate::formatter::format_and_print(&result, &cfg.output_format, cfg.agent_mode, None)?;
    } else {
        for d in &dirs_used {
            println!("  {d}");
        }
        println!("Installed {} skill(s) and agent(s)", installed);
    }

    Ok(())
}

pub fn path(target_agent: Option<String>) -> Result<()> {
    let project_root =
        skills::find_project_root().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let agent = skills::resolve_agent(target_agent.as_deref());
    let sd = skills::skills_dir(&agent, &project_root);
    let ad = skills::agents_dir(&agent, &project_root);
    if sd == ad {
        println!("{}", sd.display());
    } else {
        println!("skills: {}", sd.display());
        println!("agents: {}", ad.display());
    }
    Ok(())
}

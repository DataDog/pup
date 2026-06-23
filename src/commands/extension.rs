use anyhow::{bail, Result};
use std::path::PathBuf;

use crate::config::Config;
use crate::extensions;

/// List all installed extensions.
pub fn list(cfg: &Config) -> Result<()> {
    let exts = extensions::discovery::list_extensions()?;
    if exts.is_empty() {
        match cfg.output_format {
            crate::config::OutputFormat::Table => {
                println!("No extensions installed.");
                println!();
                println!("Install one with: pup extension install <source>");
            }
            _ => {
                crate::formatter::format_and_print(
                    &Vec::<serde_json::Value>::new(),
                    &cfg.output_format,
                    cfg.agent_mode,
                    None,
                    cfg.jq.as_deref(),
                )?;
            }
        }
        return Ok(());
    }

    match cfg.output_format {
        crate::config::OutputFormat::Table => {
            for ext in &exts {
                let desc = if ext.description.is_empty() {
                    String::new()
                } else {
                    format!(" - {}", ext.description)
                };
                println!("{} v{}{}", ext.name, ext.version, desc);
            }
        }
        _ => {
            let items: Vec<serde_json::Value> = exts
                .iter()
                .map(|ext| {
                    serde_json::json!({
                        "name": ext.name,
                        "version": ext.version,
                        "source": ext.source,
                        "description": ext.description,
                        "installed_at": ext.installed_at,
                    })
                })
                .collect();
            crate::formatter::format_and_print(
                &items,
                &cfg.output_format,
                cfg.agent_mode,
                None,
                cfg.jq.as_deref(),
            )?;
        }
    }
    Ok(())
}

/// Options for installing an extension.
pub struct InstallOptions {
    pub source: String,
    pub extension: Option<String>,
    pub all: bool,
    pub tag: Option<String>,
    pub local: bool,
    pub link: bool,
    pub name: Option<String>,
    pub force: bool,
    pub description: Option<String>,
}

/// Install an extension from a source.
pub fn install(_cfg: &Config, opts: InstallOptions) -> Result<()> {
    let InstallOptions {
        source,
        extension,
        all,
        tag,
        local,
        link,
        name,
        force,
        description,
    } = opts;
    if local {
        if extension.is_some() || all {
            bail!("--extension and --all are only supported for GitHub installs");
        }
        let source_path = PathBuf::from(&source);
        // Derive name from filename if not provided.
        let ext_name = match name {
            Some(n) => n,
            None => {
                let file_name = source_path
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or("");
                // Strip pup- prefix and .exe suffix if present.
                let stripped = file_name.strip_prefix("pup-").unwrap_or(file_name);
                let stripped = stripped.strip_suffix(".exe").unwrap_or(stripped);
                if stripped.is_empty() {
                    bail!(
                        "could not derive extension name from '{}', use --name to specify it",
                        source
                    );
                }
                stripped.to_string()
            }
        };

        extensions::install::install_from_local(
            &source_path,
            &ext_name,
            link,
            force,
            description.as_deref(),
        )?;
        if link {
            println!("Linked extension '{ext_name}' from {source}");
        } else {
            println!("Installed extension '{ext_name}' from {source}");
        }
        return Ok(());
    }

    // GitHub-based installation: source is "owner/repo".
    let installed = extensions::install::install_from_github(
        &source,
        tag.as_deref(),
        name.as_deref(),
        extension.as_deref(),
        all,
        force,
        description.as_deref(),
    )?;

    if installed.len() == 1 {
        println!(
            "Installed extension '{}' from github:{source}",
            installed[0]
        );
    } else {
        println!(
            "Installed {} extensions from github:{source}: {}",
            installed.len(),
            installed.join(", ")
        );
    }

    Ok(())
}

/// List extensions available from a remote GitHub repository.
pub fn list_remote(cfg: &Config, source: String, extension: Option<String>) -> Result<()> {
    let items = extensions::install::list_remote_extensions(&source, extension.as_deref())?;
    match cfg.output_format {
        crate::config::OutputFormat::Table => {
            if items.is_empty() {
                println!("No remote extensions found.");
            } else {
                for item in &items {
                    println!("{} v{} ({})", item.name, item.version, item.tag);
                }
            }
        }
        _ => {
            let values: Vec<serde_json::Value> = items
                .iter()
                .map(|item| {
                    serde_json::json!({
                        "name": item.name,
                        "version": item.version,
                        "tag": item.tag,
                        "source": item.source,
                        "asset": item.asset,
                        "inferred_from_archive": item.inferred_from_archive,
                    })
                })
                .collect();
            crate::formatter::format_and_print(
                &values,
                &cfg.output_format,
                cfg.agent_mode,
                None,
                cfg.jq.as_deref(),
            )?;
        }
    }
    Ok(())
}

/// Remove an installed extension.
pub fn remove(_cfg: &Config, name: String) -> Result<()> {
    extensions::install::remove_extension(&name)?;
    println!("Removed extension '{name}'");
    Ok(())
}

/// Upgrade one or all installed extensions.
pub fn upgrade(_cfg: &Config, name: Option<String>, all: bool) -> Result<()> {
    if all {
        let results = extensions::install::upgrade_all_extensions()?;
        for msg in &results {
            println!("{msg}");
        }
        return Ok(());
    }

    match name {
        Some(n) => {
            let msg = extensions::install::upgrade_extension(&n)?;
            println!("{msg}");
        }
        None => {
            bail!(
                "specify an extension name to upgrade, or use --all to upgrade all extensions.\n\
                 Usage: pup extension upgrade <name>\n\
                 Usage: pup extension upgrade --all"
            );
        }
    }
    Ok(())
}

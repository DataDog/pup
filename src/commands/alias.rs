use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config;

fn aliases_path() -> Result<PathBuf> {
    let dir = config::config_dir().context("could not determine config directory")?;
    Ok(dir.join("aliases.yaml"))
}

fn load_aliases() -> Result<BTreeMap<String, String>> {
    let path = aliases_path()?;
    match std::fs::read_to_string(&path) {
        Ok(contents) => Ok(serde_norway::from_str(&contents).unwrap_or_default()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e.into()),
    }
}

fn save_aliases(aliases: &BTreeMap<String, String>) -> Result<()> {
    let path = aliases_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let yaml = serde_norway::to_string(aliases)?;
    std::fs::write(&path, yaml)?;
    Ok(())
}

pub fn list(cfg: &crate::config::Config) -> Result<()> {
    let aliases = load_aliases()?;
    match cfg.output_format {
        crate::config::OutputFormat::Table => {
            if aliases.is_empty() {
                println!("No aliases configured.");
            } else {
                for (name, command) in &aliases {
                    println!("{name} = {command}");
                }
            }
        }
        _ => {
            let items: Vec<serde_json::Value> = aliases
                .iter()
                .map(|(name, command)| serde_json::json!({"name": name, "command": command}))
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

pub fn set(name: String, command: String) -> Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    if crate::extensions::is_builtin_command(&name) {
        bail!("'{name}' is a built-in command and cannot be used as an alias name");
    }
    let mut aliases = load_aliases()?;
    aliases.insert(name.clone(), command.clone());
    save_aliases(&aliases)?;
    println!("Alias set: {name} = {command}");
    Ok(())
}

pub fn delete(names: Vec<String>) -> Result<()> {
    let mut aliases = load_aliases()?;
    for name in &names {
        if aliases.remove(name).is_none() {
            bail!("alias not found: {name}");
        }
    }
    save_aliases(&aliases)?;
    println!("Deleted {} alias(es).", names.len());
    Ok(())
}

/// Core expansion logic, operating on an already-loaded alias map.
/// Exposed separately so unit tests can exercise it without touching the
/// filesystem.
fn apply_expansion(args: &[String], name: &str, aliases: &BTreeMap<String, String>) -> Vec<String> {
    // Built-in commands cannot be shadowed by aliases.
    #[cfg(not(target_arch = "wasm32"))]
    if crate::extensions::is_builtin_command(name) {
        return args.to_vec();
    }
    let Some(expansion) = aliases.get(name) else {
        return args.to_vec();
    };
    // Use POSIX shell word splitting so quoted strings (e.g. --query "SELECT ...") are
    // kept as a single token rather than split on interior whitespace.
    let expansion_tokens = match shell_words::split(expansion) {
        Ok(tokens) => tokens,
        Err(_) => return args.to_vec(),
    };
    let Some(idx) = args.iter().position(|a| a == name) else {
        return args.to_vec();
    };
    let mut out = args[..idx].to_vec();
    out.extend(expansion_tokens);
    out.extend_from_slice(&args[idx + 1..]);
    out
}

/// Rewrite `args` by replacing the `name` token with its stored alias expansion.
/// Returns a clone of `args` unchanged if `name` is not a known alias or on
/// any I/O error.
pub fn expand(args: &[String], name: &str) -> Vec<String> {
    let aliases = match load_aliases() {
        Ok(m) => m,
        Err(_) => return args.to_vec(),
    };
    apply_expansion(args, name, &aliases)
}

pub fn import(file: &str) -> Result<()> {
    let contents = std::fs::read_to_string(file)
        .with_context(|| format!("failed to read alias file: {file}"))?;

    // Try YAML first, then JSON
    let imported: BTreeMap<String, String> = if file.ends_with(".json") {
        serde_json::from_str(&contents)
            .with_context(|| format!("failed to parse JSON from {file}"))?
    } else {
        serde_norway::from_str(&contents)
            .with_context(|| format!("failed to parse YAML from {file}"))?
    };

    if imported.is_empty() {
        bail!("no aliases found in {file}");
    }

    let mut aliases = load_aliases()?;
    let count = imported.len();
    for (name, command) in imported {
        aliases.insert(name, command);
    }
    save_aliases(&aliases)?;
    println!("Imported {count} alias(es) from {file}.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_apply_expansion_no_match() {
        let aliases = map(&[]);
        let result = apply_expansion(&args("pup infra-list"), "infra-list", &aliases);
        assert_eq!(result, args("pup infra-list"));
    }

    #[test]
    fn test_apply_expansion_simple() {
        let aliases = map(&[("infra-list", "infrastructure hosts list")]);
        let result = apply_expansion(&args("pup infra-list"), "infra-list", &aliases);
        assert_eq!(result, args("pup infrastructure hosts list"));
    }

    #[test]
    fn test_apply_expansion_preserves_trailing_args() {
        let aliases = map(&[("infra-list", "infrastructure hosts list")]);
        let result = apply_expansion(
            &args("pup infra-list --filter env:prod"),
            "infra-list",
            &aliases,
        );
        assert_eq!(
            result,
            args("pup infrastructure hosts list --filter env:prod")
        );
    }

    #[test]
    fn test_apply_expansion_preserves_leading_flags() {
        let aliases = map(&[("infra-list", "infrastructure hosts list")]);
        let result = apply_expansion(
            &args("pup --output json infra-list"),
            "infra-list",
            &aliases,
        );
        assert_eq!(result, args("pup --output json infrastructure hosts list"));
    }

    #[test]
    fn test_apply_expansion_preserves_quoted_spaces() {
        // A quoted SQL string must arrive at clap as a single token, not split
        // on the spaces inside the quotes.
        let aliases = map(&[(
            "host-count",
            r#"ddsql table --query "SELECT COUNT(*) FROM dd.hosts""#,
        )]);
        let result = apply_expansion(&args("pup host-count"), "host-count", &aliases);
        assert_eq!(
            result,
            vec![
                "pup",
                "ddsql",
                "table",
                "--query",
                "SELECT COUNT(*) FROM dd.hosts"
            ]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_apply_expansion_preserves_multiline_quoted_query() {
        // Newlines inside a double-quoted string must be preserved as part of
        // the single --query token (mirrors the YAML |- block scalar case).
        let aliases = map(&[(
            "host-count",
            "ddsql table --query \"SELECT COUNT(*)\nFROM dd.hosts\"",
        )]);
        let result = apply_expansion(&args("pup host-count"), "host-count", &aliases);
        assert_eq!(
            result,
            vec![
                "pup",
                "ddsql",
                "table",
                "--query",
                "SELECT COUNT(*)\nFROM dd.hosts",
            ]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_apply_expansion_unclosed_quote_returns_unchanged() {
        // A malformed alias value (unclosed quote) must not panic or produce
        // garbled args — fall back to the original args unchanged.
        let aliases = map(&[("bad-alias", "ddsql table --query \"unclosed")]);
        let result = apply_expansion(&args("pup bad-alias"), "bad-alias", &aliases);
        assert_eq!(result, args("pup bad-alias"));
    }

    #[test]
    fn test_set_rejects_builtin_name() {
        let result = super::set(
            "infrastructure".to_string(),
            "infrastructure hosts list".to_string(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("built-in command"));
    }
}

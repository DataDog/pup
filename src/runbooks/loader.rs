use anyhow::Result;
use std::path::{Path, PathBuf};

use super::{Runbook, RunbookMeta, Step, StepTemplate};
use crate::config::Config;

/// Returns the runbooks directory: ~/.config/pup/runbooks/
pub fn runbooks_dir(_cfg: &Config) -> Option<PathBuf> {
    crate::settings::config_dir().map(|d| d.join("runbooks"))
}

/// Returns the templates directory: ~/.config/pup/runbooks/_templates/
pub fn templates_dir(cfg: &Config) -> Option<PathBuf> {
    runbooks_dir(cfg).map(|d| d.join("_templates"))
}

/// Load a named step template from the _templates/ directory.
fn load_template(dir: &Path, name: &str) -> Result<StepTemplate> {
    for ext in &["yaml", "yml"] {
        let path = dir.join(format!("{name}.{ext}"));
        if path.exists() {
            let contents = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("failed to read template {:?}: {e}", path))?;
            return serde_norway::from_str(&contents)
                .map_err(|e| anyhow::anyhow!("failed to parse template '{name}': {e}"));
        }
    }
    anyhow::bail!("step template '{name}' not found in {}", dir.display())
}

/// Apply a template to a step: template fields fill in any fields the step left unset.
fn apply_template(mut step: Step, tmpl: &StepTemplate) -> Result<Step> {
    if step.kind.is_empty() {
        step.kind = tmpl.kind.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "step '{}' has no 'kind' and template provides none",
                step.name
            )
        })?;
    }
    macro_rules! fill {
        ($field:ident) => {
            if step.$field.is_none() {
                step.$field = tmpl.$field.clone();
            }
        };
    }
    fill!(run);
    fill!(workflow_id);
    fill!(inputs);
    fill!(url);
    fill!(method);
    fill!(body);
    fill!(content_type);
    fill!(accept);
    fill!(body_file);
    fill!(output_file);
    fill!(message);
    fill!(on_failure);
    fill!(when);
    fill!(optional);
    fill!(capture);
    fill!(poll);
    fill!(assert);
    // Merge headers: template headers are the base; step headers override per-key.
    step.headers = match (step.headers, tmpl.headers.clone()) {
        (Some(mut sh), Some(th)) => {
            for (k, v) in th {
                sh.entry(k).or_insert(v);
            }
            Some(sh)
        }
        (sh, th) => sh.or(th),
    };
    Ok(step)
}

/// Resolve template references in a runbook's steps.
fn resolve_steps(steps: Vec<Step>, tmpl_dir: Option<&Path>) -> Result<Vec<Step>> {
    steps
        .into_iter()
        .map(|step| {
            let tmpl_name = step.template.clone();
            match tmpl_name {
                None => Ok(step),
                Some(ref name) => {
                    let dir = tmpl_dir.ok_or_else(|| {
                        anyhow::anyhow!(
                            "step '{}' references template '{name}' but no templates directory found",
                            step.name
                        )
                    })?;
                    let tmpl = load_template(dir, name)?;
                    apply_template(step, &tmpl)
                }
            }
        })
        .collect()
}

/// List all runbooks, optionally filtered by tags (format: "key:value").
pub fn list_runbooks(cfg: &Config, tags: &[String]) -> Result<Vec<RunbookMeta>> {
    let dir = match runbooks_dir(cfg) {
        Some(d) => d,
        None => return Ok(vec![]),
    };
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut results = vec![];
    let mut entries: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let path = e.path();
            // Skip the _templates/ subdirectory and any file starting with '_'
            if path.is_dir() {
                return false;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if stem.starts_with('_') {
                return false;
            }
            path.extension()
                .and_then(|x| x.to_str())
                .map(|x| x == "yaml" || x == "yml")
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("failed to read {:?}: {e}", path))?;
        let runbook: Runbook = serde_norway::from_str(&contents)
            .map_err(|e| anyhow::anyhow!("failed to parse {:?}: {e}", path))?;

        if !tags.is_empty() {
            let runbook_tags = runbook.tags.clone().unwrap_or_default();
            let matches = tags.iter().all(|tag| {
                let mut parts = tag.splitn(2, ':');
                let key = parts.next().unwrap_or("");
                if let Some(val) = parts.next() {
                    runbook_tags.get(key).map(|v| v == val).unwrap_or(false)
                } else {
                    runbook_tags.contains_key(key)
                }
            });
            if !matches {
                continue;
            }
        }

        results.push(RunbookMeta {
            name: runbook.name,
            description: runbook.description,
            tags: runbook.tags.unwrap_or_default(),
            steps: runbook.steps.len(),
        });
    }

    Ok(results)
}

/// Load a single runbook by name from the runbooks directory, resolving template steps.
pub fn load_runbook(cfg: &Config, name: &str) -> Result<Runbook> {
    let dir = runbooks_dir(cfg)
        .ok_or_else(|| anyhow::anyhow!("could not determine runbooks directory"))?;

    // Try <name>.yaml then <name>.yml
    for ext in &["yaml", "yml"] {
        let path = dir.join(format!("{name}.{ext}"));
        if path.exists() {
            let contents = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("failed to read {:?}: {e}", path))?;
            let mut runbook: Runbook = serde_norway::from_str(&contents)
                .map_err(|e| anyhow::anyhow!("failed to parse runbook '{name}': {e}"))?;
            let tmpl_dir = templates_dir(cfg);
            runbook.steps = resolve_steps(runbook.steps, tmpl_dir.as_deref())?;
            return Ok(runbook);
        }
    }

    anyhow::bail!(
        "runbook '{}' not found in {:?} (expected {}.yaml)",
        name,
        dir,
        name
    )
}

/// Import a runbook from a file path or HTTP(S) URL.
pub async fn import_runbook(cfg: &Config, source: &str) -> Result<()> {
    let dir = runbooks_dir(cfg)
        .ok_or_else(|| anyhow::anyhow!("could not determine runbooks directory"))?;
    std::fs::create_dir_all(&dir)?;

    if source.starts_with("http://") || source.starts_with("https://") {
        let client = reqwest::Client::new();
        let body = client
            .get(source)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("failed to fetch {source}: {e}"))?
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("failed to read response body: {e}"))?;

        let filename = source
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("imported");
        let filename = if filename.ends_with(".yaml") || filename.ends_with(".yml") {
            filename.to_string()
        } else {
            format!("{filename}.yaml")
        };
        let dest = dir.join(&filename);
        std::fs::write(&dest, &body)
            .map_err(|e| anyhow::anyhow!("failed to write {:?}: {e}", dest))?;
        println!("Imported runbook to {}", dest.display());
    } else {
        let src_path = std::path::Path::new(source);
        if !src_path.exists() {
            anyhow::bail!("source file not found: {}", source);
        }
        let filename = src_path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("invalid source path: {}", source))?;
        let dest = dir.join(filename);
        std::fs::copy(src_path, &dest)
            .map_err(|e| anyhow::anyhow!("failed to copy to {:?}: {e}", dest))?;
        println!("Imported runbook to {}", dest.display());
    }

    Ok(())
}

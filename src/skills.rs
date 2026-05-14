use std::path::{Path, PathBuf};

pub struct SkillEntry {
    pub name: &'static str,
    pub description: &'static str,
    /// One of: "skill", "agent", "extension".
    /// - skill / agent: single-file markdown installed under a skills/ dir
    /// - extension: multi-file bundle for an AI coding agent platform (e.g. pi)
    pub entry_type: &'static str,
    /// SKILL.md / agent.md body, or empty for entry_type == "extension".
    pub content: &'static str,
    /// Platform slug for entry_type == "extension". One of: "pi".
    /// Empty for skills and agents.
    pub platform: &'static str,
    /// Files to materialize for entry_type == "extension".
    /// Each tuple is `(relative_path_within_extension_dir, file_contents)`.
    /// Empty for skills and agents.
    pub files: &'static [(&'static str, &'static str)],
}

/// Files for the `dd-pup-pi` extension bundle (pi coding agent).
static DD_PUP_PI_FILES: &[(&str, &str)] = &[
    (
        "index.ts",
        include_str!("../skills/extensions/dd-pup-pi/index.ts"),
    ),
    (
        "package.json",
        include_str!("../skills/extensions/dd-pup-pi/package.json"),
    ),
    (
        "README.md",
        include_str!("../skills/extensions/dd-pup-pi/README.md"),
    ),
];

pub static SKILLS: &[SkillEntry] = &[
    // --- Skills (from agent-skills + claude-plugin) ---
    SkillEntry {
        name: "dd-pup",
        description: "Datadog CLI (pup). OAuth2 auth with token refresh.",
        entry_type: "skill",
        content: include_str!("../skills/dd-pup/SKILL.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "dd-monitors",
        description: "Monitor management - create, update, mute, and alerting best practices.",
        entry_type: "skill",
        content: include_str!("../skills/dd-monitors/SKILL.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "dd-logs",
        description: "Log management - search, pipelines, archives, and cost control.",
        entry_type: "skill",
        content: include_str!("../skills/dd-logs/SKILL.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "dd-apm",
        description: "APM - traces, services, dependencies, performance analysis.",
        entry_type: "skill",
        content: include_str!("../skills/dd-apm/SKILL.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "dd-debugger",
        description: "Live Debugger - create, delete, and watch log probes and events.",
        entry_type: "skill",
        content: include_str!("../skills/dd-debugger/SKILL.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "dd-docs",
        description: "Datadog docs lookup using docs.datadoghq.com/llms.txt.",
        entry_type: "skill",
        content: include_str!("../skills/dd-docs/SKILL.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "dd-code-generation",
        description: "Use pup CLI or generate code (TypeScript, Python, Java, Go, Rust).",
        entry_type: "skill",
        content: include_str!("../skills/dd-code-generation/SKILL.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "dd-file-issue",
        description: "File GitHub issues to the right repository (pup CLI or plugin).",
        entry_type: "skill",
        content: include_str!("../skills/dd-file-issue/SKILL.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "dd-symdb",
        description: "Symbol Database - search service symbols, find probe-able methods.",
        entry_type: "skill",
        content: include_str!("../skills/dd-symdb/SKILL.md"),
        platform: "",
        files: &[],
    },
    // --- Domain Agents (from datadog-api-claude-plugin) ---
    SkillEntry {
        name: "agentless-scanning",
        description: "Manage Datadog Agentless Scanning for AWS and Azure resources.",
        entry_type: "agent",
        content: include_str!("../agents/agentless-scanning.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "api-management",
        description: "Manage API keys and Application keys for authentication.",
        entry_type: "agent",
        content: include_str!("../agents/api-management.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "apm-configuration",
        description: "Manage APM retention filters and span-based metrics.",
        entry_type: "agent",
        content: include_str!("../agents/apm-configuration.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "app-builder",
        description: "Manage App Builder applications (low-code internal tools).",
        entry_type: "agent",
        content: include_str!("../agents/app-builder.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "application-security",
        description: "Manage ASM including WAF rules, threat detection, API protection.",
        entry_type: "agent",
        content: include_str!("../agents/application-security.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "audience-management",
        description: "Query and segment RUM users and accounts.",
        entry_type: "agent",
        content: include_str!("../agents/audience-management.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "audit-logs",
        description: "Query and manage Audit Trail events for compliance.",
        entry_type: "agent",
        content: include_str!("../agents/audit-logs.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "aws-integration",
        description: "Configure AWS integration for monitoring and log collection.",
        entry_type: "agent",
        content: include_str!("../agents/aws-integration.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "azure-integration",
        description: "Configure Azure integration for monitoring and resources.",
        entry_type: "agent",
        content: include_str!("../agents/azure-integration.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "cicd",
        description: "Manage CI/CD Visibility including tests, pipelines, DORA metrics.",
        entry_type: "agent",
        content: include_str!("../agents/cicd.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "cloud-cost",
        description: "Manage Cloud Cost Management including multi-cloud config.",
        entry_type: "agent",
        content: include_str!("../agents/cloud-cost.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "cloud-workload-security",
        description: "Manage CSM Threats and Workload Protection agent rules.",
        entry_type: "agent",
        content: include_str!("../agents/cloud-workload-security.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "container-monitoring",
        description: "Monitor Kubernetes and containerized environments.",
        entry_type: "agent",
        content: include_str!("../agents/container-monitoring.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "dashboards",
        description: "Manage dashboards including CRUD and widgets.",
        entry_type: "agent",
        content: include_str!("../agents/dashboards.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "data-deletion",
        description: "GDPR/data privacy compliance through targeted deletion.",
        entry_type: "agent",
        content: include_str!("../agents/data-deletion.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "data-governance",
        description: "Access control, data enrichment, data protection.",
        entry_type: "agent",
        content: include_str!("../agents/data-governance.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "database-monitoring",
        description: "Query and manage DBM data and monitors.",
        entry_type: "agent",
        content: include_str!("../agents/database-monitoring.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "error-tracking",
        description: "Manage error tracking issues, triage, and assignment.",
        entry_type: "agent",
        content: include_str!("../agents/error-tracking.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "events",
        description: "Manage events including submission, search, filtering.",
        entry_type: "agent",
        content: include_str!("../agents/events.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "fleet-automation",
        description: "Manage Agent fleet, deployments, upgrades, schedules.",
        entry_type: "agent",
        content: include_str!("../agents/fleet-automation.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "gcp-integration",
        description: "Configure GCP integration for monitoring and resources.",
        entry_type: "agent",
        content: include_str!("../agents/gcp-integration.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "incident-response",
        description: "Manage incident lifecycle, teams, and response.",
        entry_type: "agent",
        content: include_str!("../agents/incident-response.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "infrastructure",
        description: "Query infrastructure hosts, counts, and metadata.",
        entry_type: "agent",
        content: include_str!("../agents/infrastructure.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "log-configuration",
        description: "Manage log archives, pipelines, indexes, custom destinations.",
        entry_type: "agent",
        content: include_str!("../agents/log-configuration.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "logs",
        description: "Search and analyze log data with flexible queries.",
        entry_type: "agent",
        content: include_str!("../agents/logs.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "metrics",
        description: "Query, list, and manage metrics.",
        entry_type: "agent",
        content: include_str!("../agents/metrics.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "monitoring-alerting",
        description: "Full monitor management, downtimes, and templates.",
        entry_type: "agent",
        content: include_str!("../agents/monitoring-alerting.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "network-performance",
        description: "Network Performance Monitoring and DNS monitoring.",
        entry_type: "agent",
        content: include_str!("../agents/network-performance.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "notebooks",
        description: "Manage investigation notebooks.",
        entry_type: "agent",
        content: include_str!("../agents/notebooks.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "observability-pipelines",
        description: "Manage Observability Pipelines for data routing.",
        entry_type: "agent",
        content: include_str!("../agents/observability-pipelines.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "organization-management",
        description: "Manage organization settings, teams, and users.",
        entry_type: "agent",
        content: include_str!("../agents/organization-management.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "powerpacks",
        description: "Manage reusable dashboard widget groups.",
        entry_type: "agent",
        content: include_str!("../agents/powerpacks.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "rum-metrics-retention",
        description: "Manage RUM metrics and retention filters.",
        entry_type: "agent",
        content: include_str!("../agents/rum-metrics-retention.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "rum",
        description: "Query Real User Monitoring data.",
        entry_type: "agent",
        content: include_str!("../agents/rum.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "saml-configuration",
        description: "Manage SAML SSO configuration.",
        entry_type: "agent",
        content: include_str!("../agents/saml-configuration.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "scorecards",
        description: "Manage service quality scorecards.",
        entry_type: "agent",
        content: include_str!("../agents/scorecards.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "security-posture-management",
        description: "Manage CSPM findings and compliance.",
        entry_type: "agent",
        content: include_str!("../agents/security-posture-management.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "security",
        description: "Security monitoring signals and rules.",
        entry_type: "agent",
        content: include_str!("../agents/security.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "service-catalog",
        description: "Manage service registry and metadata.",
        entry_type: "agent",
        content: include_str!("../agents/service-catalog.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "slos",
        description: "Manage Service Level Objectives.",
        entry_type: "agent",
        content: include_str!("../agents/slos.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "spark-pod-autosizing",
        description: "Manage Spark pod autosizing for Kubernetes.",
        entry_type: "agent",
        content: include_str!("../agents/spark-pod-autosizing.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "static-analysis",
        description: "Manage static code analysis.",
        entry_type: "agent",
        content: include_str!("../agents/static-analysis.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "synthetics",
        description: "Manage synthetic monitoring tests.",
        entry_type: "agent",
        content: include_str!("../agents/synthetics.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "third-party-integrations",
        description: "Manage third-party integrations (PagerDuty, Slack, etc.).",
        entry_type: "agent",
        content: include_str!("../agents/third-party-integrations.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "traces",
        description: "Query APM traces and spans.",
        entry_type: "agent",
        content: include_str!("../agents/traces.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "usage-metering",
        description: "Track Datadog usage and billing.",
        entry_type: "agent",
        content: include_str!("../agents/usage-metering.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "user-access-management",
        description: "Manage users, roles, teams, and permissions.",
        entry_type: "agent",
        content: include_str!("../agents/user-access-management.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "workflows",
        description: "Manage workflow automations.",
        entry_type: "agent",
        content: include_str!("../agents/workflows.md"),
        platform: "",
        files: &[],
    },
    // --- Extensions (multi-file bundles for AI coding agent platforms) ---
    SkillEntry {
        name: "dd-pup-pi",
        description: "pi coding agent extension: exposes pup as LLM tools (logs, metrics, traces, monitors, ...).",
        entry_type: "extension",
        content: "",
        platform: "pi",
        files: DD_PUP_PI_FILES,
    },
];

/// Resolve the detected agent name, applying override if provided.
pub fn resolve_agent(agent: Option<&str>) -> String {
    agent
        .map(String::from)
        .unwrap_or_else(|| crate::useragent::detect_agent_info().name)
}

/// Resolve the extension platform slug.
///
/// Precedence:
///   1. explicit `--platform` flag value
///   2. mapping from the detected/resolved agent name (e.g. `pi-dev` -> `pi`)
///   3. empty string (caller must decide how to handle)
///
/// Supported platforms today: `pi`.
pub fn resolve_platform(platform: Option<&str>, agent: &str) -> String {
    if let Some(p) = platform {
        if !p.is_empty() {
            return p.to_string();
        }
    }
    match agent {
        "pi-dev" | "pi" => "pi".to_string(),
        _ => String::new(),
    }
}

/// Determine the install directory for an extension on a given platform.
///
/// When `user_scope` is true, returns the per-user global location
/// (`~/.pi/agent/extensions` for pi). Otherwise returns the project-local
/// location (`<project_root>/.pi/extensions` for pi).
///
/// Returns `None` for unsupported platforms, or in user-scope mode when the
/// home directory cannot be resolved (HOME/USERPROFILE unset). The caller is
/// expected to surface this as an error — see [`install_paths`].
pub fn extensions_dir(platform: &str, project_root: &Path, user_scope: bool) -> Option<PathBuf> {
    extensions_dir_with_home(
        platform,
        dirs::home_dir().as_deref(),
        project_root,
        user_scope,
    )
}

/// Same as [`extensions_dir`] but takes an explicit `home` directory, so tests
/// don't have to mutate the process-global `HOME` env var.
pub fn extensions_dir_with_home(
    platform: &str,
    home: Option<&Path>,
    project_root: &Path,
    user_scope: bool,
) -> Option<PathBuf> {
    match platform {
        "pi" => {
            if user_scope {
                Some(home?.join(".pi").join("agent").join("extensions"))
            } else {
                Some(project_root.join(".pi").join("extensions"))
            }
        }
        _ => None,
    }
}

/// Determine the skills install directory for the given agent.
/// If an existing skills directory is found, use it regardless of detected agent.
pub fn skills_dir(agent: &str, project_root: &Path) -> PathBuf {
    let existing_dirs = [
        ".agents/skills",
        ".claude/skills",
        ".cursor/skills",
        ".windsurf/skills",
        ".gemini/skills",
    ];
    for dir in &existing_dirs {
        let path = project_root.join(dir);
        if path.is_dir() {
            return path;
        }
    }

    match agent {
        "claude-code" => project_root.join(".claude/skills"),
        "codex" | "opencode" => project_root.join(".agents/skills"),
        "cursor" => project_root.join(".cursor/skills"),
        "windsurf" => project_root.join(".windsurf/skills"),
        "gemini-code" => project_root.join(".gemini/skills"),
        _ => project_root.join(".agents/skills"),
    }
}

/// Determine the agents (subagents) install directory for the given agent.
/// Claude Code uses `.claude/agents/`; other tools use their skills directory.
pub fn agents_dir(agent: &str, project_root: &Path) -> PathBuf {
    match agent {
        "claude-code" => project_root.join(".claude/agents"),
        _ => skills_dir(agent, project_root),
    }
}

/// Determine the install path for a single (single-file) entry.
///
/// Skills always go to `<skills_dir>/<name>/SKILL.md`.
/// Agents go to `<agents_dir>/<name>.md` for Claude Code (subagent format),
/// or `<skills_dir>/<name>/SKILL.md` for other tools.
///
/// Panics if called for an `extension` entry; use [`install_paths`] for those.
pub fn install_path(
    entry: &SkillEntry,
    agent: &str,
    project_root: &Path,
    dir_override: Option<&str>,
) -> (PathBuf, InstallFormat) {
    debug_assert_ne!(
        entry.entry_type, "extension",
        "install_path() does not handle extensions; use install_paths()"
    );

    if let Some(d) = dir_override {
        // Explicit --dir: everything as SKILL.md
        return (
            PathBuf::from(d).join(entry.name).join("SKILL.md"),
            InstallFormat::SkillMd,
        );
    }

    if entry.entry_type == "agent" && agent == "claude-code" {
        // Claude Code subagent: .claude/agents/<name>.md
        let dir = agents_dir(agent, project_root);
        (
            dir.join(format!("{}.md", entry.name)),
            InstallFormat::AgentMd,
        )
    } else {
        // Everything else: <skills_dir>/<name>/SKILL.md
        let dir = skills_dir(agent, project_root);
        (
            dir.join(entry.name).join("SKILL.md"),
            InstallFormat::SkillMd,
        )
    }
}

/// Resolve install destinations for any entry, including multi-file extensions.
///
/// Returns a list of `(absolute_path, contents)` tuples. For skills and agents
/// this is always a single-element list using [`install_path`] + [`format_content`].
/// For extensions this expands to one entry per bundled file.
pub fn install_paths(
    entry: &SkillEntry,
    agent: &str,
    platform: &str,
    project_root: &Path,
    dir_override: Option<&str>,
    user_scope: bool,
) -> anyhow::Result<Vec<(PathBuf, String)>> {
    if entry.entry_type != "extension" {
        let (path, fmt) = install_path(entry, agent, project_root, dir_override);
        return Ok(vec![(path, format_content(entry, &fmt))]);
    }

    // entry_type == "extension": materialize each bundled file under the
    // platform-appropriate extension directory.
    let base = if let Some(d) = dir_override {
        PathBuf::from(d).join(entry.name)
    } else {
        let plat = if platform.is_empty() {
            entry.platform
        } else {
            platform
        };
        let root = extensions_dir(plat, project_root, user_scope).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown or unsupported extension platform: '{}' (entry: {})",
                plat,
                entry.name,
            )
        })?;
        root.join(entry.name)
    };

    Ok(entry
        .files
        .iter()
        .map(|(rel, body)| (base.join(rel), (*body).to_string()))
        .collect())
}

#[derive(Debug, PartialEq)]
pub enum InstallFormat {
    SkillMd,
    AgentMd,
}

/// Format content for SKILL.md install (adds name: to frontmatter if missing).
pub fn format_as_skill_md(entry: &SkillEntry) -> String {
    if entry.content.starts_with("---") {
        let end = entry.content[3..].find("---");
        if let Some(pos) = end {
            let frontmatter = &entry.content[3..3 + pos];
            if frontmatter.contains("name:") {
                return entry.content.to_string();
            }
            return format!(
                "---\nname: {}\n{}---{}",
                entry.name,
                frontmatter,
                &entry.content[3 + pos + 3..]
            );
        }
    }
    format!(
        "---\nname: {}\ndescription: {}\n---\n\n{}",
        entry.name, entry.description, entry.content
    )
}

/// Format content for Claude Code agent .md install (adds name: to frontmatter).
pub fn format_as_agent_md(entry: &SkillEntry) -> String {
    if entry.content.starts_with("---") {
        let end = entry.content[3..].find("---");
        if let Some(pos) = end {
            let frontmatter = &entry.content[3..3 + pos];
            if frontmatter.contains("name:") {
                return entry.content.to_string();
            }
            return format!(
                "---\nname: {}\n{}---{}",
                entry.name,
                frontmatter,
                &entry.content[3 + pos + 3..]
            );
        }
    }
    format!(
        "---\nname: {}\ndescription: {}\n---\n\n{}",
        entry.name, entry.description, entry.content
    )
}

/// Format content for the given install format.
pub fn format_content(entry: &SkillEntry, format: &InstallFormat) -> String {
    match format {
        InstallFormat::SkillMd => format_as_skill_md(entry),
        InstallFormat::AgentMd => format_as_agent_md(entry),
    }
}

/// Find the project root (nearest ancestor containing `.git`) and fall back to
/// the current working directory if none is found. Returns `(root, found)`
/// where `found` is true iff a project root was actually located — callers can
/// use that to decide between project-local and user-global defaults without
/// re-walking the tree.
pub fn project_root_or_cwd() -> (PathBuf, bool) {
    match find_project_root() {
        Some(p) => (p, true),
        None => (std::env::current_dir().unwrap_or_default(), false),
    }
}

/// Find the project root by walking up from cwd looking for .git.
pub fn find_project_root() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_entries_have_valid_names() {
        for entry in SKILLS {
            assert!(!entry.name.is_empty(), "empty name found");
            assert!(entry.name.len() <= 64, "name too long: {}", entry.name);
            assert!(
                entry
                    .name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "invalid chars in name: {}",
                entry.name
            );
        }
    }

    #[test]
    fn test_all_entries_have_descriptions() {
        for entry in SKILLS {
            assert!(
                !entry.description.is_empty(),
                "empty description for {}",
                entry.name
            );
        }
    }

    #[test]
    fn test_all_entries_have_valid_type() {
        for entry in SKILLS {
            assert!(
                matches!(entry.entry_type, "skill" | "agent" | "extension"),
                "invalid type '{}' for {}",
                entry.entry_type,
                entry.name
            );
        }
    }

    #[test]
    fn test_all_entries_have_content_or_files() {
        for entry in SKILLS {
            if entry.entry_type == "extension" {
                assert!(
                    !entry.files.is_empty(),
                    "extension {} has no files",
                    entry.name
                );
                assert!(
                    !entry.platform.is_empty(),
                    "extension {} has empty platform",
                    entry.name
                );
                for (rel, body) in entry.files {
                    assert!(!rel.is_empty(), "empty file path in {}", entry.name);
                    assert!(
                        !body.is_empty(),
                        "empty file body for {}:{}",
                        entry.name,
                        rel
                    );
                }
            } else {
                assert!(
                    !entry.content.is_empty(),
                    "empty content for {}",
                    entry.name
                );
            }
        }
    }

    #[test]
    fn test_skill_count() {
        let skills: Vec<_> = SKILLS.iter().filter(|e| e.entry_type == "skill").collect();
        assert_eq!(skills.len(), 9, "expected 9 skills");
    }

    #[test]
    fn test_agent_count() {
        let agents: Vec<_> = SKILLS.iter().filter(|e| e.entry_type == "agent").collect();
        assert!(
            agents.len() >= 46,
            "expected at least 46 agents, got {}",
            agents.len()
        );
    }

    #[test]
    fn test_no_duplicate_names() {
        let mut names: Vec<&str> = SKILLS.iter().map(|e| e.name).collect();
        names.sort();
        for w in names.windows(2) {
            assert_ne!(w[0], w[1], "duplicate name: {}", w[0]);
        }
    }

    #[test]
    fn test_skills_dir_claude_code() {
        let root = PathBuf::from("/tmp/test-project");
        assert_eq!(
            skills_dir("claude-code", &root),
            root.join(".claude/skills")
        );
    }

    #[test]
    fn test_skills_dir_cursor() {
        let root = PathBuf::from("/tmp/test-project");
        assert_eq!(skills_dir("cursor", &root), root.join(".cursor/skills"));
    }

    #[test]
    fn test_skills_dir_codex() {
        let root = PathBuf::from("/tmp/test-project");
        assert_eq!(skills_dir("codex", &root), root.join(".agents/skills"));
    }

    #[test]
    fn test_skills_dir_windsurf() {
        let root = PathBuf::from("/tmp/test-project");
        assert_eq!(skills_dir("windsurf", &root), root.join(".windsurf/skills"));
    }

    #[test]
    fn test_skills_dir_unknown_defaults() {
        let root = PathBuf::from("/tmp/test-project");
        assert_eq!(
            skills_dir("unknown-agent", &root),
            root.join(".agents/skills")
        );
    }

    #[test]
    fn test_skills_dir_respects_existing() {
        let tmp = std::env::temp_dir().join("pup-test-existing-skills");
        let existing = tmp.join(".cursor/skills");
        std::fs::create_dir_all(&existing).unwrap();
        assert_eq!(skills_dir("claude-code", &tmp), existing);
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn test_agents_dir_claude_code() {
        let root = PathBuf::from("/tmp/test-project");
        assert_eq!(
            agents_dir("claude-code", &root),
            root.join(".claude/agents")
        );
    }

    #[test]
    fn test_agents_dir_cursor_falls_back() {
        let root = PathBuf::from("/tmp/test-project");
        assert_eq!(agents_dir("cursor", &root), root.join(".cursor/skills"));
    }

    fn entry(name: &'static str, entry_type: &'static str, content: &'static str) -> SkillEntry {
        SkillEntry {
            name,
            description: "test",
            entry_type,
            content,
            platform: "",
            files: &[],
        }
    }

    #[test]
    fn test_install_path_skill_claude_code() {
        let root = PathBuf::from("/tmp/test-project");
        let e = entry("dd-pup", "skill", "");
        let (path, fmt) = install_path(&e, "claude-code", &root, None);
        assert_eq!(path, root.join(".claude/skills/dd-pup/SKILL.md"));
        assert_eq!(fmt, InstallFormat::SkillMd);
    }

    #[test]
    fn test_install_path_agent_claude_code() {
        let root = PathBuf::from("/tmp/test-project");
        let e = entry("logs", "agent", "");
        let (path, fmt) = install_path(&e, "claude-code", &root, None);
        assert_eq!(path, root.join(".claude/agents/logs.md"));
        assert_eq!(fmt, InstallFormat::AgentMd);
    }

    #[test]
    fn test_install_path_agent_cursor_as_skill() {
        let root = PathBuf::from("/tmp/test-project");
        let e = entry("logs", "agent", "");
        let (path, fmt) = install_path(&e, "cursor", &root, None);
        assert_eq!(path, root.join(".cursor/skills/logs/SKILL.md"));
        assert_eq!(fmt, InstallFormat::SkillMd);
    }

    #[test]
    fn test_install_path_dir_override() {
        let root = PathBuf::from("/tmp/test-project");
        let e = entry("logs", "agent", "");
        let (path, fmt) = install_path(&e, "claude-code", &root, Some("/tmp/out"));
        assert_eq!(path, PathBuf::from("/tmp/out/logs/SKILL.md"));
        assert_eq!(fmt, InstallFormat::SkillMd);
    }

    #[test]
    fn test_format_as_skill_md_adds_name() {
        let e = SkillEntry {
            description: "Test agent",
            content: "---\ndescription: Test agent\n---\n\n# Test\n",
            ..entry("test-agent", "agent", "")
        };
        let result = format_as_skill_md(&e);
        assert!(result.contains("name: test-agent"));
        assert!(result.contains("description: Test agent"));
    }

    #[test]
    fn test_format_preserves_existing_name() {
        let e = SkillEntry {
            description: "Test skill",
            content: "---\nname: test-skill\ndescription: Test skill\n---\n\n# Test\n",
            ..entry("test-skill", "skill", "")
        };
        assert_eq!(format_as_skill_md(&e), e.content);
    }

    #[test]
    fn test_format_no_frontmatter() {
        let e = SkillEntry {
            description: "Bare content",
            content: "# No Frontmatter\n\nJust content.\n",
            ..entry("bare", "agent", "")
        };
        let result = format_as_skill_md(&e);
        assert!(result.starts_with("---\nname: bare\n"));
        assert!(result.contains("# No Frontmatter"));
    }

    // ---- extension helpers ---------------------------------------------------

    #[test]
    fn test_resolve_platform_explicit_wins() {
        assert_eq!(resolve_platform(Some("pi"), "claude-code"), "pi");
        assert_eq!(resolve_platform(Some("pi"), ""), "pi");
    }

    #[test]
    fn test_resolve_platform_from_agent() {
        assert_eq!(resolve_platform(None, "pi"), "pi");
        assert_eq!(resolve_platform(None, "pi-dev"), "pi");
    }

    #[test]
    fn test_resolve_platform_empty_for_unknown() {
        assert_eq!(resolve_platform(None, "claude-code"), "");
        assert_eq!(resolve_platform(Some(""), "claude-code"), "");
    }

    #[test]
    fn test_extensions_dir_pi_project_scope() {
        let root = PathBuf::from("/tmp/proj");
        assert_eq!(
            extensions_dir("pi", &root, false),
            Some(root.join(".pi/extensions"))
        );
    }

    #[test]
    fn test_extensions_dir_pi_user_scope_with_home() {
        // Use the injectable helper so we don't mutate the process-global HOME
        // env var (set_var is `unsafe` and races with parallel test threads).
        let home = PathBuf::from("/tmp/fake-home");
        assert_eq!(
            extensions_dir_with_home("pi", Some(&home), &PathBuf::from("/unused"), true),
            Some(PathBuf::from("/tmp/fake-home/.pi/agent/extensions"))
        );
    }

    #[test]
    fn test_extensions_dir_pi_user_scope_without_home_returns_none() {
        assert_eq!(
            extensions_dir_with_home("pi", None, &PathBuf::from("/unused"), true),
            None,
        );
    }

    #[test]
    fn test_extensions_dir_unknown_platform_returns_none() {
        let root = PathBuf::from("/tmp/proj");
        assert_eq!(extensions_dir("unknown", &root, false), None);
        assert_eq!(extensions_dir("", &root, false), None);
    }

    #[test]
    fn test_install_paths_skill_single_file() {
        let root = PathBuf::from("/tmp/proj");
        let e = entry("dd-pup", "skill", "body");
        let paths = install_paths(&e, "claude-code", "", &root, None, false).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].0, root.join(".claude/skills/dd-pup/SKILL.md"));
    }

    #[test]
    fn test_install_paths_extension_expands_files() {
        static FILES: &[(&str, &str)] = &[("index.ts", "// js"), ("package.json", "{}")];
        let e = SkillEntry {
            platform: "pi",
            files: FILES,
            ..entry("dd-pup-pi", "extension", "")
        };
        let root = PathBuf::from("/tmp/proj");
        let paths = install_paths(&e, "claude-code", "pi", &root, None, false).unwrap();
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].0, root.join(".pi/extensions/dd-pup-pi/index.ts"));
        assert_eq!(paths[0].1, "// js");
        assert_eq!(
            paths[1].0,
            root.join(".pi/extensions/dd-pup-pi/package.json")
        );
    }

    #[test]
    fn test_install_paths_extension_dir_override() {
        static FILES: &[(&str, &str)] = &[("index.ts", "// js")];
        let e = SkillEntry {
            platform: "pi",
            files: FILES,
            ..entry("dd-pup-pi", "extension", "")
        };
        let paths = install_paths(
            &e,
            "claude-code",
            "pi",
            &PathBuf::from("/unused"),
            Some("/tmp/out"),
            false,
        )
        .unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].0, PathBuf::from("/tmp/out/dd-pup-pi/index.ts"));
    }

    #[test]
    fn test_install_paths_extension_unknown_platform_errors() {
        static FILES: &[(&str, &str)] = &[("index.ts", "// js")];
        let e = SkillEntry {
            platform: "bogus",
            files: FILES,
            ..entry("dd-pup-bogus", "extension", "")
        };
        let err = install_paths(
            &e,
            "claude-code",
            "bogus",
            &PathBuf::from("/tmp/proj"),
            None,
            false,
        )
        .unwrap_err();
        assert!(err.to_string().contains("bogus"));
    }

    #[test]
    fn test_dd_pup_pi_entry_registered() {
        let e = SKILLS
            .iter()
            .find(|e| e.name == "dd-pup-pi")
            .expect("dd-pup-pi must be registered");
        assert_eq!(e.entry_type, "extension");
        assert_eq!(e.platform, "pi");
        let names: Vec<&str> = e.files.iter().map(|(p, _)| *p).collect();
        assert!(names.contains(&"index.ts"));
        assert!(names.contains(&"package.json"));
        assert!(names.contains(&"README.md"));
    }
}

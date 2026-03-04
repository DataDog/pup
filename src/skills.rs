use std::path::{Path, PathBuf};

pub struct SkillEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub entry_type: &'static str, // "skill" or "agent"
    pub content: &'static str,
}

pub static SKILLS: &[SkillEntry] = &[
    // --- Skills (from agent-skills + claude-plugin) ---
    SkillEntry {
        name: "dd-pup",
        description: "Datadog CLI (pup). OAuth2 auth with token refresh.",
        entry_type: "skill",
        content: include_str!("../skills/dd-pup/SKILL.md"),
    },
    SkillEntry {
        name: "dd-monitors",
        description: "Monitor management - create, update, mute, and alerting best practices.",
        entry_type: "skill",
        content: include_str!("../skills/dd-monitors/SKILL.md"),
    },
    SkillEntry {
        name: "dd-logs",
        description: "Log management - search, pipelines, archives, and cost control.",
        entry_type: "skill",
        content: include_str!("../skills/dd-logs/SKILL.md"),
    },
    SkillEntry {
        name: "dd-apm",
        description: "APM - traces, services, dependencies, performance analysis.",
        entry_type: "skill",
        content: include_str!("../skills/dd-apm/SKILL.md"),
    },
    SkillEntry {
        name: "dd-debugger",
        description: "Live Debugger - create, delete, and watch log probes and events.",
        entry_type: "skill",
        content: include_str!("../skills/dd-debugger/SKILL.md"),
    },
    SkillEntry {
        name: "dd-docs",
        description: "Datadog docs lookup using docs.datadoghq.com/llms.txt.",
        entry_type: "skill",
        content: include_str!("../skills/dd-docs/SKILL.md"),
    },
    SkillEntry {
        name: "dd-code-generation",
        description: "Use pup CLI or generate code (TypeScript, Python, Java, Go, Rust).",
        entry_type: "skill",
        content: include_str!("../skills/dd-code-generation/SKILL.md"),
    },
    SkillEntry {
        name: "dd-file-issue",
        description: "File GitHub issues to the right repository (pup CLI or plugin).",
        entry_type: "skill",
        content: include_str!("../skills/dd-file-issue/SKILL.md"),
    },
    SkillEntry {
        name: "dd-symdb",
        description: "Symbol Database - search service symbols, find probe-able methods.",
        entry_type: "skill",
        content: include_str!("../skills/dd-symdb/SKILL.md"),
    },
    // --- Domain Agents (from datadog-api-claude-plugin) ---
    SkillEntry {
        name: "agentless-scanning",
        description: "Manage Datadog Agentless Scanning for AWS and Azure resources.",
        entry_type: "agent",
        content: include_str!("../agents/agentless-scanning.md"),
    },
    SkillEntry {
        name: "api-management",
        description: "Manage API keys and Application keys for authentication.",
        entry_type: "agent",
        content: include_str!("../agents/api-management.md"),
    },
    SkillEntry {
        name: "apm-configuration",
        description: "Manage APM retention filters and span-based metrics.",
        entry_type: "agent",
        content: include_str!("../agents/apm-configuration.md"),
    },
    SkillEntry {
        name: "app-builder",
        description: "Manage App Builder applications (low-code internal tools).",
        entry_type: "agent",
        content: include_str!("../agents/app-builder.md"),
    },
    SkillEntry {
        name: "application-security",
        description: "Manage ASM including WAF rules, threat detection, API protection.",
        entry_type: "agent",
        content: include_str!("../agents/application-security.md"),
    },
    SkillEntry {
        name: "audience-management",
        description: "Query and segment RUM users and accounts.",
        entry_type: "agent",
        content: include_str!("../agents/audience-management.md"),
    },
    SkillEntry {
        name: "audit-logs",
        description: "Query and manage Audit Trail events for compliance.",
        entry_type: "agent",
        content: include_str!("../agents/audit-logs.md"),
    },
    SkillEntry {
        name: "aws-integration",
        description: "Configure AWS integration for monitoring and log collection.",
        entry_type: "agent",
        content: include_str!("../agents/aws-integration.md"),
    },
    SkillEntry {
        name: "azure-integration",
        description: "Configure Azure integration for monitoring and resources.",
        entry_type: "agent",
        content: include_str!("../agents/azure-integration.md"),
    },
    SkillEntry {
        name: "cicd",
        description: "Manage CI/CD Visibility including tests, pipelines, DORA metrics.",
        entry_type: "agent",
        content: include_str!("../agents/cicd.md"),
    },
    SkillEntry {
        name: "cloud-cost",
        description: "Manage Cloud Cost Management including multi-cloud config.",
        entry_type: "agent",
        content: include_str!("../agents/cloud-cost.md"),
    },
    SkillEntry {
        name: "cloud-workload-security",
        description: "Manage CSM Threats and Workload Protection agent rules.",
        entry_type: "agent",
        content: include_str!("../agents/cloud-workload-security.md"),
    },
    SkillEntry {
        name: "container-monitoring",
        description: "Monitor Kubernetes and containerized environments.",
        entry_type: "agent",
        content: include_str!("../agents/container-monitoring.md"),
    },
    SkillEntry {
        name: "dashboards",
        description: "Manage dashboards including CRUD and widgets.",
        entry_type: "agent",
        content: include_str!("../agents/dashboards.md"),
    },
    SkillEntry {
        name: "data-deletion",
        description: "GDPR/data privacy compliance through targeted deletion.",
        entry_type: "agent",
        content: include_str!("../agents/data-deletion.md"),
    },
    SkillEntry {
        name: "data-governance",
        description: "Access control, data enrichment, data protection.",
        entry_type: "agent",
        content: include_str!("../agents/data-governance.md"),
    },
    SkillEntry {
        name: "database-monitoring",
        description: "Query and manage DBM data and monitors.",
        entry_type: "agent",
        content: include_str!("../agents/database-monitoring.md"),
    },
    SkillEntry {
        name: "error-tracking",
        description: "Manage error tracking issues, triage, and assignment.",
        entry_type: "agent",
        content: include_str!("../agents/error-tracking.md"),
    },
    SkillEntry {
        name: "events",
        description: "Manage events including submission, search, filtering.",
        entry_type: "agent",
        content: include_str!("../agents/events.md"),
    },
    SkillEntry {
        name: "fleet-automation",
        description: "Manage Agent fleet, deployments, upgrades, schedules.",
        entry_type: "agent",
        content: include_str!("../agents/fleet-automation.md"),
    },
    SkillEntry {
        name: "gcp-integration",
        description: "Configure GCP integration for monitoring and resources.",
        entry_type: "agent",
        content: include_str!("../agents/gcp-integration.md"),
    },
    SkillEntry {
        name: "incident-response",
        description: "Manage incident lifecycle, teams, and response.",
        entry_type: "agent",
        content: include_str!("../agents/incident-response.md"),
    },
    SkillEntry {
        name: "infrastructure",
        description: "Query infrastructure hosts, counts, and metadata.",
        entry_type: "agent",
        content: include_str!("../agents/infrastructure.md"),
    },
    SkillEntry {
        name: "log-configuration",
        description: "Manage log archives, pipelines, indexes, custom destinations.",
        entry_type: "agent",
        content: include_str!("../agents/log-configuration.md"),
    },
    SkillEntry {
        name: "logs",
        description: "Search and analyze log data with flexible queries.",
        entry_type: "agent",
        content: include_str!("../agents/logs.md"),
    },
    SkillEntry {
        name: "metrics",
        description: "Query, list, and manage metrics.",
        entry_type: "agent",
        content: include_str!("../agents/metrics.md"),
    },
    SkillEntry {
        name: "monitoring-alerting",
        description: "Full monitor management, downtimes, and templates.",
        entry_type: "agent",
        content: include_str!("../agents/monitoring-alerting.md"),
    },
    SkillEntry {
        name: "network-performance",
        description: "Network Performance Monitoring and DNS monitoring.",
        entry_type: "agent",
        content: include_str!("../agents/network-performance.md"),
    },
    SkillEntry {
        name: "notebooks",
        description: "Manage investigation notebooks.",
        entry_type: "agent",
        content: include_str!("../agents/notebooks.md"),
    },
    SkillEntry {
        name: "observability-pipelines",
        description: "Manage Observability Pipelines for data routing.",
        entry_type: "agent",
        content: include_str!("../agents/observability-pipelines.md"),
    },
    SkillEntry {
        name: "organization-management",
        description: "Manage organization settings, teams, and users.",
        entry_type: "agent",
        content: include_str!("../agents/organization-management.md"),
    },
    SkillEntry {
        name: "powerpacks",
        description: "Manage reusable dashboard widget groups.",
        entry_type: "agent",
        content: include_str!("../agents/powerpacks.md"),
    },
    SkillEntry {
        name: "rum-metrics-retention",
        description: "Manage RUM metrics and retention filters.",
        entry_type: "agent",
        content: include_str!("../agents/rum-metrics-retention.md"),
    },
    SkillEntry {
        name: "rum",
        description: "Query Real User Monitoring data.",
        entry_type: "agent",
        content: include_str!("../agents/rum.md"),
    },
    SkillEntry {
        name: "saml-configuration",
        description: "Manage SAML SSO configuration.",
        entry_type: "agent",
        content: include_str!("../agents/saml-configuration.md"),
    },
    SkillEntry {
        name: "scorecards",
        description: "Manage service quality scorecards.",
        entry_type: "agent",
        content: include_str!("../agents/scorecards.md"),
    },
    SkillEntry {
        name: "security-posture-management",
        description: "Manage CSPM findings and compliance.",
        entry_type: "agent",
        content: include_str!("../agents/security-posture-management.md"),
    },
    SkillEntry {
        name: "security",
        description: "Security monitoring signals and rules.",
        entry_type: "agent",
        content: include_str!("../agents/security.md"),
    },
    SkillEntry {
        name: "service-catalog",
        description: "Manage service registry and metadata.",
        entry_type: "agent",
        content: include_str!("../agents/service-catalog.md"),
    },
    SkillEntry {
        name: "slos",
        description: "Manage Service Level Objectives.",
        entry_type: "agent",
        content: include_str!("../agents/slos.md"),
    },
    SkillEntry {
        name: "spark-pod-autosizing",
        description: "Manage Spark pod autosizing for Kubernetes.",
        entry_type: "agent",
        content: include_str!("../agents/spark-pod-autosizing.md"),
    },
    SkillEntry {
        name: "static-analysis",
        description: "Manage static code analysis.",
        entry_type: "agent",
        content: include_str!("../agents/static-analysis.md"),
    },
    SkillEntry {
        name: "synthetics",
        description: "Manage synthetic monitoring tests.",
        entry_type: "agent",
        content: include_str!("../agents/synthetics.md"),
    },
    SkillEntry {
        name: "third-party-integrations",
        description: "Manage third-party integrations (PagerDuty, Slack, etc.).",
        entry_type: "agent",
        content: include_str!("../agents/third-party-integrations.md"),
    },
    SkillEntry {
        name: "traces",
        description: "Query APM traces and spans.",
        entry_type: "agent",
        content: include_str!("../agents/traces.md"),
    },
    SkillEntry {
        name: "usage-metering",
        description: "Track Datadog usage and billing.",
        entry_type: "agent",
        content: include_str!("../agents/usage-metering.md"),
    },
    SkillEntry {
        name: "user-access-management",
        description: "Manage users, roles, teams, and permissions.",
        entry_type: "agent",
        content: include_str!("../agents/user-access-management.md"),
    },
    SkillEntry {
        name: "workflows",
        description: "Manage workflow automations.",
        entry_type: "agent",
        content: include_str!("../agents/workflows.md"),
    },
];

/// Resolve the detected agent name, applying override if provided.
pub fn resolve_agent(agent: Option<&str>) -> String {
    agent
        .map(String::from)
        .unwrap_or_else(|| crate::useragent::detect_agent_info().name)
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

/// Determine the install path for a single entry.
/// Skills always go to `<skills_dir>/<name>/SKILL.md`.
/// Agents go to `<agents_dir>/<name>.md` for Claude Code (subagent format),
/// or `<skills_dir>/<name>/SKILL.md` for other tools.
pub fn install_path(
    entry: &SkillEntry,
    agent: &str,
    project_root: &Path,
    dir_override: Option<&str>,
) -> (PathBuf, InstallFormat) {
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
                entry.entry_type == "skill" || entry.entry_type == "agent",
                "invalid type '{}' for {}",
                entry.entry_type,
                entry.name
            );
        }
    }

    #[test]
    fn test_all_entries_have_content() {
        for entry in SKILLS {
            assert!(
                !entry.content.is_empty(),
                "empty content for {}",
                entry.name
            );
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

    #[test]
    fn test_install_path_skill_claude_code() {
        let root = PathBuf::from("/tmp/test-project");
        let entry = SkillEntry {
            name: "dd-pup",
            description: "test",
            entry_type: "skill",
            content: "",
        };
        let (path, fmt) = install_path(&entry, "claude-code", &root, None);
        assert_eq!(path, root.join(".claude/skills/dd-pup/SKILL.md"));
        assert_eq!(fmt, InstallFormat::SkillMd);
    }

    #[test]
    fn test_install_path_agent_claude_code() {
        let root = PathBuf::from("/tmp/test-project");
        let entry = SkillEntry {
            name: "logs",
            description: "test",
            entry_type: "agent",
            content: "",
        };
        let (path, fmt) = install_path(&entry, "claude-code", &root, None);
        assert_eq!(path, root.join(".claude/agents/logs.md"));
        assert_eq!(fmt, InstallFormat::AgentMd);
    }

    #[test]
    fn test_install_path_agent_cursor_as_skill() {
        let root = PathBuf::from("/tmp/test-project");
        let entry = SkillEntry {
            name: "logs",
            description: "test",
            entry_type: "agent",
            content: "",
        };
        let (path, fmt) = install_path(&entry, "cursor", &root, None);
        assert_eq!(path, root.join(".cursor/skills/logs/SKILL.md"));
        assert_eq!(fmt, InstallFormat::SkillMd);
    }

    #[test]
    fn test_install_path_dir_override() {
        let root = PathBuf::from("/tmp/test-project");
        let entry = SkillEntry {
            name: "logs",
            description: "test",
            entry_type: "agent",
            content: "",
        };
        let (path, fmt) = install_path(&entry, "claude-code", &root, Some("/tmp/out"));
        assert_eq!(path, PathBuf::from("/tmp/out/logs/SKILL.md"));
        assert_eq!(fmt, InstallFormat::SkillMd);
    }

    #[test]
    fn test_format_as_skill_md_adds_name() {
        let entry = SkillEntry {
            name: "test-agent",
            description: "Test agent",
            entry_type: "agent",
            content: "---\ndescription: Test agent\n---\n\n# Test\n",
        };
        let result = format_as_skill_md(&entry);
        assert!(result.contains("name: test-agent"));
        assert!(result.contains("description: Test agent"));
    }

    #[test]
    fn test_format_preserves_existing_name() {
        let entry = SkillEntry {
            name: "test-skill",
            description: "Test skill",
            entry_type: "skill",
            content: "---\nname: test-skill\ndescription: Test skill\n---\n\n# Test\n",
        };
        assert_eq!(format_as_skill_md(&entry), entry.content);
    }

    #[test]
    fn test_format_no_frontmatter() {
        let entry = SkillEntry {
            name: "bare",
            description: "Bare content",
            entry_type: "agent",
            content: "# No Frontmatter\n\nJust content.\n",
        };
        let result = format_as_skill_md(&entry);
        assert!(result.starts_with("---\nname: bare\n"));
        assert!(result.contains("# No Frontmatter"));
    }
}

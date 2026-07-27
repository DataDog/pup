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
    SkillEntry {
        name: "dd-unblock-pr",
        description: "Investigate a failing PR CI pipeline — attribute failures as flaky, infra, or regression and propose a targeted action.",
        entry_type: "skill",
        content: include_str!("../skills/dd-unblock-pr/SKILL.md"),
        platform: "",
        files: &[],
    },
    SkillEntry {
        name: "dd-triage-flaky-test",
        description: "Investigate a specific flaky test — get history, category, and recommend fix or quarantine.",
        entry_type: "skill",
        content: include_str!("../skills/dd-triage-flaky-test/SKILL.md"),
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

/// Static description of one supported AI-coding-assistant platform.
///
/// Each platform tells us where skills, agents, and extension bundles live for
/// both project-local and user-global scopes. Empty path strings mean "not
/// supported" — e.g. a hypothetical future platform with no skills/agents dirs,
/// and most platforms have no extensions dir.
pub struct PlatformSpec {
    /// Canonical platform name as users type it on the CLI.
    pub name: &'static str,
    /// Additional accepted names (e.g. `claude` for `claude-code`).
    pub aliases: &'static [&'static str],
    /// Project-local skills dir, relative to project root.
    pub project_skills: &'static str,
    /// User-global skills dir, relative to $HOME.
    pub user_skills: &'static str,
    /// Project-local agents dir; if empty, agents share the skills dir.
    pub project_agents: &'static str,
    /// User-global agents dir; if empty, agents share the user skills dir.
    pub user_agents: &'static str,
    /// Project-local extensions dir, relative to project root.
    pub project_extensions: &'static str,
    /// User-global extensions dir, relative to $HOME.
    pub user_extensions: &'static str,
    /// True iff agents install as Claude-Code-style `<name>.md` subagents
    /// rather than `SKILL.md` files.
    pub uses_agent_md: bool,
}

impl PlatformSpec {
    /// Returns true when this platform supports only extensions (no skills or
    /// agents directory in any scope).
    ///
    /// An empty agents path means agents share the skills dir; if skills is
    /// also empty the platform has no text-content support and can only receive
    /// extension bundles.
    pub fn is_extension_only(&self) -> bool {
        self.user_skills.is_empty()
            && self.project_skills.is_empty()
            && self.user_agents.is_empty()
            && self.project_agents.is_empty()
    }
}

/// Registry of supported platforms.
pub static PLATFORMS: &[PlatformSpec] = &[
    PlatformSpec {
        name: "claude-code",
        aliases: &["claude"],
        project_skills: ".claude/skills",
        user_skills: ".claude/skills",
        project_agents: ".claude/agents",
        user_agents: ".claude/agents",
        project_extensions: "",
        user_extensions: "",
        uses_agent_md: true,
    },
    PlatformSpec {
        name: "cursor",
        aliases: &[],
        project_skills: ".cursor/skills",
        user_skills: ".cursor/skills",
        project_agents: "",
        user_agents: "",
        project_extensions: "",
        user_extensions: "",
        uses_agent_md: false,
    },
    PlatformSpec {
        name: "codex",
        aliases: &[],
        project_skills: ".codex/skills",
        user_skills: ".codex/skills",
        project_agents: "",
        user_agents: "",
        project_extensions: "",
        user_extensions: "",
        uses_agent_md: false,
    },
    PlatformSpec {
        name: "opencode",
        aliases: &[],
        project_skills: ".opencode/skills",
        user_skills: ".config/opencode/skills",
        project_agents: "",
        user_agents: "",
        project_extensions: "",
        user_extensions: "",
        uses_agent_md: false,
    },
    PlatformSpec {
        name: "windsurf",
        aliases: &[],
        project_skills: ".windsurf/skills",
        user_skills: ".windsurf/skills",
        project_agents: "",
        user_agents: "",
        project_extensions: "",
        user_extensions: "",
        uses_agent_md: false,
    },
    PlatformSpec {
        name: "gemini-code",
        aliases: &["gemini"],
        project_skills: ".gemini/skills",
        user_skills: ".gemini/skills",
        project_agents: "",
        user_agents: "",
        project_extensions: "",
        user_extensions: "",
        uses_agent_md: false,
    },
    PlatformSpec {
        name: "pi",
        aliases: &["pi-dev"],
        project_skills: ".pi/skills",
        user_skills: ".pi/agent/skills",
        project_agents: "",
        user_agents: "",
        project_extensions: ".pi/extensions",
        user_extensions: ".pi/agent/extensions",
        uses_agent_md: false,
    },
    PlatformSpec {
        name: "devin",
        aliases: &[],
        project_skills: ".agents/skills",
        user_skills: ".agents/skills",
        project_agents: "",
        user_agents: "",
        project_extensions: "",
        user_extensions: "",
        uses_agent_md: false,
    },
];

/// CLI-typed selector for `pup skills install <platform>` and `pup skills
/// path <platform>`. Each canonical variant maps onto an entry in
/// [`PLATFORMS`] via [`SkillsPlatform::as_canonical`]; aliases (`claude`,
/// `gemini`, `pi-dev`) are accepted for ergonomics. The `All` variant
/// expands to every supported platform — see [`resolve_platform_list`].
///
/// Keep this in sync with [`PLATFORMS`]: the `platform_enum_matches_table`
/// test enforces the mapping.
#[cfg(feature = "native")]
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum SkillsPlatform {
    #[clap(alias = "claude")]
    ClaudeCode,
    Cursor,
    Codex,
    Opencode,
    Windsurf,
    #[clap(alias = "gemini")]
    GeminiCode,
    #[clap(alias = "pi-dev")]
    Pi,
    Devin,
    All,
}

#[cfg(feature = "native")]
impl SkillsPlatform {
    /// Canonical platform name as used by [`lookup_platform`]. `All` returns
    /// `"all"`, which [`resolve_platform_list`] expands into every supported
    /// platform.
    pub fn as_canonical(self) -> &'static str {
        match self {
            SkillsPlatform::ClaudeCode => "claude-code",
            SkillsPlatform::Cursor => "cursor",
            SkillsPlatform::Codex => "codex",
            SkillsPlatform::Opencode => "opencode",
            SkillsPlatform::Windsurf => "windsurf",
            SkillsPlatform::GeminiCode => "gemini-code",
            SkillsPlatform::Pi => "pi",
            SkillsPlatform::Devin => "devin",
            SkillsPlatform::All => "all",
        }
    }
}

/// Look up a platform by canonical name or alias. Returns `None` for unknown.
pub fn lookup_platform(name: &str) -> Option<&'static PlatformSpec> {
    PLATFORMS
        .iter()
        .find(|p| p.name == name || p.aliases.contains(&name))
}

/// Resolve the canonical platform name from a CLI input.
///
/// `None` or empty input falls back to environment detection. Aliases are
/// normalized to the canonical name. Unknown names return the input unchanged
/// so the caller can produce a useful error.
pub fn resolve_platform_name(input: Option<&str>) -> String {
    let raw = input.unwrap_or("").trim();
    if raw.is_empty() {
        let detected = crate::useragent::detect_agent_info().name;
        return lookup_platform(&detected)
            .map(|p| p.name.to_string())
            .unwrap_or(detected);
    }
    lookup_platform(raw)
        .map(|p| p.name.to_string())
        .unwrap_or_else(|| raw.to_string())
}

/// Expand a CLI platform input into the list of platforms to operate on.
///
/// - `Some("all")` → every platform in [`PLATFORMS`].
/// - `Some(name)` → that single platform (canonicalized via aliases).
/// - `None` or empty → auto-detected platform from the environment.
pub fn resolve_platform_list(input: Option<&str>) -> Vec<String> {
    let raw = input.unwrap_or("").trim();
    if raw.eq_ignore_ascii_case("all") {
        return PLATFORMS.iter().map(|p| p.name.to_string()).collect();
    }
    vec![resolve_platform_name(input)]
}

/// Read the `CLAUDE_CONFIG_DIR` environment variable.
///
/// Claude Code honours this variable to override the default `~/.claude`
/// configuration directory, allowing multiple accounts to run side-by-side
/// (e.g. `CLAUDE_CONFIG_DIR=~/.claude-work claude`). Pup respects it when
/// resolving **user-scope** install paths for the `claude-code` platform so
/// that skills and agents land in the same place Claude Code expects.
///
/// Returns `None` when the variable is unset or empty. Project-scope installs
/// are never affected — they always resolve relative to the project root.
pub fn claude_code_config_dir_from_env() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

/// Determine the extensions install directory for a platform.
pub fn extensions_dir(platform: &str, project_root: &Path, user_scope: bool) -> Option<PathBuf> {
    extensions_dir_with_home(
        platform,
        dirs::home_dir().as_deref(),
        project_root,
        user_scope,
        claude_code_config_dir_from_env().as_deref(),
    )
}

/// Same as [`extensions_dir`] but takes an explicit `home` directory, so tests
/// don't have to mutate the process-global `HOME` env var. The
/// `claude_config_dir` parameter mirrors [`claude_code_config_dir_from_env`]
/// for the same testability reason.
pub fn extensions_dir_with_home(
    platform: &str,
    home: Option<&Path>,
    project_root: &Path,
    user_scope: bool,
    claude_config_dir: Option<&Path>,
) -> Option<PathBuf> {
    let spec = lookup_platform(platform)?;
    let sub = if user_scope {
        spec.user_extensions
    } else {
        spec.project_extensions
    };
    resolve_relative(sub, home, project_root, user_scope, claude_config_dir)
}

/// Determine the skills install directory for a platform.
pub fn skills_dir(platform: &str, project_root: &Path, user_scope: bool) -> Option<PathBuf> {
    skills_dir_with_home(
        platform,
        dirs::home_dir().as_deref(),
        project_root,
        user_scope,
        claude_code_config_dir_from_env().as_deref(),
    )
}

/// Same as [`skills_dir`] but takes an explicit `home` directory. The
/// `claude_config_dir` parameter mirrors [`claude_code_config_dir_from_env`]
/// for testability.
pub fn skills_dir_with_home(
    platform: &str,
    home: Option<&Path>,
    project_root: &Path,
    user_scope: bool,
    claude_config_dir: Option<&Path>,
) -> Option<PathBuf> {
    let spec = lookup_platform(platform)?;
    let sub = if user_scope {
        spec.user_skills
    } else {
        spec.project_skills
    };
    resolve_relative(sub, home, project_root, user_scope, claude_config_dir)
}

/// Determine the agents (subagents) install directory for a platform.
///
/// If the platform has no dedicated agents dir, agents share the skills dir.
pub fn agents_dir(platform: &str, project_root: &Path, user_scope: bool) -> Option<PathBuf> {
    agents_dir_with_home(
        platform,
        dirs::home_dir().as_deref(),
        project_root,
        user_scope,
        claude_code_config_dir_from_env().as_deref(),
    )
}

/// Same as [`agents_dir`] but takes an explicit `home` directory. The
/// `claude_config_dir` parameter mirrors [`claude_code_config_dir_from_env`]
/// for testability.
pub fn agents_dir_with_home(
    platform: &str,
    home: Option<&Path>,
    project_root: &Path,
    user_scope: bool,
    claude_config_dir: Option<&Path>,
) -> Option<PathBuf> {
    let spec = lookup_platform(platform)?;
    let sub = if user_scope {
        spec.user_agents
    } else {
        spec.project_agents
    };
    if sub.is_empty() {
        // Empty sentinel means "share the skills dir for this scope."
        return skills_dir_with_home(platform, home, project_root, user_scope, claude_config_dir);
    }
    resolve_relative(sub, home, project_root, user_scope, claude_config_dir)
}

/// Resolve a forward-slash-separated relative subpath against either the home
/// directory (user scope) or the project root (project scope). Returns `None`
/// when the subpath is empty (the sentinel for "not applicable") or when user
/// scope is requested but `home` is unavailable.
///
/// When `claude_config_dir` is `Some` and the subpath begins with `.claude/`,
/// the `.claude` prefix is replaced by the config dir. This honours the
/// `CLAUDE_CONFIG_DIR` environment variable for user-scope `claude-code`
/// installs. Project scope is never affected.
fn resolve_relative(
    sub: &str,
    home: Option<&Path>,
    project_root: &Path,
    user_scope: bool,
    claude_config_dir: Option<&Path>,
) -> Option<PathBuf> {
    if sub.is_empty() {
        return None;
    }

    // Honour CLAUDE_CONFIG_DIR for user-scope paths that start with .claude/.
    if user_scope {
        if let Some(cfg) = claude_config_dir {
            if let Some(rest) = sub.strip_prefix(".claude/") {
                let mut base = cfg.to_path_buf();
                for part in rest.split('/') {
                    base.push(part);
                }
                return Some(base);
            }
        }
    }

    let mut base = if user_scope {
        home?.to_path_buf()
    } else {
        project_root.to_path_buf()
    };
    for part in sub.split('/') {
        base.push(part);
    }
    Some(base)
}

/// Determine the install path for a single-file skill or agent entry.
///
/// Skills install to `<skills_dir>/<name>/SKILL.md`. Agents install to
/// `<agents_dir>/<name>.md` for platforms with [`PlatformSpec::uses_agent_md`]
/// (Claude Code subagent format), and `<skills_dir>/<name>/SKILL.md` elsewhere.
///
/// Returns `None` when the platform has no skills/agents dir.
/// Panics if called for an `extension` entry; use [`install_paths`] for those.
pub fn install_path(
    entry: &SkillEntry,
    platform: &str,
    project_root: &Path,
    dir_override: Option<&str>,
    user_scope: bool,
) -> Option<(PathBuf, InstallFormat)> {
    debug_assert_ne!(
        entry.entry_type, "extension",
        "install_path() does not handle extensions; use install_paths()"
    );

    // Extension-only platforms have no skills or agents directory;
    // skills and agents cannot install there even when --dir overrides the path.
    let spec = lookup_platform(platform)?;
    if spec.is_extension_only() {
        return None;
    }

    if let Some(d) = dir_override {
        // Explicit --dir: everything as SKILL.md
        return Some((
            PathBuf::from(d).join(entry.name).join("SKILL.md"),
            InstallFormat::SkillMd,
        ));
    }
    if entry.entry_type == "agent" && spec.uses_agent_md {
        let dir = agents_dir(platform, project_root, user_scope)?;
        Some((
            dir.join(format!("{}.md", entry.name)),
            InstallFormat::AgentMd,
        ))
    } else {
        let dir = skills_dir(platform, project_root, user_scope)?;
        Some((
            dir.join(entry.name).join("SKILL.md"),
            InstallFormat::SkillMd,
        ))
    }
}

/// Resolve install destinations for any entry, including multi-file extensions.
///
/// Returns a list of `(absolute_path, contents)` tuples. For skills and agents
/// this is a single-element list. For extensions this expands to one entry
/// per bundled file.
///
/// Returns `Ok(vec![])` (no-op) when the entry isn't applicable to the
/// platform (e.g. asking for a `pi` extension on `claude-code`).
/// The caller can treat an empty result as "skip".
pub fn install_paths(
    entry: &SkillEntry,
    platform: &str,
    project_root: &Path,
    dir_override: Option<&str>,
    user_scope: bool,
) -> anyhow::Result<Vec<(PathBuf, String)>> {
    if entry.entry_type == "extension" {
        // Extensions are tied to a specific platform; only install when the
        // platform matches, even when --dir overrides the destination path.
        // Without this guard a `--dir all` install would produce files for a
        // pi-only extension on every platform in the loop, inflating the
        // reported platform count with platforms that received no real content.
        if entry.platform != platform {
            return Ok(vec![]);
        }
        let base = if let Some(d) = dir_override {
            PathBuf::from(d).join(entry.name)
        } else {
            let Some(root) = extensions_dir(platform, project_root, user_scope) else {
                return Ok(vec![]);
            };
            root.join(entry.name)
        };
        return Ok(entry
            .files
            .iter()
            .map(|(rel, body)| (base.join(rel), (*body).to_string()))
            .collect());
    }

    let Some((path, fmt)) = install_path(entry, platform, project_root, dir_override, user_scope)
    else {
        return Ok(vec![]);
    };
    Ok(vec![(path, format_content(entry, &fmt))])
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
/// Currently identical to [`format_as_skill_md`]; the two will diverge when
/// agent `.md` files require Claude-Code-specific frontmatter fields.
pub fn format_as_agent_md(entry: &SkillEntry) -> String {
    format_as_skill_md(entry)
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
                assert!(
                    entry.content.is_empty(),
                    "extension {} must not have content (content is for skills/agents only)",
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
        assert_eq!(skills.len(), 11, "expected 11 skills");
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
    fn test_skills_dir_claude_code_project() {
        let root = PathBuf::from("/tmp/test-project");
        assert_eq!(
            skills_dir_with_home("claude-code", None, &root, false, None),
            Some(root.join(".claude/skills"))
        );
    }

    #[test]
    fn test_skills_dir_cursor_project() {
        let root = PathBuf::from("/tmp/test-project");
        assert_eq!(
            skills_dir_with_home("cursor", None, &root, false, None),
            Some(root.join(".cursor/skills"))
        );
    }

    #[test]
    fn test_skills_dir_codex_project() {
        let root = PathBuf::from("/tmp/test-project");
        assert_eq!(
            skills_dir_with_home("codex", None, &root, false, None),
            Some(root.join(".codex/skills"))
        );
    }

    #[test]
    fn test_skills_dir_opencode_project() {
        let root = PathBuf::from("/tmp/test-project");
        assert_eq!(
            skills_dir_with_home("opencode", None, &root, false, None),
            Some(root.join(".opencode/skills"))
        );
    }

    #[test]
    fn test_skills_dir_opencode_user() {
        let home = PathBuf::from("/tmp/fake-home");
        assert_eq!(
            skills_dir_with_home(
                "opencode",
                Some(&home),
                &PathBuf::from("/unused"),
                true,
                None
            ),
            Some(home.join(".config/opencode/skills"))
        );
    }

    #[test]
    fn test_skills_dir_claude_user() {
        let home = PathBuf::from("/tmp/fake-home");
        assert_eq!(
            skills_dir_with_home(
                "claude-code",
                Some(&home),
                &PathBuf::from("/unused"),
                true,
                None
            ),
            Some(home.join(".claude/skills"))
        );
    }

    #[test]
    fn test_skills_dir_pi_project_scope() {
        let root = PathBuf::from("/tmp/test-project");
        assert_eq!(
            skills_dir_with_home("pi", None, &root, false, None),
            Some(root.join(".pi/skills"))
        );
    }

    #[test]
    fn test_skills_dir_pi_user_scope() {
        let home = PathBuf::from("/tmp/fake-home");
        assert_eq!(
            skills_dir_with_home("pi", Some(&home), &PathBuf::from("/unused"), true, None),
            Some(home.join(".pi/agent/skills"))
        );
    }

    #[test]
    fn test_skills_dir_devin_project_scope() {
        let root = PathBuf::from("/tmp/test-project");
        assert_eq!(
            skills_dir_with_home("devin", None, &root, false, None),
            Some(root.join(".agents/skills"))
        );
    }

    #[test]
    fn test_skills_dir_devin_user_scope() {
        let home = PathBuf::from("/tmp/fake-home");
        assert_eq!(
            skills_dir_with_home("devin", Some(&home), &PathBuf::from("/unused"), true, None),
            Some(home.join(".agents/skills"))
        );
    }

    #[test]
    fn test_skills_dir_unknown_returns_none() {
        let root = PathBuf::from("/tmp/test-project");
        assert_eq!(skills_dir_with_home("nope", None, &root, false, None), None);
    }

    #[test]
    fn test_agents_dir_claude_code_project() {
        let root = PathBuf::from("/tmp/test-project");
        assert_eq!(
            agents_dir_with_home("claude-code", None, &root, false, None),
            Some(root.join(".claude/agents"))
        );
    }

    #[test]
    fn test_agents_dir_claude_code_user() {
        let home = PathBuf::from("/tmp/fake-home");
        assert_eq!(
            agents_dir_with_home(
                "claude-code",
                Some(&home),
                &PathBuf::from("/unused"),
                true,
                None
            ),
            Some(home.join(".claude/agents"))
        );
    }

    #[test]
    fn test_agents_dir_cursor_falls_back_to_skills() {
        let root = PathBuf::from("/tmp/test-project");
        assert_eq!(
            agents_dir_with_home("cursor", None, &root, false, None),
            Some(root.join(".cursor/skills"))
        );
    }

    // ---- CLAUDE_CONFIG_DIR support ------------------------------------------

    #[test]
    fn test_skills_dir_claude_user_with_config_dir() {
        let home = PathBuf::from("/tmp/fake-home");
        let cfg = PathBuf::from("/tmp/claude-config");
        assert_eq!(
            skills_dir_with_home(
                "claude-code",
                Some(&home),
                &PathBuf::from("/unused"),
                true,
                Some(&cfg),
            ),
            Some(cfg.join("skills"))
        );
    }

    #[test]
    fn test_agents_dir_claude_user_with_config_dir() {
        let home = PathBuf::from("/tmp/fake-home");
        let cfg = PathBuf::from("/tmp/claude-config");
        assert_eq!(
            agents_dir_with_home(
                "claude-code",
                Some(&home),
                &PathBuf::from("/unused"),
                true,
                Some(&cfg),
            ),
            Some(cfg.join("agents"))
        );
    }

    #[test]
    fn test_skills_dir_claude_user_config_dir_overrides_home() {
        // When CLAUDE_CONFIG_DIR is set, home is irrelevant for claude-code
        // user-scope paths — the config dir wins.
        let cfg = PathBuf::from("/custom/claude");
        assert_eq!(
            skills_dir_with_home(
                "claude-code",
                None,
                &PathBuf::from("/unused"),
                true,
                Some(&cfg),
            ),
            Some(cfg.join("skills"))
        );
    }

    #[test]
    fn test_skills_dir_claude_project_scope_ignores_config_dir() {
        // Project scope must never be affected by CLAUDE_CONFIG_DIR.
        let root = PathBuf::from("/tmp/test-project");
        let cfg = PathBuf::from("/custom/claude");
        assert_eq!(
            skills_dir_with_home("claude-code", None, &root, false, Some(&cfg)),
            Some(root.join(".claude/skills"))
        );
    }

    #[test]
    fn test_agents_dir_claude_project_scope_ignores_config_dir() {
        let root = PathBuf::from("/tmp/test-project");
        let cfg = PathBuf::from("/custom/claude");
        assert_eq!(
            agents_dir_with_home("claude-code", None, &root, false, Some(&cfg)),
            Some(root.join(".claude/agents"))
        );
    }

    #[test]
    fn test_skills_dir_other_platform_ignores_config_dir() {
        // CLAUDE_CONFIG_DIR only applies to claude-code, not other platforms.
        let home = PathBuf::from("/tmp/fake-home");
        let cfg = PathBuf::from("/custom/claude");
        assert_eq!(
            skills_dir_with_home(
                "cursor",
                Some(&home),
                &PathBuf::from("/unused"),
                true,
                Some(&cfg),
            ),
            Some(home.join(".cursor/skills"))
        );
    }

    #[test]
    fn test_claude_code_config_dir_from_env_unset() {
        // We can't fully control env vars in a concurrent test runner, but we
        // can at least verify the function returns Option<PathBuf> without
        // panicking. The actual value depends on the test environment.
        let _ = claude_code_config_dir_from_env();
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
        let (path, fmt) = install_path(&e, "claude-code", &root, None, false).unwrap();
        assert_eq!(path, root.join(".claude/skills/dd-pup/SKILL.md"));
        assert_eq!(fmt, InstallFormat::SkillMd);
    }

    #[test]
    fn test_install_path_agent_claude_code() {
        let root = PathBuf::from("/tmp/test-project");
        let e = entry("logs", "agent", "");
        let (path, fmt) = install_path(&e, "claude-code", &root, None, false).unwrap();
        assert_eq!(path, root.join(".claude/agents/logs.md"));
        assert_eq!(fmt, InstallFormat::AgentMd);
    }

    #[test]
    fn test_install_path_agent_cursor_as_skill() {
        let root = PathBuf::from("/tmp/test-project");
        let e = entry("logs", "agent", "");
        let (path, fmt) = install_path(&e, "cursor", &root, None, false).unwrap();
        assert_eq!(path, root.join(".cursor/skills/logs/SKILL.md"));
        assert_eq!(fmt, InstallFormat::SkillMd);
    }

    #[test]
    fn test_install_path_agent_codex_as_skill() {
        let root = PathBuf::from("/tmp/test-project");
        let e = entry("logs", "agent", "");
        let (path, fmt) = install_path(&e, "codex", &root, None, false).unwrap();
        assert_eq!(path, root.join(".codex/skills/logs/SKILL.md"));
        assert_eq!(fmt, InstallFormat::SkillMd);
    }

    #[test]
    fn test_install_path_agent_devin_as_skill() {
        let root = PathBuf::from("/tmp/test-project");
        let e = entry("logs", "agent", "");
        let (path, fmt) = install_path(&e, "devin", &root, None, false).unwrap();
        assert_eq!(path, root.join(".agents/skills/logs/SKILL.md"));
        assert_ne!(path, root.join(".agents/skills/logs.md"));
        assert_eq!(fmt, InstallFormat::SkillMd);
    }

    #[test]
    fn test_install_path_dir_override() {
        let root = PathBuf::from("/tmp/test-project");
        let e = entry("logs", "agent", "");
        let (path, fmt) = install_path(&e, "claude-code", &root, Some("/tmp/out"), false).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/out/logs/SKILL.md"));
        assert_eq!(fmt, InstallFormat::SkillMd);
    }

    #[test]
    fn test_install_path_skill_on_pi() {
        let root = PathBuf::from("/tmp/test-project");
        let e = entry("dd-pup", "skill", "");
        let (path, fmt) = install_path(&e, "pi", &root, None, false).unwrap();
        assert_eq!(path, root.join(".pi/skills/dd-pup/SKILL.md"));
        assert_eq!(fmt, InstallFormat::SkillMd);
    }

    #[test]
    fn test_install_path_skill_on_pi_with_dir_override() {
        let root = PathBuf::from("/tmp/test-project");
        let e = entry("dd-pup", "skill", "");
        let (path, fmt) = install_path(&e, "pi", &root, Some("/tmp/out"), false).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/out/dd-pup/SKILL.md"));
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

    // ---- platform resolution -------------------------------------------------

    #[test]
    fn test_lookup_platform_canonical() {
        assert_eq!(
            lookup_platform("claude-code").map(|p| p.name),
            Some("claude-code")
        );
        assert_eq!(lookup_platform("codex").map(|p| p.name), Some("codex"));
        assert_eq!(
            lookup_platform("opencode").map(|p| p.name),
            Some("opencode")
        );
        assert_eq!(lookup_platform("pi").map(|p| p.name), Some("pi"));
    }

    #[test]
    fn test_lookup_platform_devin() {
        let spec = lookup_platform("devin").expect("devin must be a known platform");
        assert_eq!(spec.name, "devin");
        assert!(spec.aliases.is_empty());
        assert!(!spec.uses_agent_md);
        assert!(!spec.is_extension_only());
    }

    #[test]
    fn test_lookup_platform_aliases() {
        assert_eq!(
            lookup_platform("claude").map(|p| p.name),
            Some("claude-code")
        );
        assert_eq!(lookup_platform("pi-dev").map(|p| p.name), Some("pi"));
        assert_eq!(
            lookup_platform("gemini").map(|p| p.name),
            Some("gemini-code")
        );
    }

    #[test]
    fn test_lookup_platform_unknown() {
        assert!(lookup_platform("nope").is_none());
        assert!(lookup_platform("").is_none());
    }

    #[test]
    fn test_resolve_platform_name_canonical_passthrough() {
        assert_eq!(resolve_platform_name(Some("cursor")), "cursor");
    }

    #[test]
    fn test_resolve_platform_name_alias_normalizes() {
        assert_eq!(resolve_platform_name(Some("claude")), "claude-code");
        assert_eq!(resolve_platform_name(Some("pi-dev")), "pi");
    }

    #[test]
    fn test_resolve_platform_name_unknown_passthrough() {
        assert_eq!(resolve_platform_name(Some("nope")), "nope");
    }

    #[test]
    fn test_resolve_platform_list_all_expands() {
        let list = resolve_platform_list(Some("all"));
        let expected: Vec<String> = PLATFORMS.iter().map(|p| p.name.to_string()).collect();
        assert_eq!(list, expected);
    }

    #[test]
    fn test_resolve_platform_list_all_case_insensitive() {
        assert_eq!(resolve_platform_list(Some("ALL")).len(), PLATFORMS.len());
    }

    #[test]
    fn platform_enum_matches_table() {
        // Every non-`All` SkillsPlatform variant must canonicalize to a real
        // entry in PLATFORMS, and every PLATFORMS entry must be reachable
        // from the enum. Failing this means the CLI accepts a value the
        // runtime can't service, or the runtime supports a platform users
        // can't select.
        use clap::ValueEnum;
        let table: std::collections::BTreeSet<&str> = PLATFORMS.iter().map(|p| p.name).collect();
        let mut from_enum = std::collections::BTreeSet::new();
        for variant in SkillsPlatform::value_variants() {
            let canonical = variant.as_canonical();
            if canonical == "all" {
                continue;
            }
            assert!(
                lookup_platform(canonical).is_some(),
                "SkillsPlatform::{variant:?} -> '{canonical}' not in PLATFORMS",
            );
            from_enum.insert(canonical);
        }
        assert_eq!(table, from_enum, "PLATFORMS and SkillsPlatform diverge");
    }

    #[test]
    fn platform_enum_aliases_match_table_aliases() {
        // Aliases live in two places: `#[clap(alias = ...)]` on each variant
        // (parsed by clap from the CLI) and `PlatformSpec.aliases` (used by
        // `lookup_platform` at runtime). They must agree, or the CLI will
        // accept a name the runtime can't resolve.
        use clap::ValueEnum;
        for variant in SkillsPlatform::value_variants() {
            let canonical = variant.as_canonical();
            if canonical == "all" {
                continue;
            }
            let spec = lookup_platform(canonical).expect("variant maps to a known platform");
            let pv = variant
                .to_possible_value()
                .expect("non-hidden value enum variant");
            let clap_aliases: std::collections::BTreeSet<&str> = pv
                .get_name_and_aliases()
                .filter(|a| *a != canonical)
                .collect();
            let table_aliases: std::collections::BTreeSet<&str> =
                spec.aliases.iter().copied().collect();
            assert_eq!(
                clap_aliases, table_aliases,
                "alias drift for SkillsPlatform::{variant:?}",
            );
        }
    }

    #[test]
    fn platform_enum_all_does_not_resolve_to_a_spec() {
        // `All` is a CLI quantifier, not a platform — it must not have a
        // PLATFORMS row, and `as_canonical()` returns the sentinel "all"
        // that `resolve_platform_list` expands.
        assert_eq!(SkillsPlatform::All.as_canonical(), "all");
        assert!(lookup_platform("all").is_none());
    }

    #[test]
    fn test_resolve_platform_list_single() {
        assert_eq!(
            resolve_platform_list(Some("cursor")),
            vec!["cursor".to_string()]
        );
        assert_eq!(
            resolve_platform_list(Some("claude")),
            vec!["claude-code".to_string()]
        );
    }

    // ---- extension helpers ---------------------------------------------------

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
            extensions_dir_with_home("pi", Some(&home), &PathBuf::from("/unused"), true, None),
            Some(PathBuf::from("/tmp/fake-home/.pi/agent/extensions"))
        );
    }

    #[test]
    fn test_extensions_dir_pi_user_scope_without_home_returns_none() {
        assert_eq!(
            extensions_dir_with_home("pi", None, &PathBuf::from("/unused"), true, None),
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
    fn test_extensions_dir_claude_returns_none() {
        // Claude has no extensions concept.
        let root = PathBuf::from("/tmp/proj");
        assert_eq!(extensions_dir("claude-code", &root, false), None);
    }

    #[test]
    fn test_install_paths_skill_single_file() {
        let root = PathBuf::from("/tmp/proj");
        let e = entry("dd-pup", "skill", "body");
        let paths = install_paths(&e, "claude-code", &root, None, false).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].0, root.join(".claude/skills/dd-pup/SKILL.md"));
    }

    #[test]
    fn test_install_paths_skill_on_pi() {
        let root = PathBuf::from("/tmp/proj");
        let e = entry("dd-pup", "skill", "body");
        let paths = install_paths(&e, "pi", &root, None, false).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].0, root.join(".pi/skills/dd-pup/SKILL.md"));
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
        let paths = install_paths(&e, "pi", &root, None, false).unwrap();
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].0, root.join(".pi/extensions/dd-pup-pi/index.ts"));
        assert_eq!(paths[0].1, "// js");
        assert_eq!(
            paths[1].0,
            root.join(".pi/extensions/dd-pup-pi/package.json")
        );
    }

    #[test]
    fn test_install_paths_extension_skipped_for_wrong_platform() {
        static FILES: &[(&str, &str)] = &[("index.ts", "// js")];
        let e = SkillEntry {
            platform: "pi",
            files: FILES,
            ..entry("dd-pup-pi", "extension", "")
        };
        let root = PathBuf::from("/tmp/proj");
        let paths = install_paths(&e, "claude-code", &root, None, false).unwrap();
        assert!(paths.is_empty(), "pi extension must not install on claude");
    }

    #[test]
    fn test_install_paths_extension_dir_override() {
        static FILES: &[(&str, &str)] = &[("index.ts", "// js")];
        let e = SkillEntry {
            platform: "pi",
            files: FILES,
            ..entry("dd-pup-pi", "extension", "")
        };
        let paths =
            install_paths(&e, "pi", &PathBuf::from("/unused"), Some("/tmp/out"), false).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].0, PathBuf::from("/tmp/out/dd-pup-pi/index.ts"));
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

    #[test]
    fn test_platform_extension_only_structural_invariant() {
        // Every platform must be consistently classified: extension-only
        // platforms must have at least one extensions dir, and non-extension-only
        // platforms must have at least one skills dir.
        for spec in PLATFORMS {
            if spec.is_extension_only() {
                assert!(
                    !spec.user_extensions.is_empty() || !spec.project_extensions.is_empty(),
                    "extension-only platform '{}' must have at least one extensions directory",
                    spec.name
                );
            } else {
                assert!(
                    !spec.user_skills.is_empty() || !spec.project_skills.is_empty(),
                    "non-extension-only platform '{}' must have at least one skills directory",
                    spec.name
                );
            }
        }
    }

    #[test]
    fn test_pi_is_not_extension_only() {
        assert!(!lookup_platform("pi").unwrap().is_extension_only());
    }

    #[test]
    fn test_is_extension_only_false_for_skill_platforms() {
        for name in &[
            "claude-code",
            "cursor",
            "codex",
            "opencode",
            "windsurf",
            "gemini-code",
        ] {
            assert!(
                !lookup_platform(name).unwrap().is_extension_only(),
                "{name} should not be extension-only"
            );
        }
    }

    #[test]
    fn extension_platform_fields_are_recognized() {
        for e in SKILLS {
            if e.entry_type == "extension" {
                assert!(
                    lookup_platform(e.platform).is_some(),
                    "extension '{}' has unrecognized platform slug '{}'",
                    e.name,
                    e.platform
                );
            }
        }
    }
}

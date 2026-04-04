// api.rs provides raw HTTP helpers used by test_commands.rs; not needed in production binary.
#[cfg(test)]
mod api;
mod auth;
mod client;
mod commands;
mod config;
#[cfg(not(target_arch = "wasm32"))]
mod extensions;
mod formatter;
#[cfg(not(target_arch = "wasm32"))]
mod runbooks;
mod skills;
#[cfg(not(target_arch = "wasm32"))]
mod tunnel;
mod useragent;
mod util;
mod version;

#[cfg(test)]
mod test_commands;

/// Shared test utilities — only compiled in test builds.
#[cfg(test)]
pub(crate) mod test_utils {
    use std::sync::Mutex;
    /// Serialize tests that mutate global env vars (PUP_MOCK_SERVER, DD_API_KEY, etc.).
    pub static ENV_LOCK: Mutex<()> = Mutex::new(());
}

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pup", version = version::VERSION, about = "Datadog API CLI")]
pub(crate) struct Cli {
    /// Output format (json, table, yaml, csv)
    #[arg(short, long, global = true, default_value = "json")]
    output: String,
    /// Auto-approve destructive operations
    #[arg(short = 'y', long = "yes", global = true)]
    yes: bool,
    /// Enable agent mode
    #[arg(long, global = true)]
    agent: bool,
    /// Block all write operations (create, update, delete)
    #[arg(long, global = true)]
    read_only: bool,
    /// Named org session (see 'pup auth login --org')
    #[arg(long, global = true)]
    org: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a local ACP server that proxies to Datadog Bits AI
    ///
    /// Spawns an HTTP server implementing the Agent Communication Protocol (ACP).
    /// ACP clients (AI agents, coding assistants) can connect and ask questions
    /// about your Datadog environment. Requests are forwarded to the Bits AI
    /// assistant and streamed back via ACP's SSE protocol.
    ///
    /// COMMANDS:
    ///   serve     Start the ACP server (default port 9099)
    ///
    /// EXAMPLES:
    ///   # Start on default port
    ///   pup acp serve
    ///
    ///   # Start on a custom port
    ///   pup acp serve --port 8080
    ///
    ///   # Start bound to all interfaces
    ///   pup acp serve --host 0.0.0.0
    ///
    /// AUTHENTICATION:
    ///   Requires OAuth2 (via 'pup auth login') or DD_API_KEY + DD_APP_KEY.
    ///
    /// ACP SPEC: https://agentcommunicationprotocol.dev/
    #[command(verbatim_doc_comment)]
    Acp {
        #[command(subcommand)]
        action: AcpActions,
    },
    /// Schema and guide for the datadog-agent daemon and AI coding assistants
    ///
    /// This command group covers two distinct purposes:
    ///
    ///   schema  — Outputs a JSON schema of all pup commands for AI coding
    ///             assistants (Claude, Copilot, etc.) to understand pup's API.
    ///
    ///   guide   — Displays an operational reference for the datadog-agent
    ///             daemon (the Datadog host agent that collects metrics, traces,
    ///             and logs). This is the `datadog-agent` binary, NOT an AI agent.
    ///
    /// In agent mode (auto-detected or via --agent / FORCE_AGENT_MODE=1),
    /// --help returns structured JSON schema instead of human-readable text.
    ///
    /// COMMANDS:
    ///   schema    Output the complete pup command schema as JSON (for AI assistants)
    ///   guide     Display the datadog-agent (Datadog-Agent) operational reference
    ///
    /// EXAMPLES:
    ///   # Get full JSON schema (all commands, flags, query syntax)
    ///   pup agent schema
    ///
    ///   # Get compact schema (command names and flags only, fewer tokens)
    ///   pup agent schema --compact
    ///
    ///   # Get the datadog-agent operational guide
    ///   pup agent guide
    #[command(name = "agent", verbatim_doc_comment)]
    Agent {
        #[command(subcommand)]
        action: AgentActions,
    },
    /// Create shortcuts for pup commands
    ///
    /// Aliases can be used to make shortcuts for pup commands or to compose multiple commands.
    ///
    /// Aliases are stored in ~/.config/pup/config.yml and can be used like any other pup command.
    ///
    /// EXAMPLES:
    ///   # Create an alias for a complex logs query
    ///   pup alias set prod-errors "logs search --query='status:error' --tag='env:prod'"
    ///
    ///   # Use the alias
    ///   pup prod-errors
    ///
    ///   # List all aliases
    ///   pup alias list
    ///
    ///   # Delete an alias
    ///   pup alias delete prod-errors
    ///
    ///   # Import aliases from a file
    ///   pup alias import aliases.yml
    #[command(verbatim_doc_comment)]
    Alias {
        #[command(subcommand)]
        action: AliasActions,
    },
    /// Manage API keys
    ///
    /// Manage Datadog API keys.
    ///
    /// API keys authenticate requests to Datadog APIs. This command manages API keys
    /// only (not application keys).
    ///
    /// CAPABILITIES:
    ///   • List API keys
    ///   • Get API key details
    ///   • Create new API keys
    ///   • Update API keys (name only)
    ///   • Delete API keys (requires confirmation)
    ///
    /// EXAMPLES:
    ///   # List all API keys
    ///   pup api-keys list
    ///
    ///   # Get API key details
    ///   pup api-keys get key-id
    ///
    ///   # Create new API key
    ///   pup api-keys create --name="Production Key"
    ///
    ///   # Delete an API key (with confirmation prompt)
    ///   pup api-keys delete key-id
    ///
    /// AUTHENTICATION:
    ///   Requires OAuth2 (via 'pup auth login') or a valid API key + Application key
    ///   combination. Note: You cannot use an API key to delete itself.
    #[command(name = "api-keys", verbatim_doc_comment)]
    ApiKeys {
        #[command(subcommand)]
        action: ApiKeyActions,
    },
    /// Manage APM services and entities
    ///
    /// Manage Datadog APM services and entities.
    ///
    /// APM (Application Performance Monitoring) tracks your services, operations, and dependencies
    /// to provide performance insights. This command provides access to dynamic operational data
    /// about traced services, datastores, queues, and other APM entities.
    ///
    /// DISTINCTION FROM SERVICE CATALOG:
    ///   • service-catalog: Static metadata registry (ownership, definitions, documentation)
    ///   • apm: Dynamic operational data (performance stats, traces, actual runtime behavior)
    ///
    ///   Service catalog shows "what services exist and who owns them"
    ///   APM shows "what's running, how it's performing, and what it's calling"
    ///
    /// CAPABILITIES:
    ///   • List services with performance statistics (requests, errors, latency)
    ///   • Query entities with rich metadata (services, datastores, queues, inferred services)
    ///   • List operations and resources (endpoints) for services
    ///   • View service dependencies and flow maps with performance metrics
    ///
    /// COMMAND GROUPS:
    ///   services       List and query APM services with performance data
    ///   entities       Query APM entities (services, datastores, queues, etc.)
    ///   dependencies   View service dependencies and call relationships
    ///   flow-map       Visualize service flow with performance metrics
    ///
    /// EXAMPLES:
    ///   # List services with stats
    ///   pup apm services stats --start $(date -d '1 hour ago' +%s) --end $(date +%s)
    ///
    ///   # Query entities with filtering
    ///   pup apm entities list --start $(date -d '1 hour ago' +%s) --end $(date +%s) --env prod
    ///
    ///   # View service dependencies
    ///   pup apm dependencies list --env prod --start $(date -d '1 hour ago' +%s) --end $(date +%s)
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys
    ///   (DD_API_KEY and DD_APP_KEY environment variables).
    #[command(verbatim_doc_comment)]
    Apm {
        #[command(subcommand)]
        action: ApmActions,
    },
    /// Manage App Builder applications
    ///
    /// Create, manage, and deploy custom internal tools using Datadog's
    /// low-code App Builder platform.
    ///
    /// CAPABILITIES:
    ///   • List, get, create, update, and delete apps
    ///   • Bulk delete multiple apps
    ///   • Publish and unpublish apps
    ///
    /// EXAMPLES:
    ///   # List all apps
    ///   pup app-builder list
    ///
    ///   # Filter apps by query
    ///   pup app-builder list --query="incident"
    ///
    ///   # Get app details
    ///   pup app-builder get <app-id>
    ///
    ///   # Create an app from file
    ///   pup app-builder create --body @app.json
    ///
    ///   # Update an app
    ///   pup app-builder update <app-id> --body @updated.json
    ///
    ///   # Delete an app
    ///   pup app-builder delete <app-id>
    ///
    ///   # Delete multiple apps
    ///   pup app-builder delete-batch --app-ids="id1,id2,id3"
    ///
    ///   # Publish an app
    ///   pup app-builder publish <app-id>
    ///
    ///   # Unpublish an app
    ///   pup app-builder unpublish <app-id>
    ///
    /// AUTHENTICATION:
    ///   Requires API key authentication with Actions API access.
    ///   The application key must be registered for Actions API access.
    #[command(name = "app-builder", verbatim_doc_comment)]
    AppBuilder {
        #[command(subcommand)]
        action: AppBuilderActions,
    },
    /// Manage application keys
    ///
    /// Manage Datadog application keys.
    ///
    /// Application keys, in conjunction with your organization's API key, give you
    /// full access to Datadog's API. Application keys are associated with the user
    /// who created them and have the same permissions and scopes as the user.
    ///
    /// CAPABILITIES:
    ///   • List your application keys (or all org keys with --all)
    ///   • Get application key details
    ///   • Create new application keys (with optional scopes)
    ///   • Update application key name or scopes
    ///   • Delete application keys (requires confirmation)
    ///
    /// EXAMPLES:
    ///   # List your application keys
    ///   pup app-keys list
    ///
    ///   # List all application keys in the org (requires API keys)
    ///   pup app-keys list --all
    ///
    ///   # Get application key details
    ///   pup app-keys get <app-key-id>
    ///
    ///   # Create a new application key
    ///   pup app-keys create --name="My Key"
    ///
    ///   # Create a scoped application key
    ///   pup app-keys create --name="Read Only" --scopes="dashboards_read,metrics_read"
    ///
    ///   # Update an application key name
    ///   pup app-keys update <app-key-id> --name="New Name"
    ///
    ///   # Delete an application key
    ///   pup app-keys delete <app-key-id>
    ///
    /// AUTHENTICATION:
    ///   Most commands use the current_user endpoints and support OAuth2 (via
    ///   'pup auth login'). The 'list --all' command uses the org-wide endpoint
    ///   and requires API + Application keys (DD_API_KEY + DD_APP_KEY).
    #[command(name = "app-keys", verbatim_doc_comment)]
    AppKeys {
        #[command(subcommand)]
        action: AppKeyActions,
    },
    /// Query audit logs
    ///
    /// Search and list audit logs for your Datadog organization.
    ///
    /// Audit logs track all actions performed in your Datadog organization,
    /// providing a complete audit trail for compliance and security.
    ///
    /// CAPABILITIES:
    ///   • Search audit logs with queries
    ///   • List recent audit events
    ///   • Filter by action, user, resource, outcome
    ///
    /// EXAMPLES:
    ///   # List recent audit logs
    ///   pup audit-logs list
    ///
    ///   # Search for specific user actions
    ///   pup audit-logs search --query="@usr.name:admin@example.com"
    ///
    ///   # Search for failed actions
    ///   pup audit-logs search --query="@evt.outcome:error"
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(name = "audit-logs", verbatim_doc_comment)]
    AuditLogs {
        #[command(subcommand)]
        action: AuditLogActions,
    },
    /// OAuth2 authentication commands
    ///
    /// Manage OAuth2 authentication with Datadog.
    ///
    /// OAuth2 provides secure, browser-based authentication with better security than
    /// API keys. It uses PKCE (Proof Key for Code Exchange) and Dynamic Client
    /// Registration for maximum security.
    ///
    /// AUTHENTICATION METHODS:
    ///   Pup supports two authentication methods:
    ///
    ///   1. OAuth2 (RECOMMENDED):
    ///      - Browser-based login flow
    ///      - Short-lived access tokens (1 hour)
    ///      - Automatic token refresh
    ///      - Per-installation credentials
    ///      - Granular OAuth scopes
    ///      - Better audit trail
    ///
    ///   2. API Keys (LEGACY):
    ///      - Environment variables (DD_API_KEY, DD_APP_KEY)
    ///      - Long-lived credentials
    ///      - Organization-wide access
    ///      - Manual rotation required
    ///
    /// OAUTH2 FEATURES:
    ///   • PKCE Protection (S256): Prevents authorization code interception
    ///   • Dynamic Client Registration: Unique credentials per installation
    ///   • CSRF Protection: State parameter validation
    ///   • Secure Storage: Tokens stored in ~/.config/pup/ with 0600 permissions
    ///   • Auto Refresh: Tokens refresh automatically before expiration
    ///   • Multi-Site: Separate credentials for each Datadog site
    ///
    /// COMMANDS:
    ///   login       Authenticate via browser with OAuth2
    ///   logout      Clear all stored credentials
    ///   refresh     Manually refresh access token
    ///   status      Check current authentication status
    ///   test        Test connection and credentials
    ///
    /// OAUTH2 SCOPES:
    ///   The following scopes are requested during login:
    ///   • Dashboards: dashboards_read, dashboards_write
    ///   • Monitors: monitors_read, monitors_write, monitors_downtime
    ///   • APM: apm_read
    ///   • SLOs: slos_read, slos_write, slos_corrections
    ///   • Incidents: incident_read, incident_write
    ///   • Synthetics: synthetics_read, synthetics_write
    ///   • Security: security_monitoring_*
    ///   • RUM: rum_apps_read, rum_apps_write
    ///   • Infrastructure: hosts_read
    ///   • Users: user_access_read, user_self_profile_read
    ///   • Cases: cases_read, cases_write
    ///   • Events: events_read
    ///   • Logs: logs_read_data, logs_read_index_data
    ///   • Metrics: metrics_read, timeseries_query
    ///   • Usage: usage_read
    ///
    /// EXAMPLES:
    ///   # Login with OAuth2
    ///   pup auth login
    ///
    ///   # Check authentication status
    ///   pup auth status
    ///
    ///   # Refresh access token
    ///   pup auth refresh
    ///
    ///   # Logout and clear credentials
    ///   pup auth logout
    ///
    ///   # Test connection and credentials
    ///   pup auth test
    ///
    ///   # Login to different Datadog site
    ///   pup auth login --site datadoghq.eu
    ///   DD_SITE=datadoghq.eu pup auth login
    ///
    ///   # Login to a child org (multi-org support)
    ///   pup auth login --org prod-child
    ///   pup auth login --org staging-child
    ///
    ///   # List all stored org sessions
    ///   pup auth list
    ///
    ///   # Use a named org session for any command
    ///   pup monitors list --org prod-child
    ///   DD_ORG=prod-child pup metrics query --query "avg:system.cpu.user{*}"
    ///
    ///   # Logout a specific org session
    ///   pup auth logout --org staging-child
    ///
    /// MULTI-SITE SUPPORT:
    ///   Each Datadog site maintains separate credentials:
    ///
    ///   pup auth login --site datadoghq.com     # US1 (default)
    ///   pup auth login --site datadoghq.eu      # EU1
    ///   pup auth login --site us3.datadoghq.com # US3
    ///   pup auth login --site us5.datadoghq.com # US5
    ///   pup auth login --site ap1.datadoghq.com # AP1
    ///   DD_SITE=datadoghq.com pup auth login     # US1 (via env var)
    ///   DD_SITE=datadoghq.eu pup auth login      # EU1 (via env var)
    ///
    /// MULTI-ORG SUPPORT:
    ///   Datadog parent/child sub-organizations each get their own session:
    ///
    ///   pup auth login --org prod-child    # saves tokens_<site>_prod_child
    ///   pup auth login --org staging-child # saves tokens_<site>_staging_child
    ///   pup monitors list --org prod-child # uses prod-child session
    ///
    /// TOKEN STORAGE:
    ///   Credentials are stored in:
    ///   • ~/.config/pup/tokens_<site>.json          - default OAuth2 tokens
    ///   • ~/.config/pup/tokens_<site>_<org>.json    - named org tokens
    ///   • ~/.config/pup/client_<site>.json          - DCR client credentials (shared)
    ///   • ~/.config/pup/sessions.json               - session registry (no secrets)
    ///
    ///   File permissions are set to 0600 (read/write owner only).
    ///
    /// SECURITY:
    ///   • Tokens never logged or printed
    ///   • PKCE prevents code interception
    ///   • State parameter prevents CSRF
    ///   • Unique client per installation
    ///   • Tokens auto-refresh before expiration
    ///
    /// For detailed OAuth2 documentation, see: docs/OAUTH2.md
    #[command(verbatim_doc_comment)]
    Auth {
        #[command(subcommand)]
        action: AuthActions,
    },
    /// Ask Datadog Bits AI a question in natural language
    ///
    /// Query your Datadog environment using plain English.
    ///
    /// Bits AI answers questions about infrastructure, metrics, logs, traces, and
    /// incidents by connecting directly to your Datadog account. Responses are
    /// streamed to the terminal as they arrive.
    ///
    /// CAPABILITIES:
    ///   • Ask questions about your infrastructure, services, and health
    ///   • Investigate incidents without leaving the terminal
    ///   • Get summaries of logs, metrics, APM, and more
    ///   • Use in scripts or remote SSH sessions
    ///
    /// EXAMPLES:
    ///   # Ask a natural-language question (streaming by default)
    ///   pup bits ask "what's causing high latency in the checkout service?"
    ///
    ///   # Summarize current incidents
    ///   pup bits ask "summarize active incidents"
    ///
    ///   # Start an interactive conversation
    ///   pup bits ask --interactive
    ///
    ///   # Start interactive with an opening question
    ///   pup bits ask --interactive "walk me through the recent incidents"
    ///
    ///   # Target a specific Bits AI agent
    ///   pup bits ask --agent-id <uuid> "show errors in production from the last hour"
    ///
    ///   # Disable streaming (collect the full response before printing)
    ///   pup bits ask --no-stream "which endpoints are slowest?"
    ///
    /// AUTHENTICATION:
    ///   Requires OAuth2 (via 'pup auth login') or a valid API key + Application key.
    ///   OAuth2 is recommended for interactive use.
    #[command(verbatim_doc_comment)]
    Bits {
        #[command(subcommand)]
        action: BitsActions,
    },
    /// Manage case management cases and projects
    ///
    /// Manage Datadog Case Management for tracking and resolving issues.
    ///
    /// Case Management provides structured workflows for handling customer issues,
    /// bugs, and internal requests. Cases can be organized into projects with
    /// custom attributes, priorities, and assignments.
    ///
    /// CAPABILITIES:
    ///   • Create and manage cases with custom attributes
    ///   • Search and filter cases
    ///   • Assign cases to users
    ///   • Archive/unarchive cases
    ///   • Manage projects
    ///   • Add comments and track timelines
    ///
    /// CASE PRIORITIES:
    ///   • NOT_DEFINED: No priority set
    ///   • P1: Critical priority
    ///   • P2: High priority
    ///   • P3: Medium priority
    ///   • P4: Low priority
    ///   • P5: Lowest priority
    ///
    /// EXAMPLES:
    ///   # Search cases
    ///   pup cases search --query="bug"
    ///
    ///   # Get case details
    ///   pup cases get case-123
    ///
    ///   # Create a new case
    ///   pup cases create --title="Bug report" --type-id="type-uuid" --priority=P2
    ///
    ///   # List projects
    ///   pup cases projects list
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys.
    #[command(verbatim_doc_comment)]
    Cases {
        #[command(subcommand)]
        action: CaseActions,
    },
    /// Manage change requests
    #[command(name = "change-requests")]
    ChangeManagement {
        #[command(subcommand)]
        action: ChangeManagementActions,
    },
    /// Manage CI/CD visibility
    ///
    /// Manage Datadog CI/CD visibility for pipeline and test monitoring.
    ///
    /// CI/CD Visibility provides insights into your CI/CD pipelines, tracking pipeline
    /// performance, test results, and failure patterns.
    ///
    /// CAPABILITIES:
    ///   • List and search CI pipelines with filtering
    ///   • Get detailed pipeline execution information
    ///   • Aggregate pipeline events for analytics
    ///   • Track pipeline performance metrics
    ///   • Query CI test events and flaky tests
    ///
    /// EXAMPLES:
    ///   # List recent pipelines
    ///   pup cicd pipelines list
    ///
    ///   # Get pipeline details
    ///   pup cicd pipelines get --pipeline-id="abc-123"
    ///
    ///   # Search for failed pipelines
    ///   pup cicd events search --query="@ci.status:error" --from="1h"
    ///
    ///   # Aggregate by status
    ///   pup cicd events aggregate --query="*" --compute="count" --group-by="@ci.status"
    ///
    ///   # List recent test events
    ///   pup cicd tests list --from="1h"
    ///
    ///   # Search flaky tests
    ///   pup cicd flaky-tests search --query="flaky_test_state:active"
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys.
    #[command(verbatim_doc_comment)]
    Cicd {
        #[command(subcommand)]
        action: CicdActions,
    },
    /// Manage cloud integrations
    ///
    /// Manage cloud provider integrations (AWS, GCP, Azure).
    ///
    /// Cloud integrations collect metrics and logs from your cloud providers
    /// and provide insights into cloud resource usage and performance.
    ///
    /// CAPABILITIES:
    ///   • Manage AWS integrations
    ///   • Manage GCP integrations
    ///   • Manage Azure integrations
    ///   • View cloud metrics
    ///
    /// EXAMPLES:
    ///   # List AWS integrations
    ///   pup cloud aws list
    ///
    ///   # List GCP integrations
    ///   pup cloud gcp list
    ///
    ///   # List Azure integrations
    ///   pup cloud azure list
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(verbatim_doc_comment)]
    Cloud {
        #[command(subcommand)]
        action: CloudActions,
    },
    /// Query code coverage data
    ///
    /// Query code coverage summaries from Datadog Test Optimization.
    ///
    /// Code coverage provides branch-level and commit-level coverage summaries
    /// for your repositories.
    ///
    /// EXAMPLES:
    ///   # Get branch coverage summary
    ///   pup code-coverage branch-summary --repo="github.com/org/repo" --branch="main"
    ///
    ///   # Get commit coverage summary
    ///   pup code-coverage commit-summary --repo="github.com/org/repo" --commit="abc123"
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(name = "code-coverage", verbatim_doc_comment)]
    CodeCoverage {
        #[command(subcommand)]
        action: CodeCoverageActions,
    },
    /// Generate shell completions
    ///
    /// Generate shell completions for pup.
    ///
    /// Shell completions enable tab-completion of commands, subcommands, and flags
    /// in your terminal. Use --install to automatically write completions to the
    /// standard location for your shell, or pipe to stdout to manage it yourself.
    ///
    /// SUPPORTED SHELLS:
    ///   • bash: Bourne Again Shell
    ///   • zsh: Z Shell
    ///   • fish: Friendly Interactive Shell
    ///   • elvish: Elvish Shell
    ///   • powershell: PowerShell
    ///
    /// EXAMPLES:
    ///   # Auto-install completions (recommended)
    ///   pup completions bash --install
    ///   pup completions zsh --install
    ///   pup completions fish --install
    ///   pup completions powershell --install
    ///
    ///   # Or pipe to a custom location
    ///   pup completions bash > /etc/bash_completion.d/pup
    ///   pup completions zsh > ~/.zfunc/_pup
    ///   pup completions fish > ~/.config/fish/completions/pup.fish
    #[cfg(not(target_arch = "wasm32"))]
    #[command(verbatim_doc_comment)]
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::Shell,
        /// Install completions to the default location for the shell
        #[arg(long)]
        install: bool,
    },
    /// Query running containers and container images
    ///
    /// List and filter containers and container images monitored by Datadog.
    ///
    /// CAPABILITIES:
    ///   • List running containers with filtering and grouping
    ///   • List container images with filtering and grouping
    ///
    /// EXAMPLES:
    ///   # List all containers
    ///   pup containers list
    ///
    ///   # List containers filtered by tag
    ///   pup containers list --filter-tags="env:production"
    ///
    ///   # List container images
    ///   pup containers images list
    ///
    /// AUTHENTICATION:
    ///   Requires OAuth2 (via 'pup auth login') or valid API + Application keys.
    #[command(verbatim_doc_comment)]
    Containers {
        #[command(subcommand)]
        action: ContainerActions,
    },
    /// Manage cost and billing data
    ///
    /// Query cost management and billing information.
    ///
    /// Access projected costs, cost attribution by tags, and organizational cost breakdowns.
    /// Cost data is typically available with 12-24 hour delay.
    ///
    /// CAPABILITIES:
    ///   • View projected end-of-month costs
    ///   • Get cost attribution by tags and teams
    ///   • Query historical and estimated costs by organization
    ///
    /// EXAMPLES:
    ///   # Get projected costs for current month
    ///   pup cost projected
    ///
    ///   # Get cost attribution by team tag
    ///   pup cost attribution --start-month=2024-01 --fields=team
    ///
    ///   # Get actual costs for a specific month
    ///   pup cost by-org --start-month=2024-01
    ///
    /// AUTHENTICATION:
    ///   Requires OAuth2 (via 'pup auth login') or valid API + Application keys.
    ///   Cost management features require billing:read permissions.
    #[command(verbatim_doc_comment)]
    Cost {
        #[command(subcommand)]
        action: CostActions,
    },
    /// Manage dashboards
    ///
    /// Manage Datadog dashboards for data visualization and monitoring.
    ///
    /// Dashboards provide customizable views of your metrics, logs, traces, and other
    /// observability data through various widget types including timeseries, heatmaps,
    /// tables, and more.
    ///
    /// CAPABILITIES:
    ///   • List all dashboards with metadata
    ///   • Get detailed dashboard configuration including all widgets
    ///   • Delete dashboards (requires confirmation unless --yes flag is used)
    ///   • View dashboard layouts, templates, and template variables
    ///
    /// DASHBOARD TYPES:
    ///   • Timeboard: Grid-based layout with synchronized timeseries graphs
    ///   • Screenboard: Flexible free-form layout with any widget placement
    ///
    /// WIDGET TYPES:
    ///   • Timeseries: Line, area, or bar graphs over time
    ///   • Query value: Single numeric value with thresholds
    ///   • Table: Tabular data with columns
    ///   • Heatmap: Heat map visualization
    ///   • Toplist: Top N values
    ///   • Change: Value change over time
    ///   • Event timeline: Event stream
    ///   • Free text: Markdown text and images
    ///   • Group: Container for organizing widgets
    ///   • Note: Text annotations
    ///   • Service map: Service dependency visualization
    ///   • And many more...
    ///
    /// EXAMPLES:
    ///   # List all dashboards
    ///   pup dashboards list
    ///
    ///   # Get detailed dashboard configuration
    ///   pup dashboards get abc-def-123
    ///
    ///   # Get dashboard and save to file
    ///   pup dashboards get abc-def-123 > dashboard.json
    ///
    ///   # Delete a dashboard with confirmation
    ///   pup dashboards delete abc-def-123
    ///
    ///   # Delete a dashboard without confirmation (automation)
    ///   pup dashboards delete abc-def-123 --yes
    ///
    /// TEMPLATE VARIABLES:
    ///   Dashboards can include template variables for dynamic filtering:
    ///   • $env: Environment filter
    ///   • $service: Service filter
    ///   • $host: Host filter
    ///   • Custom variables based on tags
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys
    ///   (DD_API_KEY and DD_APP_KEY environment variables).
    #[command(verbatim_doc_comment)]
    Dashboards {
        #[command(subcommand)]
        action: DashboardActions,
    },
    /// Manage data governance
    ///
    /// Manage data governance, sensitive data scanning, and data deletion.
    ///
    /// CAPABILITIES:
    ///   • Manage sensitive data scanner
    ///   • Configure data deletion policies
    ///   • View scan results
    ///   • Manage scanning rules
    ///
    /// EXAMPLES:
    ///   # List scanning rules
    ///   pup data-governance scanner rules list
    ///
    ///   # Get rule details
    ///   pup data-governance scanner rules get rule-id
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(name = "data-governance", verbatim_doc_comment)]
    DataGovernance {
        #[command(subcommand)]
        action: DataGovActions,
    },
    /// Query Datadog data using DDSQL (Datadog SQL)
    ///
    /// DDSQL lets you query metrics, logs, and reference tables using SQL syntax.
    ///
    /// COMMANDS:
    ///   table        Execute query and return table data (supports -o json/yaml/table/csv)
    ///   time-series  Execute query and return time series data
    ///
    /// EXAMPLES:
    ///   pup ddsql table --query "SELECT * FROM reference_tables.offices_ips LIMIT 5"
    ///   pup ddsql table --query "SELECT * FROM reference_tables.offices_ips" -o csv > results.csv
    ///   pup ddsql time-series --query "SELECT avg(system.cpu.user) FROM metrics GROUP BY host" --from 1h --interval 300000
    ///
    /// AUTHENTICATION:
    ///   Requires OAuth2 (via 'pup auth login') or API key + Application key.
    #[command(verbatim_doc_comment)]
    Ddsql {
        #[command(subcommand)]
        action: DdsqlActions,
    },
    /// Manage Live Debugger
    ///
    /// Create and manage Live Debugger log probes.
    ///
    /// CAPABILITIES:
    ///   • List log probes
    ///   • Get probe details
    ///   • Create log probes with conditions and templates
    ///   • Delete log probes
    ///   • Watch probe events
    ///
    /// EXAMPLES:
    ///   # List all log probes
    ///   pup debugger probes list
    ///
    ///   # Create a log probe
    ///   pup debugger probes create --service my-svc --env staging --probe-location com.example.MyClass:myMethod
    ///
    ///   # Create a log probe with custom budget and TTL
    ///   pup debugger probes create --service my-svc --env staging --probe-location com.example.MyClass:myMethod --budget 500 --ttl 2h
    ///
    ///   # Watch probe events
    ///   pup debugger probes watch <PROBE_ID> --timeout 60
    ///
    /// AUTHENTICATION:
    ///   Requires API keys (DD_API_KEY + DD_APP_KEY).
    #[command(verbatim_doc_comment)]
    Debugger {
        #[command(subcommand)]
        action: DebuggerActions,
    },
    /// Manage Deployment Gates
    ///
    /// Deployment Gates reduce the likelihood and impact of incidents caused by deployments.
    ///
    /// COMMANDS:
    ///   gates list                  List all deployment gates
    ///   gates get <id>              Get a deployment gate by ID
    ///   gates create --file <f>     Create a deployment gate from JSON file
    ///   gates update <id> --file    Update a deployment gate
    ///   gates delete <id>           Delete a deployment gate
    ///   evaluations get <id>        Get a deployment gates evaluation result
    ///   evaluations trigger --file  Trigger a deployment gates evaluation
    ///   rules list <gate-id>        List rules for a deployment gate
    ///   rules get <gate-id> <id>    Get a specific deployment rule
    ///   rules create <gate-id> -f   Create a deployment rule
    ///   rules update <gate-id> <id> Update a deployment rule
    ///   rules delete <gate-id> <id> Delete a deployment rule
    ///
    /// EXAMPLES:
    ///   pup deployment-gates gates list
    ///   pup deployment-gates gates get my-gate-id
    ///   pup deployment-gates gates create --file gate.json
    ///   pup deployment-gates evaluations trigger --file eval.json
    ///   pup deployment-gates rules list my-gate-id
    ///
    /// AUTHENTICATION:
    ///   Requires OAuth2 (via 'pup auth login') or DD_API_KEY + DD_APP_KEY.
    #[command(name = "deployment-gates", verbatim_doc_comment)]
    DeploymentGates {
        #[command(subcommand)]
        action: DeploymentGatesActions,
    },
    /// Manage monitor downtimes
    ///
    /// Manage downtimes to silence monitors during maintenance windows.
    ///
    /// Downtimes prevent monitors from alerting during scheduled maintenance,
    /// deployments, or other planned events.
    ///
    /// CAPABILITIES:
    ///   • List all downtimes
    ///   • Get downtime details
    ///   • Create new downtimes
    ///   • Update existing downtimes
    ///   • Cancel downtimes
    ///
    /// EXAMPLES:
    ///   # List all active downtimes
    ///   pup downtime list
    ///
    ///   # Get downtime details
    ///   pup downtime get abc-123-def
    ///
    ///   # Cancel a downtime
    ///   pup downtime cancel abc-123-def
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(verbatim_doc_comment)]
    Downtime {
        #[command(subcommand)]
        action: DowntimeActions,
    },
    /// Manage error tracking
    ///
    /// Manage error tracking for application errors and crashes.
    ///
    /// Error tracking automatically groups and prioritizes errors from
    /// your applications to help you identify and fix critical issues.
    ///
    /// CAPABILITIES:
    ///   • Search error issues with filtering and sorting
    ///   • Get detailed information about a specific issue
    ///
    /// EXAMPLES:
    ///   # Search error issues
    ///   pup error-tracking issues search
    ///
    ///   # Get issue details
    ///   pup error-tracking issues get issue-id
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(name = "error-tracking", verbatim_doc_comment)]
    ErrorTracking {
        #[command(subcommand)]
        action: ErrorTrackingActions,
    },
    /// Manage Datadog events
    ///
    /// Query and search Datadog events.
    ///
    /// Events represent important occurrences in your infrastructure such as
    /// deployments, configuration changes, alerts, and custom events.
    ///
    /// CAPABILITIES:
    ///   • List recent events
    ///   • Search events with queries
    ///   • Get event details
    ///
    /// EXAMPLES:
    ///   # List recent events
    ///   pup events list
    ///
    ///   # Search for deployment events
    ///   pup events search --query="tags:deployment"
    ///
    ///   # Get specific event
    ///   pup events get 1234567890
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(verbatim_doc_comment)]
    Events {
        #[command(subcommand)]
        action: EventActions,
    },
    /// Manage pup extensions
    ///
    /// Install, list, remove, and upgrade pup extensions.
    /// Extensions are external executables that add new subcommands to pup.
    ///
    /// COMMANDS:
    ///   list      List installed extensions
    ///   install   Install an extension from a local path or GitHub repo
    ///   remove    Remove an installed extension
    ///   upgrade   Upgrade an extension to the latest version
    ///
    /// EXAMPLES:
    ///   pup extension list
    ///   pup extension install --local /path/to/pup-my-tool
    ///   pup extension remove my-tool
    #[cfg(not(target_arch = "wasm32"))]
    #[command(verbatim_doc_comment)]
    Extension {
        #[command(subcommand)]
        action: ExtensionActions,
    },
    /// Manage feature flags
    ///
    /// Manage Datadog feature flags and their environments.
    ///
    /// Feature flags let you control the rollout of features to your users.
    /// This command group covers flag lifecycle management as well as
    /// environment configuration and per-environment enable/disable controls.
    ///
    /// COMMAND GROUPS:
    ///   flags           Manage feature flag definitions (list, get, create, update, archive, unarchive)
    ///   environments    Manage feature flag environments (list, get, create, update, delete)
    ///
    /// DIRECT COMMANDS:
    ///   enable          Enable a feature flag in an environment
    ///   disable         Disable a feature flag in an environment
    ///
    /// EXAMPLES:
    ///   # List all feature flags
    ///   pup feature-flags flags list
    ///
    ///   # Get a feature flag
    ///   pup feature-flags flags get <flag-id>
    ///
    ///   # Create a feature flag from file
    ///   pup feature-flags flags create --file=flag.json
    ///
    ///   # Update a feature flag
    ///   pup feature-flags flags update <flag-id> --file=flag.json
    ///
    ///   # Archive a feature flag
    ///   pup feature-flags flags archive <flag-id>
    ///
    ///   # List environments
    ///   pup feature-flags environments list
    ///
    ///   # Enable a flag in an environment
    ///   pup feature-flags enable <flag-id> <env-id>
    ///
    ///   # Disable a flag in an environment
    ///   pup feature-flags disable <flag-id> <env-id>
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys
    ///   (DD_API_KEY and DD_APP_KEY environment variables).
    #[command(name = "feature-flags", verbatim_doc_comment)]
    FeatureFlags {
        #[command(subcommand)]
        action: FeatureFlagActions,
    },
    /// Manage Fleet Automation
    ///
    /// Manage Fleet Automation for remote agent configuration and deployment.
    ///
    /// Fleet Automation provides centralized management of Datadog Agents across
    /// your infrastructure, enabling remote configuration changes, scheduled
    /// deployments, and agent lifecycle management.
    ///
    /// CAPABILITIES:
    ///   • List and inspect fleet agents
    ///   • Manage deployment configurations
    ///   • Schedule configuration changes
    ///   • Monitor agent health and status
    ///
    /// EXAMPLES:
    ///   # List fleet agents
    ///   pup fleet agents list
    ///
    ///   # Get agent details
    ///   pup fleet agents get <agent-key>
    ///
    ///   # List deployments
    ///   pup fleet deployments list
    ///
    ///   # Deploy a configuration change
    ///   pup fleet deployments configure --file=config.json
    ///
    ///   # List schedules
    ///   pup fleet schedules list
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys
    ///   (DD_API_KEY and DD_APP_KEY environment variables).
    #[command(verbatim_doc_comment)]
    Fleet {
        #[command(subcommand)]
        action: FleetActions,
    },
    /// Manage High Availability Multi-Region (HAMR)
    ///
    /// Manage Datadog High Availability Multi-Region (HAMR) connections.
    ///
    /// HAMR provides high availability and multi-region failover capabilities
    /// for your Datadog organization.
    ///
    /// EXAMPLES:
    ///   # Get HAMR connection status
    ///   pup hamr connections get
    ///
    ///   # Create a HAMR connection
    ///   pup hamr connections create --file=connection.json
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(verbatim_doc_comment)]
    Hamr {
        #[command(subcommand)]
        action: HamrActions,
    },
    /// Internal Developer Portal — agent-native context layer
    ///
    /// Retrieve service context, ownership, health, dependencies, and
    /// suggested next actions from the Datadog Service Catalog / IDP.
    ///
    /// CAPABILITIES:
    ///   • Get a full context summary for any entity (assist)
    ///   • Find entities by name or query (find)
    ///   • Resolve ownership and on-call (owner)
    ///   • Show upstream/downstream dependencies (deps)
    ///   • Register a service definition from YAML (register)
    ///
    /// EXAMPLES:
    ///   # Get full context for a service
    ///   pup idp assist catalog-http
    ///
    ///   # Find entities matching a query
    ///   pup idp find "catalog"
    ///
    ///   # Who owns this service?
    ///   pup idp owner catalog-http
    ///
    ///   # Show dependencies
    ///   pup idp deps catalog-http
    ///
    ///   # Register a service definition
    ///   pup idp register service.datadog.yaml
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys
    ///   (DD_API_KEY and DD_APP_KEY environment variables).
    #[command(verbatim_doc_comment)]
    Idp {
        #[command(subcommand)]
        action: IdpActions,
    },
    /// Manage incidents
    ///
    /// Manage Datadog incidents for incident response and tracking.
    ///
    /// Incidents provide a centralized place to track, communicate, and resolve issues
    /// affecting your services. They integrate with monitors, timelines, tasks, and
    /// postmortems.
    ///
    /// CAPABILITIES:
    ///   • List all incidents with filtering and pagination
    ///   • Get detailed incident information including timeline, tasks, and attachments
    ///   • View incident severity, status, and customer impact
    ///   • Track incident response and resolution
    ///
    /// INCIDENT SEVERITIES:
    ///   • SEV-1: Critical impact - complete service outage
    ///   • SEV-2: High impact - major functionality unavailable
    ///   • SEV-3: Moderate impact - partial functionality affected
    ///   • SEV-4: Low impact - minor issues
    ///   • SEV-5: Minimal impact - cosmetic issues
    ///
    /// INCIDENT STATES:
    ///   • active: Incident is ongoing, actively being worked
    ///   • stable: Incident is under control but not fully resolved
    ///   • resolved: Incident has been resolved
    ///   • completed: Post-incident tasks completed (postmortem, etc.)
    ///
    /// EXAMPLES:
    ///   # List all incidents
    ///   pup incidents list
    ///
    ///   # Get detailed incident information
    ///   pup incidents get abc-123-def
    ///
    ///   # Get incident and view timeline
    ///   pup incidents get abc-123-def | jq '.data.timeline'
    ///
    ///   # Check incident status
    ///   pup incidents get abc-123-def | jq '{status: .data.status, severity: .data.severity}'
    ///
    /// INCIDENT FIELDS:
    ///   • id: Incident ID
    ///   • title: Incident title
    ///   • description: Detailed description
    ///   • severity: Severity level (SEV-1 through SEV-5)
    ///   • state: Incident state (active, stable, resolved, completed)
    ///   • customer_impacted: Whether customers are affected
    ///   • customer_impact_scope: Description of customer impact
    ///   • detected_at: When incident was detected
    ///   • created_at: When incident was created in Datadog
    ///   • resolved_at: When incident was resolved
    ///   • commander: Incident commander (user)
    ///   • responders: Team members responding
    ///   • attachments: Related documents, runbooks, etc.
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys
    ///   (DD_API_KEY and DD_APP_KEY environment variables).
    #[command(verbatim_doc_comment)]
    Incidents {
        #[command(subcommand)]
        action: IncidentActions,
    },
    /// Manage infrastructure monitoring
    ///
    /// Query and manage infrastructure hosts and metrics.
    ///
    /// CAPABILITIES:
    ///   • List hosts in your infrastructure
    ///   • Get host details and metrics
    ///   • Search hosts by tags or status
    ///   • Monitor host health
    ///
    /// EXAMPLES:
    ///   # List all hosts
    ///   pup infrastructure hosts list
    ///
    ///   # Search for hosts by tag
    ///   pup infrastructure hosts list --filter="env:production"
    ///
    ///   # Get host details
    ///   pup infrastructure hosts get my-host
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(verbatim_doc_comment)]
    Infrastructure {
        #[command(subcommand)]
        action: InfraActions,
    },
    /// Manage third-party integrations
    ///
    /// Manage third-party integrations with external services.
    ///
    /// Integrations connect Datadog with external services like Slack, PagerDuty,
    /// Jira, and many others for notifications and workflow automation.
    ///
    /// CAPABILITIES:
    ///   • List Slack integrations
    ///   • Manage PagerDuty integrations
    ///   • Configure webhook integrations
    ///   • View integration status
    ///
    /// EXAMPLES:
    ///   # List Slack integrations
    ///   pup integrations slack list
    ///
    ///   # List PagerDuty integrations
    ///   pup integrations pagerduty list
    ///
    ///   # List webhooks
    ///   pup integrations webhooks list
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(verbatim_doc_comment)]
    Integrations {
        #[command(subcommand)]
        action: IntegrationActions,
    },
    /// Manage Bits AI investigations
    ///
    /// Manage Bits AI investigations.
    ///
    /// Bits AI investigations allow you to trigger automated root cause analysis
    /// for monitor alerts.
    ///
    /// CAPABILITIES:
    ///   • Trigger a new investigation (monitor alert)
    ///   • Get investigation details by ID
    ///   • List investigations with optional filters
    ///
    /// EXAMPLES:
    ///   # Trigger investigation from a monitor alert
    ///   pup investigations trigger --type=monitor_alert --monitor-id=123456 --event-id="evt-abc" --event-ts=1706918956000
    ///
    ///   # Get investigation details
    ///   pup investigations get <investigation-id>
    ///
    ///   # List investigations
    ///   pup investigations list --page-limit=20
    ///
    /// AUTHENTICATION:
    ///   Requires OAuth2 (via 'pup auth login') or a valid API key + Application key.
    #[command(verbatim_doc_comment)]
    Investigations {
        #[command(subcommand)]
        action: InvestigationActions,
    },
    /// Manage LLM Observability projects, experiments, and datasets
    ///
    /// Manage LLM Observability resources for AI/ML application monitoring.
    ///
    /// CAPABILITIES:
    ///   • Create and list LLM Obs projects
    ///   • Create, list, update, and delete experiments
    ///   • Create and list datasets within a project
    ///
    /// EXAMPLES:
    ///   pup llm-obs projects list
    ///   pup llm-obs experiments list
    ///   pup llm-obs datasets list --project-id=my-project
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(name = "llm-obs", verbatim_doc_comment)]
    LlmObs {
        #[command(subcommand)]
        action: LlmObsActions,
    },
    /// Search and analyze logs
    ///
    /// Search and analyze log data with flexible queries and time ranges.
    ///
    /// The logs command provides comprehensive access to Datadog's log management capabilities
    /// including search, querying, aggregation, archives management, custom destinations,
    /// log-based metrics, and restriction queries.
    ///
    /// CAPABILITIES:
    ///   • Search logs with flexible queries (v1 API)
    ///   • Query and aggregate logs (v2 API)
    ///   • List logs with filtering (v2 API)
    ///   • Search across different storage tiers (indexes, online-archives, flex)
    ///   • Manage log archives (CRUD operations)
    ///   • Manage custom destinations for logs
    ///   • Create and manage log-based metrics
    ///   • Configure restriction queries for access control
    ///
    /// STORAGE TIERS:
    ///   Datadog logs can be stored in different tiers with different performance and cost characteristics:
    ///   • indexes - Standard indexed logs (default, real-time searchable)
    ///   • online-archives - Rehydrated logs from archives (slower queries, lower cost)
    ///   • flex - Flex logs (cost-optimized storage tier, balanced performance)
    ///
    /// LOG QUERY SYNTAX:
    ///   Logs use a query language similar to web search:
    ///   • status:error - Match by status
    ///   • service:web-app - Match by service
    ///   • @user.id:12345 - Match by attribute
    ///   • host:i-* - Wildcard matching
    ///   • "exact phrase" - Exact phrase matching
    ///   • AND, OR, NOT - Boolean operators
    ///
    /// TIME RANGES:
    ///   Supported time formats:
    ///   • Relative short: 1h, 30m, 7d, 5s, 1w
    ///   • Relative long: 5min, 5minutes, 2hr, 2hours, 3days, 1week
    ///   • With spaces: "5 minutes", "2 hours"
    ///   • With minus: -5m, -2h (treated same as 5m, 2h)
    ///   • Absolute: Unix timestamp in milliseconds
    ///   • RFC3339: 2024-01-01T00:00:00Z
    ///   • now: Current time
    ///
    /// EXAMPLES:
    ///   # Search for error logs in the last hour
    ///   pup logs search --query="status:error" --from="1h"
    ///
    ///   # Search Flex logs specifically
    ///   pup logs search --query="status:error" --from="1h" --storage="flex"
    ///
    ///   # Query logs from a specific service
    ///   pup logs query --query="service:web-app" --from="4h" --to="now"
    ///
    ///   # Query online archives
    ///   pup logs query --query="service:web-app" --from="30d" --storage="online-archives"
    ///
    ///   # Aggregate logs by status
    ///   pup logs aggregate --query="*" --compute="count" --group-by="status"
    ///
    ///   # Multiple computes in one query (comma-separated)
    ///   pup logs aggregate --query="*" --compute="count,avg(@duration),percentile(@duration, 95)"
    ///
    ///   # Multiple group-by dimensions (comma-separated)
    ///   pup logs aggregate --query="*" --compute="count" --group-by="service,status"
    ///
    ///   # List log archives
    ///   pup logs archives list
    ///
    ///   # Get specific archive details
    ///   pup logs archives get "my-archive-id"
    ///
    ///   # List log-based metrics
    ///   pup logs metrics list
    ///
    ///   # Create a log-based metric
    ///   pup logs metrics create --name="error.count" --query="status:error"
    ///
    ///   # List custom destinations
    ///   pup logs custom-destinations list
    ///
    ///   # List restriction queries
    ///   pup logs restriction-queries list
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys
    ///   (DD_API_KEY and DD_APP_KEY environment variables).
    #[command(verbatim_doc_comment)]
    Logs {
        #[command(subcommand)]
        action: LogActions,
    },
    /// Query and manage metrics
    ///
    /// Query time-series metrics, list available metrics, and manage metric metadata.
    ///
    /// Metrics are the foundation of monitoring in Datadog. This command provides
    /// comprehensive access to query metrics data, list available metrics, manage
    /// metadata, and submit custom metrics.
    ///
    /// CAPABILITIES:
    ///   • Query time-series metrics data with flexible time ranges
    ///   • List all available metrics with optional filtering
    ///   • Get and update metric metadata (description, unit, type)
    ///   • Submit custom metrics to Datadog
    ///   • List metric tags and tag configurations
    ///
    /// METRIC TYPES:
    ///   • gauge: Point-in-time value (e.g., CPU usage, memory)
    ///   • count: Cumulative count (e.g., request count, errors)
    ///   • rate: Rate of change per second (e.g., requests per second)
    ///   • distribution: Statistical distribution (e.g., latency percentiles)
    ///
    /// TIME RANGES:
    ///   Supports flexible time range specifications:
    ///   • Relative: 1h, 30m, 7d, 1w (hours, minutes, days, weeks)
    ///   • Absolute: Unix timestamps or ISO 8601 format
    ///   • Special: now (current time)
    ///
    /// EXAMPLES:
    ///   # Query metrics
    ///   pup metrics query --query="avg:system.cpu.user{*}" --from="1h" --to="now"
    ///   pup metrics query --query="sum:app.requests{env:prod} by {service}" --from="4h"
    ///
    ///   # List metrics
    ///   pup metrics list
    ///   pup metrics list --filter="system.*"
    ///
    ///   # Get metric metadata
    ///   pup metrics metadata get system.cpu.user
    ///   pup metrics metadata get system.cpu.user --output=table
    ///
    ///   # Update metric metadata
    ///   pup metrics metadata update system.cpu.user \
    ///     --description="CPU user time" \
    ///     --unit="percent" \
    ///     --type="gauge"
    ///
    ///   # Submit custom metrics
    ///   pup metrics submit --name="custom.metric" --value=123 --tags="env:prod,team:backend"
    ///   pup metrics submit --name="custom.gauge" --value=99.5 --type="gauge" --timestamp=now
    ///
    ///   # List metric tags
    ///   pup metrics tags list system.cpu.user
    ///   pup metrics tags list system.cpu.user --from="1h"
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys
    ///   (DD_API_KEY and DD_APP_KEY environment variables).
    #[command(verbatim_doc_comment)]
    Metrics {
        #[command(subcommand)]
        action: MetricActions,
    },
    /// Miscellaneous API operations
    ///
    /// Miscellaneous API operations for various Datadog features.
    ///
    /// CAPABILITIES:
    ///   • Query IP ranges
    ///   • Check API status
    ///   • View service level agreements
    ///   • Access miscellaneous endpoints
    ///
    /// EXAMPLES:
    ///   # Get Datadog IP ranges
    ///   pup misc ip-ranges
    ///
    ///   # Check API status
    ///   pup misc status
    ///
    /// AUTHENTICATION:
    ///   Some endpoints may not require authentication.
    #[command(verbatim_doc_comment)]
    Misc {
        #[command(subcommand)]
        action: MiscActions,
    },
    /// Manage monitors
    ///
    /// Manage Datadog monitors for alerting and notifications.
    ///
    /// Monitors watch your metrics, logs, traces, and other data sources to alert you when
    /// conditions are met. They support various monitor types including metric, log, trace,
    /// composite, and more.
    ///
    /// CAPABILITIES:
    ///   • List all monitors with optional filtering by name or tags
    ///   • Get detailed information about a specific monitor
    ///   • Delete monitors (requires confirmation unless --yes flag is used)
    ///   • View monitor configuration, thresholds, and notification settings
    ///
    /// MONITOR TYPES:
    ///   • metric alert: Alert on metric threshold
    ///   • log alert: Alert on log query matches
    ///   • trace-analytics alert: Alert on APM trace patterns
    ///   • composite: Combine multiple monitors with boolean logic
    ///   • service check: Alert on service check status
    ///   • event alert: Alert on event patterns
    ///   • process alert: Alert on process status
    ///
    /// EXAMPLES:
    ///   # List all monitors
    ///   pup monitors list
    ///
    ///   # Filter monitors by name
    ///   pup monitors list --name="CPU"
    ///
    ///   # Filter monitors by tags
    ///   pup monitors list --tags="env:production,team:backend"
    ///
    ///   # Get detailed information about a specific monitor
    ///   pup monitors get 12345678
    ///
    ///   # Delete a monitor with confirmation prompt
    ///   pup monitors delete 12345678
    ///
    ///   # Delete a monitor without confirmation (automation)
    ///   pup monitors delete 12345678 --yes
    ///
    /// OUTPUT FORMAT:
    ///   All commands output JSON by default. Use --output flag for other formats.
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys
    ///   (DD_API_KEY and DD_APP_KEY environment variables).
    #[command(verbatim_doc_comment)]
    Monitors {
        #[command(subcommand)]
        action: MonitorActions,
    },
    /// Manage network monitoring
    ///
    /// Query network monitoring data including flows and devices.
    ///
    /// Network Performance Monitoring provides visibility into network traffic
    /// flows between services, containers, and availability zones.
    ///
    /// CAPABILITIES:
    ///   • Query network flows
    ///   • List network devices
    ///   • View network metrics
    ///   • Monitor network performance
    ///
    /// EXAMPLES:
    ///   # List network flows
    ///   pup network flows list
    ///
    ///   # List network devices
    ///   pup network devices list
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(verbatim_doc_comment)]
    Network {
        #[command(subcommand)]
        action: NetworkActions,
    },
    /// Manage notebooks
    ///
    /// Manage Datadog notebooks for investigation and documentation.
    ///
    /// Notebooks combine graphs, logs, and narrative text to document
    /// investigations, share findings, and create runbooks.
    ///
    /// CAPABILITIES:
    ///   • List notebooks
    ///   • Get notebook details
    ///   • Create new notebooks
    ///   • Update notebooks
    ///   • Delete notebooks
    ///
    /// EXAMPLES:
    ///   # List all notebooks
    ///   pup notebooks list
    ///
    ///   # Get notebook details
    ///   pup notebooks get notebook-id
    ///
    ///   # Create a notebook from file
    ///   pup notebooks create --body @notebook.json
    ///
    ///   # Create from stdin
    ///   cat notebook.json | pup notebooks create --body -
    ///
    ///   # Update a notebook
    ///   pup notebooks update 12345 --body @updated.json
    ///
    ///   # Delete a notebook
    ///   pup notebooks delete 12345
    ///
    /// AUTHENTICATION:
    ///   Requires API key authentication (DD_API_KEY + DD_APP_KEY).
    ///   OAuth2 is not supported for this endpoint.
    #[command(verbatim_doc_comment)]
    Notebooks {
        #[command(subcommand)]
        action: NotebookActions,
    },
    /// Manage observability pipelines
    ///
    /// Manage observability pipelines for data collection and routing.
    ///
    /// Observability Pipelines allow you to collect, transform, and route
    /// observability data at scale before sending it to Datadog or other destinations.
    ///
    /// CAPABILITIES:
    ///   • List pipeline configurations
    ///   • Get pipeline details
    ///   • View pipeline metrics
    ///   • Monitor pipeline health
    ///
    /// EXAMPLES:
    ///   # List pipelines
    ///   pup obs-pipelines list
    ///
    ///   # Get pipeline details
    ///   pup obs-pipelines get pipeline-id
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(name = "obs-pipelines", verbatim_doc_comment)]
    ObsPipelines {
        #[command(subcommand)]
        action: ObsPipelinesActions,
    },
    /// Manage teams and on-call operations
    ///
    /// Manage teams, memberships, links, and notification rules.
    ///
    /// Teams in Datadog represent groups of users that collaborate on monitoring,
    /// incident response, and on-call duties. Use this command to manage team
    /// structure, members, and notification settings.
    ///
    /// CAPABILITIES:
    ///   • Create, update, and delete teams
    ///   • Manage team memberships and roles
    ///   • Configure team links (documentation, runbooks)
    ///   • Set up notification rules for team alerts
    ///
    /// EXAMPLES:
    ///   # List all teams
    ///   pup on-call teams list
    ///
    ///   # Create a new team
    ///   pup on-call teams create --name="SRE Team" --handle="sre-team"
    ///
    ///   # Add a member to a team
    ///   pup on-call teams memberships add <team-id> --user-id=<uuid> --role=member
    ///
    ///   # List team members
    ///   pup on-call teams memberships list <team-id>
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys.
    #[command(name = "on-call", verbatim_doc_comment)]
    OnCall {
        #[command(subcommand)]
        action: OnCallActions,
    },
    /// Manage organization settings
    ///
    /// Manage organization-level settings and configuration.
    ///
    /// CAPABILITIES:
    ///   • View organization details
    ///   • List child organizations
    ///   • Manage organization settings
    ///   • Configure billing and usage
    ///
    /// EXAMPLES:
    ///   # Get organization details
    ///   pup organizations get
    ///
    ///   # List child organizations
    ///   pup organizations list
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys with org management permissions.
    #[command(verbatim_doc_comment)]
    Organizations {
        #[command(subcommand)]
        action: OrgActions,
    },
    /// Send product analytics events
    ///
    /// Send server-side product analytics events to Datadog.
    ///
    /// Product Analytics provides insights into user behavior and product usage
    /// through server-side event tracking.
    ///
    /// CAPABILITIES:
    ///   • Send individual server-side events with custom properties
    ///
    /// EXAMPLES:
    ///   # Send a basic event
    ///   pup product-analytics events send \
    ///     --app-id=my-app \
    ///     --event=button_clicked
    ///
    ///   # Send event with properties and user context
    ///   pup product-analytics events send \
    ///     --app-id=my-app \
    ///     --event=purchase_completed \
    ///     --properties='{"amount":99.99,"currency":"USD"}' \
    ///     --user-id=user-123
    ///
    /// AUTHENTICATION:
    ///   Requires OAuth2 (via 'pup auth login') or valid API + Application keys.
    #[command(name = "product-analytics", verbatim_doc_comment)]
    ProductAnalytics {
        #[command(subcommand)]
        action: ProductAnalyticsActions,
    },
    /// Manage reference tables for log enrichment
    ///
    /// Reference tables allow you to enrich logs with additional data from
    /// CSV files stored in cloud storage or uploaded directly.
    ///
    /// CAPABILITIES:
    ///   • List, get, and create reference tables
    ///   • Batch query rows by primary key
    ///
    /// EXAMPLES:
    ///   pup reference-tables list
    ///   pup reference-tables get <table-id>
    ///   pup reference-tables batch-query --file=query.json
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(name = "reference-tables", verbatim_doc_comment)]
    ReferenceTables {
        #[command(subcommand)]
        action: ReferenceTablesActions,
    },
    /// Manage Real User Monitoring (RUM)
    ///
    /// Manage Datadog Real User Monitoring (RUM) for frontend application performance.
    ///
    /// RUM provides visibility into real user experiences across web and mobile applications,
    /// capturing frontend performance metrics, user sessions, errors, and user journeys.
    ///
    /// CAPABILITIES:
    ///   • Manage RUM applications (web, mobile, browser)
    ///   • Configure RUM metrics and custom metrics
    ///   • Set up retention filters for session replay and data
    ///   • Query session replay data and playlists
    ///   • Analyze user interaction heatmaps
    ///
    /// RUM DATA TYPES:
    ///   • Views: Page views and screen loads
    ///   • Actions: User interactions (clicks, taps, scrolls)
    ///   • Errors: Frontend errors and crashes
    ///   • Resources: Network requests and asset loading
    ///   • Long Tasks: Performance bottlenecks
    ///
    /// APPLICATION TYPES:
    ///   • browser: Web applications
    ///   • ios: iOS mobile applications
    ///   • android: Android mobile applications
    ///   • react-native: React Native applications
    ///   • flutter: Flutter applications
    ///
    /// EXAMPLES:
    ///   # List all RUM applications
    ///   pup rum apps list
    ///
    ///   # Get RUM application details
    ///   pup rum apps get --app-id="abc-123-def"
    ///
    ///   # Create a new browser RUM application
    ///   pup rum apps create --name="my-web-app" --type="browser"
    ///
    ///   # List RUM custom metrics
    ///   pup rum metrics list
    ///
    ///   # List retention filters
    ///   pup rum retention-filters list
    ///
    ///   # Query session replay data
    ///   pup rum sessions list --from="1h"
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys
    ///   (DD_API_KEY and DD_APP_KEY environment variables).
    #[command(verbatim_doc_comment)]
    Rum {
        #[command(subcommand)]
        action: RumActions,
    },
    #[cfg(not(target_arch = "wasm32"))]
    /// Execute and manage local operational runbooks
    ///
    /// Runbooks are YAML files stored in ~/.config/pup/runbooks/ that define
    /// sequential operational steps mixing pup commands, shell tools, Datadog
    /// Workflows, and human confirmation gates.
    ///
    /// COMMANDS:
    ///   list        List available runbooks (optionally filtered by tags)
    ///   describe    Show runbook details and steps
    ///   run         Execute a runbook with optional variable overrides
    ///   validate    Validate runbook structure without executing
    ///   import      Import a runbook from a file path or URL
    ///
    /// EXAMPLES:
    ///   # List all runbooks
    ///   pup runbooks list
    ///
    ///   # List runbooks tagged type:deployment
    ///   pup runbooks list --tag=type:deployment
    ///
    ///   # Run a runbook with variable overrides
    ///   pup runbooks run deploy-service --arg SERVICE=payments --arg VERSION=1.2.3
    ///
    ///   # Validate without executing
    ///   pup runbooks validate deploy-service
    ///
    ///   # Import from a file or URL
    ///   pup runbooks import ./my-runbook.yaml
    #[command(verbatim_doc_comment)]
    Runbooks {
        #[command(subcommand)]
        action: RunbookActions,
    },
    /// Manage service scorecards
    ///
    /// Manage service quality scorecards and rules.
    ///
    /// Scorecards help you track and improve service quality by defining
    /// rules and measuring compliance across your services.
    ///
    /// CAPABILITIES:
    ///   • List scorecards
    ///   • Get scorecard details
    ///   • View scorecard rules
    ///   • Track service scores
    ///
    /// EXAMPLES:
    ///   # List scorecards
    ///   pup scorecards list
    ///
    ///   # Get scorecard details
    ///   pup scorecards get scorecard-id
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(verbatim_doc_comment)]
    Scorecards {
        #[command(subcommand)]
        action: ScorecardsActions,
    },
    /// Manage security monitoring
    ///
    /// Manage security monitoring rules, signals, and findings.
    ///
    /// CAPABILITIES:
    ///   • List and manage security monitoring rules
    ///   • View security signals and findings
    ///   • Configure suppression rules
    ///   • Manage security filters
    ///
    /// EXAMPLES:
    ///   # List security monitoring rules
    ///   pup security rules list
    ///
    ///   # Get rule details
    ///   pup security rules get rule-id
    ///
    ///   # List security signals
    ///   pup security signals list
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(verbatim_doc_comment)]
    Security {
        #[command(subcommand)]
        action: SecurityActions,
    },
    /// Manage service catalog
    ///
    /// Manage services in the Datadog service catalog.
    ///
    /// The service catalog provides a centralized registry of all services
    /// in your infrastructure with ownership, dependencies, and documentation.
    ///
    /// CAPABILITIES:
    ///   • List services in the catalog
    ///   • Get service details
    ///   • View service definitions
    ///   • Manage service metadata
    ///
    /// EXAMPLES:
    ///   # List all services
    ///   pup service-catalog list
    ///
    ///   # Get service details
    ///   pup service-catalog get service-name
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(name = "service-catalog", verbatim_doc_comment)]
    ServiceCatalog {
        #[command(subcommand)]
        action: ServiceCatalogActions,
    },
    /// Manage agent skills for AI coding assistants
    ///
    /// Install structured workflow guides, domain references, and specialized
    /// agents that teach AI coding assistants how to compose pup commands.
    ///
    /// COMMANDS:
    ///   list      List available skills and agents
    ///   install   Install skills for the detected AI coding assistant
    ///   path      Show where skills would be installed
    ///
    /// EXAMPLES:
    ///   pup skills list
    ///   pup skills install
    ///   pup skills install dd-pup
    ///   pup skills install --type=agent
    ///   pup skills install --target-agent=cursor
    ///   pup skills path
    #[command(verbatim_doc_comment)]
    Skills {
        #[command(subcommand)]
        action: SkillsActions,
    },
    /// Manage Service Level Objectives
    ///
    /// Manage Datadog Service Level Objectives (SLOs) for tracking service reliability.
    ///
    /// SLOs help you define and track service reliability targets based on Service Level
    /// Indicators (SLIs). They support various calculation types and target windows.
    ///
    /// CAPABILITIES:
    ///   • List all SLOs with status and error budget
    ///   • Get detailed SLO configuration and history
    ///   • Delete SLOs (requires confirmation unless --yes flag is used)
    ///   • View SLO status, error budget burn rate, and target compliance
    ///
    /// SLO TYPES:
    ///   • Metric-based: Based on metric queries (e.g., success rate, latency)
    ///   • Monitor-based: Based on monitor uptime
    ///   • Time slice: Based on time slices meeting criteria
    ///
    /// TARGET WINDOWS:
    ///   • 7 days (7d)
    ///   • 30 days (30d)
    ///   • 90 days (90d)
    ///   • Custom rolling windows
    ///
    /// CALCULATION METHODS:
    ///   • by_count: Count of good events / total events
    ///   • by_uptime: Percentage of time in good state
    ///
    /// EXAMPLES:
    ///   # List all SLOs
    ///   pup slos list
    ///
    ///   # Get detailed SLO information
    ///   pup slos get abc-123-def
    ///
    ///   # Get SLO history and status
    ///   pup slos get abc-123-def | jq '.data'
    ///
    ///   # Delete an SLO with confirmation
    ///   pup slos delete abc-123-def
    ///
    ///   # Delete an SLO without confirmation (automation)
    ///   pup slos delete abc-123-def --yes
    ///
    /// ERROR BUDGET:
    ///   Error budget represents the allowed amount of unreliability before breaching
    ///   the SLO target. It's calculated as (1 - target) * time_window.
    ///
    ///   Example: 99.9% target over 30 days = 0.1% * 30 days = 43.2 minutes allowed downtime
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys
    ///   (DD_API_KEY and DD_APP_KEY environment variables).
    #[command(verbatim_doc_comment)]
    Slos {
        #[command(subcommand)]
        action: SloActions,
    },
    /// Manage static analysis
    ///
    /// Manage static analysis for code security and quality.
    ///
    /// CAPABILITIES:
    ///   • AST analysis results
    ///   • Custom security rulesets
    ///   • Software Composition Analysis (SCA)
    ///   • Code coverage analysis
    ///
    /// EXAMPLES:
    ///   # List custom rulesets
    ///   pup static-analysis custom-rulesets list
    ///
    ///   # Get ruleset details
    ///   pup static-analysis custom-rulesets get abc-123
    #[command(name = "static-analysis", verbatim_doc_comment)]
    StaticAnalysis {
        #[command(subcommand)]
        action: StaticAnalysisActions,
    },
    /// Manage status pages
    ///
    /// Manage Datadog Status Pages for communicating service status.
    ///
    /// Status Pages provide a public-facing view of your service health, including
    /// components, degradations, and incident updates.
    ///
    /// CAPABILITIES:
    ///   • Manage status pages (list, create, update, delete)
    ///   • Manage page components
    ///   • Manage degradation events
    ///
    /// EXAMPLES:
    ///   # List status pages
    ///   pup status-pages pages list
    ///
    ///   # Create a status page
    ///   pup status-pages pages create --file=page.json
    ///
    ///   # List components for a page
    ///   pup status-pages components list <page-id>
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(name = "status-pages", verbatim_doc_comment)]
    StatusPages {
        #[command(subcommand)]
        action: StatusPageActions,
    },
    /// Query the Symbol Database (SymDB)
    ///
    /// Search and inspect indexed service symbols (classes, methods, fields)
    /// from the Datadog Symbol Database.
    ///
    /// CAPABILITIES:
    ///   • Search for scopes (classes, methods) in a service
    ///
    /// EXAMPLES:
    ///   # Search scopes by name
    ///   pup symdb search --service my-service --query MyController
    ///
    ///   # List all scopes
    ///   pup symdb search --service my-service
    ///
    ///   # Get probe locations
    ///   pup symdb search --service my-service --query MyController --view probe-locations
    ///
    ///   # Get full JSON response
    ///   pup symdb search --service my-service --query MyController --view full
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(name = "symdb", verbatim_doc_comment)]
    Symdb {
        #[command(subcommand)]
        action: SymdbActions,
    },
    /// Manage synthetic monitoring
    ///
    /// Manage synthetic tests for monitoring application availability.
    ///
    /// Synthetic monitoring simulates user interactions and API requests to
    /// monitor application performance and availability from various locations.
    ///
    /// CAPABILITIES:
    ///   • List synthetic tests
    ///   • Search synthetic tests by text query
    ///   • Get test details
    ///   • Get test results
    ///   • List test locations
    ///   • Manage global variables
    ///
    /// EXAMPLES:
    ///   # List all synthetic tests
    ///   pup synthetics tests list
    ///
    ///   # Search tests by creator or team
    ///   pup synthetics tests search --text='creator:"Jane Doe"'
    ///   pup synthetics tests search --text="team:my-team"
    ///
    ///   # Get test details
    ///   pup synthetics tests get test-id
    ///
    ///   # List available locations
    ///   pup synthetics locations list
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(verbatim_doc_comment)]
    Synthetics {
        #[command(subcommand)]
        action: SyntheticsActions,
    },
    /// Manage host tags
    ///
    /// Manage tags for hosts in your infrastructure.
    ///
    /// Tags provide metadata about your hosts and help organize and filter
    /// your infrastructure.
    ///
    /// CAPABILITIES:
    ///   • List all host tags
    ///   • Get tags for a specific host
    ///   • Add tags to a host
    ///   • Update host tags
    ///   • Remove tags from a host
    ///
    /// EXAMPLES:
    ///   # List all host tags
    ///   pup tags list
    ///
    ///   # Get tags for a host
    ///   pup tags get my-host
    ///
    ///   # Add tags to a host
    ///   pup tags add my-host env:prod team:backend
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(verbatim_doc_comment)]
    Tags {
        #[command(subcommand)]
        action: TagActions,
    },
    /// Manage Test Optimization settings and flaky tests
    ///
    /// Configure Test Optimization service settings and manage flaky tests
    /// through the Datadog Test Optimization API.
    ///
    /// CAPABILITIES:
    ///   • Get, update, or delete service-level Test Optimization settings
    ///   • Search for flaky tests across your test suites
    ///   • Update flaky test state (e.g. mark as known flaky)
    ///
    /// EXAMPLES:
    ///   # Get service settings
    ///   pup test-optimization settings get --file=body.json
    ///
    ///   # Search for flaky tests
    ///   pup test-optimization flaky-tests search
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(name = "test-optimization", verbatim_doc_comment)]
    TestOptimization {
        #[command(subcommand)]
        action: TestOptimizationActions,
    },
    /// Search and aggregate APM traces
    ///
    /// Search and aggregate APM span data for distributed tracing analysis.
    ///
    /// The traces command provides access to individual span-level data collected by
    /// Datadog APM. Use it to find specific spans matching a query or compute
    /// aggregated statistics over spans.
    ///
    /// COMPLEMENTS THE APM COMMAND:
    ///   - apm: Service-level aggregated data (services, operations, dependencies)
    ///   - traces: Individual span-level data (search, aggregate)
    ///
    /// EXAMPLES:
    ///   # Search for error spans in the last hour
    ///   pup traces search --query="service:web-server @http.status_code:500"
    ///
    ///   # Count spans by service
    ///   pup traces aggregate --query="*" --compute="count" --group-by="service"
    ///
    ///   # P99 latency by resource
    ///   pup traces aggregate --query="service:api" --compute="percentile(@duration, 99)" --group-by="resource_name"
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (apm_read scope) or API keys.
    #[command(verbatim_doc_comment)]
    Traces {
        #[command(subcommand)]
        action: TracesActions,
    },
    /// Query usage and billing information
    ///
    /// Query usage metrics and billing information for your organization.
    ///
    /// CAPABILITIES:
    ///   • View usage summary
    ///   • Get hourly usage
    ///   • Track usage by product
    ///   • Monitor cost attribution
    ///
    /// EXAMPLES:
    ///   # Get usage summary
    ///   pup usage summary --start="2024-01-01" --end="2024-01-31"
    ///
    ///   # Get hourly usage
    ///   pup usage hourly --start="2024-01-01" --end="2024-01-02"
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys with billing permissions.
    #[command(verbatim_doc_comment)]
    Usage {
        #[command(subcommand)]
        action: UsageActions,
    },
    /// Manage users and access
    ///
    /// Manage users, roles, and access permissions.
    ///
    /// CAPABILITIES:
    ///   • List users in your organization
    ///   • Get user details
    ///   • Manage user roles and permissions
    ///   • Invite new users
    ///   • Disable users
    ///
    /// EXAMPLES:
    ///   # List all users
    ///   pup users list
    ///
    ///   # Get user details
    ///   pup users get user-id
    ///
    ///   # List roles
    ///   pup users roles list
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(verbatim_doc_comment)]
    Users {
        #[command(subcommand)]
        action: UserActions,
    },
    /// Print version information
    Version,
    /// Manage Datadog workflows
    ///
    /// Create, update, delete, and execute Datadog Workflow Automation workflows.
    ///
    /// CAPABILITIES:
    ///   • Get workflow details
    ///   • Create, update, and delete workflows
    ///   • Execute workflows via API trigger (requires DD_API_KEY + DD_APP_KEY)
    ///   • List, inspect, and cancel workflow instances (executions)
    ///
    /// EXAMPLES:
    ///   # Get a workflow
    ///   pup workflows get <workflow-id>
    ///
    ///   # Execute a workflow via API trigger
    ///   pup workflows run <workflow-id> --payload '{"key": "value"}'
    ///
    ///   # Execute and wait for completion
    ///   pup workflows run <workflow-id> --wait --timeout 2m
    ///
    ///   # List recent executions
    ///   pup workflows instances list <workflow-id>
    ///
    ///   # Cancel a running execution
    ///   pup workflows instances cancel <workflow-id> <instance-id>
    ///
    /// AUTHENTICATION:
    ///   All workflow commands require DD_API_KEY + DD_APP_KEY.
    ///   OAuth2 bearer tokens are not supported for workflow operations at this time.
    #[command(verbatim_doc_comment)]
    Workflows {
        #[command(subcommand)]
        action: WorkflowActions,
    },
}

// ---- Extensions ----
#[cfg(not(target_arch = "wasm32"))]
#[derive(Subcommand)]
enum ExtensionActions {
    /// List installed extensions
    List,
    /// Install an extension
    Install {
        /// Source: local file path (with --local) or GitHub owner/repo
        source: String,
        /// Install a specific release tag (GitHub only)
        #[arg(long, conflicts_with = "local")]
        tag: Option<String>,
        /// Install from a local file path
        #[arg(long)]
        local: bool,
        /// Symlink instead of copy (with --local)
        #[arg(long, requires = "local")]
        link: bool,
        /// Extension name (auto-derived from source if omitted)
        #[arg(long)]
        name: Option<String>,
        /// Overwrite an existing extension
        #[arg(long)]
        force: bool,
        /// Short description shown in `pup help`
        #[arg(long)]
        description: Option<String>,
    },
    /// Remove an installed extension
    Remove {
        /// Extension name (without pup- prefix)
        name: String,
    },
    /// Upgrade an extension to the latest version
    Upgrade {
        /// Extension name (or omit with --all)
        name: Option<String>,
        /// Upgrade all installed extensions
        #[arg(long)]
        all: bool,
    },
}

// ---- Monitors ----
#[derive(Subcommand)]
enum MonitorActions {
    /// List monitors (limited results)
    List {
        #[arg(long, help = "Filter monitors by name")]
        name: Option<String>,
        #[arg(
            long,
            help = "Filter by monitor tags (comma-separated, e.g., team:backend,env:prod)"
        )]
        tags: Option<String>,
        #[arg(
            long,
            default_value_t = 200,
            help = "Maximum number of monitors to return (default: 200, max: 1000)"
        )]
        limit: i32,
    },
    /// Get monitor details
    Get { monitor_id: i64 },
    /// Create a monitor from JSON file
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update a monitor from JSON file
    Update {
        monitor_id: i64,
        #[arg(long)]
        file: String,
    },
    /// Search monitors
    Search {
        #[arg(long, help = "Search query string")]
        query: Option<String>,
        #[arg(long, default_value_t = 0, help = "Page number")]
        page: i64,
        #[arg(long, default_value_t = 30, help = "Results per page")]
        per_page: i64,
        #[arg(long, help = "Sort order")]
        sort: Option<String>,
    },
    /// Delete a monitor
    Delete { monitor_id: i64 },
}

// ---- Logs ----
#[derive(Subcommand)]
enum LogActions {
    /// Search logs (v1 API)
    Search {
        #[arg(long, help = "Search query (required)")]
        query: String,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time: 1h, 5min, 2hours, '5 minutes', RFC3339, Unix timestamp, or 'now'"
        )]
        from: String,
        #[arg(
            long,
            default_value = "now",
            help = "End time: 1h, 5min, 2hours, '5 minutes', RFC3339, Unix timestamp, or 'now'"
        )]
        to: String,
        #[arg(long, default_value_t = 50, help = "Maximum number of logs (1-1000)")]
        limit: i32,
        #[arg(long, help = "Sort order: asc or desc", default_value = "desc")]
        sort: String,
        #[arg(long, help = "Comma-separated log indexes")]
        index: Option<String>,
        #[arg(long, help = "Storage tier: indexes, online-archives, or flex")]
        storage: Option<String>,
    },
    /// List logs (v2 API)
    List {
        #[arg(long, default_value = "*", help = "Search query")]
        query: String,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time: 1h, 5min, 2hours, '5 minutes', RFC3339, Unix timestamp, or 'now'"
        )]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, default_value_t = 10, help = "Number of logs")]
        limit: i32,
        #[arg(long, default_value = "-timestamp", help = "Sort order")]
        sort: String,
        #[arg(long, help = "Storage tier: indexes, online-archives, or flex")]
        storage: Option<String>,
    },
    /// Query logs (v2 API)
    Query {
        #[arg(long, help = "Log query (required)")]
        query: String,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time: 1h, 5min, 2hours, '5 minutes', RFC3339, Unix timestamp, or 'now'"
        )]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, default_value_t = 50, help = "Maximum results")]
        limit: i32,
        #[arg(long, default_value = "-timestamp", help = "Sort order")]
        sort: String,
        #[arg(long, help = "Storage tier: indexes, online-archives, or flex")]
        storage: Option<String>,
        #[arg(long, help = "Timezone for timestamps")]
        timezone: Option<String>,
    },
    /// Aggregate logs (v2 API)
    Aggregate {
        #[arg(long, help = "Log query (required)")]
        query: Option<String>,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time: 1h, 5min, 2hours, '5 minutes', RFC3339, Unix timestamp, or 'now'"
        )]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(
            long,
            default_value = "count",
            help = "Metrics to compute (comma-separated, e.g. count,avg(@duration),percentile(@duration, 95))"
        )]
        compute: String,
        #[arg(
            long,
            help = "Fields to group by (comma-separated, e.g. service,status)"
        )]
        group_by: Option<String>,
        #[arg(long, default_value_t = 10, help = "Maximum groups per facet")]
        limit: i32,
        #[arg(long, help = "Storage tier: indexes, online-archives, or flex")]
        storage: Option<String>,
    },
    /// Manage log archives
    Archives {
        #[command(subcommand)]
        action: LogArchiveActions,
    },
    /// Manage custom log destinations
    #[command(name = "custom-destinations")]
    CustomDestinations {
        #[command(subcommand)]
        action: LogCustomDestinationActions,
    },
    /// Manage log-based metrics
    Metrics {
        #[command(subcommand)]
        action: LogMetricActions,
    },
    /// Manage log restriction queries
    #[command(name = "restriction-queries")]
    RestrictionQueries {
        #[command(subcommand)]
        action: LogRestrictionQueryActions,
    },
}

#[derive(Subcommand)]
enum LogRestrictionQueryActions {
    /// List restriction queries
    List,
    /// Get restriction query details
    Get { query_id: String },
}

#[derive(Subcommand)]
enum LogArchiveActions {
    /// List all log archives
    List,
    /// Get log archive details
    Get { archive_id: String },
    /// Delete a log archive
    Delete { archive_id: String },
}

#[derive(Subcommand)]
enum LogCustomDestinationActions {
    /// List custom log destinations
    List,
    /// Get custom destination details
    Get { destination_id: String },
}

#[derive(Subcommand)]
enum LogMetricActions {
    /// List log-based metrics
    List,
    /// Get log-based metric details
    Get { metric_id: String },
    /// Delete a log-based metric
    Delete { metric_id: String },
}

// ---- Incidents ----
#[derive(Subcommand)]
enum IncidentActions {
    /// List all incidents
    List {
        #[arg(long, default_value_t = 50)]
        limit: i64,
    },
    /// Get incident details
    Get { incident_id: String },
    /// Manage incident attachments
    Attachments {
        #[command(subcommand)]
        action: IncidentAttachmentActions,
    },
    /// Manage global incident settings
    Settings {
        #[command(subcommand)]
        action: IncidentSettingsActions,
    },
    /// Manage global incident handles
    Handles {
        #[command(subcommand)]
        action: IncidentHandleActions,
    },
    /// Manage incident postmortem templates
    #[command(name = "postmortem-templates")]
    PostmortemTemplates {
        #[command(subcommand)]
        action: IncidentPostmortemActions,
    },
    /// Import an incident
    Import {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
}

#[derive(Subcommand)]
enum IncidentAttachmentActions {
    /// List incident attachments
    List { incident_id: String },
    /// Delete an incident attachment
    Delete {
        incident_id: String,
        attachment_id: String,
    },
}

#[derive(Subcommand)]
enum IncidentSettingsActions {
    /// Get global incident settings
    Get,
    /// Update global incident settings
    Update {
        #[arg(long, help = "JSON file with settings (required)")]
        file: String,
    },
}

#[derive(Subcommand)]
enum IncidentHandleActions {
    /// List global incident handles
    List,
    /// Create global incident handle
    Create {
        #[arg(long, help = "JSON file with handle data (required)")]
        file: String,
    },
    /// Update global incident handle
    Update {
        #[arg(long, help = "JSON file with handle data (required)")]
        file: String,
    },
    /// Delete global incident handle
    Delete { handle_id: String },
}

#[derive(Subcommand)]
enum IncidentPostmortemActions {
    /// List postmortem templates
    List,
    /// Get postmortem template
    Get { template_id: String },
    /// Create postmortem template
    Create {
        #[arg(long, help = "JSON file with template (required)")]
        file: String,
    },
    /// Update postmortem template
    Update {
        template_id: String,
        #[arg(long, help = "JSON file with template (required)")]
        file: String,
    },
    /// Delete postmortem template
    Delete { template_id: String },
}

// ---- Dashboards ----
#[derive(Subcommand)]
enum DashboardActions {
    /// List all dashboards
    List,
    /// Get dashboard details
    Get { id: String },
    /// Create a dashboard from JSON file
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update a dashboard from JSON file
    Update {
        id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete a dashboard
    Delete { id: String },
    /// Manage saved widgets
    Widgets {
        #[command(subcommand)]
        action: WidgetActions,
    },
}

// ---- Debugger ----

#[derive(Subcommand)]
enum DebuggerActions {
    /// Manage Live Debugger log probes
    Probes {
        #[command(subcommand)]
        action: DebuggerProbeActions,
    },
    /// Show service debugger context (environments, probe support, language features)
    Context {
        #[arg(help = "Service name")]
        service: String,
        #[arg(long, help = "Filter to a specific environment")]
        env: Option<String>,
        #[arg(long, help = "Comma-separated fields: service, language, envs, repo")]
        fields: Option<String>,
    },
}

#[derive(Subcommand)]
enum DebuggerProbeActions {
    /// List log probes
    List {
        #[arg(long, help = "Filter by service name")]
        service: Option<String>,
    },
    /// Get log probe details
    Get {
        #[arg(help = "Probe ID")]
        probe_id: String,
    },
    /// Create a log probe
    Create {
        #[arg(long, help = "Service name")]
        service: String,
        #[arg(long, help = "Environment")]
        env: String,
        #[arg(long, help = "Probe location as type_name:method_name")]
        probe_location: String,
        #[arg(long, help = "Tracer language (auto-detected from symdb if omitted)")]
        language: Option<String>,
        #[arg(long, help = "Log template string")]
        template: Option<String>,
        #[arg(long, help = "Condition expression")]
        condition: Option<String>,
        #[arg(
            long,
            num_args = 0..=1,
            default_missing_value = "",
            action = clap::ArgAction::Append,
            help = "Capture expression (e.g., 'user.name'). Repeatable. Without value: full snapshot."
        )]
        capture: Vec<String>,
        #[arg(long, default_value_t = 1, help = "Snapshots per second")]
        rate: u32,
        #[arg(
            long,
            default_value_t = 1000,
            help = "Max probe hits (budget). Only 'total' window is supported."
        )]
        budget: u32,
        #[arg(
            long,
            default_value = "1h",
            help = "Probe time-to-live (e.g., 10m, 1h, 24h). Probe auto-expires after this duration."
        )]
        ttl: String,
        #[arg(
            long,
            default_value_t = 1,
            value_parser = clap::value_parser!(u32).range(1..=5),
            help = "Max object graph traversal depth for captures (1-5). Start low, increase to drill into nested objects."
        )]
        depth: u32,
        #[arg(
            long,
            help = "Comma-separated fields to include: id, service, env, location, template, expires. Default: full response."
        )]
        fields: Option<String>,
    },
    /// Delete a log probe
    Delete {
        #[arg(help = "Probe ID")]
        probe_id: String,
    },
    /// Watch probe events (log data + status errors)
    Watch {
        #[arg(help = "Probe ID")]
        probe_id: String,
        #[arg(long, help = "Exit after N log events")]
        limit: Option<u32>,
        #[arg(long, default_value_t = 120, help = "Timeout in seconds")]
        timeout: u64,
        #[arg(long, help = "Log query start time (default: now)")]
        from: Option<String>,
        #[arg(
            long,
            default_value_t = 0,
            help = "Wait up to N seconds for the probe to become available"
        )]
        wait: u64,
        #[arg(
            long,
            help = "Comma-separated fields to include: message, captures, timestamp. Default: full debugger payload."
        )]
        fields: Option<String>,
    },
}

// ---- Metrics ----
#[derive(Subcommand)]
enum MetricActions {
    /// List all available metrics
    List {
        #[arg(
            long,
            help = "Filter metrics by name pattern (e.g., system.*, *.cpu.*)"
        )]
        filter: Option<String>,
        #[arg(long, help = "Filter metrics by tags (e.g., env:prod,service:api)")]
        tag_filter: Option<String>,
        #[arg(long, default_value = "1h")]
        from: String,
    },
    /// Search metrics (v1 API)
    Search {
        #[arg(long, help = "Metric query string (required)")]
        query: String,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time (e.g., 1h, 30m, 7d, now, unix timestamp)"
        )]
        from: String,
        #[arg(
            long,
            default_value = "now",
            help = "End time (e.g., now, unix timestamp)"
        )]
        to: String,
    },
    /// Query time-series metrics data (v2 API)
    Query {
        #[arg(long, help = "Metric query string (required)")]
        query: String,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time (e.g., 1h, 30m, 7d, now, unix timestamp)"
        )]
        from: String,
        #[arg(
            long,
            default_value = "now",
            help = "End time (e.g., now, unix timestamp)"
        )]
        to: String,
    },
    /// Submit custom metrics to Datadog
    Submit {
        #[arg(
            long,
            help = "Metric name (required)",
            required_unless_present = "file"
        )]
        name: Option<String>,
        #[arg(long, default_value_t = 0.0, help = "Metric value (required)")]
        value: f64,
        #[arg(long, help = "Tags (comma-separated)")]
        tags: Option<String>,
        #[arg(
            long,
            default_value = "gauge",
            help = "Metric type (gauge, count, rate)"
        )]
        r#type: String,
        #[arg(long, help = "Host name")]
        host: Option<String>,
        #[arg(
            long,
            default_value_t = 0,
            help = "Interval in seconds for rate/count metrics"
        )]
        interval: i64,
        #[arg(long, help = "JSON file with metrics data", conflicts_with = "name")]
        file: Option<String>,
    },
    /// Manage metric metadata
    Metadata {
        #[command(subcommand)]
        action: MetricMetadataActions,
    },
    /// Manage metric tags
    Tags {
        #[command(subcommand)]
        action: MetricTagActions,
    },
}

#[derive(Subcommand)]
enum MetricTagActions {
    /// List tags for a metric
    List {
        metric_name: String,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
    },
}

#[derive(Subcommand)]
enum MetricMetadataActions {
    /// Get metric metadata
    Get { metric_name: String },
    /// Update metric metadata
    Update {
        metric_name: String,
        #[arg(long, help = "Metric description", required_unless_present = "file")]
        description: Option<String>,
        #[arg(long, help = "Short display name")]
        short_name: Option<String>,
        #[arg(long, help = "Metric unit")]
        unit: Option<String>,
        #[arg(long, help = "Per-unit for rate metrics")]
        per_unit: Option<String>,
        #[arg(long, help = "Metric type (gauge, count, rate, distribution)")]
        r#type: Option<String>,
        #[arg(long, help = "JSON file with metadata", conflicts_with = "description")]
        file: Option<String>,
    },
}

// ---- SLOs ----
#[derive(Subcommand)]
enum SloActions {
    /// List all SLOs
    List,
    /// Get SLO details
    Get { id: String },
    /// Create an SLO from JSON file
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update an SLO from JSON file
    Update {
        id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete an SLO
    Delete { id: String },
    /// Get SLO status
    Status {
        id: String,
        #[arg(long, help = "Start time (1h, 30d, Unix timestamp, or RFC3339)")]
        from: String,
        #[arg(long, help = "End time (now, Unix timestamp, or RFC3339)")]
        to: String,
    },
}

// ---- SymDB ----
#[derive(Subcommand)]
enum SymdbActions {
    /// Search for scopes (classes, methods) in a service
    Search {
        #[arg(long, help = "Service name")]
        service: String,
        #[arg(long, help = "Search query (matches scope names)")]
        query: Option<String>,
        #[arg(long, help = "Service version filter")]
        version: Option<String>,
        #[arg(long, default_value_t = commands::symdb::SymdbView::Full, help = "Output view")]
        view: commands::symdb::SymdbView,
    },
}

// ---- Synthetics ----
#[derive(Subcommand)]
enum SyntheticsActions {
    /// Manage synthetic tests
    Tests {
        #[command(subcommand)]
        action: SyntheticsTestActions,
    },
    /// Manage test locations
    Locations {
        #[command(subcommand)]
        action: SyntheticsLocationActions,
    },
    /// Manage synthetic test suites
    Suites {
        #[command(subcommand)]
        action: SyntheticsSuiteActions,
    },
    /// Manage multistep API tests
    Multistep {
        #[command(subcommand)]
        action: SyntheticsMultistepActions,
    },
}

#[derive(Subcommand)]
enum SyntheticsTestActions {
    /// List synthetic tests
    List {
        #[arg(long, default_value_t = 10, help = "Results per page")]
        page_size: i64,
        #[arg(long, default_value_t = 0, help = "Page number")]
        page_number: i64,
    },
    /// Get test details
    Get { public_id: String },
    /// Search synthetic tests
    Search {
        #[arg(long, help = "Search text query")]
        text: Option<String>,
        #[arg(long, help = "Return only facets (no test results)")]
        facets_only: bool,
        #[arg(long, help = "Include full test configuration in results")]
        include_full_config: bool,
        #[arg(
            long,
            default_value_t = 50,
            help = "Maximum number of results to return"
        )]
        count: i64,
        #[arg(
            long,
            default_value_t = 0,
            help = "Offset from which to start returning results"
        )]
        start: i64,
        #[arg(long, help = "Sort order")]
        sort: Option<String>,
    },
    /// Run synthetic tests (requires DD_API_KEY + DD_APP_KEY)
    #[cfg(not(target_arch = "wasm32"))]
    Run {
        /// Public IDs of tests to run (e.g. abc-def-ghi)
        public_ids: Vec<String>,
        /// Run tests against internal environments (e.g. dev server) using a SSH tunnel
        #[arg(long)]
        tunnel: bool,
        /// Maximum seconds to wait for results
        #[arg(long, default_value_t = 1800)]
        timeout: u64,
    },
    /// Get a fast (latest) test result by result ID
    GetFastResult {
        /// Result ID
        result_id: String,
    },
    /// Get a specific version of a synthetic test
    GetVersion {
        /// Public ID of the test
        public_id: String,
        /// Version number to retrieve
        version: i64,
        /// Include change metadata in the response
        #[arg(long)]
        include_change_metadata: bool,
    },
    /// List version history for a synthetic test
    ListVersions {
        /// Public ID of the test
        public_id: String,
        /// Maximum number of versions to return per page
        #[arg(long)]
        limit: Option<i64>,
        /// Version number of the last item from the previous page
        #[arg(long)]
        last_version_number: Option<i64>,
    },
}

#[derive(Subcommand)]
enum SyntheticsMultistepActions {
    /// Get subtests for a multistep API test
    GetSubtests {
        /// Public ID of the multistep test
        public_id: String,
    },
    /// Get parent tests for a multistep API subtest
    GetSubtestParents {
        /// Public ID of the subtest
        public_id: String,
    },
}

#[derive(Subcommand)]
enum SyntheticsLocationActions {
    /// List available locations
    List,
}

#[derive(Subcommand)]
enum SyntheticsSuiteActions {
    /// Search synthetic suites
    List {
        #[arg(long)]
        query: Option<String>,
    },
    /// Get suite details
    Get { suite_id: String },
    /// Create a synthetic suite
    Create {
        #[arg(long, help = "JSON file with suite definition (required)")]
        file: String,
    },
    /// Update a synthetic suite
    Update {
        suite_id: String,
        #[arg(long, help = "JSON file with suite definition (required)")]
        file: String,
    },
    /// Delete synthetic suites
    Delete {
        /// Suite IDs to delete
        suite_ids: Vec<String>,
        #[arg(long, help = "Comma-separated suite public IDs (required)")]
        ids: Option<String>,
    },
}

// ---- Test Optimization ----
#[derive(Subcommand)]
enum TestOptimizationActions {
    /// Manage Test Optimization service settings
    Settings {
        #[command(subcommand)]
        action: TestOptimizationSettingsActions,
    },
    /// Manage flaky tests
    #[command(name = "flaky-tests")]
    FlakyTests {
        #[command(subcommand)]
        action: TestOptimizationFlakyTestsActions,
    },
}

#[derive(Subcommand)]
enum TestOptimizationSettingsActions {
    /// Get Test Optimization service settings (body from JSON file)
    Get {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update Test Optimization service settings (body from JSON file)
    Update {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete Test Optimization service settings (body from JSON file)
    Delete {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
}

#[derive(Subcommand)]
enum TestOptimizationFlakyTestsActions {
    /// Search for flaky tests
    Search {
        #[arg(long, help = "Optional JSON file with search request body")]
        file: Option<String>,
    },
    /// Update flaky test state (body from JSON file)
    Update {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
}

// ---- Events ----
#[derive(Subcommand)]
enum EventActions {
    /// List recent events
    List {
        #[arg(
            long,
            default_value = "1h",
            help = "Start time (1h, 30m, 7d, Unix timestamp, or RFC3339)"
        )]
        from: String,
        #[arg(
            long,
            default_value = "now",
            help = "End time (now, Unix timestamp, or RFC3339)"
        )]
        to: String,
        #[arg(long, help = "Filter query")]
        filter: Option<String>,
        #[arg(long, help = "Filter by tags")]
        tags: Option<String>,
    },
    /// Search events
    Search {
        #[arg(long, help = "Search query")]
        query: String,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, default_value_t = 100, help = "Maximum results")]
        limit: i32,
    },
    /// Get event details
    Get { event_id: i64 },
}

// ---- Downtime ----
#[derive(Subcommand)]
enum DowntimeActions {
    /// List all downtimes
    List,
    /// Get downtime details
    Get { id: String },
    /// Create a downtime from JSON file
    Create {
        #[arg(long)]
        file: String,
    },
    /// Cancel a downtime
    Cancel { id: String },
}

// ---- DDSQL ----
#[derive(Subcommand)]
enum DdsqlActions {
    /// Execute DDSQL query and return columnar table data
    Table {
        #[arg(long, help = "DDSQL query string")]
        query: String,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time (e.g., 1h, 30m, 7d, now, unix timestamp)"
        )]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, help = "Aggregation interval in milliseconds (default: 60000)")]
        interval: Option<i64>,
        #[arg(long, default_value_t = 50, help = "Maximum number of rows to return")]
        limit: i32,
        #[arg(long, help = "Number of rows to skip (for pagination)")]
        offset: Option<i32>,
    },
    /// Execute DDSQL query and return time series data
    #[command(name = "time-series")]
    TimeSeries {
        #[arg(long, help = "DDSQL query string")]
        query: String,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, help = "Aggregation interval in milliseconds (default: 60000)")]
        interval: Option<i64>,
        #[arg(
            long,
            default_value_t = 5000,
            help = "Maximum number of rows to return"
        )]
        limit: i32,
    },
}

// ---- Tags ----
#[derive(Subcommand)]
enum TagActions {
    /// List all host tags
    List,
    /// Get tags for a host
    Get { hostname: String },
    /// Add tags to a host
    Add { hostname: String, tags: Vec<String> },
    /// Update host tags
    Update { hostname: String, tags: Vec<String> },
    /// Delete all tags from a host
    Delete { hostname: String },
}

// ---- Users ----
#[derive(Subcommand)]
enum UserActions {
    /// List users
    List,
    /// Get user details
    Get { user_id: String },
    /// Manage roles
    Roles {
        #[command(subcommand)]
        action: UserRoleActions,
    },
    /// Manage seat assignments
    Seats {
        #[command(subcommand)]
        action: SeatsActions,
    },
}

#[derive(Subcommand)]
enum UserRoleActions {
    /// List roles
    List,
}

// ---- Widgets ----
#[derive(Subcommand)]
enum WidgetActions {
    /// Search and list widgets for a given experience type
    List {
        /// Experience type (ccm_reports, logs_reports, csv_reports, product_analytics)
        experience_type: String,
        #[arg(long, help = "Filter by widget type")]
        filter_widget_type: Option<String>,
        #[arg(long, help = "Filter by creator email handle")]
        filter_creator_handle: Option<String>,
        #[arg(long, help = "Filter to only favorited widgets")]
        filter_is_favorited: Option<bool>,
        #[arg(long, help = "Filter by title (substring match)")]
        filter_title: Option<String>,
        #[arg(
            long,
            help = "Filter by tags (bracket-delimited CSV, e.g. [tag1,tag2])"
        )]
        filter_tags: Option<String>,
        #[arg(
            long,
            help = "Sort field (title, created_at, modified_at; prefix with - for descending)"
        )]
        sort: Option<String>,
        #[arg(long, help = "Page number (0-indexed)")]
        page_number: Option<i32>,
        #[arg(long, help = "Number of widgets per page")]
        page_size: Option<i32>,
    },
    /// Get a widget by UUID
    Get {
        /// Experience type (ccm_reports, logs_reports, csv_reports, product_analytics)
        experience_type: String,
        /// Widget UUID
        widget_id: String,
    },
    /// Create a widget from a JSON file
    Create {
        /// Experience type (ccm_reports, logs_reports, csv_reports, product_analytics)
        experience_type: String,
        #[arg(long)]
        file: String,
    },
    /// Update a widget from a JSON file
    Update {
        /// Experience type (ccm_reports, logs_reports, csv_reports, product_analytics)
        experience_type: String,
        /// Widget UUID
        widget_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete a widget by UUID
    Delete {
        /// Experience type (ccm_reports, logs_reports, csv_reports, product_analytics)
        experience_type: String,
        /// Widget UUID
        widget_id: String,
    },
}

// ---- Workflows ----
#[derive(Subcommand)]
enum WorkflowActions {
    /// Get a workflow by ID
    Get { workflow_id: String },
    /// Create a workflow from a JSON file
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update a workflow from a JSON file
    Update {
        workflow_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete a workflow
    Delete { workflow_id: String },
    /// Execute a workflow via API trigger (requires DD_API_KEY + DD_APP_KEY)
    ///
    /// The workflow must have an API trigger configured.
    /// OAuth tokens are not supported — this command requires API key authentication.
    #[command(verbatim_doc_comment)]
    Run {
        workflow_id: String,
        #[arg(long, help = "JSON payload for workflow input parameters")]
        payload: Option<String>,
        #[arg(long, help = "Path to a JSON file with input parameters")]
        payload_file: Option<String>,
        #[arg(long, help = "Wait for the workflow to complete before returning")]
        wait: bool,
        #[arg(
            long,
            default_value = "5m",
            help = "Timeout when --wait is set (e.g. 30s, 5m, 1h)"
        )]
        timeout: String,
    },
    /// Manage workflow instances (executions)
    Instances {
        #[command(subcommand)]
        action: WorkflowInstanceActions,
    },
}

#[derive(Subcommand)]
enum WorkflowInstanceActions {
    /// List instances of a workflow
    List {
        workflow_id: String,
        #[arg(long, default_value_t = 10, help = "Page size (max 100)")]
        limit: i64,
        #[arg(long, default_value_t = 0, help = "Page number")]
        page: i64,
    },
    /// Get a specific workflow instance
    Get {
        workflow_id: String,
        instance_id: String,
    },
    /// Cancel a running workflow instance
    Cancel {
        workflow_id: String,
        instance_id: String,
    },
}

// ---- Infrastructure ----
#[derive(Subcommand)]
enum InfraActions {
    /// Manage hosts
    Hosts {
        #[command(subcommand)]
        action: InfraHostActions,
    },
}

#[derive(Subcommand)]
enum InfraHostActions {
    /// List hosts
    List {
        #[arg(long, help = "Filter hosts")]
        filter: Option<String>,
        #[arg(long, default_value = "status", help = "Sort field")]
        sort: String,
        #[arg(long, default_value_t = 100, help = "Maximum hosts")]
        count: i64,
    },
    /// Get host details
    Get { hostname: String },
}

// ---- IDP (Internal Developer Portal) ----
#[derive(Subcommand)]
enum IdpActions {
    /// Get full context summary with suggested next actions
    ///
    /// The flagship IDP command. Makes parallel API calls to return
    /// a single unified view of any service entity:
    ///
    /// RETURNS:
    ///   • Entity info (name, kind, description, lifecycle, tier, owner)
    ///   • Owner team details (handle, name, member count, Slack channels)
    ///   • Health signals (monitors, incidents, SLOs — pre-computed counts)
    ///   • Dependencies (upstream/downstream services)
    ///   • Links (dashboards, repos, runbooks)
    ///   • Metadata gaps (missing description, lifecycle, tier, runbook, docs)
    ///   • Suggested next actions (based on current health and gaps)
    ///
    /// START HERE — this is the best first command to run for any entity.
    /// Use the other commands (owner, deps, find) to drill deeper.
    ///
    /// EXAMPLES:
    ///   pup idp assist catalog-http
    ///   pup idp assist payment-service
    ///   pup idp assist api-gateway
    #[command(verbatim_doc_comment)]
    Assist {
        /// Entity name (e.g. "catalog-http", "payment-service")
        entity: String,
    },
    /// Find entities by name or query
    ///
    /// Search the entity graph for services, resources, or other entities.
    /// Useful when you don't know the exact entity name.
    ///
    /// QUERY SYNTAX:
    ///   Simple text searches by name. Prefix with kind: to filter by type.
    ///   Use AND to combine filters.
    ///
    /// EXAMPLES:
    ///   pup idp find "catalog"
    ///   pup idp find "kind:service AND name:payment"
    ///   pup idp find "kind:service AND owner:platform"
    #[command(verbatim_doc_comment)]
    Find {
        /// Search query (e.g. "catalog", "kind:service AND name:payment")
        query: String,
    },
    /// Resolve ownership, team details, and on-call context
    ///
    /// Returns the owning team, member count, Slack channels,
    /// and on-call information for routing questions or incidents.
    ///
    /// EXAMPLES:
    ///   pup idp owner catalog-http
    ///   pup idp owner payment-service
    #[command(verbatim_doc_comment)]
    Owner {
        /// Entity name
        entity: String,
    },
    /// Show upstream and downstream service dependencies
    ///
    /// Returns which services depend on this entity (upstream)
    /// and which services this entity calls (downstream).
    /// Useful for blast-radius analysis before making changes.
    ///
    /// EXAMPLES:
    ///   pup idp deps catalog-http
    ///   pup idp deps api-gateway
    #[command(verbatim_doc_comment)]
    Deps {
        /// Entity name
        entity: String,
    },
    /// Register a service definition from a YAML file
    ///
    /// POSTs a service.datadog.yaml file to the Datadog Service Catalog API.
    /// The file should use the v2.2 schema format.
    ///
    /// EXAMPLES:
    ///   pup idp register services/checkout-api/service.datadog.yaml
    ///   pup idp register ./service.datadog.yaml
    #[command(verbatim_doc_comment)]
    Register {
        /// Path to the service.datadog.yaml file
        file: String,
    },
}

// ---- Audit Logs ----
#[derive(Subcommand)]
enum AuditLogActions {
    /// List recent audit logs
    List {
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, default_value_t = 100, help = "Maximum results")]
        limit: i32,
    },
    /// Search audit logs
    Search {
        #[arg(long, help = "Search query (required)")]
        query: String,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, default_value_t = 100, help = "Maximum results")]
        limit: i32,
    },
}

// ---- Bits AI ----
#[derive(Subcommand)]
enum BitsActions {
    /// Ask Bits AI a natural-language question
    Ask {
        /// The question or prompt to send to Bits AI (optional with --interactive)
        query: Option<String>,
        #[arg(long, help = "Bits AI agent ID (auto-discovered if omitted)")]
        agent_id: Option<String>,
        #[arg(
            long,
            help = "Collect the full response before printing (disables streaming)"
        )]
        no_stream: bool,
        #[arg(
            long,
            short = 'i',
            help = "Start an interactive conversation (Ctrl+D or 'exit' to quit)"
        )]
        interactive: bool,
    },
}

// ---- Security ----
#[derive(Subcommand)]
enum SecurityActions {
    /// Manage security rules
    Rules {
        #[command(subcommand)]
        action: SecurityRuleActions,
    },
    /// Manage security signals
    Signals {
        #[command(subcommand)]
        action: SecuritySignalActions,
    },
    /// Manage security findings
    Findings {
        #[command(subcommand)]
        action: SecurityFindingActions,
    },
    /// Manage security content packs
    #[command(name = "content-packs")]
    ContentPacks {
        #[command(subcommand)]
        action: SecurityContentPackActions,
    },
    /// Manage entity risk scores
    #[command(name = "risk-scores")]
    RiskScores {
        #[command(subcommand)]
        action: SecurityRiskScoreActions,
    },
    /// Manage security suppression rules
    Suppressions {
        #[command(subcommand)]
        action: SecuritySuppressionActions,
    },
}

#[derive(Subcommand)]
enum SecurityRuleActions {
    /// List security rules
    List {
        #[arg(long, help = "Filter query")]
        filter: Option<String>,
        #[arg(
            long,
            help = "Sort order (name, -name, creation_date, -creation_date, update_date, -update_date, enabled, -enabled, type, -type, highest_severity, -highest_severity, source, -source)"
        )]
        sort: Option<String>,
    },
    /// Get rule details
    Get { rule_id: String },
    /// Bulk export security monitoring rules
    #[command(name = "bulk-export")]
    BulkExport {
        /// Rule IDs to export
        rule_ids: Vec<String>,
    },
}

#[derive(Subcommand)]
enum SecuritySignalActions {
    /// List security signals
    List {
        #[arg(long, help = "Search query using log search syntax (required)")]
        query: String,
        #[arg(long, default_value = "1h")]
        from: String,
        #[arg(long, default_value = "now")]
        to: String,
        #[arg(long, default_value_t = 100, help = "Maximum results (1-1000)")]
        limit: i32,
        #[arg(long, help = "Sort field: severity, status, timestamp")]
        sort: Option<String>,
    },
}

#[derive(Subcommand)]
enum SecurityFindingActions {
    /// Search security findings
    Search {
        #[arg(long)]
        query: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: i64,
    },
}

#[derive(Subcommand)]
enum SecurityContentPackActions {
    /// List content pack states
    List,
    /// Activate a content pack
    Activate { pack_id: String },
    /// Deactivate a content pack
    Deactivate { pack_id: String },
}

#[derive(Subcommand)]
enum SecurityRiskScoreActions {
    /// List entity risk scores
    List {
        #[arg(long)]
        query: Option<String>,
    },
}

#[derive(Subcommand)]
enum SecuritySuppressionActions {
    /// List suppression rules
    List {
        #[arg(
            long,
            help = "Sort order (name, -name, start_date, -start_date, expiration_date, -expiration_date, update_date, -update_date, -creation_date, enabled, -enabled)"
        )]
        sort: Option<String>,
    },
    /// Get suppression rule details
    Get { suppression_id: String },
    /// Create a suppression rule
    Create {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update a suppression rule
    Update {
        suppression_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete a suppression rule
    Delete { suppression_id: String },
    /// Validate a suppression rule
    Validate {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
}

// ---- Seats ----
#[derive(Subcommand)]
enum SeatsActions {
    /// Manage seat assignments
    Users {
        #[command(subcommand)]
        action: SeatsUserActions,
    },
}

#[derive(Subcommand)]
enum SeatsUserActions {
    /// List users with seats
    List {
        #[arg(long, help = "Product (e.g. incident_response)")]
        product: String,
        #[arg(long, default_value_t = 100, help = "Page limit")]
        limit: i32,
    },
    /// Assign seats to users
    Assign {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Unassign seats from users
    Unassign {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
}

// ---- Change Management ----
#[derive(Subcommand)]
enum ChangeManagementActions {
    /// Create a change request
    Create {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Get a change request
    Get { change_request_id: String },
    /// Update a change request
    Update {
        change_request_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Create a branch for a change request
    #[command(name = "create-branch")]
    CreateBranch {
        change_request_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Manage change request decisions
    Decisions {
        #[command(subcommand)]
        action: ChangeRequestDecisionActions,
    },
}

#[derive(Subcommand)]
enum ChangeRequestDecisionActions {
    /// Delete a decision
    Delete {
        change_request_id: String,
        decision_id: String,
    },
    /// Update a decision
    Update {
        change_request_id: String,
        decision_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
}

// ---- Cloud Authentication ----
#[derive(Subcommand)]
enum CloudAuthActions {
    /// Manage AWS persona mappings
    #[command(name = "persona-mappings")]
    PersonaMappings {
        #[command(subcommand)]
        action: CloudAuthPersonaMappingActions,
    },
}

#[derive(Subcommand)]
enum CloudAuthPersonaMappingActions {
    /// List persona mappings
    List,
    /// Get a persona mapping
    Get { mapping_id: String },
    /// Create a persona mapping
    Create {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete a persona mapping
    Delete { mapping_id: String },
}

// ---- Google Chat Integration ----
#[derive(Subcommand)]
enum GoogleChatActions {
    /// Manage organization handles
    Handles {
        #[command(subcommand)]
        action: GoogleChatHandleActions,
    },
    /// Get a space by display name
    #[command(name = "space-get")]
    SpaceGet {
        domain_name: String,
        space_display_name: String,
    },
}

#[derive(Subcommand)]
enum GoogleChatHandleActions {
    /// List organization handles
    List { org_id: String },
    /// Get an organization handle
    Get { org_id: String, handle_id: String },
    /// Create an organization handle
    Create {
        org_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update an organization handle
    Update {
        org_id: String,
        handle_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete an organization handle
    Delete { org_id: String, handle_id: String },
}

// ---- Organizations ----
#[derive(Subcommand)]
enum OrgActions {
    /// List organizations
    List,
    /// Get organization details
    Get,
}

// ---- Cloud ----
#[derive(Subcommand)]
enum CloudActions {
    /// Manage AWS integrations
    Aws {
        #[command(subcommand)]
        action: CloudAwsActions,
    },
    /// Manage GCP integrations
    Gcp {
        #[command(subcommand)]
        action: CloudGcpActions,
    },
    /// Manage Azure integrations
    Azure {
        #[command(subcommand)]
        action: CloudAzureActions,
    },
    /// Manage OCI integrations
    Oci {
        #[command(subcommand)]
        action: CloudOciActions,
    },
}

#[derive(Subcommand)]
enum CloudAwsActions {
    /// List AWS integrations
    List,
}

#[derive(Subcommand)]
enum CloudGcpActions {
    /// List GCP integrations
    List,
}

#[derive(Subcommand)]
enum CloudAzureActions {
    /// List Azure integrations
    List,
}

#[derive(Subcommand)]
enum CloudOciActions {
    /// Manage OCI tenancy configurations
    Tenancies {
        #[command(subcommand)]
        action: CloudOciTenancyActions,
    },
    /// Manage OCI products
    Products {
        #[command(subcommand)]
        action: CloudOciProductActions,
    },
}

#[derive(Subcommand)]
enum CloudOciProductActions {
    /// List OCI tenancy products
    List {
        #[arg(long, help = "Comma-separated product keys (required)")]
        product_keys: String,
    },
}

#[derive(Subcommand)]
enum CloudOciTenancyActions {
    /// List OCI tenancy configurations
    List,
    /// Get OCI tenancy configuration
    Get { tenancy_id: String },
    /// Create OCI tenancy configuration
    Create {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update OCI tenancy configuration
    Update {
        tenancy_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete OCI tenancy configuration
    Delete { tenancy_id: String },
}

// ---- Cases ----
#[derive(Subcommand)]
enum CaseActions {
    /// Search cases
    Search {
        #[arg(long, help = "Search query")]
        query: Option<String>,
        #[arg(long, default_value_t = 10, help = "Results per page")]
        page_size: i64,
        #[arg(long, default_value_t = 0, help = "Page number")]
        page_number: i64,
    },
    /// Get case details
    Get { case_id: String },
    /// Create a new case
    Create {
        #[arg(long, help = "Case title (required)", required_unless_present = "file")]
        title: Option<String>,
        #[arg(
            long,
            name = "type-id",
            help = "Case type UUID (required)",
            required_unless_present = "file"
        )]
        type_id: Option<String>,
        #[arg(long, default_value = "NOT_DEFINED", help = "Priority level")]
        priority: String,
        #[arg(long, help = "Case description")]
        description: Option<String>,
        #[arg(long, help = "JSON file with request body (required)", conflicts_with_all = ["title", "type-id"])]
        file: Option<String>,
    },
    /// Archive a case
    Archive { case_id: String },
    /// Unarchive a case
    Unarchive { case_id: String },
    /// Assign a case to a user
    Assign {
        case_id: String,
        #[arg(long, help = "User UUID (required)")]
        user_id: String,
    },
    /// Update case priority
    #[command(name = "update-priority")]
    UpdatePriority {
        case_id: String,
        #[arg(long, help = "New priority (required)")]
        priority: String,
    },
    /// Update case status
    #[command(name = "update-status")]
    UpdateStatus {
        case_id: String,
        #[arg(long, help = "New status (required)")]
        status: String,
    },
    /// Manage case projects
    Projects {
        #[command(subcommand)]
        action: CaseProjectActions,
    },
    /// Move a case to a different project
    Move {
        case_id: String,
        #[arg(long, help = "Target project ID (required)")]
        project_id: String,
    },
    /// Update case title
    #[command(name = "update-title")]
    UpdateTitle {
        case_id: String,
        #[arg(long, help = "New title (required)")]
        title: String,
    },
    /// Manage Jira integrations for cases
    Jira {
        #[command(subcommand)]
        action: CaseJiraActions,
    },
    /// Manage ServiceNow integrations for cases
    Servicenow {
        #[command(subcommand)]
        action: CaseServicenowActions,
    },
}

#[derive(Subcommand)]
enum CaseProjectActions {
    /// List all projects
    List,
    /// Get project details
    Get { project_id: String },
    /// Create a new project
    Create {
        #[arg(long, help = "Project name (required)")]
        name: String,
        #[arg(long, help = "Project key (required)")]
        key: String,
    },
    /// Delete a project
    Delete { project_id: String },
    /// Update a project
    Update {
        project_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Manage project notification rules
    #[command(name = "notification-rules")]
    NotificationRules {
        #[command(subcommand)]
        action: CaseNotificationRuleActions,
    },
}

#[derive(Subcommand)]
enum CaseJiraActions {
    /// Create a Jira issue for a case
    #[command(name = "create-issue")]
    CreateIssue {
        case_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Link a Jira issue to a case
    Link {
        case_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Unlink a Jira issue from a case
    Unlink { case_id: String },
}

#[derive(Subcommand)]
enum CaseServicenowActions {
    /// Create a ServiceNow ticket for a case
    #[command(name = "create-ticket")]
    CreateTicket {
        case_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
}

#[derive(Subcommand)]
enum CaseNotificationRuleActions {
    /// List notification rules for a project
    List { project_id: String },
    /// Create a notification rule
    Create {
        project_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update a notification rule
    Update {
        project_id: String,
        rule_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete a notification rule
    Delete { project_id: String, rule_id: String },
}

// ---- Service Catalog ----
#[derive(Subcommand)]
enum ServiceCatalogActions {
    /// List services
    List,
    /// Get service details
    Get { service_name: String },
}

// ---- API Keys ----
#[derive(Subcommand)]
enum ApiKeyActions {
    /// List API keys
    List,
    /// Get API key details
    Get { key_id: String },
    /// Create new API key
    Create {
        #[arg(long, help = "API key name (required)")]
        name: String,
    },
    /// Delete an API key (DESTRUCTIVE)
    Delete { key_id: String },
}

// ---- App Builder ----
#[derive(Subcommand)]
enum AppBuilderActions {
    /// List App Builder applications
    List {
        /// Filter apps by query string
        #[arg(long, help = "Filter apps by query string")]
        query: Option<String>,
    },
    /// Get App Builder application details
    Get {
        /// App ID (UUID)
        #[arg(name = "app-id")]
        app_id: String,
    },
    /// Create a new App Builder application
    Create {
        #[arg(
            long,
            name = "body",
            help = "JSON body (@filepath or - for stdin) (required)"
        )]
        file: String,
    },
    /// Update an App Builder application
    Update {
        /// App ID (UUID)
        #[arg(name = "app-id")]
        app_id: String,
        #[arg(
            long,
            name = "body",
            help = "JSON body (@filepath or - for stdin) (required)"
        )]
        file: String,
    },
    /// Delete an App Builder application (DESTRUCTIVE)
    Delete {
        /// App ID (UUID)
        #[arg(name = "app-id")]
        app_id: String,
    },
    /// Delete multiple App Builder applications (DESTRUCTIVE)
    #[command(name = "delete-batch")]
    DeleteBatch {
        /// Comma-separated list of app IDs (UUIDs)
        #[arg(long, value_delimiter = ',', help = "Comma-separated list of app IDs")]
        app_ids: Vec<String>,
    },
    /// Publish an App Builder application
    Publish {
        /// App ID (UUID)
        #[arg(name = "app-id")]
        app_id: String,
    },
    /// Unpublish an App Builder application
    Unpublish {
        /// App ID (UUID)
        #[arg(name = "app-id")]
        app_id: String,
    },
}

// ---- App Keys ----
#[derive(Subcommand)]
enum AppKeyActions {
    /// List application keys
    List {
        /// Results per page
        #[arg(long, default_value = "10", help = "Number of results per page")]
        page_size: i64,
        /// Page number (0-indexed)
        #[arg(
            long,
            default_value = "0",
            help = "Page number to retrieve (0-indexed)"
        )]
        page_number: i64,
        /// Filter by key name
        #[arg(long, default_value = "", help = "Filter by key name")]
        filter: String,
        /// Sort field (name, -name, created_at, -created_at)
        #[arg(
            long,
            default_value = "",
            help = "Sort field (name, -name, created_at, -created_at)"
        )]
        sort: String,
        /// List all org keys (requires API keys, not OAuth)
        #[arg(
            long,
            default_value_t = false,
            help = "List all org keys (requires API keys, not OAuth)"
        )]
        all: bool,
    },
    /// Get application key details
    Get {
        /// App key ID
        #[arg(name = "app-key-id")]
        key_id: String,
    },
    /// Create a new application key
    Create {
        /// Application key name (required)
        #[arg(long, help = "Application key name (required)")]
        name: String,
        /// Comma-separated authorization scopes (e.g. dashboards_read,metrics_read)
        #[arg(
            long,
            default_value = "",
            help = "Comma-separated authorization scopes"
        )]
        scopes: String,
    },
    /// Update an application key
    Update {
        /// App key ID
        #[arg(name = "app-key-id")]
        key_id: String,
        /// New name for the application key
        #[arg(long, default_value = "", help = "New name for the application key")]
        name: String,
        /// Comma-separated authorization scopes
        #[arg(
            long,
            default_value = "",
            help = "Comma-separated authorization scopes"
        )]
        scopes: String,
    },
    /// Delete an application key (DESTRUCTIVE)
    Delete {
        /// App key ID to delete
        #[arg(name = "app-key-id")]
        key_id: String,
    },
}

// ---- Usage ----
#[derive(Subcommand)]
enum UsageActions {
    /// Get usage summary
    Summary {
        #[arg(
            long,
            default_value = "30d",
            help = "Start time (30d, 60d, YYYY-MM-DD, or RFC3339)"
        )]
        from: String,
        #[arg(long, help = "End time (now, YYYY-MM-DD, or RFC3339)")]
        to: Option<String>,
    },
    /// Get hourly usage
    Hourly {
        #[arg(
            long,
            default_value = "1d",
            help = "Start time (1d, 7d, YYYY-MM-DD, or RFC3339)"
        )]
        from: String,
        #[arg(long, help = "End time (now, YYYY-MM-DD, or RFC3339)")]
        to: Option<String>,
    },
}

// ---- Notebooks ----
#[derive(Subcommand)]
enum NotebookActions {
    /// List notebooks
    List,
    /// Get notebook details
    Get { notebook_id: i64 },
    /// Create a new notebook
    Create {
        #[arg(
            long,
            name = "body",
            help = "JSON body (@filepath or - for stdin) (required)"
        )]
        file: String,
    },
    /// Update a notebook
    Update {
        notebook_id: i64,
        #[arg(
            long,
            name = "body",
            help = "JSON body (@filepath or - for stdin) (required)"
        )]
        file: String,
    },
    /// Delete a notebook
    Delete { notebook_id: i64 },
}

// ---- RUM ----
#[derive(Subcommand)]
enum RumActions {
    /// Manage RUM applications
    Apps {
        #[command(subcommand)]
        action: RumAppActions,
    },
    /// List RUM events
    Events {
        #[arg(
            long,
            help = "RUM query filter (e.g. '@type:error @application.name:\"My App\"')"
        )]
        query: Option<String>,
        #[arg(long, default_value = "1h")]
        from: String,
        #[arg(long, default_value = "now")]
        to: String,
        #[arg(long, default_value_t = 100)]
        limit: i32,
    },
    /// Query RUM session replay data
    Sessions {
        #[command(subcommand)]
        action: RumSessionActions,
    },
    /// Manage RUM custom metrics
    Metrics {
        #[command(subcommand)]
        action: RumMetricActions,
    },
    /// Manage RUM retention filters
    #[command(name = "retention-filters")]
    RetentionFilters {
        #[command(subcommand)]
        action: RumRetentionFilterActions,
    },
    /// Manage session replay playlists
    Playlists {
        #[command(subcommand)]
        action: RumPlaylistActions,
    },
    /// Query RUM interaction heatmaps
    Heatmaps {
        #[command(subcommand)]
        action: RumHeatmapActions,
    },
}

#[derive(Subcommand)]
enum RumAppActions {
    /// List all RUM applications
    List,
    /// Get RUM application details
    Get {
        #[arg(help = "Application ID (required)")]
        app_id: String,
    },
    /// Create a new RUM application
    Create {
        #[arg(long, help = "Application name (required)")]
        name: String,
        #[arg(long, name = "type", help = "Application type (required)")]
        app_type: Option<String>,
    },
    /// Update a RUM application
    Update {
        #[arg(help = "Application ID (required)")]
        app_id: String,
        #[arg(long, help = "Application name")]
        name: Option<String>,
        #[arg(long, name = "type", help = "Application type")]
        app_type: Option<String>,
        #[arg(long)]
        file: Option<String>,
    },
    /// Delete a RUM application
    Delete {
        #[arg(help = "Application ID (required)")]
        app_id: String,
    },
}

#[derive(Subcommand)]
enum RumSessionActions {
    /// Search RUM sessions
    Search {
        #[arg(long)]
        query: Option<String>,
        #[arg(long, default_value = "1h")]
        from: String,
        #[arg(long, default_value = "now")]
        to: String,
        #[arg(long, default_value_t = 100)]
        limit: i32,
    },
    /// List RUM sessions
    List {
        #[arg(long, default_value = "1h")]
        from: String,
        #[arg(long, default_value = "now")]
        to: String,
        #[arg(long, default_value_t = 100)]
        limit: i32,
    },
}

#[derive(Subcommand)]
enum RumMetricActions {
    /// List all RUM custom metrics
    List,
    /// Get RUM custom metric details
    Get { metric_id: String },
    /// Create a RUM custom metric
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update a RUM custom metric
    Update {
        metric_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete a RUM custom metric
    Delete { metric_id: String },
}

#[derive(Subcommand)]
enum RumRetentionFilterActions {
    /// List all retention filters
    List { app_id: String },
    /// Get retention filter details
    Get { app_id: String, filter_id: String },
    /// Create a retention filter
    Create {
        app_id: String,
        #[arg(long)]
        file: String,
    },
    /// Update a retention filter
    Update {
        app_id: String,
        filter_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete a retention filter
    Delete { app_id: String, filter_id: String },
}

#[derive(Subcommand)]
enum RumPlaylistActions {
    /// List session replay playlists
    List,
    /// Get playlist details
    Get { playlist_id: i32 },
}

#[derive(Subcommand)]
enum RumHeatmapActions {
    /// Query heatmap data
    Query {
        #[arg(long)]
        view_name: String,
        #[arg(long, help = "Time range start")]
        from: Option<String>,
        #[arg(long, help = "Time range end")]
        to: Option<String>,
    },
}

// ---- CI/CD ----
#[derive(Subcommand)]
enum CicdActions {
    /// Manage CI pipelines
    Pipelines {
        #[command(subcommand)]
        action: CicdPipelineActions,
    },
    /// Query CI test events
    Tests {
        #[command(subcommand)]
        action: CicdTestActions,
    },
    /// Query CI/CD events
    Events {
        #[command(subcommand)]
        action: CicdEventActions,
    },
    /// Manage DORA metrics
    Dora {
        #[command(subcommand)]
        action: CicdDoraActions,
    },
    /// Manage flaky tests
    #[command(name = "flaky-tests")]
    FlakyTests {
        #[command(subcommand)]
        action: CicdFlakyTestActions,
    },
}

#[derive(Subcommand)]
enum CicdPipelineActions {
    /// List CI pipelines
    List {
        #[arg(long, help = "Search query")]
        query: Option<String>,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, default_value_t = 50, help = "Maximum results")]
        limit: i32,
        #[arg(long, help = "Filter by git branch")]
        branch: Option<String>,
        #[arg(long, help = "Filter by pipeline name")]
        pipeline_name: Option<String>,
    },
    /// Get pipeline details
    Get {
        #[arg(long, help = "Pipeline ID (required)")]
        pipeline_id: String,
    },
}

#[derive(Subcommand)]
enum CicdTestActions {
    /// List CI test events
    List {
        #[arg(long, help = "Search query")]
        query: Option<String>,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, default_value_t = 50, help = "Maximum results")]
        limit: i32,
    },
    /// Search CI test events
    Search {
        #[arg(long, help = "Search query (required)")]
        query: String,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, default_value_t = 50, help = "Maximum results")]
        limit: i32,
    },
    /// Aggregate CI test events
    Aggregate {
        #[arg(long, help = "Search query (required)")]
        query: String,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, default_value = "count", help = "Aggregation function")]
        compute: String,
        #[arg(long, help = "Group by field(s)")]
        group_by: Option<String>,
        #[arg(long, default_value_t = 10, help = "Maximum groups")]
        limit: i32,
    },
}

#[derive(Subcommand)]
enum CicdEventActions {
    /// Search CI/CD events
    Search {
        #[arg(long, help = "Search query (required)")]
        query: String,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, default_value_t = 50, help = "Maximum results")]
        limit: i32,
        #[arg(long, default_value = "desc", help = "Sort order (asc or desc)")]
        sort: String,
    },
    /// Aggregate CI/CD events
    Aggregate {
        #[arg(long, help = "Search query (required)")]
        query: String,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, default_value = "count", help = "Aggregation function")]
        compute: String,
        #[arg(long, help = "Group by field(s)")]
        group_by: Option<String>,
        #[arg(long, default_value_t = 10, help = "Maximum groups")]
        limit: i32,
    },
}

#[derive(Subcommand)]
enum CicdDoraActions {
    /// Patch a DORA deployment
    #[command(name = "patch-deployment")]
    PatchDeployment {
        deployment_id: String,
        #[arg(long, help = "JSON file with patch data (required)")]
        file: String,
    },
}

#[derive(Subcommand)]
enum CicdFlakyTestActions {
    /// Search flaky tests
    Search {
        #[arg(long, help = "Search query")]
        query: Option<String>,
        #[arg(long, help = "Pagination cursor")]
        cursor: Option<String>,
        #[arg(long, default_value_t = 100, help = "Maximum results")]
        limit: i64,
        #[arg(long, default_value_t = false, help = "Include status history")]
        include_history: bool,
        #[arg(
            long,
            help = "Sort order (fqn, -fqn, first_flaked, -first_flaked, last_flaked, -last_flaked, failure_rate, -failure_rate, pipelines_failed, -pipelines_failed, pipelines_duration_lost, -pipelines_duration_lost)"
        )]
        sort: Option<String>,
    },
    /// Update flaky tests
    Update {
        #[arg(long, help = "JSON file with flaky tests data (required)")]
        file: String,
    },
}

// ---- On-Call ----
#[derive(Subcommand)]
enum OnCallActions {
    /// Manage teams
    Teams {
        #[command(subcommand)]
        action: OnCallTeamActions,
    },
}

#[derive(Subcommand)]
enum OnCallTeamActions {
    /// List all teams
    List,
    /// Get team details
    Get { team_id: String },
    /// Create a new team
    Create {
        #[arg(long, help = "Team display name (required)")]
        name: String,
        #[arg(long, help = "Team handle (required)")]
        handle: String,
        #[arg(long, help = "Team description")]
        description: Option<String>,
        #[arg(long, help = "Team avatar URL")]
        avatar: Option<String>,
        #[arg(long, default_value_t = false, help = "Hide team from UI")]
        hidden: bool,
    },
    /// Update team details
    Update {
        team_id: String,
        #[arg(long, help = "Team display name (required)")]
        name: String,
        #[arg(long, help = "Team handle (required)")]
        handle: String,
    },
    /// Delete a team
    Delete { team_id: String },
    /// Manage team memberships
    Memberships {
        #[command(subcommand)]
        action: OnCallMembershipActions,
    },
}

#[derive(Subcommand)]
enum OnCallMembershipActions {
    /// List team members
    List {
        team_id: String,
        #[arg(long, default_value_t = 100, help = "Results per page")]
        page_size: i64,
        #[arg(long, default_value_t = 0, help = "Page number")]
        page_number: i64,
        #[arg(long, default_value = "name", help = "Sort order: name, email")]
        sort: String,
    },
    /// Add a member to team
    Add {
        team_id: String,
        #[arg(long, help = "User UUID (required)")]
        user_id: String,
        #[arg(long, default_value = "member", help = "Role: member or admin")]
        role: Option<String>,
    },
    /// Update member role
    Update {
        team_id: String,
        user_id: String,
        #[arg(long, help = "Role: member or admin")]
        role: String,
    },
    /// Remove member from team
    Remove { team_id: String, user_id: String },
}

// ---- Fleet ----
#[derive(Subcommand)]
enum FleetActions {
    /// Manage fleet agents
    Agents {
        #[command(subcommand)]
        action: FleetAgentActions,
    },
    /// Manage fleet deployments
    Deployments {
        #[command(subcommand)]
        action: FleetDeploymentActions,
    },
    /// Manage fleet schedules
    Schedules {
        #[command(subcommand)]
        action: FleetScheduleActions,
    },
}

#[derive(Subcommand)]
enum FleetAgentActions {
    /// List fleet agents
    List {
        #[arg(long)]
        page_size: Option<i64>,
        #[arg(
            long,
            help = "Filter query (e.g. ip_address:1.2.3.4, hostname:my-host)"
        )]
        filter: Option<String>,
    },
    /// Get fleet agent details
    Get { agent_key: String },
    /// List available agent versions
    Versions,
}

#[derive(Subcommand)]
enum FleetDeploymentActions {
    /// List fleet deployments
    List {
        #[arg(long)]
        page_size: Option<i64>,
    },
    /// Get fleet deployment details
    Get { deployment_id: String },
    /// Cancel a fleet deployment
    Cancel { deployment_id: String },
    /// Create a configuration deployment
    Configure {
        #[arg(long)]
        file: String,
    },
    /// Create an upgrade deployment
    Upgrade {
        #[arg(long)]
        file: String,
    },
}

#[derive(Subcommand)]
enum FleetScheduleActions {
    /// List fleet schedules
    List,
    /// Get fleet schedule details
    Get { schedule_id: String },
    /// Create a fleet schedule
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update a fleet schedule
    Update {
        schedule_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete a fleet schedule
    Delete { schedule_id: String },
    /// Trigger a fleet schedule
    Trigger { schedule_id: String },
}

// ---- Data Governance ----
#[derive(Subcommand)]
enum DataGovActions {
    /// Manage sensitive data scanner
    Scanner {
        #[command(subcommand)]
        action: DataGovScannerActions,
    },
}

#[derive(Subcommand)]
enum DataGovScannerActions {
    /// Manage scanning rules
    Rules {
        #[command(subcommand)]
        action: DataGovScannerRuleActions,
    },
}

#[derive(Subcommand)]
enum DataGovScannerRuleActions {
    /// List scanning rules
    List,
}

// ---- Deployment Gates ----
#[derive(Subcommand)]
enum DeploymentGatesActions {
    /// Manage deployment gates (CRUD)
    Gates {
        #[command(subcommand)]
        action: DeploymentGatesGateActions,
    },
    /// Manage deployment gate evaluations
    Evaluations {
        #[command(subcommand)]
        action: DeploymentGatesEvaluationActions,
    },
    /// Manage deployment gate rules
    Rules {
        #[command(subcommand)]
        action: DeploymentGatesRuleActions,
    },
}

#[derive(Subcommand)]
enum DeploymentGatesGateActions {
    /// List all deployment gates
    List {
        /// Pagination cursor
        #[arg(long)]
        page_cursor: Option<String>,
        /// Page size (1-1000, default 50)
        #[arg(long)]
        page_size: Option<i64>,
    },
    /// Get a deployment gate by ID
    Get { gate_id: String },
    /// Create a deployment gate from a JSON file
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update a deployment gate from a JSON file
    Update {
        gate_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete a deployment gate
    Delete { gate_id: String },
}

#[derive(Subcommand)]
enum DeploymentGatesEvaluationActions {
    /// Get a deployment gates evaluation result by evaluation ID (UUID)
    Get { evaluation_id: String },
    /// Trigger a deployment gates evaluation from a JSON file
    Trigger {
        #[arg(long)]
        file: String,
    },
}

#[derive(Subcommand)]
enum DeploymentGatesRuleActions {
    /// List rules for a deployment gate
    List { gate_id: String },
    /// Get a specific deployment rule
    Get { gate_id: String, rule_id: String },
    /// Create a deployment rule from a JSON file
    Create {
        gate_id: String,
        #[arg(long)]
        file: String,
    },
    /// Update a deployment rule from a JSON file
    Update {
        gate_id: String,
        rule_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete a deployment rule
    Delete { gate_id: String, rule_id: String },
}

// ---- Error Tracking ----
#[derive(Subcommand)]
enum ErrorTrackingActions {
    /// Manage error issues
    Issues {
        #[command(subcommand)]
        action: ErrorTrackingIssueActions,
    },
}

#[derive(Subcommand)]
enum ErrorTrackingIssueActions {
    /// Search error issues
    Search {
        #[arg(long, default_value = "*", help = "Search query to filter issues")]
        query: Option<String>,
        #[arg(
            long,
            default_value_t = 10,
            help = "Maximum number of issues to return"
        )]
        limit: i32,
        #[arg(long, default_value = "1d", help = "Start time (relative or absolute)")]
        from: String,
        #[arg(long, default_value = "now", help = "End time (relative or absolute)")]
        to: String,
        #[arg(
            long,
            default_value = "TOTAL_COUNT",
            help = "Sort order: TOTAL_COUNT, FIRST_SEEN, IMPACTED_SESSIONS, PRIORITY"
        )]
        order_by: String,
        #[arg(
            long,
            conflicts_with = "persona",
            required_unless_present = "persona",
            help = "Error source track: trace, logs, or rum"
        )]
        track: Option<String>,
        #[arg(
            long,
            conflicts_with = "track",
            required_unless_present = "track",
            help = "Client persona filter: ALL, BROWSER, MOBILE, or BACKEND"
        )]
        persona: Option<String>,
    },
    /// Get issue details
    Get { issue_id: String },
}

// ---- Code Coverage ----
#[derive(Subcommand)]
enum CodeCoverageActions {
    /// Get branch coverage summary
    #[command(name = "branch-summary")]
    BranchSummary {
        #[arg(long, help = "Repository name (required)")]
        repo: String,
        #[arg(long, help = "Branch name (required)")]
        branch: String,
    },
    /// Get commit coverage summary
    #[command(name = "commit-summary")]
    CommitSummary {
        #[arg(long, help = "Repository name (required)")]
        repo: String,
        #[arg(long, help = "Commit SHA (required)")]
        commit: String,
    },
}

// ---- Feature Flags ----
#[derive(Subcommand)]
enum FeatureFlagActions {
    /// Manage feature flag definitions
    Flags {
        #[command(subcommand)]
        action: FeatureFlagFlagActions,
    },
    /// Manage feature flag environments
    Environments {
        #[command(subcommand)]
        action: FeatureFlagEnvActions,
    },
    /// Enable a feature flag in an environment
    Enable {
        #[arg(help = "Feature flag ID (UUID)")]
        feature_flag_id: String,
        #[arg(help = "Environment ID (UUID)")]
        feature_flags_env_id: String,
    },
    /// Disable a feature flag in an environment
    Disable {
        #[arg(help = "Feature flag ID (UUID)")]
        feature_flag_id: String,
        #[arg(help = "Environment ID (UUID)")]
        feature_flags_env_id: String,
    },
}

#[derive(Subcommand)]
enum FeatureFlagFlagActions {
    /// List feature flags
    List {
        #[arg(long, help = "Filter by key (partial match)")]
        key: Option<String>,
        #[arg(long, help = "Filter by archived status")]
        is_archived: Option<bool>,
        #[arg(long, help = "Maximum number of results to return")]
        limit: Option<i32>,
        #[arg(long, help = "Number of results to skip")]
        offset: Option<i32>,
    },
    /// Get a feature flag
    Get {
        #[arg(help = "Feature flag ID (UUID)")]
        feature_flag_id: String,
    },
    /// Create a feature flag from a JSON file
    Create {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update a feature flag
    Update {
        #[arg(help = "Feature flag ID (UUID)")]
        feature_flag_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Archive a feature flag
    Archive {
        #[arg(help = "Feature flag ID (UUID)")]
        feature_flag_id: String,
    },
    /// Unarchive a feature flag
    Unarchive {
        #[arg(help = "Feature flag ID (UUID)")]
        feature_flag_id: String,
    },
}

#[derive(Subcommand)]
enum FeatureFlagEnvActions {
    /// List feature flag environments
    List {
        #[arg(long, help = "Filter by name (partial match)")]
        name: Option<String>,
        #[arg(long, help = "Filter by key (partial match)")]
        key: Option<String>,
        #[arg(long, help = "Maximum number of results to return")]
        limit: Option<i32>,
        #[arg(long, help = "Number of results to skip")]
        offset: Option<i32>,
    },
    /// Get a feature flag environment
    Get {
        #[arg(help = "Environment ID (UUID)")]
        feature_flags_env_id: String,
    },
    /// Create a feature flag environment from a JSON file
    Create {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update a feature flag environment
    Update {
        #[arg(help = "Environment ID (UUID)")]
        feature_flags_env_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete a feature flag environment
    Delete {
        #[arg(help = "Environment ID (UUID)")]
        feature_flags_env_id: String,
    },
}

// ---- HAMR ----
#[derive(Subcommand)]
enum HamrActions {
    /// Manage HAMR organization connections
    Connections {
        #[command(subcommand)]
        action: HamrConnectionActions,
    },
}

#[derive(Subcommand)]
enum HamrConnectionActions {
    /// Get HAMR organization connection
    Get,
    /// Create HAMR organization connection
    Create {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
}

// ---- Status Pages ----
#[derive(Subcommand)]
enum StatusPageActions {
    /// Manage status pages
    Pages {
        #[command(subcommand)]
        action: StatusPagePageActions,
    },
    /// Manage status page components
    Components {
        #[command(subcommand)]
        action: StatusPageComponentActions,
    },
    /// Manage status page degradations
    Degradations {
        #[command(subcommand)]
        action: StatusPageDegradationActions,
    },
    /// View third-party service outage signals
    #[command(name = "third-party")]
    ThirdParty {
        #[command(subcommand)]
        action: StatusPageThirdPartyActions,
    },
    /// Manage status page maintenances
    Maintenances {
        #[command(subcommand)]
        action: StatusPageMaintenanceActions,
    },
}

#[derive(Subcommand)]
enum StatusPageMaintenanceActions {
    /// List all maintenances
    List,
    /// Get maintenance details
    Get {
        page_id: String,
        maintenance_id: String,
    },
    /// Create a maintenance
    Create {
        page_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update a maintenance
    Update {
        page_id: String,
        maintenance_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
}

#[derive(Subcommand)]
enum StatusPagePageActions {
    /// List all status pages
    List,
    /// Get status page details
    Get { page_id: String },
    /// Create a status page
    Create {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update a status page
    Update {
        page_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete a status page
    Delete { page_id: String },
}

#[derive(Subcommand)]
enum StatusPageComponentActions {
    /// List components for a page
    List { page_id: String },
    /// Get component details
    Get {
        page_id: String,
        component_id: String,
    },
    /// Create a component
    Create {
        page_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update a component
    Update {
        page_id: String,
        component_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete a component
    Delete {
        page_id: String,
        component_id: String,
    },
}

#[derive(Subcommand)]
enum StatusPageDegradationActions {
    /// List degradations
    List,
    /// Get degradation details
    Get {
        page_id: String,
        degradation_id: String,
    },
    /// Create a degradation
    Create {
        page_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update a degradation
    Update {
        page_id: String,
        degradation_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete a degradation
    Delete {
        page_id: String,
        degradation_id: String,
    },
}

#[derive(Subcommand)]
enum StatusPageThirdPartyActions {
    /// List third-party status pages
    List {
        #[arg(long, help = "Show only providers with active (unresolved) outages")]
        active: bool,
        #[arg(
            long,
            help = "Search by provider name or display name (case-insensitive)"
        )]
        search: Option<String>,
    },
}

// ---- Integrations ----
#[derive(Subcommand)]
enum IntegrationActions {
    /// List all configured integrations
    List,
    /// Manage Jira integration
    Jira {
        #[command(subcommand)]
        action: JiraActions,
    },
    /// Manage ServiceNow integration
    Servicenow {
        #[command(subcommand)]
        action: ServiceNowActions,
    },
    /// Manage Slack integration
    Slack {
        #[command(subcommand)]
        action: SlackActions,
    },
    /// Manage PagerDuty integration
    Pagerduty {
        #[command(subcommand)]
        action: PagerdutyActions,
    },
    /// Manage webhooks
    Webhooks {
        #[command(subcommand)]
        action: WebhooksActions,
    },
    /// Manage Google Chat integration
    #[command(name = "google-chat")]
    GoogleChat {
        #[command(subcommand)]
        action: GoogleChatActions,
    },
    /// Manage AWS integrations
    Aws {
        #[command(subcommand)]
        action: IntegrationAwsActions,
    },
}

#[derive(Subcommand)]
enum IntegrationAwsActions {
    /// Manage AWS cloud authentication
    #[command(name = "cloud-auth")]
    CloudAuth {
        #[command(subcommand)]
        action: CloudAuthActions,
    },
}

#[derive(Subcommand)]
enum JiraActions {
    /// Manage Jira accounts
    Accounts {
        #[command(subcommand)]
        action: JiraAccountActions,
    },
    /// Manage Jira issue templates
    Templates {
        #[command(subcommand)]
        action: JiraTemplateActions,
    },
}

#[derive(Subcommand)]
enum JiraAccountActions {
    /// List Jira accounts
    List,
    /// Delete a Jira account
    Delete { account_id: String },
}

#[derive(Subcommand)]
enum JiraTemplateActions {
    /// List Jira issue templates
    List,
    /// Get Jira issue template
    Get { template_id: String },
    /// Create Jira issue template
    Create {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update Jira issue template
    Update {
        template_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete Jira issue template
    Delete { template_id: String },
}

#[derive(Subcommand)]
enum ServiceNowActions {
    /// Manage ServiceNow instances
    Instances {
        #[command(subcommand)]
        action: ServiceNowInstanceActions,
    },
    /// Manage ServiceNow templates
    Templates {
        #[command(subcommand)]
        action: ServiceNowTemplateActions,
    },
    /// Manage ServiceNow users
    Users {
        #[command(subcommand)]
        action: ServiceNowUserActions,
    },
    /// Manage ServiceNow assignment groups
    #[command(name = "assignment-groups")]
    AssignmentGroups {
        #[command(subcommand)]
        action: ServiceNowAssignmentGroupActions,
    },
    /// Manage ServiceNow business services
    #[command(name = "business-services")]
    BusinessServices {
        #[command(subcommand)]
        action: ServiceNowBusinessServiceActions,
    },
}

#[derive(Subcommand)]
enum ServiceNowInstanceActions {
    /// List ServiceNow instances
    List,
}

#[derive(Subcommand)]
enum ServiceNowUserActions {
    /// List ServiceNow users
    List { instance_name: String },
}

#[derive(Subcommand)]
enum ServiceNowAssignmentGroupActions {
    /// List ServiceNow assignment groups
    List { instance_name: String },
}

#[derive(Subcommand)]
enum ServiceNowBusinessServiceActions {
    /// List ServiceNow business services
    List { instance_name: String },
}

#[derive(Subcommand)]
enum ServiceNowTemplateActions {
    /// List ServiceNow templates
    List,
    /// Get ServiceNow template
    Get { template_id: String },
    /// Create ServiceNow template
    Create {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update ServiceNow template
    Update {
        template_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete ServiceNow template
    Delete { template_id: String },
}

#[derive(Subcommand)]
enum SlackActions {
    /// List Slack channels
    List,
}

#[derive(Subcommand)]
enum PagerdutyActions {
    /// List PagerDuty services
    List,
}

#[derive(Subcommand)]
enum WebhooksActions {
    /// List webhooks
    List,
}

// ---- Containers ----
#[derive(Subcommand)]
enum ContainerActions {
    /// List running containers
    List {
        /// Comma-separated list of tags to filter containers by
        #[arg(long)]
        filter_tags: Option<String>,
        /// Comma-separated list of tags to group containers by
        #[arg(long)]
        group_by: Option<String>,
        /// Attribute to sort containers by
        #[arg(long)]
        sort: Option<String>,
        /// Maximum number of results returned
        #[arg(long)]
        page_size: Option<i32>,
    },
    /// Manage container images
    Images {
        #[command(subcommand)]
        action: ContainerImageActions,
    },
}

#[derive(Subcommand)]
enum ContainerImageActions {
    /// List container images
    List {
        /// Comma-separated list of tags to filter container images by
        #[arg(long)]
        filter_tags: Option<String>,
        /// Comma-separated list of tags to group container images by
        #[arg(long)]
        group_by: Option<String>,
        /// Attribute to sort container images by
        #[arg(long)]
        sort: Option<String>,
        /// Maximum number of results returned
        #[arg(long)]
        page_size: Option<i32>,
    },
}

// ---- Cost ----
#[derive(Subcommand)]
enum CostActions {
    /// Get projected end-of-month costs
    Projected,
    /// Get costs by organization
    #[command(name = "by-org")]
    ByOrg {
        #[arg(long, help = "Start month (YYYY-MM) (required)")]
        start_month: String,
        #[arg(long, help = "End month (YYYY-MM)")]
        end_month: Option<String>,
        #[arg(
            long,
            default_value = "actual",
            help = "View type: actual, estimated, historical"
        )]
        view: String,
    },
    /// Get cost attribution by tags
    Attribution {
        #[arg(long, name = "start-month", help = "Start month (YYYY-MM) (required)")]
        start: String,
        #[arg(long, name = "end-month", help = "End month (YYYY-MM)")]
        end: Option<String>,
        #[arg(long, help = "Tag keys for breakdown (required)")]
        fields: Option<String>,
    },
    /// Manage AWS CUR cloud cost configs
    #[command(name = "aws-config")]
    AwsConfig {
        #[command(subcommand)]
        action: CostCloudConfigActions,
    },
    /// Manage Azure UC cloud cost configs
    #[command(name = "azure-config")]
    AzureConfig {
        #[command(subcommand)]
        action: CostCloudConfigActions,
    },
    /// Manage GCP usage cost configs
    #[command(name = "gcp-config")]
    GcpConfig {
        #[command(subcommand)]
        action: CostCloudConfigActions,
    },
}

#[derive(Subcommand)]
enum CostCloudConfigActions {
    /// List cloud cost configs
    List,
    /// Get a specific cloud cost config by ID
    Get {
        #[arg(help = "Cloud account ID")]
        id: i64,
    },
    /// Create a cloud cost config from a JSON file
    Create {
        #[arg(long, help = "JSON file with config body (required)")]
        file: String,
    },
    /// Delete a cloud cost config by ID
    Delete {
        #[arg(help = "Cloud account ID")]
        id: i64,
    },
}

// ---- Misc ----
#[derive(Subcommand)]
enum MiscActions {
    /// Get Datadog IP ranges
    #[command(name = "ip-ranges")]
    IpRanges,
    /// Check API status
    Status,
}

// ---- APM ----
#[derive(Subcommand)]
enum ApmActions {
    /// Manage APM services
    Services {
        #[command(subcommand)]
        action: ApmServiceActions,
    },
    /// Manage APM entities
    Entities {
        #[command(subcommand)]
        action: ApmEntityActions,
    },
    /// Manage service dependencies
    Dependencies {
        #[command(subcommand)]
        action: ApmDependencyActions,
    },
    /// View service flow map
    #[command(name = "flow-map")]
    FlowMap {
        #[arg(long, help = "Query filter (required)")]
        query: String,
        #[arg(long, default_value_t = 100, help = "Max nodes")]
        limit: i64,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, help = "Environment filter")]
        env: Option<String>,
    },
    /// Troubleshoot APM instrumentation issues
    Troubleshooting {
        #[command(subcommand)]
        action: ApmTroubleshootingActions,
    },
}

#[derive(Subcommand)]
enum ApmServiceActions {
    /// List APM services
    List {
        #[arg(long, help = "Environment filter (required)")]
        env: String,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, help = "Primary tag")]
        primary_tag: Option<String>,
    },
    /// List services with performance statistics
    Stats {
        #[arg(long, help = "Environment filter (required)")]
        env: String,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, help = "Primary tag")]
        primary_tag: Option<String>,
    },
    /// List operations for a service
    Operations {
        #[arg(long, help = "Service name (required)")]
        service: String,
        #[arg(long, help = "Environment filter (required)")]
        env: String,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, help = "Primary tag")]
        primary_tag: Option<String>,
        #[arg(long, default_value_t = false, help = "Only primary operations")]
        primary_only: bool,
    },
    /// List resources (endpoints) for a service operation
    Resources {
        #[arg(long, help = "Service name (required)")]
        service: String,
        #[arg(long, help = "Operation name (required)")]
        operation: String,
        #[arg(long, help = "Environment filter (required)")]
        env: String,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, help = "Primary tag")]
        primary_tag: Option<String>,
        #[arg(long, help = "Peer service filter")]
        peer_service: Option<String>,
    },
}

#[derive(Subcommand)]
enum ApmEntityActions {
    /// Query APM entities
    List {
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, help = "Environment filter")]
        env: Option<String>,
        #[arg(long, help = "Fields to include (comma-separated)")]
        include: Option<String>,
        #[arg(long, default_value_t = 50, help = "Max results")]
        limit: i32,
        #[arg(long, default_value_t = 0, help = "Page offset")]
        offset: i32,
        #[arg(long, help = "Primary tag")]
        primary_tag: Option<String>,
        #[arg(long, help = "Entity types (comma-separated)")]
        types: Option<String>,
    },
}

#[derive(Subcommand)]
enum ApmDependencyActions {
    /// List service dependencies
    List {
        #[arg(long, help = "Environment filter (required)")]
        env: String,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(long, help = "Primary tag (group:value)")]
        primary_tag: Option<String>,
    },
}

#[derive(Subcommand)]
enum ApmTroubleshootingActions {
    /// List instrumentation errors for a host
    List {
        #[arg(long, help = "Hostname to query (required)")]
        hostname: String,
        #[arg(long, help = "Time window (e.g. 4h, 24h, 1h30m)")]
        timeframe: Option<String>,
    },
}

// ---- Investigations ----
#[derive(Subcommand)]
enum InvestigationActions {
    /// List investigations
    List {
        #[arg(long, default_value_t = 10, help = "Page size")]
        page_limit: i64,
        #[arg(long, default_value_t = 0, help = "Pagination offset")]
        page_offset: i64,
        #[arg(long, default_value_t = 0, help = "Filter by monitor ID")]
        monitor_id: i64,
    },
    /// Get investigation details
    Get { investigation_id: String },
    /// Trigger a new investigation
    Trigger {
        #[arg(
            long,
            help = "Investigation type: monitor_alert (required)",
            required_unless_present = "file"
        )]
        r#type: Option<String>,
        #[arg(
            long,
            default_value_t = 0,
            help = "Monitor ID (required for monitor_alert)"
        )]
        monitor_id: i64,
        #[arg(long, help = "Event ID (required for monitor_alert)")]
        event_id: Option<String>,
        #[arg(
            long,
            default_value_t = 0,
            help = "Event timestamp in milliseconds (required for monitor_alert)"
        )]
        event_ts: i64,
        #[arg(long, help = "JSON file with request body", conflicts_with_all = ["type", "event_id"])]
        file: Option<String>,
    },
}

// ---- Network (placeholder) ----
#[derive(Subcommand)]
enum NetworkActions {
    /// List network devices/monitors
    List,
    /// Query network flows
    Flows {
        #[command(subcommand)]
        action: NetworkFlowActions,
    },
    /// Manage network devices
    Devices {
        #[command(subcommand)]
        action: NetworkDeviceActions,
    },
    /// Manage network interface tags
    Interfaces {
        #[command(subcommand)]
        action: NetworkInterfaceTagActions,
    },
}

#[derive(Subcommand)]
enum NetworkFlowActions {
    /// List network flows
    List,
}

#[derive(Subcommand)]
enum NetworkDeviceActions {
    /// List network devices
    List,
    /// Get device details
    Get { device_id: String },
    /// List interfaces for a device
    Interfaces {
        device_id: String,
        #[arg(long, help = "Include IP addresses")]
        ip_addresses: bool,
    },
    /// Manage device tags
    Tags {
        #[command(subcommand)]
        action: NetworkDeviceTagActions,
    },
}

#[derive(Subcommand)]
enum NetworkDeviceTagActions {
    /// List tags for a device
    List { device_id: String },
    /// Update tags for a device
    Update {
        device_id: String,
        #[arg(long, help = "JSON file with tags body (required)")]
        file: String,
    },
}

#[derive(Subcommand)]
enum NetworkInterfaceTagActions {
    /// List tags for an interface
    List { interface_id: String },
    /// Update tags for an interface
    Update {
        interface_id: String,
        #[arg(long, help = "JSON file with tags body (required)")]
        file: String,
    },
}

// ---- Obs Pipelines ----
#[derive(Subcommand)]
enum ObsPipelinesActions {
    /// List observability pipelines
    List {
        #[arg(
            long,
            default_value = "50",
            help = "Maximum number of pipelines to return"
        )]
        limit: i64,
    },
    /// Get pipeline details
    Get { pipeline_id: String },
    /// Create a new pipeline from a JSON file
    Create {
        #[arg(long, help = "JSON file with pipeline spec body (required)")]
        file: String,
    },
    /// Update an existing pipeline from a JSON file
    Update {
        pipeline_id: String,
        #[arg(long, help = "JSON file with pipeline body (required)")]
        file: String,
    },
    /// Delete a pipeline
    Delete { pipeline_id: String },
    /// Validate a pipeline configuration without creating it
    Validate {
        #[arg(long, help = "JSON file with pipeline spec body (required)")]
        file: String,
    },
}

// ---- LLM Observability ----
#[derive(Subcommand)]
enum LlmObsActions {
    /// Manage LLM Observability projects
    Projects {
        #[command(subcommand)]
        action: LlmObsProjectsActions,
    },
    /// Manage LLM Observability experiments
    Experiments {
        #[command(subcommand)]
        action: LlmObsExperimentsActions,
    },
    /// Manage LLM Observability datasets
    Datasets {
        #[command(subcommand)]
        action: LlmObsDatasetsActions,
    },
    /// Search LLM Observability spans
    Spans {
        #[command(subcommand)]
        action: LlmObsSpansActions,
    },
}

#[derive(Subcommand)]
enum LlmObsProjectsActions {
    /// Create a new LLM Obs project
    Create {
        #[arg(long, help = "JSON file with project body (required)")]
        file: String,
    },
    /// List LLM Obs projects
    List,
}

#[derive(Subcommand)]
enum LlmObsExperimentsActions {
    /// Create a new LLM Obs experiment
    Create {
        #[arg(long, help = "JSON file with experiment body (required)")]
        file: String,
    },
    /// List LLM Obs experiments
    List {
        #[arg(long, help = "Filter by project ID")]
        filter_project_id: Option<String>,
        #[arg(long, help = "Filter by dataset ID")]
        filter_dataset_id: Option<String>,
    },
    /// Update an existing LLM Obs experiment
    Update {
        experiment_id: String,
        #[arg(long, help = "JSON file with experiment update body (required)")]
        file: String,
    },
    /// Delete LLM Obs experiments (provide IDs in a JSON file)
    Delete {
        #[arg(long, help = "JSON file with experiment IDs to delete (required)")]
        file: String,
    },
    /// Get a summary of an experiment (event counts, metrics, available dimensions)
    Summary { experiment_id: String },
    /// Query events from an experiment with optional filtering and sorting
    Events {
        #[command(subcommand)]
        action: LlmObsExperimentsEventsActions,
    },
    /// Get metric stats for an experiment, optionally segmented by a dimension
    #[command(name = "metric-values")]
    MetricValues {
        experiment_id: String,
        #[arg(long, help = "Metric label to query (required)")]
        metric_label: String,
        #[arg(long, help = "Dimension to segment results by")]
        segment_by_dimension: Option<String>,
        #[arg(long, help = "Filter to a specific dimension value")]
        segment_dimension_value: Option<String>,
    },
    /// Get unique values for a dimension across experiment events
    #[command(name = "dimension-values")]
    DimensionValues {
        experiment_id: String,
        #[arg(long, help = "Dimension key to enumerate values for (required)")]
        dimension_key: String,
    },
}

#[derive(Subcommand)]
enum LlmObsExperimentsEventsActions {
    /// List events for an experiment with optional filtering and sorting
    List {
        experiment_id: String,
        #[arg(long, default_value = "20", help = "Number of events to return")]
        limit: u32,
        #[arg(long, default_value = "0", help = "Offset for pagination")]
        offset: u32,
        #[arg(long, help = "Filter by dimension key")]
        filter_dimension_key: Option<String>,
        #[arg(
            long,
            help = "Filter by dimension value (use with --filter-dimension-key)"
        )]
        filter_dimension_value: Option<String>,
        #[arg(long, help = "Filter by metric label")]
        filter_metric_label: Option<String>,
        #[arg(long, help = "Sort by metric label")]
        sort_by_metric: Option<String>,
        #[arg(long, default_value = "desc", help = "Sort direction: asc or desc")]
        sort_direction: String,
    },
    /// Get a single event from an experiment by ID
    Get {
        experiment_id: String,
        event_id: String,
    },
}

#[derive(Subcommand)]
enum LlmObsSpansActions {
    /// Search LLM Observability spans
    Search {
        #[arg(long, help = "Search query string")]
        query: Option<String>,
        #[arg(long, help = "Filter by trace ID")]
        trace_id: Option<String>,
        #[arg(long, help = "Filter by span ID")]
        span_id: Option<String>,
        #[arg(
            long,
            help = "Filter by span kind (llm, retrieval, embedding, agent, tool, task, workflow)"
        )]
        span_kind: Option<String>,
        #[arg(long, help = "Filter by span name")]
        span_name: Option<String>,
        #[arg(long, help = "Filter by ML app name")]
        ml_app: Option<String>,
        #[arg(long, help = "Return only root spans")]
        root_spans_only: bool,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time: 1h, 5min, 2hours, '5 minutes', RFC3339, Unix timestamp, or 'now'"
        )]
        from: String,
        #[arg(
            long,
            default_value = "now",
            help = "End time: 1h, 5min, 2hours, '5 minutes', RFC3339, Unix timestamp, or 'now'"
        )]
        to: String,
        #[arg(long, default_value = "20", help = "Number of spans to return")]
        limit: u32,
        #[arg(long, help = "Pagination cursor from a previous response")]
        cursor: Option<String>,
    },
}

#[derive(Subcommand)]
enum LlmObsDatasetsActions {
    /// Create a new LLM Obs dataset
    Create {
        #[arg(long, help = "Project ID (required)")]
        project_id: String,
        #[arg(long, help = "JSON file with dataset body (required)")]
        file: String,
    },
    /// List LLM Obs datasets for a project
    List {
        #[arg(long, help = "Project ID (required)")]
        project_id: String,
    },
}

// ---- Reference Tables ----
#[derive(Subcommand)]
enum ReferenceTablesActions {
    /// List reference tables
    List {
        #[arg(
            long,
            default_value = "50",
            help = "Maximum number of tables to return"
        )]
        limit: i64,
    },
    /// Get a reference table by ID
    Get {
        #[arg(help = "Table ID")]
        table_id: String,
    },
    /// Create a reference table from a JSON file
    Create {
        #[arg(long, help = "JSON file with table body (required)")]
        file: String,
    },
    /// Batch query reference table rows by primary key
    #[command(name = "batch-query")]
    BatchQuery {
        #[arg(long, help = "JSON file with batch query body (required)")]
        file: String,
    },
}

// ---- Scorecards (placeholder) ----
#[derive(Subcommand)]
enum ScorecardsActions {
    /// List scorecards
    List,
    /// Get scorecard details
    Get { scorecard_id: String },
}

// ---- Traces ----
#[derive(Subcommand)]
enum TracesActions {
    /// Search for spans
    ///
    /// Search for individual spans matching a query.
    ///
    /// Returns span data including service, resource, duration, tags, and trace IDs.
    ///
    /// SPAN QUERY SYNTAX:
    ///   - service:web-server          Match by service
    ///   - resource_name:"GET /api"    Match by resource
    ///   - @http.status_code:500       Match by tag
    ///   - @duration:>1000000000       Match by duration (nanoseconds)
    ///   - env:production              Match by environment
    ///
    /// EXAMPLES:
    ///   pup traces search --query="@http.status_code:>=500"
    ///   pup traces search --query="service:api @duration:>1000000000" --from="4h"
    ///   pup traces search --query="env:prod" --sort="timestamp" --limit=20
    #[command(verbatim_doc_comment)]
    Search {
        #[arg(long, default_value = "*", help = "Span search query")]
        query: String,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time: 1h, 30m, 7d, RFC3339, Unix timestamp, or 'now'"
        )]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(
            long,
            default_value_t = 50,
            help = "Maximum number of spans to return (1-1000)"
        )]
        limit: i32,
        #[arg(
            long,
            default_value = "-timestamp",
            help = "Sort order: timestamp or -timestamp"
        )]
        sort: String,
    },
    /// Compute aggregated stats over spans
    ///
    /// Compute aggregated statistics over spans matching a query.
    ///
    /// Returns computed metrics (count, avg, sum, percentiles, etc.) optionally
    /// grouped by a facet. Unlike search, this returns statistical buckets, not
    /// individual spans.
    ///
    /// COMPUTE FORMATS:
    ///   count                        Count of matching spans
    ///   avg(@duration)               Average of a metric
    ///   sum(@duration)               Sum of a metric
    ///   min(@duration) / max(@duration)
    ///   median(@duration)            Median of a metric
    ///   cardinality(@usr.id)         Unique count of a facet
    ///   percentile(@duration, 99)    Percentile (75, 90, 95, 98, 99)
    ///
    /// EXAMPLES:
    ///   pup traces aggregate --query="@http.status_code:>=500" --compute="count"
    ///   pup traces aggregate --query="env:prod" --compute="avg(@duration)" --group-by="service"
    ///   pup traces aggregate --query="service:api" --compute="percentile(@duration, 99)" --group-by="resource_name"
    #[command(verbatim_doc_comment)]
    Aggregate {
        #[arg(long, default_value = "*", help = "Span search query")]
        query: String,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time: 1h, 30m, 7d, RFC3339, Unix timestamp, or 'now'"
        )]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(
            long,
            help = "Aggregation: count, avg(@duration), percentile(@duration, 99), etc."
        )]
        compute: String,
        #[arg(
            long,
            help = "Facet to group by (e.g., service, resource_name, @http.status_code)"
        )]
        group_by: Option<String>,
    },
}

// ---- ACP ----
#[derive(Subcommand)]
enum AcpActions {
    /// Start an ACP server that delegates to Datadog Bits AI
    ///
    /// Spawns a local HTTP server implementing the Agent Communication Protocol (ACP).
    /// Requests are proxied to the Datadog Bits AI agent endpoint
    /// (/api/unstable/lassie-ng/v1/agents/{id}/messages).
    ///
    /// Endpoints served:
    ///   GET  /agent.json       — ACP agent card
    ///   POST /runs             — synchronous run
    ///   POST /runs/stream      — streaming run (SSE)
    ///
    /// EXAMPLES:
    ///   # Start on default port 9099 (auto-discovers first agent)
    ///   pup acp serve
    ///
    ///   # Start with a specific agent ID
    ///   pup acp serve --agent-id <uuid>
    ///
    ///   # Start on a custom port
    ///   pup acp serve --port 8080
    #[command(verbatim_doc_comment)]
    Serve {
        #[arg(
            long,
            default_value_t = commands::acp::DEFAULT_PORT,
            help = "Port to listen on"
        )]
        port: u16,
        #[arg(
            long,
            default_value = commands::acp::DEFAULT_HOST,
            help = "Host address to bind to"
        )]
        host: String,
        #[arg(
            long,
            help = "Datadog Bits AI agent ID to proxy (auto-discovered if omitted)"
        )]
        agent_id: Option<String>,
    },
}

// ---- Agent (placeholder) ----
#[derive(Subcommand)]
enum AgentActions {
    /// Output command schema as JSON
    Schema {
        #[arg(
            long,
            default_value_t = false,
            help = "Output minimal schema (names + flags only)"
        )]
        compact: bool,
    },
    /// Display the datadog-agent (Datadog-Agent) operational reference
    Guide,
}

// ---- Runbooks ----
#[cfg(not(target_arch = "wasm32"))]
#[derive(Subcommand)]
enum RunbookActions {
    /// List available runbooks
    List {
        #[arg(long, help = "Filter by tag (key:value, repeatable)", action = clap::ArgAction::Append)]
        tag: Vec<String>,
    },
    /// Show runbook details and steps
    Describe { name: String },
    /// Execute a runbook
    Run {
        name: String,
        #[arg(long, help = "Set a variable: KEY=VALUE", action = clap::ArgAction::Append)]
        arg: Vec<String>,
    },
    /// Validate a runbook without executing
    Validate { name: String },
    /// Import a runbook from a file path or URL
    Import { source: String },
}

// ---- Alias ----
#[derive(Subcommand)]
enum AliasActions {
    /// List your aliases
    List,
    /// Create a shortcut for a pup command
    Set { name: String, command: String },
    /// Delete set aliases
    Delete { names: Vec<String> },
    /// Import aliases from a YAML file
    Import {
        /// Path to YAML file containing aliases
        file: String,
    },
}

// ---- Skills ----
#[derive(Subcommand)]
enum SkillsActions {
    /// List available skills and agents
    List {
        /// Filter by type: skill, agent
        #[arg(long = "type", name = "type")]
        entry_type: Option<String>,
    },
    /// Install skills for the detected AI coding assistant
    Install {
        /// Install a specific skill or agent by name
        name: Option<String>,
        /// Override detected AI agent (claude-code, cursor, codex, windsurf, gemini-code)
        #[arg(long = "target-agent")]
        target_agent: Option<String>,
        /// Override install directory
        #[arg(long)]
        dir: Option<String>,
        /// Filter by type: skill, agent
        #[arg(long = "type", name = "type")]
        entry_type: Option<String>,
    },
    /// Show where skills would be installed
    Path {
        /// Override detected AI agent
        #[arg(long = "target-agent")]
        target_agent: Option<String>,
    },
}

// ---- Product Analytics ----
#[derive(Subcommand)]
enum ProductAnalyticsActions {
    /// Send product analytics events
    Events {
        #[command(subcommand)]
        action: ProductAnalyticsEventActions,
    },
    /// Run product analytics queries
    Query {
        #[command(subcommand)]
        action: ProductAnalyticsQueryActions,
    },
}

#[derive(Subcommand)]
enum ProductAnalyticsQueryActions {
    /// Compute scalar analytics
    Scalar {
        #[arg(long, help = "JSON file with query body (required)")]
        file: String,
    },
    /// Compute timeseries analytics
    Timeseries {
        #[arg(long, help = "JSON file with query body (required)")]
        file: String,
    },
}

#[derive(Subcommand)]
enum ProductAnalyticsEventActions {
    /// Send a product analytics event
    Send {
        #[arg(long)]
        file: Option<String>,
        #[arg(long, name = "app-id", help = "Application ID")]
        app_id: Option<String>,
        #[arg(long, help = "Event name")]
        event: Option<String>,
        #[arg(long, help = "Event properties (JSON string)")]
        properties: Option<String>,
        #[arg(long, name = "user-id", help = "User ID")]
        user_id: Option<String>,
    },
}

// ---- Static Analysis ----
#[derive(Subcommand)]
enum StaticAnalysisActions {
    /// AST analysis
    Ast {
        #[command(subcommand)]
        action: StaticAnalysisAstActions,
    },
    /// Custom security rulesets
    #[command(name = "custom-rulesets")]
    CustomRulesets {
        #[command(subcommand)]
        action: StaticAnalysisCustomRulesetActions,
    },
    /// Software Composition Analysis
    Sca {
        #[command(subcommand)]
        action: StaticAnalysisScaActions,
    },
    /// Code coverage analysis
    Coverage {
        #[command(subcommand)]
        action: StaticAnalysisCoverageActions,
    },
}

#[derive(Subcommand)]
enum StaticAnalysisAstActions {
    /// List AST analyses
    List {
        #[arg(long, help = "Filter by branch")]
        branch: Option<String>,
        #[arg(long, help = "Start time")]
        from: Option<String>,
        #[arg(long, help = "End time")]
        to: Option<String>,
        #[arg(long, help = "Filter by repository")]
        repository: Option<String>,
        #[arg(long, help = "Filter by language")]
        language: Option<String>,
        #[arg(long, help = "Filter by severity")]
        severity: Option<String>,
        #[arg(long, help = "Filter by status")]
        status: Option<String>,
    },
    /// Get AST analysis details
    Get { id: String },
}

#[derive(Subcommand)]
enum StaticAnalysisCustomRulesetActions {
    /// List custom rulesets
    List {
        #[arg(long, help = "Filter by branch")]
        branch: Option<String>,
        #[arg(long, help = "Start time")]
        from: Option<String>,
        #[arg(long, help = "End time")]
        to: Option<String>,
        #[arg(long, help = "Filter by repository")]
        repository: Option<String>,
        #[arg(long, help = "Filter by language")]
        language: Option<String>,
        #[arg(long, help = "Filter by severity")]
        severity: Option<String>,
        #[arg(long, help = "Filter by status")]
        status: Option<String>,
    },
    /// Get custom ruleset details
    Get { id: String },
}

#[derive(Subcommand)]
enum StaticAnalysisScaActions {
    /// List SCA results
    List {
        #[arg(long, help = "Filter by branch")]
        branch: Option<String>,
        #[arg(long, help = "Start time")]
        from: Option<String>,
        #[arg(long, help = "End time")]
        to: Option<String>,
        #[arg(long, help = "Filter by repository")]
        repository: Option<String>,
        #[arg(long, help = "Filter by language")]
        language: Option<String>,
        #[arg(long, help = "Filter by severity")]
        severity: Option<String>,
        #[arg(long, help = "Filter by status")]
        status: Option<String>,
    },
    /// Get SCA scan details
    Get { id: String },
}

#[derive(Subcommand)]
enum StaticAnalysisCoverageActions {
    /// List coverage analyses
    List {
        #[arg(long, help = "Filter by branch")]
        branch: Option<String>,
        #[arg(long, help = "Start time")]
        from: Option<String>,
        #[arg(long, help = "End time")]
        to: Option<String>,
        #[arg(long, help = "Filter by repository")]
        repository: Option<String>,
        #[arg(long, help = "Filter by language")]
        language: Option<String>,
        #[arg(long, help = "Filter by severity")]
        severity: Option<String>,
        #[arg(long, help = "Filter by status")]
        status: Option<String>,
    },
    /// Get coverage analysis details
    Get { id: String },
}

// ---- Auth ----
#[derive(Subcommand)]
enum AuthActions {
    /// Login via OAuth2
    Login {
        /// Comma-separated OAuth scopes to request (e.g. dashboards_read,metrics_read).
        /// Overrides profile and config file scopes. Unknown scopes are skipped with a warning.
        #[arg(long, value_name = "SCOPES")]
        scopes: Option<String>,
        /// Request only read-only scopes (excludes write, manage, and org-level scopes).
        /// Shorthand: --ro
        #[arg(long, alias = "ro", visible_alias = "ro")]
        read_only: bool,
        /// Datadog site to authenticate against (e.g. datadoghq.eu, us3.datadoghq.com).
        /// Overrides DD_SITE env var and config file. Defaults to datadoghq.com.
        #[arg(long, value_name = "SITE")]
        site: Option<String>,
        /// Organization subdomain for SAML/SSO login (e.g. mycompany for mycompany.datadoghq.com).
        #[arg(long, value_name = "SUBDOMAIN")]
        subdomain: Option<String>,
    },
    /// Logout and clear tokens
    Logout,
    /// Check authentication status
    Status {
        /// Datadog site to check status for (e.g. datadoghq.eu, us3.datadoghq.com).
        /// Overrides DD_SITE env var and config file. Defaults to datadoghq.com.
        #[arg(long, value_name = "SITE")]
        site: Option<String>,
    },
    /// Print access token (debug builds only)
    #[cfg(debug_assertions)]
    Token,
    /// Refresh access token
    Refresh,
    /// List all stored org sessions
    List,
    /// Test connection and credentials
    Test,
}

// ---- Agent-mode JSON schema for --help ----

/// Walk the clap command tree to find the subcommand matching the given path.
fn find_subcommand<'a>(cmd: &'a clap::Command, path: &[&str]) -> Option<&'a clap::Command> {
    let mut current = cmd;
    for name in path {
        current = current.get_subcommands().find(|s| s.get_name() == *name)?;
    }
    if path.is_empty() {
        None
    } else {
        Some(current)
    }
}

/// Build a scoped agent schema for a specific subcommand (e.g. `pup logs --help`).
fn build_agent_schema_scoped(
    _root_cmd: &clap::Command,
    target: &clap::Command,
    sub_path: &[&str],
) -> serde_json::Value {
    let mut root = serde_json::Map::new();
    root.insert("version".into(), serde_json::json!(version::VERSION));

    // Use the subcommand's description
    let desc = target
        .get_about()
        .map(|a| a.to_string())
        .unwrap_or_default();
    root.insert("description".into(), serde_json::json!(desc));

    let mut auth = serde_json::Map::new();
    auth.insert("oauth".into(), serde_json::json!("pup auth login"));
    auth.insert(
        "api_keys".into(),
        serde_json::json!("Set DD_API_KEY + DD_APP_KEY + DD_SITE environment variables"),
    );
    root.insert("auth".into(), serde_json::Value::Object(auth));

    // Global flags
    root.insert(
        "global_flags".into(),
        serde_json::json!([
            {
                "name": "--agent",
                "type": "bool",
                "default": "false",
                "description": "Enable agent mode (auto-detected for AI coding assistants)"
            },
            {
                "name": "--org",
                "type": "string",
                "default": null,
                "description": "Named org session for multi-org support (see 'pup auth login --org')"
            },
            {
                "name": "--output",
                "type": "string",
                "default": "json",
                "description": "Output format (json, table, yaml, csv)"
            },
            {
                "name": "--yes",
                "type": "bool",
                "default": "false",
                "description": "Skip confirmation prompts (auto-approve all operations)"
            }
        ]),
    );

    // Build scoped command tree — only the target command
    let cmd_schema = build_command_schema(target, "");
    root.insert("commands".into(), serde_json::json!([cmd_schema]));

    // Include query_syntax: scoped to the matching command if it has one, full map otherwise
    let top_name = sub_path[0];
    let all_syntax = serde_json::json!({
        "apm": "service:<name> resource_name:<path> @duration:>5000000000 (nanoseconds!) status:error operation_name:<op>. Duration is always in nanoseconds",
        "events": "sources:nagios,pagerduty status:error priority:normal tags:env:prod",
        "logs": "status:error, service:web-app, @attr:val, host:i-*, \"exact phrase\", AND/OR/NOT operators, -status:info (negation), wildcards with *",
        "metrics": "<aggregation>:<metric_name>{<filter>} by {<group>}. Example: avg:system.cpu.user{env:prod} by {host}. Aggregations: avg, sum, min, max, count",
        "monitors": "Use --name for substring search, --tags for tag filtering (comma-separated). Search via --query for full-text search",
        "rum": "@type:error @session.type:user @view.url_path:/checkout @action.type:click service:<app-name>",
        "security": "@workflow.rule.type:log_detection source:cloudtrail @network.client.ip:10.0.0.0/8 status:critical",
        "traces": "service:<name> resource_name:<path> @duration:>5s (shorthand) env:production"
    });
    if let Some(syntax) = all_syntax.get(top_name) {
        // Scope to just this command's entry
        let mut scoped = serde_json::Map::new();
        scoped.insert(top_name.to_string(), syntax.clone());
        root.insert("query_syntax".into(), serde_json::Value::Object(scoped));
    } else {
        // No match — include the full map
        root.insert("query_syntax".into(), all_syntax);
    }

    root.insert(
        "time_formats".into(),
        serde_json::json!({
            "relative": ["5s", "30m", "1h", "4h", "1d", "7d", "1w", "30d", "5min", "2hours", "3days"],
            "absolute": ["Unix timestamp in milliseconds", "RFC3339 (2024-01-01T00:00:00Z)"],
            "examples": [
                "--from=1h (1 hour ago)",
                "--from=30m --to=now",
                "--from=7d --to=1d (7 days ago to 1 day ago)",
                "--from=2024-01-01T00:00:00Z --to=2024-01-02T00:00:00Z",
                "--from=\"5 minutes\""
            ]
        }),
    );

    // No workflows for scoped help
    root.insert("workflows".into(), serde_json::Value::Null);

    root.insert("best_practices".into(), serde_json::json!([
        "Always specify --from to set a time range; most commands default to 1h but be explicit",
        "Start with narrow time ranges (1h) then widen if needed; large ranges are slow and expensive",
        "Filter by service first when investigating issues: --query='service:<name>'",
        "Use --limit to control result size; default varies by command (50-200)",
        "For monitors, use --tags to filter rather than listing all and parsing locally",
        "APM durations are in NANOSECONDS: 1 second = 1000000000, 5ms = 5000000",
        "Use 'pup logs aggregate' for counts and distributions instead of fetching all logs and counting locally",
        "Prefer JSON output (default) for structured parsing; use --output=table only for human display",
        "Chain narrow queries: first aggregate to find patterns, then search for specific examples",
        "Use 'pup monitors search' for full-text search, 'pup monitors list' for tag/name filtering"
    ]));

    root.insert("anti_patterns".into(), serde_json::json!([
        "Don't omit --from on time-series queries; you'll get unexpected time ranges or errors",
        "Don't use --limit=1000 as a first step; start with small limits and refine queries",
        "Don't list all monitors/logs without filters in large organizations (>10k monitors)",
        "Don't assume APM durations are in seconds or milliseconds; they are in NANOSECONDS",
        "Don't fetch raw logs to count them; use 'pup logs aggregate --compute=count' instead",
        "Don't use --from=30d unless you specifically need a month of data; it's slow",
        "Don't retry failed requests without checking the error; 401 means re-authenticate, 403 means missing permissions",
        "Don't use 'pup metrics query' without specifying an aggregation (avg, sum, max, min, count)",
        "Don't pipe large JSON responses through multiple jq transforms; use query filters at the API level"
    ]));

    serde_json::Value::Object(root)
}

fn build_agent_schema(cmd: &clap::Command) -> serde_json::Value {
    let mut root = serde_json::Map::new();
    root.insert("version".into(), serde_json::json!(version::VERSION));
    root.insert(
        "description".into(),
        serde_json::json!(
            "Pup - Datadog API CLI. Provides OAuth2 + API key authentication for querying metrics, logs, monitors, traces, and 30+ other Datadog API domains."
        ),
    );
    let mut auth = serde_json::Map::new();
    auth.insert("oauth".into(), serde_json::json!("pup auth login"));
    auth.insert(
        "api_keys".into(),
        serde_json::json!("Set DD_API_KEY + DD_APP_KEY + DD_SITE environment variables"),
    );
    root.insert("auth".into(), serde_json::Value::Object(auth));

    // Global flags — hardcoded to match Go ordering and descriptions exactly
    root.insert(
        "global_flags".into(),
        serde_json::json!([
            {
                "name": "--agent",
                "type": "bool",
                "default": "false",
                "description": "Enable agent mode (auto-detected for AI coding assistants)"
            },
            {
                "name": "--org",
                "type": "string",
                "default": null,
                "description": "Named org session for multi-org support (see 'pup auth login --org')"
            },
            {
                "name": "--output",
                "type": "string",
                "default": "json",
                "description": "Output format (json, table, yaml, csv)"
            },
            {
                "name": "--yes",
                "type": "bool",
                "default": "false",
                "description": "Skip confirmation prompts (auto-approve all operations)"
            }
        ]),
    );

    // Operational knowledge sections — critical for AI agent effectiveness
    root.insert("anti_patterns".into(), serde_json::json!([
        "Don't omit --from on time-series queries; you'll get unexpected time ranges or errors",
        "Don't use --limit=1000 as a first step; start with small limits and refine queries",
        "Don't list all monitors/logs without filters in large organizations (>10k monitors)",
        "Don't assume APM durations are in seconds or milliseconds; they are in NANOSECONDS",
        "Don't fetch raw logs to count them; use 'pup logs aggregate --compute=count' instead",
        "Don't use --from=30d unless you specifically need a month of data; it's slow",
        "Don't retry failed requests without checking the error; 401 means re-authenticate, 403 means missing permissions",
        "Don't use 'pup metrics query' without specifying an aggregation (avg, sum, max, min, count)",
        "Don't pipe large JSON responses through multiple jq transforms; use query filters at the API level"
    ]));

    root.insert("best_practices".into(), serde_json::json!([
        "Always specify --from to set a time range; most commands default to 1h but be explicit",
        "Start with narrow time ranges (1h) then widen if needed; large ranges are slow and expensive",
        "Filter by service first when investigating issues: --query='service:<name>'",
        "Use --limit to control result size; default varies by command (50-200)",
        "For monitors, use --tags to filter rather than listing all and parsing locally",
        "APM durations are in NANOSECONDS: 1 second = 1000000000, 5ms = 5000000",
        "Use 'pup logs aggregate' for counts and distributions instead of fetching all logs and counting locally",
        "Prefer JSON output (default) for structured parsing; use --output=table only for human display",
        "Chain narrow queries: first aggregate to find patterns, then search for specific examples",
        "Use 'pup monitors search' for full-text search, 'pup monitors list' for tag/name filtering"
    ]));

    root.insert("query_syntax".into(), serde_json::json!({
        "apm": "service:<name> resource_name:<path> @duration:>5000000000 (nanoseconds!) status:error operation_name:<op>. Duration is always in nanoseconds",
        "events": "sources:nagios,pagerduty status:error priority:normal tags:env:prod",
        "logs": "status:error, service:web-app, @attr:val, host:i-*, \"exact phrase\", AND/OR/NOT operators, -status:info (negation), wildcards with *",
        "metrics": "<aggregation>:<metric_name>{<filter>} by {<group>}. Example: avg:system.cpu.user{env:prod} by {host}. Aggregations: avg, sum, min, max, count",
        "monitors": "Use --name for substring search, --tags for tag filtering (comma-separated). Search via --query for full-text search",
        "rum": "@type:error @session.type:user @view.url_path:/checkout @action.type:click service:<app-name>",
        "security": "@workflow.rule.type:log_detection source:cloudtrail @network.client.ip:10.0.0.0/8 status:critical",
        "traces": "service:<name> resource_name:<path> @duration:>5s (shorthand) env:production"
    }));

    root.insert("time_formats".into(), serde_json::json!({
        "relative": ["5s", "30m", "1h", "4h", "1d", "7d", "1w", "30d", "5min", "2hours", "3days"],
        "absolute": ["Unix timestamp in milliseconds", "RFC3339 (2024-01-01T00:00:00Z)"],
        "examples": [
            "--from=1h (1 hour ago)",
            "--from=30m --to=now",
            "--from=7d --to=1d (7 days ago to 1 day ago)",
            "--from=2024-01-01T00:00:00Z --to=2024-01-02T00:00:00Z",
            "--from=\"5 minutes\""
        ]
    }));

    root.insert("workflows".into(), serde_json::json!([
        {
            "name": "Investigate errors",
            "steps": [
                "pup logs search --query=\"status:error\" --from=1h --limit=20",
                "pup logs aggregate --query=\"status:error\" --from=1h --compute=\"count\" --group-by=\"service\"",
                "pup monitors list --tags=\"env:production\" --limit=50"
            ]
        },
        {
            "name": "Performance investigation",
            "steps": [
                "pup metrics query --query=\"avg:trace.servlet.request.duration{env:prod} by {service}\" --from=1h",
                "pup logs search --query=\"@duration:>5000000000\" --from=1h --limit=20",
                "pup apm services list"
            ]
        },
        {
            "name": "Monitor status check",
            "steps": [
                "pup monitors list --tags=\"env:production\" --limit=500",
                "pup monitors search --query=\"status:Alert\"",
                "pup monitors get <monitor_id>"
            ]
        },
        {
            "name": "Security audit",
            "steps": [
                "pup audit-logs search --query=\"*\" --from=1d --limit=100",
                "pup security rules list",
                "pup security signals list --query=\"status:critical\" --from=1d"
            ]
        },
        {
            "name": "Service health overview",
            "steps": [
                "pup slos list",
                "pup monitors list --tags=\"team:<team_name>\"",
                "pup incidents list --query=\"status:active\""
            ]
        }
    ]));

    // Commands — sorted alphabetically to match Go
    let mut commands: Vec<serde_json::Value> = cmd
        .get_subcommands()
        .filter(|s| s.get_name() != "help")
        .map(|s| build_command_schema(s, ""))
        .collect();
    commands.sort_by(|a, b| {
        let an = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let bn = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
        an.cmp(bn)
    });
    root.insert("commands".into(), serde_json::Value::Array(commands));

    serde_json::Value::Object(root)
}

fn build_compact_agent_schema(cmd: &clap::Command) -> serde_json::Value {
    fn compact_cmd(cmd: &clap::Command, parent_path: &str) -> serde_json::Value {
        let name = cmd.get_name().to_string();
        let full_path = if parent_path.is_empty() {
            name.clone()
        } else {
            format!("{parent_path} {name}")
        };
        let mut obj = serde_json::Map::new();
        obj.insert("name".into(), serde_json::json!(name));
        obj.insert("full_path".into(), serde_json::json!(full_path));

        let mut flags: Vec<String> = cmd
            .get_arguments()
            .filter(|a| {
                let id = a.get_id().as_str();
                id != "help" && id != "version" && !a.is_global_set() && a.get_long().is_some()
            })
            .map(|a| format!("--{}", a.get_long().unwrap()))
            .collect();
        flags.sort();
        if !flags.is_empty() {
            obj.insert("flags".into(), serde_json::json!(flags));
        }

        let mut subs: Vec<serde_json::Value> = cmd
            .get_subcommands()
            .filter(|s| s.get_name() != "help")
            .map(|s| compact_cmd(s, &full_path))
            .collect();
        subs.sort_by(|a, b| {
            a.get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .cmp(b.get("name").and_then(|v| v.as_str()).unwrap_or(""))
        });
        if !subs.is_empty() {
            obj.insert("subcommands".into(), serde_json::Value::Array(subs));
        }

        serde_json::Value::Object(obj)
    }

    let mut root = serde_json::Map::new();
    root.insert("version".into(), serde_json::json!(version::VERSION));

    let mut commands: Vec<serde_json::Value> = cmd
        .get_subcommands()
        .filter(|s| s.get_name() != "help")
        .map(|s| compact_cmd(s, ""))
        .collect();
    commands.sort_by(|a, b| {
        a.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .cmp(b.get("name").and_then(|v| v.as_str()).unwrap_or(""))
    });
    root.insert("commands".into(), serde_json::Value::Array(commands));

    serde_json::Value::Object(root)
}

/// Returns true if a leaf subcommand name represents a write (mutating) operation.
/// Used by both the read-only runtime guard and the agent JSON schema.
pub(crate) fn is_write_command_name(name: &str) -> bool {
    name == "delete"
        || name == "create"
        || name == "update"
        || name == "cancel"
        || name == "trigger"
        || name == "set"
        || name == "add"
        || name == "remove"
        || name == "install"
        || name == "assign"
        || name == "archive"
        || name == "unarchive"
        || name == "activate"
        || name == "deactivate"
        || name == "move"
        || name == "link"
        || name == "unlink"
        || name == "configure"
        || name == "upgrade"
        || name.starts_with("update-")
        || name.starts_with("create-")
        || name == "submit"
        || name == "send"
        || name == "import"
        || name == "register"
        || name == "unregister"
        || name.contains("delete")
        || name == "patch"
        || name.starts_with("patch-")
}

fn build_command_schema(cmd: &clap::Command, parent_path: &str) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    let name = cmd.get_name().to_string();
    let full_path = if parent_path.is_empty() {
        name.clone()
    } else {
        format!("{parent_path} {name}")
    };

    obj.insert("name".into(), serde_json::json!(name));
    obj.insert("full_path".into(), serde_json::json!(full_path));

    if let Some(about) = cmd.get_about() {
        obj.insert("description".into(), serde_json::json!(about.to_string()));
    }

    // Determine read_only based on command name — but only emit for leaf commands
    // (commands with no subcommands), matching Go behavior
    let is_write = is_write_command_name(&name);

    // Positional arguments (excluding globals and help/version)
    // Enumerated to preserve declaration order for correct CLI invocation
    let args: Vec<serde_json::Value> = cmd
        .get_arguments()
        .filter(|a| {
            let id = a.get_id().as_str();
            id != "help" && id != "version" && !a.is_global_set() && a.get_long().is_none()
        })
        .enumerate()
        .map(|(i, a)| {
            let mut arg = serde_json::Map::new();
            arg.insert("name".into(), serde_json::json!(a.get_id().as_str()));
            arg.insert("type".into(), serde_json::json!("string"));
            arg.insert("required".into(), serde_json::json!(a.is_required_set()));
            arg.insert("index".into(), serde_json::json!(i + 1));
            if let Some(help) = a.get_help() {
                arg.insert("description".into(), serde_json::json!(help.to_string()));
            }
            serde_json::Value::Object(arg)
        })
        .collect();
    if !args.is_empty() {
        obj.insert("args".into(), serde_json::Value::Array(args));
    }

    // Flags (named --flags only, excluding positional args and globals)
    let flags: Vec<serde_json::Value> = cmd
        .get_arguments()
        .filter(|a| {
            let id = a.get_id().as_str();
            id != "help" && id != "version" && !a.is_global_set() && a.get_long().is_some()
        })
        .map(|a| {
            let mut flag = serde_json::Map::new();
            let flag_name = format!("--{}", a.get_long().unwrap());
            flag.insert("name".into(), serde_json::json!(flag_name));
            // Detect int types by checking if the default value parses as an integer
            let type_str = if !a.get_action().takes_values() {
                "bool"
            } else {
                let is_int = a
                    .get_default_values()
                    .first()
                    .and_then(|d| d.to_str())
                    .map(|s| s.parse::<i64>().is_ok())
                    .unwrap_or(false);
                if is_int {
                    "int"
                } else {
                    "string"
                }
            };
            flag.insert("type".into(), serde_json::json!(type_str));
            flag.insert("required".into(), serde_json::json!(a.is_required_set()));
            if let Some(def) = a.get_default_values().first() {
                flag.insert(
                    "default".into(),
                    serde_json::json!(def.to_str().unwrap_or("").to_string()),
                );
            }
            if let Some(help) = a.get_help() {
                flag.insert("description".into(), serde_json::json!(help.to_string()));
            }
            serde_json::Value::Object(flag)
        })
        .collect();

    // Sort flags alphabetically to match Go output
    let mut flags = flags;
    flags.sort_by(|a, b| {
        let an = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let bn = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
        an.cmp(bn)
    });

    if !flags.is_empty() {
        obj.insert("flags".into(), serde_json::Value::Array(flags));
    }

    // read_only goes after flags but before subcommands (matching Go field ordering)
    obj.insert("read_only".into(), serde_json::json!(!is_write));

    // Subcommands — sorted alphabetically to match Go
    let mut subs: Vec<serde_json::Value> = cmd
        .get_subcommands()
        .filter(|s| s.get_name() != "help")
        .map(|s| build_command_schema(s, &full_path))
        .collect();
    subs.sort_by(|a, b| {
        let an = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let bn = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
        an.cmp(bn)
    });
    if !subs.is_empty() {
        obj.insert("subcommands".into(), serde_json::Value::Array(subs));
    }

    serde_json::Value::Object(obj)
}

#[cfg(test)]
mod test_agent_schema {
    use super::*;

    fn get_schema() -> serde_json::Value {
        let cmd = Cli::command();
        build_agent_schema(&cmd)
    }

    fn find_command<'a>(
        commands: &'a [serde_json::Value],
        path: &[&str],
    ) -> Option<&'a serde_json::Value> {
        let name = path[0];
        let cmd = commands
            .iter()
            .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))?;
        if path.len() == 1 {
            Some(cmd)
        } else {
            let subs = cmd.get("subcommands")?.as_array()?;
            find_command(subs, &path[1..])
        }
    }

    #[test]
    fn schema_has_commands_array() {
        let schema = get_schema();
        assert!(schema.get("commands").and_then(|v| v.as_array()).is_some());
    }

    #[test]
    fn leaf_command_has_required_fields() {
        let schema = get_schema();
        let commands = schema["commands"].as_array().unwrap();
        let cmd = find_command(commands, &["monitors", "get"]).expect("monitors get not found");

        // Must have name, full_path, description, read_only
        assert_eq!(cmd["name"].as_str(), Some("get"));
        assert_eq!(cmd["full_path"].as_str(), Some("monitors get"));
        assert!(cmd.get("description").is_some());
        assert!(cmd["read_only"].is_boolean());
    }

    #[test]
    fn positional_args_included_with_index() {
        let schema = get_schema();
        let commands = schema["commands"].as_array().unwrap();
        let cmd = find_command(commands, &["monitors", "get"]).expect("monitors get not found");

        let args = cmd["args"]
            .as_array()
            .expect("monitors get should have args");
        assert!(
            !args.is_empty(),
            "monitors get should have at least one positional arg"
        );

        let first = &args[0];
        assert!(
            first.get("name").and_then(|v| v.as_str()).is_some(),
            "arg must have name"
        );
        assert_eq!(
            first["type"].as_str(),
            Some("string"),
            "positional args must be string type"
        );
        assert!(
            first["required"].is_boolean(),
            "arg must have required field"
        );
        assert!(first["index"].is_u64(), "arg must have numeric index");
        assert_eq!(
            first["index"].as_u64(),
            Some(1),
            "first arg should have index 1"
        );
    }

    #[test]
    fn flags_have_required_field() {
        let schema = get_schema();
        let commands = schema["commands"].as_array().unwrap();
        let cmd = find_command(commands, &["logs", "search"]).expect("logs search not found");

        let flags = cmd["flags"]
            .as_array()
            .expect("logs search should have flags");
        assert!(!flags.is_empty());

        for flag in flags {
            let name = flag["name"].as_str().unwrap_or("<unknown>");
            assert!(
                flag["required"].is_boolean(),
                "flag {name} must have required field"
            );
            assert!(
                flag["type"].as_str().is_some(),
                "flag {name} must have type"
            );
        }
    }

    #[test]
    fn flag_has_name_type_description() {
        let schema = get_schema();
        let commands = schema["commands"].as_array().unwrap();
        let cmd = find_command(commands, &["logs", "search"]).expect("logs search not found");

        let flags = cmd["flags"].as_array().unwrap();
        let query_flag = flags
            .iter()
            .find(|f| f["name"].as_str() == Some("--query"))
            .expect("logs search should have --query flag");

        assert_eq!(query_flag["type"].as_str(), Some("string"));
        assert!(query_flag
            .get("description")
            .and_then(|v| v.as_str())
            .is_some());
    }

    #[test]
    fn multi_positional_args_ordered_by_index() {
        let schema = get_schema();
        let commands = schema["commands"].as_array().unwrap();
        let cmd = find_command(commands, &["alias", "set"]).expect("alias set not found");

        let args = cmd["args"].as_array().expect("alias set should have args");
        assert!(
            args.len() >= 2,
            "alias set should have at least 2 positional args"
        );

        let indices: Vec<u64> = args.iter().filter_map(|a| a["index"].as_u64()).collect();
        let mut sorted = indices.clone();
        sorted.sort();
        assert_eq!(indices, sorted, "positional args must be ordered by index");
        assert_eq!(indices[0], 1, "first index should be 1");
    }

    #[test]
    fn group_commands_have_subcommands_not_args() {
        let schema = get_schema();
        let commands = schema["commands"].as_array().unwrap();
        let monitors = commands
            .iter()
            .find(|c| c["name"].as_str() == Some("monitors"))
            .expect("monitors not found");

        assert!(
            monitors
                .get("subcommands")
                .and_then(|v| v.as_array())
                .is_some(),
            "group command should have subcommands"
        );
    }
}

// ---- Main ----

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    main_inner().await
}

#[cfg(target_arch = "wasm32")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    main_inner().await
}

pub(crate) fn get_leaf_subcommand_name(matches: &clap::ArgMatches) -> Option<String> {
    match matches.subcommand() {
        Some((name, sub_matches)) => {
            get_leaf_subcommand_name(sub_matches).or(Some(name.to_string()))
        }
        None => None,
    }
}

pub(crate) fn get_top_level_subcommand_name(matches: &clap::ArgMatches) -> Option<String> {
    matches.subcommand().map(|(name, _)| name.to_string())
}

/// Resolve OAuth scopes for `pup auth login`.
///
/// Priority: CLI --scopes > config profile scopes > config top-level scopes > defaults.
/// Unknown scopes (not in the known list) are warned and excluded.
/// In --read-only mode, defaults/profile scopes are filtered to read-only-safe scopes;
/// explicitly-provided --scopes are passed through as-is (user intent is explicit).
#[cfg(not(target_arch = "wasm32"))]
fn resolve_login_scopes(
    cli_scopes: Option<&str>,
    org: Option<&str>,
    read_only: bool,
) -> Vec<String> {
    use crate::auth::types::{all_known_scopes, default_scopes, read_only_scopes};

    if let Some(raw) = cli_scopes {
        // User explicitly specified scopes — validate against known list, warn on unknowns
        let known: std::collections::HashSet<&str> = all_known_scopes().into_iter().collect();
        let mut result = Vec::new();
        for scope in crate::config::parse_scopes(raw) {
            if known.contains(scope.as_str()) {
                result.push(scope);
            } else {
                eprintln!("⚠️  Unknown scope ignored: {scope}");
            }
        }
        return result;
    }

    // Load from config file (per-org profile, then top-level scopes)
    if let Some(configured) = crate::config::load_configured_scopes(org) {
        let known: std::collections::HashSet<&str> = all_known_scopes().into_iter().collect();
        let mut result = Vec::new();
        for scope in &configured {
            if known.contains(scope.as_str()) {
                if !read_only || read_only_scopes().contains(&scope.as_str()) {
                    result.push(scope.clone());
                }
            } else {
                eprintln!("⚠️  Unknown scope in config ignored: {scope}");
            }
        }
        return result;
    }

    // Default scopes, filtered for read-only if needed
    if read_only {
        read_only_scopes().into_iter().map(String::from).collect()
    } else {
        default_scopes().into_iter().map(String::from).collect()
    }
}

#[cfg(target_arch = "wasm32")]
fn resolve_login_scopes(
    _cli_scopes: Option<&str>,
    _org: Option<&str>,
    _read_only: bool,
) -> Vec<String> {
    crate::auth::types::default_scopes()
        .into_iter()
        .map(String::from)
        .collect()
}

async fn main_inner() -> anyhow::Result<()> {
    // In agent mode, intercept --help to return a JSON schema instead of plain text.
    let args: Vec<String> = std::env::args().collect();
    let has_help = args.iter().any(|a| a == "--help" || a == "-h");
    let has_agent_flag = args.iter().any(|a| a == "--agent");
    if has_help && (useragent::is_agent_mode() || has_agent_flag) {
        let cmd = Cli::command();
        // Collect subcommand path from args (skip binary name, flags, and --help/-h)
        let sub_path: Vec<&str> = args
            .iter()
            .skip(1)
            .filter(|a| *a != "--help" && *a != "-h" && !a.starts_with('-'))
            .map(|s| s.as_str())
            .collect();
        // Always scope to the top-level subcommand (e.g., "logs" even if "logs search")
        let top_level: Vec<&str> = sub_path.iter().take(1).copied().collect();
        let target_cmd = find_subcommand(&cmd, &top_level);
        let schema = match target_cmd {
            Some(target) if !top_level.is_empty() => {
                build_agent_schema_scoped(&cmd, target, &top_level)
            }
            _ => build_agent_schema(&cmd),
        };
        println!("{}", serde_json::to_string_pretty(&schema).unwrap());
        return Ok(());
    }

    // --- Extension interception (before clap parsing) ---
    // If the first positional arg is not a built-in command but matches an
    // installed extension, dispatch to it directly and bypass clap entirely.
    #[cfg(not(target_arch = "wasm32"))]
    {
        let parsed = extensions::parse_extension_args(&args);
        if let Some(ref candidate) = parsed.candidate {
            if !extensions::is_builtin_command(candidate) {
                if let Some(ext_path) = extensions::extension_path(candidate) {
                    let mut cfg = config::Config::from_env()?;
                    parsed.globals.apply_to(&mut cfg);
                    let exit_code = extensions::exec_extension(&ext_path, &parsed.ext_args, &cfg)?;
                    std::process::exit(exit_code);
                }
            }
        }
    }

    // Build the clap Command and, when extensions are installed, append an
    // "EXTENSIONS:" section to the help output so they are visible in
    // `pup --help` / `pup help`, similar to how `gh` lists extensions.
    let mut cmd = Cli::command();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let ext_help = extensions::discovery::build_extensions_help_section();
        if let Some(section) = ext_help {
            cmd = cmd.after_help(section);
        }
    }
    let matches = cmd.get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());

    // Handle commands that do not require authentication before Config::from_env() so
    // that sourcing completions in a shell rc (e.g. `source <(pup completions bash)`)
    // never triggers a token refresh or prints auth-related messages.
    #[cfg(not(target_arch = "wasm32"))]
    if let Commands::Completions { shell, install } = cli.command {
        if install {
            commands::completions::install(shell)?;
        } else {
            commands::completions::generate(shell);
        }
        return Ok(());
    }
    if let Commands::Version = cli.command {
        println!("{}", version::build_info());
        return Ok(());
    }

    let mut cfg = config::Config::from_env()?;

    // Apply flag overrides
    if let Ok(fmt) = cli.output.parse() {
        cfg.output_format = fmt;
    }
    if cli.yes {
        cfg.auto_approve = true;
    }
    cfg.agent_mode = cli.agent || useragent::is_agent_mode();
    if cfg.agent_mode {
        cfg.auto_approve = true;
    }
    // Apply --org flag (higher priority than DD_ORG env var / config file)
    if let Some(org) = cli.org {
        cfg.org = Some(org);
        // Reload token from storage for this org, unless DD_ACCESS_TOKEN was explicitly set
        #[cfg(all(not(feature = "browser"), not(target_arch = "wasm32")))]
        if std::env::var("DD_ACCESS_TOKEN")
            .ok()
            .filter(|s| !s.is_empty())
            .is_none()
        {
            cfg.access_token = config::load_token_from_storage(&cfg.site, cfg.org.as_deref());
        }
    }

    if cli.read_only {
        cfg.read_only = true;
    }
    if cfg.read_only {
        let top = get_top_level_subcommand_name(&matches);
        let is_local_only = matches!(
            top.as_deref(),
            Some("auth") | Some("alias") | Some("skills")
        );
        if !is_local_only {
            if let Some(leaf) = get_leaf_subcommand_name(&matches) {
                if is_write_command_name(&leaf) {
                    anyhow::bail!(
                        "write operation '{}' is blocked in read-only mode \
                         (--read-only flag, DD_READ_ONLY / DD_CLI_READ_ONLY env var, \
                         or read_only: true in config file)",
                        leaf
                    );
                }
            }
        }
    }

    match cli.command {
        // --- Monitors ---
        Commands::Monitors { action } => {
            cfg.validate_auth()?;
            match action {
                MonitorActions::List { name, tags, limit } => {
                    commands::monitors::list(&cfg, name, tags, limit).await?;
                }
                MonitorActions::Get { monitor_id } => {
                    commands::monitors::get(&cfg, monitor_id).await?;
                }
                MonitorActions::Create { file } => {
                    commands::monitors::create(&cfg, &file).await?;
                }
                MonitorActions::Update { monitor_id, file } => {
                    commands::monitors::update(&cfg, monitor_id, &file).await?;
                }
                MonitorActions::Search { query, .. } => {
                    commands::monitors::search(&cfg, query).await?;
                }
                MonitorActions::Delete { monitor_id } => {
                    commands::monitors::delete(&cfg, monitor_id).await?;
                }
            }
        }
        // --- Logs ---
        Commands::Logs { action } => {
            cfg.validate_auth()?;
            match action {
                LogActions::Search {
                    query,
                    from,
                    to,
                    limit,
                    sort,
                    index: _,
                    storage,
                } => {
                    commands::logs::search(&cfg, query, from, to, limit, sort, storage).await?;
                }
                LogActions::List {
                    query,
                    from,
                    to,
                    limit,
                    sort,
                    storage,
                } => {
                    commands::logs::list(&cfg, query, from, to, limit, sort, storage).await?;
                }
                LogActions::Query {
                    query,
                    from,
                    to,
                    limit,
                    sort,
                    storage,
                    timezone: _,
                } => {
                    commands::logs::query(&cfg, query, from, to, limit, sort, storage).await?;
                }
                LogActions::Aggregate {
                    query,
                    from,
                    to,
                    compute,
                    group_by,
                    limit,
                    storage,
                } => {
                    commands::logs::aggregate(
                        &cfg,
                        commands::logs::AggregateArgs {
                            query: query.unwrap_or_default(),
                            from,
                            to,
                            compute: commands::logs::split_compute_args(&compute),
                            group_by: group_by
                                .map(|g| {
                                    g.split(',')
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .collect()
                                })
                                .unwrap_or_default(),
                            limit,
                            storage,
                        },
                    )
                    .await?;
                }
                LogActions::Archives { action } => match action {
                    LogArchiveActions::List => commands::logs::archives_list(&cfg).await?,
                    LogArchiveActions::Get { archive_id } => {
                        commands::logs::archives_get(&cfg, &archive_id).await?;
                    }
                    LogArchiveActions::Delete { archive_id } => {
                        commands::logs::archives_delete(&cfg, &archive_id).await?;
                    }
                },
                LogActions::CustomDestinations { action } => match action {
                    LogCustomDestinationActions::List => {
                        commands::logs::custom_destinations_list(&cfg).await?;
                    }
                    LogCustomDestinationActions::Get { destination_id } => {
                        commands::logs::custom_destinations_get(&cfg, &destination_id).await?;
                    }
                },
                LogActions::Metrics { action } => match action {
                    LogMetricActions::List => commands::logs::metrics_list(&cfg).await?,
                    LogMetricActions::Get { metric_id } => {
                        commands::logs::metrics_get(&cfg, &metric_id).await?;
                    }
                    LogMetricActions::Delete { metric_id } => {
                        commands::logs::metrics_delete(&cfg, &metric_id).await?;
                    }
                },
                LogActions::RestrictionQueries { action } => match action {
                    LogRestrictionQueryActions::List => {
                        commands::logs::restriction_queries_list(&cfg).await?;
                    }
                    LogRestrictionQueryActions::Get { query_id } => {
                        commands::logs::restriction_queries_get(&cfg, &query_id).await?;
                    }
                },
            }
        }
        // --- Incidents ---
        Commands::Incidents { action } => {
            cfg.validate_auth()?;
            match action {
                IncidentActions::List { limit } => {
                    commands::incidents::list(&cfg, limit).await?;
                }
                IncidentActions::Get { incident_id } => {
                    commands::incidents::get(&cfg, &incident_id).await?;
                }
                IncidentActions::Attachments { action } => match action {
                    IncidentAttachmentActions::List { incident_id } => {
                        commands::incidents::attachments_list(&cfg, &incident_id).await?;
                    }
                    IncidentAttachmentActions::Delete {
                        incident_id,
                        attachment_id,
                    } => {
                        commands::incidents::attachments_delete(&cfg, &incident_id, &attachment_id)
                            .await?;
                    }
                },
                IncidentActions::Settings { action } => match action {
                    IncidentSettingsActions::Get => {
                        commands::incidents::settings_get(&cfg).await?;
                    }
                    IncidentSettingsActions::Update { file } => {
                        commands::incidents::settings_update(&cfg, &file).await?;
                    }
                },
                IncidentActions::Handles { action } => match action {
                    IncidentHandleActions::List => {
                        commands::incidents::handles_list(&cfg).await?;
                    }
                    IncidentHandleActions::Create { file } => {
                        commands::incidents::handles_create(&cfg, &file).await?;
                    }
                    IncidentHandleActions::Update { file } => {
                        commands::incidents::handles_update(&cfg, &file).await?;
                    }
                    IncidentHandleActions::Delete { handle_id } => {
                        commands::incidents::handles_delete(&cfg, &handle_id).await?;
                    }
                },
                IncidentActions::PostmortemTemplates { action } => match action {
                    IncidentPostmortemActions::List => {
                        commands::incidents::postmortem_templates_list(&cfg).await?;
                    }
                    IncidentPostmortemActions::Get { template_id } => {
                        commands::incidents::postmortem_templates_get(&cfg, &template_id).await?;
                    }
                    IncidentPostmortemActions::Create { file } => {
                        commands::incidents::postmortem_templates_create(&cfg, &file).await?;
                    }
                    IncidentPostmortemActions::Update { template_id, file } => {
                        commands::incidents::postmortem_templates_update(&cfg, &template_id, &file)
                            .await?;
                    }
                    IncidentPostmortemActions::Delete { template_id } => {
                        commands::incidents::postmortem_templates_delete(&cfg, &template_id)
                            .await?;
                    }
                },
                IncidentActions::Import { file } => {
                    commands::incidents::import(&cfg, &file).await?;
                }
            }
        }
        // --- Dashboards ---
        Commands::Dashboards { action } => {
            cfg.validate_auth()?;
            match action {
                DashboardActions::List => commands::dashboards::list(&cfg).await?,
                DashboardActions::Get { id } => commands::dashboards::get(&cfg, &id).await?,
                DashboardActions::Create { file } => {
                    commands::dashboards::create(&cfg, &file).await?;
                }
                DashboardActions::Update { id, file } => {
                    commands::dashboards::update(&cfg, &id, &file).await?;
                }
                DashboardActions::Delete { id } => commands::dashboards::delete(&cfg, &id).await?,
                DashboardActions::Widgets { action } => match action {
                    WidgetActions::List {
                        experience_type,
                        filter_widget_type,
                        filter_creator_handle,
                        filter_is_favorited,
                        filter_title,
                        filter_tags,
                        sort,
                        page_number,
                        page_size,
                    } => {
                        commands::widgets::list(
                            &cfg,
                            &experience_type,
                            filter_widget_type,
                            filter_creator_handle,
                            filter_is_favorited,
                            filter_title,
                            filter_tags,
                            sort,
                            page_number,
                            page_size,
                        )
                        .await?;
                    }
                    WidgetActions::Get {
                        experience_type,
                        widget_id,
                    } => {
                        commands::widgets::get(&cfg, &experience_type, &widget_id).await?;
                    }
                    WidgetActions::Create {
                        experience_type,
                        file,
                    } => {
                        commands::widgets::create(&cfg, &experience_type, &file).await?;
                    }
                    WidgetActions::Update {
                        experience_type,
                        widget_id,
                        file,
                    } => {
                        commands::widgets::update(&cfg, &experience_type, &widget_id, &file)
                            .await?;
                    }
                    WidgetActions::Delete {
                        experience_type,
                        widget_id,
                    } => {
                        commands::widgets::delete(&cfg, &experience_type, &widget_id).await?;
                    }
                },
            }
        }
        // --- Debugger ---
        Commands::Debugger { action } => {
            cfg.validate_auth()?;
            match action {
                DebuggerActions::Probes { action } => match action {
                    DebuggerProbeActions::List { service } => {
                        commands::debugger::probes_list(&cfg, service.as_deref()).await?;
                    }
                    DebuggerProbeActions::Get { probe_id } => {
                        commands::debugger::probes_get(&cfg, &probe_id).await?;
                    }
                    DebuggerProbeActions::Create {
                        service,
                        env,
                        probe_location,
                        language,
                        template,
                        condition,
                        capture,
                        rate,
                        budget,
                        ttl,
                        depth,
                        fields,
                    } => {
                        let language = match language {
                            Some(l) => l,
                            None => commands::symdb::service_language(&cfg, &service).await?,
                        };
                        let snapshot = capture.iter().any(|c| c.is_empty());
                        let capture_expressions: Vec<&str> = capture
                            .iter()
                            .filter(|c| !c.is_empty())
                            .map(|c| c.as_str())
                            .collect();
                        commands::debugger::probes_create(
                            &cfg,
                            commands::debugger::ProbeCreateParams {
                                service: &service,
                                env: &env,
                                probe_location: &probe_location,
                                language: &language,
                                template: template.as_deref(),
                                condition: condition.as_deref(),
                                snapshot,
                                capture_expressions,
                                rate,
                                budget,
                                ttl: Some(ttl.as_str()),
                                depth,
                                fields: fields.as_deref(),
                            },
                        )
                        .await?;
                    }
                    DebuggerProbeActions::Delete { probe_id } => {
                        commands::debugger::probes_delete(&cfg, &probe_id).await?;
                    }
                    DebuggerProbeActions::Watch {
                        probe_id,
                        limit,
                        timeout,
                        from,
                        wait,
                        fields,
                    } => {
                        commands::debugger::probes_watch(
                            &cfg,
                            &probe_id,
                            limit,
                            timeout,
                            from.as_deref(),
                            wait,
                            fields.as_deref(),
                        )
                        .await?;
                    }
                },
                DebuggerActions::Context {
                    service,
                    env,
                    fields,
                } => {
                    commands::debugger::context(&cfg, &service, env.as_deref(), fields.as_deref())
                        .await?;
                }
            }
        }
        // --- Metrics ---
        Commands::Metrics { action } => {
            cfg.validate_auth()?;
            match action {
                MetricActions::List { filter, from, .. } => {
                    commands::metrics::list(&cfg, filter, from).await?;
                }
                MetricActions::Search { query, from, to } => {
                    commands::metrics::search(&cfg, query, from, to).await?;
                }
                MetricActions::Query { query, from, to } => {
                    commands::metrics::query(&cfg, query, from, to).await?;
                }
                MetricActions::Submit { file, .. } => {
                    if let Some(f) = file {
                        commands::metrics::submit(&cfg, &f).await?;
                    } else {
                        anyhow::bail!("flag-based submit not yet implemented; use --file");
                    }
                }
                MetricActions::Metadata { action } => match action {
                    MetricMetadataActions::Get { metric_name } => {
                        commands::metrics::metadata_get(&cfg, &metric_name).await?;
                    }
                    MetricMetadataActions::Update {
                        metric_name, file, ..
                    } => {
                        if let Some(f) = file {
                            commands::metrics::metadata_update(&cfg, &metric_name, &f).await?;
                        } else {
                            anyhow::bail!(
                                "flag-based metadata update not yet implemented; use --file"
                            );
                        }
                    }
                },
                MetricActions::Tags { action } => match action {
                    MetricTagActions::List { metric_name, .. } => {
                        commands::metrics::tags_list(&cfg, &metric_name).await?;
                    }
                },
            }
        }
        // --- SLOs ---
        Commands::Slos { action } => {
            cfg.validate_auth()?;
            match action {
                SloActions::List => commands::slos::list(&cfg).await?,
                SloActions::Get { id } => commands::slos::get(&cfg, &id).await?,
                SloActions::Create { file } => commands::slos::create(&cfg, &file).await?,
                SloActions::Update { id, file } => {
                    commands::slos::update(&cfg, &id, &file).await?;
                }
                SloActions::Delete { id } => commands::slos::delete(&cfg, &id).await?,
                SloActions::Status { id, from, to } => {
                    let from_ts = util::parse_time_to_unix_millis(&from)? / 1000;
                    let to_ts = util::parse_time_to_unix_millis(&to)? / 1000;
                    commands::slos::status(&cfg, &id, from_ts, to_ts).await?;
                }
            }
        }
        // --- SymDB ---
        Commands::Symdb { action } => {
            cfg.validate_auth()?;
            match action {
                SymdbActions::Search {
                    service,
                    query,
                    version,
                    view,
                } => {
                    commands::symdb::search(
                        &cfg,
                        &service,
                        query.as_deref().unwrap_or(""),
                        version.as_deref(),
                        &view,
                    )
                    .await?;
                }
            }
        }
        // --- Synthetics ---
        Commands::Synthetics { action } => {
            cfg.validate_auth()?;
            match action {
                SyntheticsActions::Tests { action } => match action {
                    SyntheticsTestActions::List {
                        page_size,
                        page_number,
                    } => commands::synthetics::tests_list(&cfg, page_size, page_number).await?,
                    SyntheticsTestActions::Get { public_id } => {
                        commands::synthetics::tests_get(&cfg, &public_id).await?;
                    }
                    SyntheticsTestActions::Search {
                        text,
                        facets_only,
                        include_full_config,
                        count,
                        start,
                        sort,
                    } => {
                        commands::synthetics::tests_search(
                            &cfg,
                            text,
                            facets_only,
                            include_full_config,
                            count,
                            start,
                            sort,
                        )
                        .await?;
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    SyntheticsTestActions::Run {
                        public_ids,
                        tunnel,
                        timeout,
                    } => {
                        commands::synthetics::tests_run(&cfg, public_ids, tunnel, timeout).await?;
                    }
                    SyntheticsTestActions::GetFastResult { result_id } => {
                        commands::synthetics::tests_get_fast_result(&cfg, &result_id).await?;
                    }
                    SyntheticsTestActions::GetVersion {
                        public_id,
                        version,
                        include_change_metadata,
                    } => {
                        commands::synthetics::tests_get_version(
                            &cfg,
                            &public_id,
                            version,
                            include_change_metadata,
                        )
                        .await?;
                    }
                    SyntheticsTestActions::ListVersions {
                        public_id,
                        limit,
                        last_version_number,
                    } => {
                        commands::synthetics::tests_list_versions(
                            &cfg,
                            &public_id,
                            limit,
                            last_version_number,
                        )
                        .await?;
                    }
                },
                SyntheticsActions::Locations { action } => match action {
                    SyntheticsLocationActions::List => {
                        commands::synthetics::locations_list(&cfg).await?;
                    }
                },
                SyntheticsActions::Suites { action } => match action {
                    SyntheticsSuiteActions::List { query } => {
                        commands::synthetics::suites_list(&cfg, query).await?;
                    }
                    SyntheticsSuiteActions::Get { suite_id } => {
                        commands::synthetics::suites_get(&cfg, &suite_id).await?;
                    }
                    SyntheticsSuiteActions::Create { file } => {
                        commands::synthetics::suites_create(&cfg, &file).await?;
                    }
                    SyntheticsSuiteActions::Update { suite_id, file } => {
                        commands::synthetics::suites_update(&cfg, &suite_id, &file).await?;
                    }
                    SyntheticsSuiteActions::Delete { suite_ids, .. } => {
                        commands::synthetics::suites_delete(&cfg, suite_ids).await?;
                    }
                },
                SyntheticsActions::Multistep { action } => match action {
                    SyntheticsMultistepActions::GetSubtests { public_id } => {
                        commands::synthetics::multistep_get_subtests(&cfg, &public_id).await?;
                    }
                    SyntheticsMultistepActions::GetSubtestParents { public_id } => {
                        commands::synthetics::multistep_get_subtest_parents(&cfg, &public_id)
                            .await?;
                    }
                },
            }
        }
        // --- Test Optimization ---
        Commands::TestOptimization { action } => {
            cfg.validate_auth()?;
            match action {
                TestOptimizationActions::Settings { action } => match action {
                    TestOptimizationSettingsActions::Get { file } => {
                        commands::test_optimization::settings_get(&cfg, &file).await?;
                    }
                    TestOptimizationSettingsActions::Update { file } => {
                        commands::test_optimization::settings_update(&cfg, &file).await?;
                    }
                    TestOptimizationSettingsActions::Delete { file } => {
                        commands::test_optimization::settings_delete(&cfg, &file).await?;
                    }
                },
                TestOptimizationActions::FlakyTests { action } => match action {
                    TestOptimizationFlakyTestsActions::Search { file } => {
                        commands::test_optimization::flaky_tests_search(&cfg, file).await?;
                    }
                    TestOptimizationFlakyTestsActions::Update { file } => {
                        commands::test_optimization::flaky_tests_update(&cfg, &file).await?;
                    }
                },
            }
        }
        // --- Events ---
        Commands::Events { action } => {
            cfg.validate_auth()?;
            match action {
                EventActions::List { from, to, tags, .. } => {
                    let start = util::parse_time_to_unix_millis(&from)? / 1000;
                    let end = util::parse_time_to_unix_millis(&to)? / 1000;
                    commands::events::list(&cfg, start, end, tags).await?;
                }
                EventActions::Search {
                    query,
                    from,
                    to,
                    limit,
                } => {
                    commands::events::search(&cfg, query, from, to, limit).await?;
                }
                EventActions::Get { event_id } => {
                    commands::events::get(&cfg, event_id).await?;
                }
            }
        }
        // --- Downtime ---
        Commands::Downtime { action } => {
            cfg.validate_auth()?;
            match action {
                DowntimeActions::List => commands::downtime::list(&cfg).await?,
                DowntimeActions::Get { id } => commands::downtime::get(&cfg, &id).await?,
                DowntimeActions::Create { file } => {
                    commands::downtime::create(&cfg, &file).await?;
                }
                DowntimeActions::Cancel { id } => commands::downtime::cancel(&cfg, &id).await?,
            }
        }
        // --- Tags ---
        Commands::Tags { action } => {
            cfg.validate_auth()?;
            match action {
                TagActions::List => commands::tags::list(&cfg).await?,
                TagActions::Get { hostname } => commands::tags::get(&cfg, &hostname).await?,
                TagActions::Add { hostname, tags } => {
                    commands::tags::add(&cfg, &hostname, tags).await?;
                }
                TagActions::Update { hostname, tags } => {
                    commands::tags::update(&cfg, &hostname, tags).await?;
                }
                TagActions::Delete { hostname } => {
                    commands::tags::delete(&cfg, &hostname).await?;
                }
            }
        }
        // --- Users ---
        Commands::Users { action } => {
            cfg.validate_auth()?;
            match action {
                UserActions::List => commands::users::list(&cfg).await?,
                UserActions::Get { user_id } => commands::users::get(&cfg, &user_id).await?,
                UserActions::Roles { action } => match action {
                    UserRoleActions::List => commands::users::roles_list(&cfg).await?,
                },
                UserActions::Seats { action } => match action {
                    SeatsActions::Users { action } => match action {
                        SeatsUserActions::List { product, limit } => {
                            commands::seats::users_list(&cfg, &product, limit).await?;
                        }
                        SeatsUserActions::Assign { file } => {
                            commands::seats::users_assign(&cfg, &file).await?;
                        }
                        SeatsUserActions::Unassign { file } => {
                            commands::seats::users_unassign(&cfg, &file).await?;
                        }
                    },
                },
            }
        }
        // --- Infrastructure ---
        Commands::Infrastructure { action } => {
            cfg.validate_auth()?;
            match action {
                InfraActions::Hosts { action } => match action {
                    InfraHostActions::List {
                        filter,
                        sort,
                        count,
                    } => {
                        commands::infrastructure::hosts_list(&cfg, filter, sort, count).await?;
                    }
                    InfraHostActions::Get { hostname } => {
                        commands::infrastructure::hosts_get(&cfg, &hostname).await?;
                    }
                },
            }
        }
        // --- IDP (Internal Developer Portal) ---
        Commands::Idp { action } => {
            cfg.validate_auth()?;
            match action {
                IdpActions::Assist { entity } => {
                    commands::idp::assist(&cfg, &entity).await?;
                }
                IdpActions::Find { query } => {
                    commands::idp::find(&cfg, &query).await?;
                }
                IdpActions::Owner { entity } => {
                    commands::idp::owner(&cfg, &entity).await?;
                }
                IdpActions::Deps { entity } => {
                    commands::idp::deps(&cfg, &entity).await?;
                }
                IdpActions::Register { file } => {
                    commands::idp::register(&cfg, &file).await?;
                }
            }
        }
        // --- Audit Logs ---
        Commands::AuditLogs { action } => {
            cfg.validate_auth()?;
            match action {
                AuditLogActions::List { from, to, limit } => {
                    commands::audit_logs::list(&cfg, from, to, limit).await?;
                }
                AuditLogActions::Search {
                    query,
                    from,
                    to,
                    limit,
                } => {
                    commands::audit_logs::search(&cfg, query, from, to, limit).await?;
                }
            }
        }
        // --- Bits AI ---
        Commands::Bits { action } => match action {
            BitsActions::Ask {
                query,
                agent_id,
                no_stream,
                interactive,
            } => {
                commands::bits::ask(&cfg, query.as_deref(), agent_id, !no_stream, interactive)
                    .await?;
            }
        },
        // --- Security ---
        Commands::Security { action } => {
            cfg.validate_auth()?;
            match action {
                SecurityActions::Rules { action } => match action {
                    SecurityRuleActions::List { sort, .. } => {
                        commands::security::rules_list(&cfg, sort).await?
                    }
                    SecurityRuleActions::Get { rule_id } => {
                        commands::security::rules_get(&cfg, &rule_id).await?;
                    }
                    SecurityRuleActions::BulkExport { rule_ids } => {
                        commands::security::rules_bulk_export(&cfg, rule_ids).await?;
                    }
                },
                SecurityActions::Signals { action } => match action {
                    SecuritySignalActions::List {
                        query,
                        from,
                        to,
                        limit,
                        ..
                    } => {
                        commands::security::signals_search(&cfg, query, from, to, limit).await?;
                    }
                },
                SecurityActions::Findings { action } => match action {
                    SecurityFindingActions::Search { query, limit } => {
                        commands::security::findings_search(&cfg, query, limit).await?;
                    }
                },
                SecurityActions::ContentPacks { action } => match action {
                    SecurityContentPackActions::List => {
                        commands::security::content_packs_list(&cfg).await?;
                    }
                    SecurityContentPackActions::Activate { pack_id } => {
                        commands::security::content_packs_activate(&cfg, &pack_id).await?;
                    }
                    SecurityContentPackActions::Deactivate { pack_id } => {
                        commands::security::content_packs_deactivate(&cfg, &pack_id).await?;
                    }
                },
                SecurityActions::RiskScores { action } => match action {
                    SecurityRiskScoreActions::List { query } => {
                        commands::security::risk_scores_list(&cfg, query).await?;
                    }
                },
                SecurityActions::Suppressions { action } => match action {
                    SecuritySuppressionActions::List { sort } => {
                        commands::security::suppressions_list(&cfg, sort).await?;
                    }
                    SecuritySuppressionActions::Get { suppression_id } => {
                        commands::security::suppressions_get(&cfg, &suppression_id).await?;
                    }
                    SecuritySuppressionActions::Create { file } => {
                        commands::security::suppressions_create(&cfg, &file).await?;
                    }
                    SecuritySuppressionActions::Update {
                        suppression_id,
                        file,
                    } => {
                        commands::security::suppressions_update(&cfg, &suppression_id, &file)
                            .await?;
                    }
                    SecuritySuppressionActions::Delete { suppression_id } => {
                        commands::security::suppressions_delete(&cfg, &suppression_id).await?;
                    }
                    SecuritySuppressionActions::Validate { file } => {
                        commands::security::suppressions_validate(&cfg, &file).await?;
                    }
                },
            }
        }
        // --- Organizations ---
        Commands::Organizations { action } => {
            cfg.validate_auth()?;
            match action {
                OrgActions::List => commands::organizations::list(&cfg).await?,
                OrgActions::Get => commands::organizations::get(&cfg).await?,
            }
        }
        // --- Change Management ---
        Commands::ChangeManagement { action } => {
            cfg.validate_auth()?;
            match action {
                ChangeManagementActions::Create { file } => {
                    commands::change_management::create(&cfg, &file).await?;
                }
                ChangeManagementActions::Get { change_request_id } => {
                    commands::change_management::get(&cfg, &change_request_id).await?;
                }
                ChangeManagementActions::Update {
                    change_request_id,
                    file,
                } => {
                    commands::change_management::update(&cfg, &change_request_id, &file).await?;
                }
                ChangeManagementActions::CreateBranch {
                    change_request_id,
                    file,
                } => {
                    commands::change_management::create_branch(&cfg, &change_request_id, &file)
                        .await?;
                }
                ChangeManagementActions::Decisions { action } => match action {
                    ChangeRequestDecisionActions::Delete {
                        change_request_id,
                        decision_id,
                    } => {
                        commands::change_management::delete_decision(
                            &cfg,
                            &change_request_id,
                            &decision_id,
                        )
                        .await?;
                    }
                    ChangeRequestDecisionActions::Update {
                        change_request_id,
                        decision_id,
                        file,
                    } => {
                        commands::change_management::update_decision(
                            &cfg,
                            &change_request_id,
                            &decision_id,
                            &file,
                        )
                        .await?;
                    }
                },
            }
        }
        // --- Cloud ---
        Commands::Cloud { action } => {
            cfg.validate_auth()?;
            match action {
                CloudActions::Aws { action } => match action {
                    CloudAwsActions::List => commands::cloud::aws_list(&cfg).await?,
                },
                CloudActions::Gcp { action } => match action {
                    CloudGcpActions::List => commands::cloud::gcp_list(&cfg).await?,
                },
                CloudActions::Azure { action } => match action {
                    CloudAzureActions::List => commands::cloud::azure_list(&cfg).await?,
                },
                CloudActions::Oci { action } => match action {
                    CloudOciActions::Tenancies { action } => match action {
                        CloudOciTenancyActions::List => {
                            commands::cloud::oci_tenancies_list(&cfg).await?;
                        }
                        CloudOciTenancyActions::Get { tenancy_id } => {
                            commands::cloud::oci_tenancies_get(&cfg, &tenancy_id).await?;
                        }
                        CloudOciTenancyActions::Create { file } => {
                            commands::cloud::oci_tenancies_create(&cfg, &file).await?;
                        }
                        CloudOciTenancyActions::Update { tenancy_id, file } => {
                            commands::cloud::oci_tenancies_update(&cfg, &tenancy_id, &file).await?;
                        }
                        CloudOciTenancyActions::Delete { tenancy_id } => {
                            commands::cloud::oci_tenancies_delete(&cfg, &tenancy_id).await?;
                        }
                    },
                    CloudOciActions::Products { action } => match action {
                        CloudOciProductActions::List { product_keys } => {
                            commands::cloud::oci_products_list(&cfg, &product_keys).await?;
                        }
                    },
                },
            }
        }
        // --- Cases ---
        Commands::Cases { action } => {
            cfg.validate_auth()?;
            match action {
                CaseActions::Search {
                    query, page_size, ..
                } => {
                    commands::cases::search(&cfg, query, page_size).await?;
                }
                CaseActions::Get { case_id } => commands::cases::get(&cfg, &case_id).await?,
                CaseActions::Create {
                    title,
                    type_id,
                    priority,
                    description,
                    file,
                } => {
                    if let Some(f) = file {
                        commands::cases::create(&cfg, &f).await?;
                    } else {
                        commands::cases::create_from_flags(
                            &cfg,
                            &title.unwrap(),
                            &type_id.unwrap(),
                            &priority,
                            description.as_deref(),
                        )
                        .await?;
                    }
                }
                CaseActions::Archive { case_id } => {
                    commands::cases::archive(&cfg, &case_id).await?;
                }
                CaseActions::Unarchive { case_id } => {
                    commands::cases::unarchive(&cfg, &case_id).await?;
                }
                CaseActions::Assign { case_id, user_id } => {
                    commands::cases::assign(&cfg, &case_id, &user_id).await?;
                }
                CaseActions::UpdatePriority { case_id, priority } => {
                    commands::cases::update_priority(&cfg, &case_id, &priority).await?;
                }
                CaseActions::UpdateStatus { case_id, status } => {
                    commands::cases::update_status(&cfg, &case_id, &status).await?;
                }
                CaseActions::Move {
                    case_id,
                    project_id,
                } => {
                    commands::cases::move_to_project(&cfg, &case_id, &project_id).await?;
                }
                CaseActions::UpdateTitle { case_id, title } => {
                    commands::cases::update_title(&cfg, &case_id, &title).await?;
                }
                CaseActions::Projects { action } => match action {
                    CaseProjectActions::List => commands::cases::projects_list(&cfg).await?,
                    CaseProjectActions::Get { project_id } => {
                        commands::cases::projects_get(&cfg, &project_id).await?;
                    }
                    CaseProjectActions::Create { name, key } => {
                        commands::cases::projects_create(&cfg, &name, &key).await?;
                    }
                    CaseProjectActions::Delete { project_id } => {
                        commands::cases::projects_delete(&cfg, &project_id).await?;
                    }
                    CaseProjectActions::Update { project_id, file } => {
                        commands::cases::projects_update(&cfg, &project_id, &file).await?;
                    }
                    CaseProjectActions::NotificationRules { action } => match action {
                        CaseNotificationRuleActions::List { project_id } => {
                            commands::cases::projects_notification_rules_list(&cfg, &project_id)
                                .await?;
                        }
                        CaseNotificationRuleActions::Create { project_id, file } => {
                            commands::cases::projects_notification_rules_create(
                                &cfg,
                                &project_id,
                                &file,
                            )
                            .await?;
                        }
                        CaseNotificationRuleActions::Update {
                            project_id,
                            rule_id,
                            file,
                        } => {
                            commands::cases::projects_notification_rules_update(
                                &cfg,
                                &project_id,
                                &rule_id,
                                &file,
                            )
                            .await?;
                        }
                        CaseNotificationRuleActions::Delete {
                            project_id,
                            rule_id,
                        } => {
                            commands::cases::projects_notification_rules_delete(
                                &cfg,
                                &project_id,
                                &rule_id,
                            )
                            .await?;
                        }
                    },
                },
                CaseActions::Jira { action } => match action {
                    CaseJiraActions::CreateIssue { case_id, file } => {
                        commands::cases::jira_create_issue(&cfg, &case_id, &file).await?;
                    }
                    CaseJiraActions::Link { case_id, file } => {
                        commands::cases::jira_link(&cfg, &case_id, &file).await?;
                    }
                    CaseJiraActions::Unlink { case_id } => {
                        commands::cases::jira_unlink(&cfg, &case_id).await?;
                    }
                },
                CaseActions::Servicenow { action } => match action {
                    CaseServicenowActions::CreateTicket { case_id, file } => {
                        commands::cases::servicenow_create_ticket(&cfg, &case_id, &file).await?;
                    }
                },
            }
        }
        // --- Service Catalog ---
        Commands::ServiceCatalog { action } => {
            cfg.validate_auth()?;
            match action {
                ServiceCatalogActions::List => commands::service_catalog::list(&cfg).await?,
                ServiceCatalogActions::Get { service_name } => {
                    commands::service_catalog::get(&cfg, &service_name).await?;
                }
            }
        }
        // --- API Keys ---
        Commands::ApiKeys { action } => {
            cfg.validate_auth()?;
            match action {
                ApiKeyActions::List => commands::api_keys::list(&cfg).await?,
                ApiKeyActions::Get { key_id } => commands::api_keys::get(&cfg, &key_id).await?,
                ApiKeyActions::Create { name } => {
                    commands::api_keys::create(&cfg, &name).await?;
                }
                ApiKeyActions::Delete { key_id } => {
                    commands::api_keys::delete(&cfg, &key_id).await?;
                }
            }
        }
        // --- App Builder ---
        Commands::AppBuilder { action } => {
            cfg.validate_auth()?;
            match action {
                AppBuilderActions::List { query } => {
                    commands::app_builder::list(&cfg, query.as_deref()).await?;
                }
                AppBuilderActions::Get { app_id } => {
                    commands::app_builder::get(&cfg, &app_id).await?;
                }
                AppBuilderActions::Create { file } => {
                    commands::app_builder::create(&cfg, &file).await?;
                }
                AppBuilderActions::Update { app_id, file } => {
                    commands::app_builder::update(&cfg, &app_id, &file).await?;
                }
                AppBuilderActions::Delete { app_id } => {
                    commands::app_builder::delete(&cfg, &app_id).await?;
                }
                AppBuilderActions::DeleteBatch { app_ids } => {
                    commands::app_builder::delete_batch(&cfg, &app_ids).await?;
                }
                AppBuilderActions::Publish { app_id } => {
                    commands::app_builder::publish(&cfg, &app_id).await?;
                }
                AppBuilderActions::Unpublish { app_id } => {
                    commands::app_builder::unpublish(&cfg, &app_id).await?;
                }
            }
        }
        // --- App Keys ---
        Commands::AppKeys { action } => {
            cfg.validate_auth()?;
            match action {
                AppKeyActions::List {
                    page_size,
                    page_number,
                    filter,
                    sort,
                    all,
                } => {
                    if all {
                        cfg.validate_api_and_app_keys()?;
                        commands::app_keys::list_all(&cfg, page_size, page_number, &filter, &sort)
                            .await?
                    } else {
                        commands::app_keys::list(&cfg, page_size, page_number, &filter, &sort)
                            .await?
                    }
                }
                AppKeyActions::Get { key_id } => commands::app_keys::get(&cfg, &key_id).await?,
                AppKeyActions::Create { name, scopes } => {
                    commands::app_keys::create(&cfg, &name, &scopes).await?
                }
                AppKeyActions::Update {
                    key_id,
                    name,
                    scopes,
                } => {
                    if name.is_empty() && scopes.is_empty() {
                        anyhow::bail!("at least one of --name or --scopes is required for update");
                    }
                    commands::app_keys::update(&cfg, &key_id, &name, &scopes).await?
                }
                AppKeyActions::Delete { key_id } => {
                    if !cfg.auto_approve {
                        eprint!(
                            "Permanently delete application key {key_id}? Type 'yes' to confirm: "
                        );
                        let mut input = String::new();
                        std::io::stdin().read_line(&mut input)?;
                        if input.trim() != "yes" {
                            println!("Operation cancelled.");
                            return Ok(());
                        }
                    }
                    commands::app_keys::delete(&cfg, &key_id).await?
                }
            }
        }
        // --- Usage ---
        Commands::Usage { action } => {
            cfg.validate_auth()?;
            match action {
                UsageActions::Summary { from, to } => {
                    commands::usage::summary(&cfg, from, to).await?;
                }
                UsageActions::Hourly { from, to } => {
                    commands::usage::hourly(&cfg, from, to).await?;
                }
            }
        }
        // --- Notebooks ---
        Commands::Notebooks { action } => {
            cfg.validate_auth()?;
            match action {
                NotebookActions::List => commands::notebooks::list(&cfg).await?,
                NotebookActions::Get { notebook_id } => {
                    commands::notebooks::get(&cfg, notebook_id).await?;
                }
                NotebookActions::Create { file } => {
                    commands::notebooks::create(&cfg, &file).await?;
                }
                NotebookActions::Update { notebook_id, file } => {
                    commands::notebooks::update(&cfg, notebook_id, &file).await?;
                }
                NotebookActions::Delete { notebook_id } => {
                    commands::notebooks::delete(&cfg, notebook_id).await?;
                }
            }
        }
        // --- RUM ---
        Commands::Rum { action } => {
            cfg.validate_auth()?;
            match action {
                RumActions::Apps { action } => match action {
                    RumAppActions::List => commands::rum::apps_list(&cfg).await?,
                    RumAppActions::Get { app_id } => commands::rum::apps_get(&cfg, &app_id).await?,
                    RumAppActions::Create { name, app_type } => {
                        commands::rum::apps_create(&cfg, &name, app_type).await?;
                    }
                    RumAppActions::Update { app_id, file, .. } => {
                        let f = file.unwrap_or_default();
                        commands::rum::apps_update(&cfg, &app_id, &f).await?;
                    }
                    RumAppActions::Delete { app_id } => {
                        commands::rum::apps_delete(&cfg, &app_id).await?;
                    }
                },
                RumActions::Events {
                    query,
                    from,
                    to,
                    limit,
                } => {
                    commands::rum::events_list(&cfg, query, from, to, limit).await?;
                }
                RumActions::Sessions { action } => match action {
                    RumSessionActions::Search {
                        query,
                        from,
                        to,
                        limit,
                    } => {
                        commands::rum::sessions_search(&cfg, query, from, to, limit).await?;
                    }
                    RumSessionActions::List { from, to, limit } => {
                        commands::rum::sessions_list(&cfg, from, to, limit).await?;
                    }
                },
                RumActions::Metrics { action } => match action {
                    RumMetricActions::List => commands::rum::metrics_list(&cfg).await?,
                    RumMetricActions::Get { metric_id } => {
                        commands::rum::metrics_get(&cfg, &metric_id).await?;
                    }
                    RumMetricActions::Create { file } => {
                        commands::rum::metrics_create(&cfg, &file).await?;
                    }
                    RumMetricActions::Update { metric_id, file } => {
                        commands::rum::metrics_update(&cfg, &metric_id, &file).await?;
                    }
                    RumMetricActions::Delete { metric_id } => {
                        commands::rum::metrics_delete(&cfg, &metric_id).await?;
                    }
                },
                RumActions::RetentionFilters { action } => match action {
                    RumRetentionFilterActions::List { app_id } => {
                        commands::rum::retention_filters_list(&cfg, &app_id).await?;
                    }
                    RumRetentionFilterActions::Get { app_id, filter_id } => {
                        commands::rum::retention_filters_get(&cfg, &app_id, &filter_id).await?;
                    }
                    RumRetentionFilterActions::Create { app_id, file } => {
                        commands::rum::retention_filters_create(&cfg, &app_id, &file).await?;
                    }
                    RumRetentionFilterActions::Update {
                        app_id,
                        filter_id,
                        file,
                    } => {
                        commands::rum::retention_filters_update(&cfg, &app_id, &filter_id, &file)
                            .await?;
                    }
                    RumRetentionFilterActions::Delete { app_id, filter_id } => {
                        commands::rum::retention_filters_delete(&cfg, &app_id, &filter_id).await?;
                    }
                },
                RumActions::Playlists { action } => match action {
                    RumPlaylistActions::List => commands::rum::playlists_list(&cfg).await?,
                    RumPlaylistActions::Get { playlist_id } => {
                        commands::rum::playlists_get(&cfg, playlist_id).await?;
                    }
                },
                RumActions::Heatmaps { action } => match action {
                    RumHeatmapActions::Query { view_name, .. } => {
                        commands::rum::heatmaps_query(&cfg, &view_name).await?;
                    }
                },
            }
        }
        // --- CI/CD ---
        Commands::Cicd { action } => {
            cfg.validate_auth()?;
            match action {
                CicdActions::Pipelines { action } => match action {
                    CicdPipelineActions::List {
                        query,
                        from,
                        to,
                        limit,
                        ..
                    } => {
                        commands::cicd::pipelines_list(&cfg, query, from, to, limit).await?;
                    }
                    CicdPipelineActions::Get { pipeline_id } => {
                        commands::cicd::pipelines_get(&cfg, &pipeline_id).await?;
                    }
                },
                CicdActions::Tests { action } => match action {
                    CicdTestActions::List {
                        query,
                        from,
                        to,
                        limit,
                    } => {
                        commands::cicd::tests_list(&cfg, query, from, to, limit).await?;
                    }
                    CicdTestActions::Search {
                        query,
                        from,
                        to,
                        limit,
                    } => {
                        commands::cicd::tests_search(&cfg, query, from, to, limit).await?;
                    }
                    CicdTestActions::Aggregate {
                        query, from, to, ..
                    } => {
                        commands::cicd::tests_aggregate(&cfg, query, from, to).await?;
                    }
                },
                CicdActions::Events { action } => match action {
                    CicdEventActions::Search {
                        query,
                        from,
                        to,
                        limit,
                        ..
                    } => {
                        commands::cicd::events_search(&cfg, query, from, to, limit).await?;
                    }
                    CicdEventActions::Aggregate {
                        query, from, to, ..
                    } => {
                        commands::cicd::events_aggregate(&cfg, query, from, to).await?;
                    }
                },
                CicdActions::Dora { action } => match action {
                    CicdDoraActions::PatchDeployment {
                        deployment_id,
                        file,
                    } => {
                        commands::cicd::dora_patch_deployment(&cfg, &deployment_id, &file).await?;
                    }
                },
                CicdActions::FlakyTests { action } => match action {
                    CicdFlakyTestActions::Search {
                        query,
                        cursor,
                        limit,
                        include_history,
                        sort,
                    } => {
                        commands::cicd::flaky_tests_search(
                            &cfg,
                            query,
                            cursor,
                            limit,
                            include_history,
                            sort,
                        )
                        .await?;
                    }
                    CicdFlakyTestActions::Update { file } => {
                        commands::cicd::flaky_tests_update(&cfg, &file).await?;
                    }
                },
            }
        }
        // --- On-Call ---
        Commands::OnCall { action } => {
            cfg.validate_auth()?;
            match action {
                OnCallActions::Teams { action } => match action {
                    OnCallTeamActions::List => commands::on_call::teams_list(&cfg).await?,
                    OnCallTeamActions::Get { team_id } => {
                        commands::on_call::teams_get(&cfg, &team_id).await?;
                    }
                    OnCallTeamActions::Create { name, handle, .. } => {
                        commands::on_call::teams_create(&cfg, &name, &handle).await?;
                    }
                    OnCallTeamActions::Update {
                        team_id,
                        name,
                        handle,
                    } => {
                        commands::on_call::teams_update(&cfg, &team_id, &name, &handle).await?;
                    }
                    OnCallTeamActions::Delete { team_id } => {
                        commands::on_call::teams_delete(&cfg, &team_id).await?;
                    }
                    OnCallTeamActions::Memberships { action } => match action {
                        OnCallMembershipActions::List {
                            team_id, page_size, ..
                        } => {
                            commands::on_call::memberships_list(&cfg, &team_id, page_size).await?;
                        }
                        OnCallMembershipActions::Add {
                            team_id,
                            user_id,
                            role,
                        } => {
                            commands::on_call::memberships_add(&cfg, &team_id, &user_id, role)
                                .await?;
                        }
                        OnCallMembershipActions::Update {
                            team_id,
                            user_id,
                            role,
                        } => {
                            commands::on_call::memberships_update(&cfg, &team_id, &user_id, &role)
                                .await?;
                        }
                        OnCallMembershipActions::Remove { team_id, user_id } => {
                            commands::on_call::memberships_remove(&cfg, &team_id, &user_id).await?;
                        }
                    },
                },
            }
        }
        // --- Fleet ---
        Commands::Fleet { action } => {
            cfg.validate_auth()?;
            match action {
                FleetActions::Agents { action } => match action {
                    FleetAgentActions::List { page_size, filter } => {
                        commands::fleet::agents_list(&cfg, page_size, filter).await?;
                    }
                    FleetAgentActions::Get { agent_key } => {
                        commands::fleet::agents_get(&cfg, &agent_key).await?;
                    }
                    FleetAgentActions::Versions => commands::fleet::agents_versions(&cfg).await?,
                },
                FleetActions::Deployments { action } => match action {
                    FleetDeploymentActions::List { page_size } => {
                        commands::fleet::deployments_list(&cfg, page_size).await?;
                    }
                    FleetDeploymentActions::Get { deployment_id } => {
                        commands::fleet::deployments_get(&cfg, &deployment_id).await?;
                    }
                    FleetDeploymentActions::Cancel { deployment_id } => {
                        commands::fleet::deployments_cancel(&cfg, &deployment_id).await?;
                    }
                    FleetDeploymentActions::Configure { file } => {
                        commands::fleet::deployments_configure(&cfg, &file).await?;
                    }
                    FleetDeploymentActions::Upgrade { file } => {
                        commands::fleet::deployments_upgrade(&cfg, &file).await?;
                    }
                },
                FleetActions::Schedules { action } => match action {
                    FleetScheduleActions::List => commands::fleet::schedules_list(&cfg).await?,
                    FleetScheduleActions::Get { schedule_id } => {
                        commands::fleet::schedules_get(&cfg, &schedule_id).await?;
                    }
                    FleetScheduleActions::Create { file } => {
                        commands::fleet::schedules_create(&cfg, &file).await?;
                    }
                    FleetScheduleActions::Update { schedule_id, file } => {
                        commands::fleet::schedules_update(&cfg, &schedule_id, &file).await?;
                    }
                    FleetScheduleActions::Delete { schedule_id } => {
                        commands::fleet::schedules_delete(&cfg, &schedule_id).await?;
                    }
                    FleetScheduleActions::Trigger { schedule_id } => {
                        commands::fleet::schedules_trigger(&cfg, &schedule_id).await?;
                    }
                },
            }
        }
        // --- Data Governance ---
        Commands::DataGovernance { action } => {
            cfg.validate_auth()?;
            match action {
                DataGovActions::Scanner { action } => match action {
                    DataGovScannerActions::Rules { action } => match action {
                        DataGovScannerRuleActions::List => {
                            commands::data_governance::scanner_rules_list(&cfg).await?;
                        }
                    },
                },
            }
        }
        // --- Deployment Gates ---
        Commands::DeploymentGates { action } => {
            cfg.validate_auth()?;
            match action {
                DeploymentGatesActions::Gates { action } => match action {
                    DeploymentGatesGateActions::List {
                        page_cursor,
                        page_size,
                    } => {
                        commands::deployment_gates::list(&cfg, page_cursor, page_size).await?;
                    }
                    DeploymentGatesGateActions::Get { gate_id } => {
                        commands::deployment_gates::get(&cfg, &gate_id).await?;
                    }
                    DeploymentGatesGateActions::Create { file } => {
                        commands::deployment_gates::create(&cfg, &file).await?;
                    }
                    DeploymentGatesGateActions::Update { gate_id, file } => {
                        commands::deployment_gates::update(&cfg, &gate_id, &file).await?;
                    }
                    DeploymentGatesGateActions::Delete { gate_id } => {
                        commands::deployment_gates::delete(&cfg, &gate_id).await?;
                    }
                },
                DeploymentGatesActions::Evaluations { action } => match action {
                    DeploymentGatesEvaluationActions::Get { evaluation_id } => {
                        commands::deployment_gates::evaluation_get(&cfg, &evaluation_id).await?;
                    }
                    DeploymentGatesEvaluationActions::Trigger { file } => {
                        commands::deployment_gates::evaluation_trigger(&cfg, &file).await?;
                    }
                },
                DeploymentGatesActions::Rules { action } => match action {
                    DeploymentGatesRuleActions::List { gate_id } => {
                        commands::deployment_gates::rules_list(&cfg, &gate_id).await?;
                    }
                    DeploymentGatesRuleActions::Get { gate_id, rule_id } => {
                        commands::deployment_gates::rule_get(&cfg, &gate_id, &rule_id).await?;
                    }
                    DeploymentGatesRuleActions::Create { gate_id, file } => {
                        commands::deployment_gates::rule_create(&cfg, &gate_id, &file).await?;
                    }
                    DeploymentGatesRuleActions::Update {
                        gate_id,
                        rule_id,
                        file,
                    } => {
                        commands::deployment_gates::rule_update(&cfg, &gate_id, &rule_id, &file)
                            .await?;
                    }
                    DeploymentGatesRuleActions::Delete { gate_id, rule_id } => {
                        commands::deployment_gates::rule_delete(&cfg, &gate_id, &rule_id).await?;
                    }
                },
            }
        }
        // --- Error Tracking ---
        Commands::ErrorTracking { action } => {
            cfg.validate_auth()?;
            match action {
                ErrorTrackingActions::Issues { action } => match action {
                    ErrorTrackingIssueActions::Search {
                        query,
                        limit,
                        track,
                        persona,
                        ..
                    } => {
                        commands::error_tracking::issues_search(&cfg, query, limit, track, persona)
                            .await?;
                    }
                    ErrorTrackingIssueActions::Get { issue_id } => {
                        commands::error_tracking::issues_get(&cfg, &issue_id).await?;
                    }
                },
            }
        }
        // --- Code Coverage ---
        Commands::CodeCoverage { action } => {
            cfg.validate_auth()?;
            match action {
                CodeCoverageActions::BranchSummary { repo, branch } => {
                    commands::code_coverage::branch_summary(&cfg, repo, branch).await?;
                }
                CodeCoverageActions::CommitSummary { repo, commit } => {
                    commands::code_coverage::commit_summary(&cfg, repo, commit).await?;
                }
            }
        }
        // --- Feature Flags ---
        Commands::FeatureFlags { action } => {
            cfg.validate_auth()?;
            match action {
                FeatureFlagActions::Flags { action } => match action {
                    FeatureFlagFlagActions::List {
                        key,
                        is_archived,
                        limit,
                        offset,
                    } => {
                        commands::feature_flags::flags_list(&cfg, key, is_archived, limit, offset)
                            .await?;
                    }
                    FeatureFlagFlagActions::Get { feature_flag_id } => {
                        commands::feature_flags::flags_get(&cfg, &feature_flag_id).await?;
                    }
                    FeatureFlagFlagActions::Create { file } => {
                        commands::feature_flags::flags_create(&cfg, &file).await?;
                    }
                    FeatureFlagFlagActions::Update {
                        feature_flag_id,
                        file,
                    } => {
                        commands::feature_flags::flags_update(&cfg, &feature_flag_id, &file)
                            .await?;
                    }
                    FeatureFlagFlagActions::Archive { feature_flag_id } => {
                        commands::feature_flags::flags_archive(&cfg, &feature_flag_id).await?;
                    }
                    FeatureFlagFlagActions::Unarchive { feature_flag_id } => {
                        commands::feature_flags::flags_unarchive(&cfg, &feature_flag_id).await?;
                    }
                },
                FeatureFlagActions::Environments { action } => match action {
                    FeatureFlagEnvActions::List {
                        name,
                        key,
                        limit,
                        offset,
                    } => {
                        commands::feature_flags::envs_list(&cfg, name, key, limit, offset).await?;
                    }
                    FeatureFlagEnvActions::Get {
                        feature_flags_env_id,
                    } => {
                        commands::feature_flags::envs_get(&cfg, &feature_flags_env_id).await?;
                    }
                    FeatureFlagEnvActions::Create { file } => {
                        commands::feature_flags::envs_create(&cfg, &file).await?;
                    }
                    FeatureFlagEnvActions::Update {
                        feature_flags_env_id,
                        file,
                    } => {
                        commands::feature_flags::envs_update(&cfg, &feature_flags_env_id, &file)
                            .await?;
                    }
                    FeatureFlagEnvActions::Delete {
                        feature_flags_env_id,
                    } => {
                        commands::feature_flags::envs_delete(&cfg, &feature_flags_env_id).await?;
                    }
                },
                FeatureFlagActions::Enable {
                    feature_flag_id,
                    feature_flags_env_id,
                } => {
                    commands::feature_flags::enable(&cfg, &feature_flag_id, &feature_flags_env_id)
                        .await?;
                }
                FeatureFlagActions::Disable {
                    feature_flag_id,
                    feature_flags_env_id,
                } => {
                    commands::feature_flags::disable(&cfg, &feature_flag_id, &feature_flags_env_id)
                        .await?;
                }
            }
        }
        // --- HAMR ---
        Commands::Hamr { action } => {
            cfg.validate_auth()?;
            match action {
                HamrActions::Connections { action } => match action {
                    HamrConnectionActions::Get => commands::hamr::connections_get(&cfg).await?,
                    HamrConnectionActions::Create { file } => {
                        commands::hamr::connections_create(&cfg, &file).await?;
                    }
                },
            }
        }
        // --- Status Pages ---
        Commands::StatusPages { action } => match action {
            StatusPageActions::Pages { action } => match action {
                StatusPagePageActions::List => commands::status_pages::pages_list(&cfg).await?,
                StatusPagePageActions::Get { page_id } => {
                    commands::status_pages::pages_get(&cfg, &page_id).await?;
                }
                StatusPagePageActions::Create { file } => {
                    commands::status_pages::pages_create(&cfg, &file).await?;
                }
                StatusPagePageActions::Update { page_id, file } => {
                    commands::status_pages::pages_update(&cfg, &page_id, &file).await?;
                }
                StatusPagePageActions::Delete { page_id } => {
                    commands::status_pages::pages_delete(&cfg, &page_id).await?;
                }
            },
            StatusPageActions::Components { action } => match action {
                StatusPageComponentActions::List { page_id } => {
                    commands::status_pages::components_list(&cfg, &page_id).await?;
                }
                StatusPageComponentActions::Get {
                    page_id,
                    component_id,
                } => {
                    commands::status_pages::components_get(&cfg, &page_id, &component_id).await?;
                }
                StatusPageComponentActions::Create { page_id, file } => {
                    commands::status_pages::components_create(&cfg, &page_id, &file).await?;
                }
                StatusPageComponentActions::Update {
                    page_id,
                    component_id,
                    file,
                } => {
                    commands::status_pages::components_update(&cfg, &page_id, &component_id, &file)
                        .await?;
                }
                StatusPageComponentActions::Delete {
                    page_id,
                    component_id,
                } => {
                    commands::status_pages::components_delete(&cfg, &page_id, &component_id)
                        .await?;
                }
            },
            StatusPageActions::Degradations { action } => match action {
                StatusPageDegradationActions::List => {
                    commands::status_pages::degradations_list(&cfg).await?;
                }
                StatusPageDegradationActions::Get {
                    page_id,
                    degradation_id,
                } => {
                    commands::status_pages::degradations_get(&cfg, &page_id, &degradation_id)
                        .await?;
                }
                StatusPageDegradationActions::Create { page_id, file } => {
                    commands::status_pages::degradations_create(&cfg, &page_id, &file).await?;
                }
                StatusPageDegradationActions::Update {
                    page_id,
                    degradation_id,
                    file,
                } => {
                    commands::status_pages::degradations_update(
                        &cfg,
                        &page_id,
                        &degradation_id,
                        &file,
                    )
                    .await?;
                }
                StatusPageDegradationActions::Delete {
                    page_id,
                    degradation_id,
                } => {
                    commands::status_pages::degradations_delete(&cfg, &page_id, &degradation_id)
                        .await?;
                }
            },
            StatusPageActions::ThirdParty { action } => match action {
                StatusPageThirdPartyActions::List { active, search } => {
                    commands::status_pages::third_party_list(&cfg, search.as_deref(), active)
                        .await?;
                }
            },
            StatusPageActions::Maintenances { action } => match action {
                StatusPageMaintenanceActions::List => {
                    commands::status_pages::maintenances_list(&cfg).await?;
                }
                StatusPageMaintenanceActions::Get {
                    page_id,
                    maintenance_id,
                } => {
                    commands::status_pages::maintenances_get(&cfg, &page_id, &maintenance_id)
                        .await?;
                }
                StatusPageMaintenanceActions::Create { page_id, file } => {
                    commands::status_pages::maintenances_create(&cfg, &page_id, &file).await?;
                }
                StatusPageMaintenanceActions::Update {
                    page_id,
                    maintenance_id,
                    file,
                } => {
                    commands::status_pages::maintenances_update(
                        &cfg,
                        &page_id,
                        &maintenance_id,
                        &file,
                    )
                    .await?;
                }
            },
        },
        // --- Integrations ---
        Commands::Integrations { action } => {
            cfg.validate_auth()?;
            match action {
                IntegrationActions::List => {
                    commands::integrations::list(&cfg).await?;
                }
                IntegrationActions::Jira { action } => match action {
                    JiraActions::Accounts { action } => match action {
                        JiraAccountActions::List => {
                            commands::integrations::jira_accounts_list(&cfg).await?
                        }
                        JiraAccountActions::Delete { account_id } => {
                            commands::integrations::jira_accounts_delete(&cfg, &account_id).await?;
                        }
                    },
                    JiraActions::Templates { action } => match action {
                        JiraTemplateActions::List => {
                            commands::integrations::jira_templates_list(&cfg).await?
                        }
                        JiraTemplateActions::Get { template_id } => {
                            commands::integrations::jira_templates_get(&cfg, &template_id).await?;
                        }
                        JiraTemplateActions::Create { file } => {
                            commands::integrations::jira_templates_create(&cfg, &file).await?;
                        }
                        JiraTemplateActions::Update { template_id, file } => {
                            commands::integrations::jira_templates_update(
                                &cfg,
                                &template_id,
                                &file,
                            )
                            .await?;
                        }
                        JiraTemplateActions::Delete { template_id } => {
                            commands::integrations::jira_templates_delete(&cfg, &template_id)
                                .await?;
                        }
                    },
                },
                IntegrationActions::Servicenow { action } => match action {
                    ServiceNowActions::Instances { action } => match action {
                        ServiceNowInstanceActions::List => {
                            commands::integrations::servicenow_instances_list(&cfg).await?;
                        }
                    },
                    ServiceNowActions::Templates { action } => match action {
                        ServiceNowTemplateActions::List => {
                            commands::integrations::servicenow_templates_list(&cfg).await?;
                        }
                        ServiceNowTemplateActions::Get { template_id } => {
                            commands::integrations::servicenow_templates_get(&cfg, &template_id)
                                .await?;
                        }
                        ServiceNowTemplateActions::Create { file } => {
                            commands::integrations::servicenow_templates_create(&cfg, &file)
                                .await?;
                        }
                        ServiceNowTemplateActions::Update { template_id, file } => {
                            commands::integrations::servicenow_templates_update(
                                &cfg,
                                &template_id,
                                &file,
                            )
                            .await?;
                        }
                        ServiceNowTemplateActions::Delete { template_id } => {
                            commands::integrations::servicenow_templates_delete(&cfg, &template_id)
                                .await?;
                        }
                    },
                    ServiceNowActions::Users { action } => match action {
                        ServiceNowUserActions::List { instance_name } => {
                            commands::integrations::servicenow_users_list(&cfg, &instance_name)
                                .await?;
                        }
                    },
                    ServiceNowActions::AssignmentGroups { action } => match action {
                        ServiceNowAssignmentGroupActions::List { instance_name } => {
                            commands::integrations::servicenow_assignment_groups_list(
                                &cfg,
                                &instance_name,
                            )
                            .await?;
                        }
                    },
                    ServiceNowActions::BusinessServices { action } => match action {
                        ServiceNowBusinessServiceActions::List { instance_name } => {
                            commands::integrations::servicenow_business_services_list(
                                &cfg,
                                &instance_name,
                            )
                            .await?;
                        }
                    },
                },
                IntegrationActions::Slack { action } => match action {
                    SlackActions::List => commands::integrations::slack_list(&cfg).await?,
                },
                IntegrationActions::Pagerduty { action } => match action {
                    PagerdutyActions::List => {
                        commands::integrations::pagerduty_list(&cfg).await?;
                    }
                },
                IntegrationActions::Webhooks { action } => match action {
                    WebhooksActions::List => commands::integrations::webhooks_list(&cfg).await?,
                },
                IntegrationActions::GoogleChat { action } => match action {
                    GoogleChatActions::Handles { action } => match action {
                        GoogleChatHandleActions::List { org_id } => {
                            commands::google_chat::handles_list(&cfg, &org_id).await?;
                        }
                        GoogleChatHandleActions::Get { org_id, handle_id } => {
                            commands::google_chat::handles_get(&cfg, &org_id, &handle_id).await?;
                        }
                        GoogleChatHandleActions::Create { org_id, file } => {
                            commands::google_chat::handles_create(&cfg, &org_id, &file).await?;
                        }
                        GoogleChatHandleActions::Update {
                            org_id,
                            handle_id,
                            file,
                        } => {
                            commands::google_chat::handles_update(&cfg, &org_id, &handle_id, &file)
                                .await?;
                        }
                        GoogleChatHandleActions::Delete { org_id, handle_id } => {
                            commands::google_chat::handles_delete(&cfg, &org_id, &handle_id)
                                .await?;
                        }
                    },
                    GoogleChatActions::SpaceGet {
                        domain_name,
                        space_display_name,
                    } => {
                        commands::google_chat::space_get(&cfg, &domain_name, &space_display_name)
                            .await?;
                    }
                },
                IntegrationActions::Aws { action } => match action {
                    IntegrationAwsActions::CloudAuth { action } => match action {
                        CloudAuthActions::PersonaMappings { action } => match action {
                            CloudAuthPersonaMappingActions::List => {
                                commands::cloud_auth::persona_mappings_list(&cfg).await?;
                            }
                            CloudAuthPersonaMappingActions::Get { mapping_id } => {
                                commands::cloud_auth::persona_mappings_get(&cfg, &mapping_id)
                                    .await?;
                            }
                            CloudAuthPersonaMappingActions::Create { file } => {
                                commands::cloud_auth::persona_mappings_create(&cfg, &file).await?;
                            }
                            CloudAuthPersonaMappingActions::Delete { mapping_id } => {
                                commands::cloud_auth::persona_mappings_delete(&cfg, &mapping_id)
                                    .await?;
                            }
                        },
                    },
                },
            }
        }
        // --- Containers ---
        Commands::Containers { action } => {
            cfg.validate_auth()?;
            match action {
                ContainerActions::List {
                    filter_tags,
                    group_by,
                    sort,
                    page_size,
                } => {
                    commands::containers::list(&cfg, filter_tags, group_by, sort, page_size)
                        .await?;
                }
                ContainerActions::Images { action } => match action {
                    ContainerImageActions::List {
                        filter_tags,
                        group_by,
                        sort,
                        page_size,
                    } => {
                        commands::containers::images_list(
                            &cfg,
                            filter_tags,
                            group_by,
                            sort,
                            page_size,
                        )
                        .await?;
                    }
                },
            }
        }
        // --- Cost ---
        Commands::Cost { action } => {
            cfg.validate_auth()?;
            match action {
                CostActions::Projected => commands::cost::projected(&cfg).await?,
                CostActions::ByOrg {
                    start_month,
                    end_month,
                    ..
                } => {
                    commands::cost::by_org(&cfg, start_month, end_month).await?;
                }
                CostActions::Attribution { start, fields, .. } => {
                    commands::cost::attribution(&cfg, start, fields).await?;
                }
                CostActions::AwsConfig { action } => match action {
                    CostCloudConfigActions::List => commands::cost::aws_config_list(&cfg).await?,
                    CostCloudConfigActions::Get { id } => {
                        commands::cost::aws_config_get(&cfg, id).await?;
                    }
                    CostCloudConfigActions::Create { file } => {
                        commands::cost::aws_config_create(&cfg, &file).await?;
                    }
                    CostCloudConfigActions::Delete { id } => {
                        commands::cost::aws_config_delete(&cfg, id).await?;
                    }
                },
                CostActions::AzureConfig { action } => match action {
                    CostCloudConfigActions::List => {
                        commands::cost::azure_config_list(&cfg).await?;
                    }
                    CostCloudConfigActions::Get { id } => {
                        commands::cost::azure_config_get(&cfg, id).await?;
                    }
                    CostCloudConfigActions::Create { file } => {
                        commands::cost::azure_config_create(&cfg, &file).await?;
                    }
                    CostCloudConfigActions::Delete { id } => {
                        commands::cost::azure_config_delete(&cfg, id).await?;
                    }
                },
                CostActions::GcpConfig { action } => match action {
                    CostCloudConfigActions::List => commands::cost::gcp_config_list(&cfg).await?,
                    CostCloudConfigActions::Get { id } => {
                        commands::cost::gcp_config_get(&cfg, id).await?;
                    }
                    CostCloudConfigActions::Create { file } => {
                        commands::cost::gcp_config_create(&cfg, &file).await?;
                    }
                    CostCloudConfigActions::Delete { id } => {
                        commands::cost::gcp_config_delete(&cfg, id).await?;
                    }
                },
            }
        }
        // --- Misc ---
        Commands::Misc { action } => {
            // No validate_auth() — ip-ranges is public, status IS the auth check
            match action {
                MiscActions::IpRanges => commands::misc::ip_ranges(&cfg).await?,
                MiscActions::Status => commands::misc::status(&cfg).await?,
            }
        }
        // --- APM ---
        Commands::Apm { action } => {
            cfg.validate_auth()?;
            match action {
                ApmActions::Services { action } => match action {
                    ApmServiceActions::List { env, from, to, .. } => {
                        commands::apm::services_list(&cfg, env, from, to).await?;
                    }
                    ApmServiceActions::Stats { env, from, to, .. } => {
                        commands::apm::services_stats(&cfg, env, from, to).await?;
                    }
                    ApmServiceActions::Operations {
                        service,
                        env,
                        from,
                        to,
                        ..
                    } => {
                        commands::apm::services_operations(&cfg, service, env, from, to).await?;
                    }
                    ApmServiceActions::Resources {
                        service,
                        operation,
                        env,
                        from,
                        to,
                        ..
                    } => {
                        commands::apm::services_resources(&cfg, service, operation, env, from, to)
                            .await?;
                    }
                },
                ApmActions::Entities { action } => match action {
                    ApmEntityActions::List { from, to, .. } => {
                        commands::apm::entities_list(&cfg, from, to).await?;
                    }
                },
                ApmActions::Dependencies { action } => match action {
                    ApmDependencyActions::List { env, from, to, .. } => {
                        commands::apm::dependencies_list(&cfg, env, from, to).await?;
                    }
                },
                ApmActions::FlowMap {
                    query,
                    limit,
                    from,
                    to,
                    ..
                } => {
                    commands::apm::flow_map(&cfg, query, limit, from, to).await?;
                }
                ApmActions::Troubleshooting { action } => match action {
                    ApmTroubleshootingActions::List {
                        hostname,
                        timeframe,
                    } => {
                        commands::apm::troubleshooting_list(&cfg, hostname, timeframe).await?;
                    }
                },
            }
        }
        // --- DDSQL ---
        Commands::Ddsql { action } => {
            cfg.validate_auth()?;
            match action {
                DdsqlActions::Table {
                    query,
                    from,
                    to,
                    interval,
                    limit,
                    offset,
                } => {
                    commands::ddsql::table(&cfg, &query, &from, &to, interval, Some(limit), offset)
                        .await?;
                }
                DdsqlActions::TimeSeries {
                    query,
                    from,
                    to,
                    interval,
                    limit,
                } => {
                    commands::ddsql::time_series(&cfg, &query, &from, &to, interval, limit).await?;
                }
            }
        }
        // --- Investigations ---
        Commands::Investigations { action } => {
            cfg.validate_auth()?;
            match action {
                InvestigationActions::List {
                    page_limit,
                    page_offset,
                    ..
                } => {
                    commands::investigations::list(&cfg, page_limit, page_offset).await?;
                }
                InvestigationActions::Get { investigation_id } => {
                    commands::investigations::get(&cfg, &investigation_id).await?;
                }
                InvestigationActions::Trigger { file, .. } => {
                    if let Some(f) = file {
                        commands::investigations::trigger(&cfg, &f).await?;
                    } else {
                        anyhow::bail!("flag-based trigger not yet implemented; use --file");
                    }
                }
            }
        }
        // --- Network (placeholder) ---
        Commands::Network { action } => match action {
            NetworkActions::List => {
                anyhow::bail!("network commands are not yet implemented (API endpoints pending)")
            }
            NetworkActions::Flows { action } => match action {
                NetworkFlowActions::List => {
                    cfg.validate_auth()?;
                    commands::network::flows_list(&cfg).await?;
                }
            },
            NetworkActions::Devices { action } => {
                cfg.validate_auth()?;
                match action {
                    NetworkDeviceActions::List => {
                        commands::network::devices_list(&cfg).await?;
                    }
                    NetworkDeviceActions::Get { device_id } => {
                        commands::network::devices_get(&cfg, &device_id).await?;
                    }
                    NetworkDeviceActions::Interfaces {
                        device_id,
                        ip_addresses,
                    } => {
                        commands::network::devices_interfaces(&cfg, &device_id, ip_addresses)
                            .await?;
                    }
                    NetworkDeviceActions::Tags { action } => match action {
                        NetworkDeviceTagActions::List { device_id } => {
                            commands::network::devices_tags_list(&cfg, &device_id).await?;
                        }
                        NetworkDeviceTagActions::Update { device_id, file } => {
                            commands::network::devices_tags_update(&cfg, &device_id, &file).await?;
                        }
                    },
                }
            }
            NetworkActions::Interfaces { action } => {
                cfg.validate_auth()?;
                match action {
                    NetworkInterfaceTagActions::List { interface_id } => {
                        commands::network::interfaces_tags_list(&cfg, &interface_id).await?;
                    }
                    NetworkInterfaceTagActions::Update { interface_id, file } => {
                        commands::network::interfaces_tags_update(&cfg, &interface_id, &file)
                            .await?;
                    }
                }
            }
        },
        // --- Obs Pipelines ---
        Commands::ObsPipelines { action } => {
            cfg.validate_auth()?;
            match action {
                ObsPipelinesActions::List { limit } => {
                    commands::obs_pipelines::list(&cfg, limit).await?;
                }
                ObsPipelinesActions::Get { pipeline_id } => {
                    commands::obs_pipelines::get(&cfg, &pipeline_id).await?;
                }
                ObsPipelinesActions::Create { file } => {
                    commands::obs_pipelines::create(&cfg, &file).await?;
                }
                ObsPipelinesActions::Update { pipeline_id, file } => {
                    commands::obs_pipelines::update(&cfg, &pipeline_id, &file).await?;
                }
                ObsPipelinesActions::Delete { pipeline_id } => {
                    commands::obs_pipelines::delete(&cfg, &pipeline_id).await?;
                }
                ObsPipelinesActions::Validate { file } => {
                    commands::obs_pipelines::validate(&cfg, &file).await?;
                }
            }
        }
        // --- Scorecards (placeholder) ---
        Commands::Scorecards { action } => match action {
            ScorecardsActions::List => commands::scorecards::list()?,
            ScorecardsActions::Get { scorecard_id } => {
                commands::scorecards::get(&scorecard_id)?;
            }
        },
        // --- Traces ---
        Commands::Traces { action } => {
            cfg.validate_auth()?;
            match action {
                TracesActions::Search {
                    query,
                    from,
                    to,
                    limit,
                    sort,
                } => {
                    commands::traces::search(&cfg, query, from, to, limit, sort).await?;
                }
                TracesActions::Aggregate {
                    query,
                    from,
                    to,
                    compute,
                    group_by,
                } => {
                    commands::traces::aggregate(&cfg, query, from, to, compute, group_by).await?;
                }
            }
        }
        // --- ACP ---
        Commands::Acp { action } => match action {
            AcpActions::Serve {
                port,
                host,
                agent_id,
            } => {
                commands::acp::serve(&cfg, port, &host, agent_id).await?;
            }
        },
        // --- Agent ---
        Commands::Agent { action } => match action {
            AgentActions::Schema { compact } => {
                let cmd = Cli::command();
                let schema = if compact {
                    build_compact_agent_schema(&cmd)
                } else {
                    build_agent_schema(&cmd)
                };
                println!("{}", serde_json::to_string_pretty(&schema).unwrap());
            }
            AgentActions::Guide => commands::agent::guide()?,
        },
        // --- Alias ---
        Commands::Alias { action } => match action {
            AliasActions::List => commands::alias::list(&cfg)?,
            AliasActions::Set { name, command } => commands::alias::set(name, command)?,
            AliasActions::Delete { names } => commands::alias::delete(names)?,
            AliasActions::Import { file } => commands::alias::import(&file)?,
        },
        // --- Skills ---
        Commands::Skills { action } => match action {
            SkillsActions::List { entry_type } => commands::skills::list(&cfg, entry_type)?,
            SkillsActions::Install {
                name,
                target_agent,
                dir,
                entry_type,
            } => commands::skills::install(&cfg, name, target_agent, dir, entry_type)?,
            SkillsActions::Path { target_agent } => commands::skills::path(target_agent)?,
        },
        // --- Product Analytics ---
        Commands::ProductAnalytics { action } => {
            cfg.validate_auth()?;
            match action {
                ProductAnalyticsActions::Events { action } => match action {
                    ProductAnalyticsEventActions::Send { file, .. } => {
                        let f = file.unwrap_or_default();
                        commands::product_analytics::events_send(&cfg, &f).await?;
                    }
                },
                ProductAnalyticsActions::Query { action } => match action {
                    ProductAnalyticsQueryActions::Scalar { file } => {
                        commands::product_analytics::query_scalar(&cfg, &file).await?;
                    }
                    ProductAnalyticsQueryActions::Timeseries { file } => {
                        commands::product_analytics::query_timeseries(&cfg, &file).await?;
                    }
                },
            }
        }
        // --- Static Analysis ---
        Commands::StaticAnalysis { action } => {
            cfg.validate_auth()?;
            match action {
                StaticAnalysisActions::Ast { action } => match action {
                    StaticAnalysisAstActions::List { .. } => {
                        commands::static_analysis::ast_list(&cfg).await?;
                    }
                    StaticAnalysisAstActions::Get { id } => {
                        commands::static_analysis::ast_get(&cfg, &id).await?;
                    }
                },
                StaticAnalysisActions::CustomRulesets { action } => match action {
                    StaticAnalysisCustomRulesetActions::List { .. } => {
                        commands::static_analysis::custom_rulesets_list(&cfg).await?;
                    }
                    StaticAnalysisCustomRulesetActions::Get { id } => {
                        commands::static_analysis::custom_rulesets_get(&cfg, &id).await?;
                    }
                },
                StaticAnalysisActions::Sca { action } => match action {
                    StaticAnalysisScaActions::List { .. } => {
                        commands::static_analysis::sca_list(&cfg).await?;
                    }
                    StaticAnalysisScaActions::Get { id } => {
                        commands::static_analysis::sca_get(&cfg, &id).await?;
                    }
                },
                StaticAnalysisActions::Coverage { action } => match action {
                    StaticAnalysisCoverageActions::List { .. } => {
                        commands::static_analysis::coverage_list(&cfg).await?;
                    }
                    StaticAnalysisCoverageActions::Get { id } => {
                        commands::static_analysis::coverage_get(&cfg, &id).await?;
                    }
                },
            }
        }
        // --- Runbooks ---
        #[cfg(not(target_arch = "wasm32"))]
        Commands::Runbooks { action } => match action {
            RunbookActions::List { tag } => {
                commands::runbooks::list(&cfg, tag)?;
            }
            RunbookActions::Describe { name } => {
                commands::runbooks::describe(&cfg, &name)?;
            }
            RunbookActions::Run { name, arg } => {
                commands::runbooks::run(&cfg, &name, arg, cfg.auto_approve).await?;
            }
            RunbookActions::Validate { name } => {
                commands::runbooks::validate(&cfg, &name)?;
            }
            RunbookActions::Import { source } => {
                commands::runbooks::import(&cfg, &source).await?;
            }
        },
        // --- Auth ---
        Commands::Auth { action } => match action {
            AuthActions::Login {
                scopes,
                read_only,
                site,
                subdomain,
            } => {
                if let Some(s) = site {
                    cfg.site = s;
                }
                let is_read_only = read_only || cfg.read_only;
                let resolved =
                    resolve_login_scopes(scopes.as_deref(), cfg.org.as_deref(), is_read_only);
                commands::auth::login(&cfg, resolved, subdomain.as_deref()).await?
            }
            AuthActions::Logout => commands::auth::logout(&cfg).await?,
            AuthActions::Status { site } => {
                if let Some(s) = site {
                    cfg.site = s;
                }
                commands::auth::status(&cfg)?
            }
            #[cfg(debug_assertions)]
            AuthActions::Token => commands::auth::token(&cfg)?,
            AuthActions::Refresh => commands::auth::refresh(&cfg).await?,
            AuthActions::List => commands::auth::list(&cfg)?,
            AuthActions::Test => commands::test::run(&cfg)?,
        },
        // --- Workflows ---
        Commands::Workflows { action } => {
            cfg.validate_api_and_app_keys().map_err(|_| {
                anyhow::anyhow!(
                    "workflow commands require DD_API_KEY and DD_APP_KEY with workflow_* scopes\n\
                     OAuth2 bearer tokens are not supported for workflow operations.\n\
                     See: https://docs.datadoghq.com/api/latest/workflow-automation"
                )
            })?;
            match action {
                WorkflowActions::Get { workflow_id } => {
                    commands::workflows::get(&cfg, &workflow_id).await?;
                }
                WorkflowActions::Create { file } => {
                    commands::workflows::create(&cfg, &file).await?;
                }
                WorkflowActions::Update { workflow_id, file } => {
                    commands::workflows::update(&cfg, &workflow_id, &file).await?;
                }
                WorkflowActions::Delete { workflow_id } => {
                    commands::workflows::delete(&cfg, &workflow_id).await?;
                }
                WorkflowActions::Run {
                    workflow_id,
                    payload,
                    payload_file,
                    wait,
                    timeout,
                } => {
                    commands::workflows::run(
                        &cfg,
                        &workflow_id,
                        payload,
                        payload_file,
                        wait,
                        &timeout,
                    )
                    .await?;
                }
                WorkflowActions::Instances { action } => match action {
                    WorkflowInstanceActions::List {
                        workflow_id,
                        limit,
                        page,
                    } => {
                        commands::workflows::instance_list(&cfg, &workflow_id, limit, page).await?;
                    }
                    WorkflowInstanceActions::Get {
                        workflow_id,
                        instance_id,
                    } => {
                        commands::workflows::instance_get(&cfg, &workflow_id, &instance_id).await?;
                    }
                    WorkflowInstanceActions::Cancel {
                        workflow_id,
                        instance_id,
                    } => {
                        commands::workflows::instance_cancel(&cfg, &workflow_id, &instance_id)
                            .await?;
                    }
                },
            }
        }
        // --- LLM Observability ---
        Commands::LlmObs { action } => {
            cfg.validate_auth()?;
            match action {
                LlmObsActions::Projects { action } => match action {
                    LlmObsProjectsActions::Create { file } => {
                        commands::llm_obs::projects_create(&cfg, &file).await?;
                    }
                    LlmObsProjectsActions::List => {
                        commands::llm_obs::projects_list(&cfg).await?;
                    }
                },
                LlmObsActions::Experiments { action } => match action {
                    LlmObsExperimentsActions::Create { file } => {
                        commands::llm_obs::experiments_create(&cfg, &file).await?;
                    }
                    LlmObsExperimentsActions::List {
                        filter_project_id,
                        filter_dataset_id,
                    } => {
                        commands::llm_obs::experiments_list(
                            &cfg,
                            filter_project_id,
                            filter_dataset_id,
                        )
                        .await?;
                    }
                    LlmObsExperimentsActions::Update {
                        experiment_id,
                        file,
                    } => {
                        commands::llm_obs::experiments_update(&cfg, &experiment_id, &file).await?;
                    }
                    LlmObsExperimentsActions::Delete { file } => {
                        commands::llm_obs::experiments_delete(&cfg, &file).await?;
                    }
                    LlmObsExperimentsActions::Summary { experiment_id } => {
                        commands::llm_obs::experiments_summary(&cfg, &experiment_id).await?;
                    }
                    LlmObsExperimentsActions::Events { action } => match action {
                        LlmObsExperimentsEventsActions::List {
                            experiment_id,
                            limit,
                            offset,
                            filter_dimension_key,
                            filter_dimension_value,
                            filter_metric_label,
                            sort_by_metric,
                            sort_direction,
                        } => {
                            commands::llm_obs::experiments_events_list(
                                &cfg,
                                &experiment_id,
                                limit,
                                offset,
                                filter_dimension_key,
                                filter_dimension_value,
                                filter_metric_label,
                                sort_by_metric,
                                &sort_direction,
                            )
                            .await?;
                        }
                        LlmObsExperimentsEventsActions::Get {
                            experiment_id,
                            event_id,
                        } => {
                            commands::llm_obs::experiments_events_get(
                                &cfg,
                                &experiment_id,
                                &event_id,
                            )
                            .await?;
                        }
                    },
                    LlmObsExperimentsActions::MetricValues {
                        experiment_id,
                        metric_label,
                        segment_by_dimension,
                        segment_dimension_value,
                    } => {
                        commands::llm_obs::experiments_metric_values(
                            &cfg,
                            &experiment_id,
                            &metric_label,
                            segment_by_dimension,
                            segment_dimension_value,
                        )
                        .await?;
                    }
                    LlmObsExperimentsActions::DimensionValues {
                        experiment_id,
                        dimension_key,
                    } => {
                        commands::llm_obs::experiments_dimension_values(
                            &cfg,
                            &experiment_id,
                            &dimension_key,
                        )
                        .await?;
                    }
                },
                LlmObsActions::Datasets { action } => match action {
                    LlmObsDatasetsActions::Create { project_id, file } => {
                        commands::llm_obs::datasets_create(&cfg, &project_id, &file).await?;
                    }
                    LlmObsDatasetsActions::List { project_id } => {
                        commands::llm_obs::datasets_list(&cfg, &project_id).await?;
                    }
                },
                LlmObsActions::Spans { action } => match action {
                    LlmObsSpansActions::Search {
                        query,
                        trace_id,
                        span_id,
                        span_kind,
                        span_name,
                        ml_app,
                        root_spans_only,
                        from,
                        to,
                        limit,
                        cursor,
                    } => {
                        commands::llm_obs::spans_search(
                            &cfg,
                            query,
                            trace_id,
                            span_id,
                            span_kind,
                            span_name,
                            ml_app,
                            root_spans_only,
                            from,
                            to,
                            limit,
                            cursor,
                        )
                        .await?;
                    }
                },
            }
        }
        // --- Reference Tables ---
        Commands::ReferenceTables { action } => {
            cfg.validate_auth()?;
            match action {
                ReferenceTablesActions::List { limit } => {
                    commands::reference_tables::list(&cfg, limit).await?;
                }
                ReferenceTablesActions::Get { table_id } => {
                    commands::reference_tables::get(&cfg, &table_id).await?;
                }
                ReferenceTablesActions::Create { file } => {
                    commands::reference_tables::create(&cfg, &file).await?;
                }
                ReferenceTablesActions::BatchQuery { file } => {
                    commands::reference_tables::batch_query(&cfg, &file).await?;
                }
            }
        }
        // --- Extensions ---
        #[cfg(not(target_arch = "wasm32"))]
        Commands::Extension { action } => match action {
            ExtensionActions::List => commands::extension::list(&cfg)?,
            ExtensionActions::Install {
                source,
                tag,
                local,
                link,
                name,
                force,
                description,
            } => {
                commands::extension::install(
                    &cfg,
                    commands::extension::InstallOptions {
                        source,
                        tag,
                        local,
                        link,
                        name,
                        force,
                        description,
                    },
                )?;
            }
            ExtensionActions::Remove { name } => commands::extension::remove(&cfg, name)?,
            ExtensionActions::Upgrade { name, all } => {
                commands::extension::upgrade(&cfg, name, all)?;
            }
        },
        // --- Utility ---
        #[cfg(not(target_arch = "wasm32"))]
        Commands::Completions { shell, install } => {
            if install {
                commands::completions::install(shell)?;
            } else {
                commands::completions::generate(shell);
            }
        }
        Commands::Version => println!("{}", version::build_info()),
    }

    Ok(())
}

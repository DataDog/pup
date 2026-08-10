// api.rs provides raw HTTP helpers used by test_commands.rs; not needed in production binary.
#[cfg(test)]
mod api;
mod auth;
mod client;
mod commands;
mod config;
#[cfg(not(target_arch = "wasm32"))]
mod extensions;
mod filter;
mod formatter;
mod generated;
mod raw_client;
#[cfg(not(target_arch = "wasm32"))]
mod runbooks;
#[cfg(not(target_arch = "wasm32"))]
mod skills;
#[cfg(not(target_arch = "wasm32"))]
mod tunnel;
mod useragent;
mod util;
mod util_ext;
mod version;

#[cfg(test)]
mod test_commands;

#[cfg(test)]
mod test_support;

/// Shared test utilities — only compiled in test builds.
#[cfg(test)]
pub(crate) mod test_utils {
    use tokio::sync::Mutex;
    /// Serialize tests that mutate global env vars (PUP_MOCK_SERVER, DD_API_KEY, etc.).
    /// `tokio::sync::Mutex` so async tests can hold the guard across `.await`,
    /// while sync tests acquire via `blocking_lock()`. Sharing one lock across
    /// both flavors prevents the parallel-test races that previously required
    /// running the suite with `--test-threads=1`.
    pub static ENV_LOCK: Mutex<()> = Mutex::const_new(());
}

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
#[cfg(not(target_arch = "wasm32"))]
use std::io::IsTerminal;

#[derive(Parser)]
#[command(name = "pup", version = version::VERSION, about = "Datadog API CLI")]
pub(crate) struct Cli {
    /// Output format (json, table, yaml, csv). Defaults to json, or $DD_OUTPUT / $PUP_OUTPUT when set.
    #[arg(short, long, global = true)]
    output: Option<String>,
    /// Auto-approve destructive operations
    #[arg(short = 'y', long = "yes", global = true)]
    yes: bool,
    /// Enable agent mode
    #[arg(long, global = true)]
    agent: bool,
    /// Disable agent mode (overrides auto-detection and --agent)
    #[arg(long, global = true)]
    no_agent: bool,
    /// Block all write operations (create, update, delete)
    #[arg(long, global = true)]
    read_only: bool,
    /// Named org session (see 'pup auth login --org')
    #[arg(long, global = true)]
    org: Option<String>,
    /// Filter command output through a jq expression (applied before formatting)
    #[arg(long, global = true)]
    jq: Option<String>,
    /// Trust a non-Datadog `--site`/`DD_SITE` host for this invocation (skip the
    /// trust prompt). For durable trust, use `trusted_sites` in config instead.
    #[arg(long, global = true)]
    trust_site: bool,
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
    /// Manage Agentless Scanning configurations
    ///
    /// Manage cloud provider agentless scanning scan options and on-demand tasks.
    ///
    /// COMMANDS:
    ///   aws list              List AWS scan options
    ///   aws get               Get AWS scan options for an account
    ///   aws create            Activate AWS scan options from JSON
    ///   aws update            Update AWS scan options for an account
    ///   aws delete            Delete AWS scan options for an account
    ///   aws on-demand list    List AWS on-demand tasks
    ///   aws on-demand get     Get an AWS on-demand task
    ///   aws on-demand create  Create an AWS on-demand task from JSON
    ///   azure list            List Azure scan options
    ///   azure get             Get Azure scan options for a subscription
    ///   azure create          Activate Azure scan options from JSON
    ///   azure update          Update Azure scan options for a subscription
    ///   azure delete          Delete Azure scan options for a subscription
    ///   gcp list              List GCP scan options
    ///   gcp get               Get GCP scan options for a project
    ///   gcp create            Activate GCP scan options from JSON
    ///   gcp update            Update GCP scan options for a project
    ///   gcp delete            Delete GCP scan options for a project
    ///
    /// EXAMPLES:
    ///   pup agentless-scanning aws list
    ///   pup agentless-scanning aws on-demand create --file task.json
    ///   pup agentless-scanning azure list
    ///   pup agentless-scanning gcp list
    #[command(verbatim_doc_comment)]
    AgentlessScanning {
        #[command(subcommand)]
        action: AgentlessScanningActions,
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
    /// Make authenticated requests directly to the Datadog API
    ///
    /// Sends raw HTTP requests to Datadog API endpoints using the current
    /// authentication credentials (OAuth2 token or API/APP keys). Useful
    /// for endpoints not yet covered by a dedicated pup subcommand.
    ///
    /// The endpoint may be given as a relative path (v2/monitors) or an
    /// absolute path (/api/v2/monitors). Relative paths are automatically
    /// prefixed with /api/.
    ///
    /// EXAMPLES:
    ///   # GET a list of monitors
    ///   pup api v2/monitors
    ///
    ///   # POST with a JSON body from stdin
    ///   pup api -X POST v2/cases/123/comment --input - << 'EOF'
    ///   {"data":{"attributes":{"comment":"Hello"},"type":"case"}}
    ///   EOF
    ///
    ///   # POST with key=value fields (no file needed)
    ///   pup api -X POST v2/tags/hosts/myhost -F host=myhost -f source=web
    ///
    ///   # GET with query parameters
    ///   pup api v2/monitors -F tags=env:prod -F page=0
    ///
    ///   # Show response status and headers
    ///   pup api -i v2/monitors
    ///
    ///   # Verbose: show request and response details
    ///   pup api --verbose v2/monitors
    #[command(verbatim_doc_comment)]
    Api {
        /// API endpoint path (e.g. v2/monitors or /api/v2/monitors)
        endpoint: String,
        /// HTTP method (default: GET)
        #[arg(short = 'X', long, default_value = "GET", value_name = "METHOD")]
        method: String,
        /// Add a typed field: int, bool, null, or string (repeatable).
        /// For POST/PUT/PATCH: JSON body field. For GET/DELETE: query parameter.
        #[arg(short = 'F', long, value_name = "KEY=VALUE")]
        field: Vec<String>,
        /// Add a custom HTTP request header (repeatable)
        #[arg(short = 'H', long, value_name = "KEY:VALUE")]
        header: Vec<String>,
        /// Include HTTP response status line and headers in output
        #[arg(short = 'i', long)]
        include: bool,
        /// Read request body from file, or use "-" for stdin
        #[arg(long, value_name = "FILE")]
        input: Option<String>,
        /// Add a raw string field, no type coercion (repeatable).
        /// For POST/PUT/PATCH: JSON body field. For GET/DELETE: query parameter.
        #[arg(short = 'f', long = "raw-field", value_name = "KEY=VALUE")]
        raw_field: Vec<String>,
        /// Do not print the response body
        #[arg(long)]
        silent: bool,
        /// Show full request URL and response headers
        #[arg(long)]
        verbose: bool,
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
    ///   pup app-builder create --file app.json
    ///
    ///   # Update an app
    ///   pup app-builder update <app-id> --file updated.json
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
    ///   pup audit list
    ///
    ///   # Search for specific user actions
    ///   pup audit search --query="@usr.email:admin@example.com"
    ///
    ///   # Who deleted resources in the last 24h
    ///   pup audit search --query="@action:deleted" --from=24h
    ///
    ///   # Investigate a specific API key
    ///   pup audit search --query="@metadata.api_key.id:KEY_ID" --from=90d
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys with audit_logs_read scope.
    #[command(name = "audit-logs", visible_alias = "audit", verbatim_doc_comment)]
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
    /// Manage AuthN mappings for federated identity providers
    ///
    /// Manage AuthN Mappings to automatically map groups of users to roles
    /// in Datadog using attributes sent from Identity Providers.
    ///
    /// COMMANDS:
    ///   list    List all AuthN mappings
    ///   get     Get an AuthN mapping
    ///   create  Create an AuthN mapping from JSON
    ///   update  Update an AuthN mapping
    ///   delete  Delete an AuthN mapping
    ///
    /// EXAMPLES:
    ///   pup authn-mappings list
    ///   pup authn-mappings get <mapping-id>
    ///   pup authn-mappings create --file mapping.json
    ///   pup authn-mappings update <mapping-id> --file mapping.json
    ///   pup authn-mappings delete <mapping-id>
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    ///   list/get work with default OAuth scopes. create/update/delete
    ///   require the user_access_manage scope, which is not requested by
    ///   default -- opt in with:
    ///     pup auth login --extra-scopes user_access_manage
    #[command(name = "authn-mappings", verbatim_doc_comment)]
    AuthnMappings {
        #[command(subcommand)]
        action: AuthnMappingsActions,
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
    /// Retrieve change events for an APM service over a time range.
    /// Use to correlate changes with anomalies, performance issues or incidents.
    ///
    /// Tracked change types: deployment (code/version), feature_flag, traffic_anomaly,
    /// watchdog (Datadog anomaly detection), kubernetes (k8s deployment manifest updates),
    /// scale (manual k8s scale events), crashloopbackoff (k8s crashloops),
    /// database (DB schema or setting changes), schema (data stream schemas only; not DB/API schema),
    /// configuration (limited to specific tracked sources — absence does not imply no config changed).
    ///
    /// COMMANDS:
    ///   list   List change stories for a service over a time window
    ///
    /// EXAMPLES:
    ///   pup change-stories list --service api --from 2024-01-15T00:00:00Z --to 2024-01-15T01:00:00Z
    ///   pup change-stories list --service api --env prod --story-types deployment --from 1h --to now
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys.
    #[command(name = "change-stories", verbatim_doc_comment)]
    ChangeStories {
        #[command(subcommand)]
        action: ChangeStoriesActions,
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
    /// Query Datadog billing and Cloud Cost Management (CCM) information.
    ///
    /// Use 'pup costs datadog' for projected costs, attribution, and cloud cost configs.
    /// Use 'pup costs ccm' for custom costs, budgets, commitments, and tag management.
    ///
    /// CAPABILITIES:
    ///   • View projected end-of-month costs and attribution (datadog)
    ///   • Manage AWS/Azure/GCP cloud cost configs (datadog)
    ///   • Upload and manage custom cost files (ccm)
    ///   • Create and track budgets (ccm)
    ///   • Analyze commitment programs (RIs, Savings Plans) (ccm)
    ///   • Explore cost tags, tag keys, and tag metadata (ccm)
    ///
    /// EXAMPLES:
    ///   # Get projected costs for current month
    ///   pup costs datadog projected
    ///
    ///   # List budgets
    ///   pup costs ccm budgets list
    ///
    ///   # View commitment utilization for AWS EC2
    ///   pup costs ccm commitments utilization --provider aws --product EC2 --from 30d --to now
    ///
    /// AUTHENTICATION:
    ///   Requires OAuth2 (via 'pup auth login') or valid API + Application keys.
    ///   Cost management features require billing:read permissions.
    #[command(name = "costs", alias = "cost", verbatim_doc_comment)]
    Cost {
        #[command(subcommand)]
        action: CostActions,
    },
    /// Manage CSM Threats agent policies and rules
    ///
    /// Manage Cloud Security Management (Workload Protection) agent policies and rules.
    ///
    /// COMMANDS:
    ///   agent-policies list       List agent policies
    ///   agent-policies get        Get an agent policy
    ///   agent-policies create     Create an agent policy from JSON
    ///   agent-policies update     Update an agent policy
    ///   agent-policies delete     Delete an agent policy
    ///   agent-rules list          List agent rules
    ///   agent-rules get           Get an agent rule
    ///   agent-rules create        Create an agent rule from JSON
    ///   agent-rules update        Update an agent rule
    ///   agent-rules delete        Delete an agent rule
    ///   backend-rules list        List workload security backend rules
    ///   backend-rules get         Get a backend rule
    ///   backend-rules create      Create a backend rule from JSON
    ///   backend-rules update      Update a backend rule
    ///   backend-rules delete      Delete a backend rule
    ///   backend-rules validate    Validate a backend rule from JSON
    ///   policy download           Download the CSM threats policy file
    ///
    /// EXAMPLES:
    ///   pup csm-threats agent-policies list
    ///   pup csm-threats agent-rules create --file rule.json
    ///   pup csm-threats backend-rules list
    ///   pup csm-threats backend-rules create --file rule.json
    ///   pup csm-threats policy download
    #[command(verbatim_doc_comment)]
    CsmThreats {
        #[command(subcommand)]
        action: CsmThreatsActions,
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
    /// Manage data deletion requests
    ///
    /// Create, list, and cancel data deletion requests for Logs and RUM.
    ///
    /// COMMANDS:
    ///   requests list    List deletion requests
    ///   requests create  Create a deletion request from JSON
    ///   requests cancel  Cancel a deletion request
    ///
    /// EXAMPLES:
    ///   pup data-deletion requests list
    ///   pup data-deletion requests create --product logs --file request.json
    ///   pup data-deletion requests cancel <request-id>
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    ///   All operations require the logs_delete_data and/or rum_delete_data
    ///   scopes, which are not requested by default -- opt in with:
    ///     pup auth login --extra-scopes logs_delete_data,rum_delete_data
    #[command(name = "data-deletion", verbatim_doc_comment)]
    DataDeletion {
        #[command(subcommand)]
        action: DataDeletionActions,
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
    /// Manage datasets
    ///
    /// Create, list, get, update, and delete Restricted Datasets for data access control.
    ///
    /// COMMANDS:
    ///   list              List all datasets
    ///   get <id>          Get a dataset by ID
    ///   create --file     Create a dataset from JSON
    ///   update <id> -f    Update a dataset
    ///   delete <id>       Delete a dataset
    ///
    /// EXAMPLES:
    ///   pup datasets list
    ///   pup datasets get my-dataset-id
    ///   pup datasets create --file dataset.json
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    ///   All operations, including list/get, require the user_access_manage
    ///   scope, which is not requested by default -- opt in with:
    ///     pup auth login --extra-scopes user_access_manage
    #[command(verbatim_doc_comment)]
    Datasets {
        #[command(subcommand)]
        action: DatasetsActions,
    },
    /// Search Database Monitoring query samples
    ///
    /// Query sample search uses Datadog's DBM logs analytics endpoint on `app.<site>`.
    ///
    /// COMMAND GROUPS:
    ///   samples    Search captured database query samples
    ///
    /// EXAMPLES:
    ///   pup dbm samples search --query "dbm_type:activity service:orders env:prod" --from 1h --limit 10
    ///
    /// AUTHENTICATION:
    ///   Accepts OAuth2 (`pup auth login`) or DD_API_KEY + DD_APP_KEY.
    #[command(verbatim_doc_comment)]
    Dbm {
        #[command(subcommand)]
        action: DbmActions,
    },
    /// Query Datadog data using DDSQL (Datadog SQL)
    ///
    /// DDSQL lets you query metrics, logs, and reference tables using SQL syntax.
    ///
    /// COMMANDS:
    ///   table         Execute query and return table data (supports -o json/yaml/table/csv)
    ///   time-series   Execute query and return time series data
    ///   spec          Print DDSQL reference guidance used by the editor tooling
    ///   schema        Discover DDSQL tables and columns
    ///
    /// EXAMPLES:
    ///   pup ddsql table --query "SELECT * FROM reference_tables.offices_ips LIMIT 5"
    ///   pup ddsql table --query "SELECT * FROM reference_tables.offices_ips" -o csv > results.csv
    ///   cat query.sql | pup ddsql table --query - -o table
    ///   pup ddsql time-series --query "SELECT timestamp, value, tags->'host' AS host FROM dd.metrics_timeseries('avg:system.cpu.user{*} by {host}')" --from 1h
    ///   pup ddsql spec
    ///   pup ddsql schema tables --query ec2 --limit 100
    ///   pup ddsql schema columns --table-id public.aws.ec2_instance
    ///
    /// AUTHENTICATION:
    ///   Query commands support OAuth2 (via 'pup auth login') or API key + Application key.
    ///   Discovery commands (`spec`, `schema ...`) currently require DD_API_KEY + DD_APP_KEY.
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
    ///   pup debugger probes create --service my-svc --env staging --probe-location "com.example.MyClass:myMethod(int,String)"
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
    /// Ask the Datadog Docs AI a question
    ///
    /// Sends a natural-language question to the Datadog Docs AI assistant
    /// and streams the response to stdout. No authentication required.
    ///
    /// EXAMPLES:
    ///   pup docs ask "how do I set up log collection?"
    ///   pup docs ask "what is the difference between a monitor and an SLO?"
    #[command(verbatim_doc_comment)]
    Docs {
        #[command(subcommand)]
        action: DocsActions,
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
    /// Post, query, and search Datadog events.
    ///
    /// Events represent important occurrences in your infrastructure such as
    /// deployments, configuration changes, alerts, and custom events.
    ///
    /// CAPABILITIES:
    ///   • Post custom events
    ///   • List recent events
    ///   • Search events with queries
    ///   • Get event details
    ///
    /// EXAMPLES:
    ///   # Post an event using dogshell-compatible flags
    ///   pup events post --tags="version:1,application:web" --no_host --type=my_apps \
    ///     --aggregation_key=application:web --alert_type=info \
    ///     "Something big happened!" "And let me tell you all about it here!"
    ///
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
    /// environment configuration, per-environment enable/disable controls,
    /// traffic allocations, and exposure schedule management.
    ///
    /// COMMAND GROUPS:
    ///   flags           Manage feature flag definitions (list, get, create, update, archive, unarchive)
    ///   environments    Manage feature flag environments (list, get, create, update, delete)
    ///   allocations     Manage traffic allocations for a flag in an environment (create, update)
    ///   exposure        Manage exposure schedules (start, stop, pause, resume)
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
    ///   # Create allocations for a flag in an environment
    ///   pup feature-flags allocations create <flag-id> <env-id> --file=alloc.json
    ///
    ///   # Update (overwrite) allocations for a flag in an environment
    ///   pup feature-flags allocations update <flag-id> <env-id> --file=alloc.json
    ///
    ///   # Start an exposure schedule
    ///   pup feature-flags exposure start <schedule-id>
    ///
    ///   # Stop an exposure schedule
    ///   pup feature-flags exposure stop <schedule-id>
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys
    ///   (DD_API_KEY and DD_APP_KEY environment variables). All 4 scopes
    ///   below are requested by default.
    ///     flags list/get, allocations list  -- feature_flag_config_read +
    ///                                          feature_flag_environment_config_read
    ///     flags create/update/archive/unarchive/delete, enable/disable,
    ///     allocations create/update, exposure schedule actions --
    ///                                          feature_flag_config_write +
    ///                                          feature_flag_environment_config_read
    ///     environments list/get             -- feature_flag_environment_config_read
    ///     environments create/update/delete -- feature_flag_environment_config_write
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
    /// Render JSON through pup's formatter
    ///
    /// Reads a JSON document from stdin (or --input FILE) and prints it using the
    /// configured output format (--output, or $DD_OUTPUT / $PUP_OUTPUT) and agent
    /// mode. Lets an extension in any language reuse pup's table/yaml/csv/tsv
    /// rendering and agent envelope instead of reimplementing them.
    ///
    /// EXAMPLES:
    ///   pup api v2/monitors --silent | pup format --output table
    ///   echo '[{"id":1}]' | pup format --output csv
    #[cfg(not(target_arch = "wasm32"))]
    #[command(visible_alias = "fmt", verbatim_doc_comment)]
    Format {
        /// Read JSON from file, or use "-" (default) for stdin
        #[arg(long, value_name = "FILE")]
        input: Option<String>,
        /// Set metadata.count in the agent-mode envelope
        #[arg(long, value_name = "N")]
        count: Option<usize>,
        /// Set metadata.command in the agent-mode envelope
        #[arg(long, value_name = "STR")]
        command: Option<String>,
        /// Set metadata.next_action in the agent-mode envelope
        #[arg(long, value_name = "STR")]
        next_action: Option<String>,
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
    ///   • Migrate service catalog YAML to v3 schema (migrate-schema)
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
    ///   # Migrate a catalog file to v3
    ///   pup idp migrate-schema service.datadog.yaml
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
    ///   # List active incidents (default)
    ///   pup incidents list
    ///
    ///   # List resolved incidents
    ///   pup incidents list --query="state:resolved"
    ///
    ///   # Filter by severity
    ///   pup incidents list --query="severity:SEV-2"
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
    /// [Experimental] Inspect Kafka clusters via Datadog
    ///
    /// Experimental. The API surface is not covered by Datadog's public API
    /// compatibility guarantees and may change without notice. OAuth2 bearer
    /// auth only.
    ///
    /// CAPABILITIES:
    ///   • Inspect Kafka topic / broker configuration
    ///   • Inspect Kafka producer / consumer client configuration
    ///   • List Kafka Schema Registry schemas / subject history
    ///   • Read messages from a Kafka cluster / topic (requires agent + perm)
    ///
    /// EXAMPLES:
    ///   pup kafka topic-configs --kafka-cluster-id <id> --topic <t>
    ///   pup kafka broker-configs --kafka-cluster-id <id> --broker-id <b>
    ///   pup kafka client-configs --kafka-cluster-id <id> \
    ///       --service web:producer --service worker:consumer
    ///   pup kafka read-messages --cluster <id> --topic <t> \
    ///       --bootstrap-servers <brokers>
    ///
    /// AUTHENTICATION:
    ///   Requires OAuth2 bearer token (`pup auth login`). API keys are not
    ///   accepted by these endpoints. `read-messages` additionally requires
    ///   the `data_streams_monitoring_capture_messages` permission on the calling user
    ///   and a Datadog Agent reachable via Remote Config.
    #[command(verbatim_doc_comment)]
    Kafka {
        #[command(subcommand)]
        action: KafkaActions,
    },
    /// Manage LLM Observability projects, experiments, and datasets
    ///
    /// Manage LLM Observability resources for AI/ML application monitoring.
    ///
    /// CAPABILITIES:
    ///   • Create and list LLM Obs projects
    ///   • Create, list, update, and delete experiments
    ///   • Create and list datasets within a project
    ///   • Triage Agent Insights and look up model pricing
    ///
    /// EXAMPLES:
    ///   pup llm-obs projects list
    ///   pup llm-obs experiments list
    ///   pup llm-obs datasets list --project-id=my-project
    ///   pup llm-obs agent-insights list --status=for_review
    ///   pup llm-obs model-pricing --provider=anthropic --model=claude-sonnet-4-20250514
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
    /// log-based metrics, restriction queries, and saved views.
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
    ///   • Manage logs saved views (experimental raw API)
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
    ///   # Manage saved views
    ///   pup logs saved-views list
    ///   pup logs saved-views get 123456
    ///   pup logs saved-views create --file saved-view.json
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys
    ///   (DD_API_KEY and DD_APP_KEY environment variables).
    #[command(verbatim_doc_comment)]
    Logs {
        #[command(subcommand)]
        action: LogActions,
    },
    /// Manage log restriction queries for role-based access control
    ///
    /// Restriction queries limit which log data users can read based on their roles.
    /// Use this command to manage the full lifecycle of restriction queries.
    ///
    /// CAPABILITIES:
    ///   • List all restriction queries
    ///   • Get a specific restriction query
    ///   • Create, update, and delete restriction queries
    ///   • List and add roles to restriction queries
    ///
    /// EXAMPLES:
    ///   pup logs-restriction list
    ///   pup logs-restriction get <query-id>
    ///   pup logs-restriction create --file query.json
    ///   pup logs-restriction update <query-id> --file query.json
    ///   pup logs-restriction delete <query-id>
    ///   pup logs-restriction roles list <query-id>
    ///   pup logs-restriction roles add <query-id> --file role.json
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    ///   list/get/roles-list work with default OAuth scopes. create/update/delete
    ///   and roles-add require the user_access_manage scope, which is not
    ///   requested by default -- opt in with:
    ///     pup auth login --extra-scopes user_access_manage
    #[command(name = "logs-restriction", verbatim_doc_comment)]
    LogsRestriction {
        #[command(subcommand)]
        action: LogsRestrictionActions,
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
    /// Explore Model Lab projects and runs
    ///
    /// Browse ML experiment projects and their training runs in Datadog Model Lab.
    ///
    /// SUBCOMMANDS:
    ///   projects    Manage Model Lab projects
    ///   runs        Manage Model Lab runs
    ///
    /// EXAMPLES:
    ///   pup model-lab projects list
    ///   pup model-lab projects get 123
    ///   pup model-lab projects star 123
    ///   pup model-lab runs list --filter-project-id 123
    ///   pup model-lab runs get 456
    ///   pup model-lab runs delete 456
    ///   pup model-lab runs pin 456
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(name = "model-lab", verbatim_doc_comment)]
    ModelLab {
        #[command(subcommand)]
        action: ModelLabActions,
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
    ///   pup notebooks create --file notebook.json
    ///
    ///   # Update a notebook
    ///   pup notebooks update 12345 --file updated.json
    ///
    ///   # Delete a notebook
    ///   pup notebooks delete 12345
    ///
    /// AUTHENTICATION:
    ///   Requires OAuth2 (via 'pup auth login') with notebooks_read/notebooks_write
    ///   scopes, or API key authentication (DD_API_KEY + DD_APP_KEY).
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
    /// List and search running processes
    ///
    /// Query the Datadog Processes API to list and search running processes
    /// across your infrastructure.
    ///
    /// CAPABILITIES:
    ///   • List all running processes with optional search and tag filters
    ///   • Paginate results with page size control
    ///
    /// EXAMPLES:
    ///   pup processes list
    ///   pup processes list --search "nginx"
    ///   pup processes list --tags "env:prod" --page-limit 50
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(verbatim_doc_comment)]
    Processes {
        #[command(subcommand)]
        action: ProcessesActions,
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
    /// Datadog Continuous Profiler (not supported in pup yet)
    ///
    /// Profiling is not supported in pup yet. Use the Datadog MCP server instead:
    /// https://docs.datadoghq.com/bits_ai/mcp_server
    ///
    /// Enable profiling in the MCP toolset with:
    /// https://mcp.datadoghq.com/api/unstable/mcp-server/mcp?toolsets=core,profiling
    #[command(verbatim_doc_comment)]
    Profiling,
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
    /// Manage saved widgets (CCM, logs, CSV, and product analytics reports)
    ///
    /// List, get, create, update, and delete saved reporting widgets scoped
    /// to an experience type.
    ///
    /// EXPERIENCE TYPES:
    ///   ccm_reports            Cloud Cost Management report widgets
    ///   logs_reports           Log Management report widgets
    ///   csv_reports            CSV export widgets
    ///   product_analytics      Product analytics widgets
    ///
    /// COMMANDS:
    ///   list    <experience-type>                  Search and list widgets
    ///   get     <experience-type> <widget-uuid>    Get widget details
    ///   create  <experience-type> --file w.json    Create a widget
    ///   update  <experience-type> <widget-uuid> --file w.json  Update a widget
    ///   delete  <experience-type> <widget-uuid>    Delete a widget
    ///
    /// EXAMPLES:
    ///   pup saved-widgets list logs_reports
    ///   pup saved-widgets get ccm_reports <uuid>
    ///   pup saved-widgets create logs_reports --file widget.json
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication (pup auth login) or API keys
    ///   (DD_API_KEY and DD_APP_KEY environment variables).
    #[command(verbatim_doc_comment, name = "saved-widgets")]
    SavedWidgets {
        #[command(subcommand)]
        action: WidgetActions,
    },
    /// Manage service scorecards
    ///
    /// Manage service quality scorecards and rules.
    ///
    /// Scorecards help you track and improve service quality by defining
    /// rules and measuring compliance across your services.
    ///
    /// COMMANDS:
    ///   rules list          List scorecard rules
    ///   rules create        Create a rule from a JSON file
    ///   rules update        Update a rule
    ///   rules delete        Delete a rule
    ///   outcomes list       List scorecard outcomes
    ///   outcomes batch-create  Create outcomes in batch from a JSON file
    ///
    /// EXAMPLES:
    ///   pup scorecards rules list
    ///   pup scorecards rules create --file rule.json
    ///   pup scorecards outcomes list
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
    /// CONCEPTS:
    ///   Signals — Real-time security detections (e.g., CWS catching a suspicious
    ///   DNS lookup). They are time-series events. Use `pup security signals`.
    ///
    ///   Findings — Point-in-time posture assessments (misconfigurations,
    ///   vulnerabilities, secrets, identity risks). They represent current state.
    ///   Use `pup security findings`.
    ///
    /// COMMANDS:
    ///   Query & Investigate:
    ///     findings           Search and analyze security findings (posture, vulns, misconfigs)
    ///     signals            Search and analyze real-time security detections
    ///     risk-scores        List entity risk scores
    ///
    ///   Configure:
    ///     rules              Manage detection rules
    ///     suppressions       Manage suppression rules
    ///     content-packs      Manage security content packs
    ///     asm-custom-rules   Manage ASM WAF custom rules
    ///     asm-exclusions     Manage ASM WAF exclusion filters
    ///     restriction-policies  Manage resource restriction policies
    ///
    /// EXAMPLES:
    ///   # Analyze open findings by severity
    ///   pup security findings analyze --query "SELECT severity, COUNT(*) as cnt
    ///     FROM dd.security_findings(columns => ARRAY['@severity'],
    ///     filter => '@status:open') AS (severity VARCHAR)
    ///     GROUP BY severity ORDER BY cnt DESC"
    ///
    ///   # List security signals
    ///   pup security signals list --query "@status:open"
    ///
    ///   # List detection rules
    ///   pup security rules list
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
    /// Manage agent skills, subagents, and extensions for AI coding assistants
    ///
    /// Install structured workflow guides, domain references, specialized
    /// agents, and platform extensions that teach AI coding assistants how to
    /// compose pup commands.
    ///
    /// ENTRY TYPES:
    ///   skill       Single-file markdown guide installed under the platform's skills dir
    ///   agent       Domain subagent (Claude Code subagent format or SKILL.md fallback)
    ///   extension   Multi-file bundle for a coding-agent platform (e.g. pi)
    ///
    /// PLATFORMS:
    ///   claude (or claude-code), cursor, codex, opencode, windsurf, gemini, pi, devin
    ///   Pass `all` to install for every supported platform.
    ///   If omitted, pup auto-detects the platform from the environment.
    ///
    /// SCOPE:
    ///   By default, installs go to the user-global directory (e.g.
    ///   ~/.claude/skills, ~/.cursor/skills, ~/.codex/skills,
    ///   ~/.config/opencode/skills, ~/.pi/agent/extensions). Pass --project to
    ///   install into the current project instead (e.g. <repo>/.claude/skills).
    ///
    /// COMMANDS:
    ///   list      List available skills, agents, and extensions
    ///   install   Install entries for one or more platforms
    ///   path      Show where entries would be installed
    ///
    /// EXAMPLES:
    ///   pup skills list
    ///   pup skills install
    ///   pup skills install claude
    ///   pup skills install codex --project
    ///   pup skills install all
    ///   pup skills install pi --name=dd-pup-pi
    ///   pup skills install --type=agent claude
    ///   pup skills path
    ///   pup skills path pi
    #[cfg(not(target_arch = "wasm32"))]
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
    ///   • List all SLOs with optional API-backed server-side filters
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
    ///   # Filter by name or API-supported query string
    ///   pup slos list --query monitor-history-reader
    ///
    ///   # Filter by a single SLO tag
    ///   pup slos list --tags-query team:slo-app
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
    ///
    /// FILTERING:
    ///   List filtering follows Datadog SLO API semantics and may differ from
    ///   the Datadog web UI query language.
    #[command(verbatim_doc_comment)]
    Slos {
        #[command(subcommand)]
        action: SloActions,
    },
    /// Manage the Datadog Software Catalog
    ///
    /// COMMANDS:
    ///   entities list     List catalog entities
    ///   entities upsert   Create or update entities from a JSON file
    ///   entities delete   Delete an entity
    ///   entities preview  Preview entities
    ///   kinds list        List catalog kinds
    ///   kinds upsert      Create or update a kind from a JSON file
    ///   kinds delete      Delete a kind
    ///   relations list    List catalog relations
    ///
    /// EXAMPLES:
    ///   pup software-catalog entities list
    ///   pup software-catalog entities upsert --file entity.json
    ///   pup software-catalog entities delete <entity-id>
    ///   pup software-catalog kinds list
    ///   pup software-catalog relations list
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(name = "software-catalog", verbatim_doc_comment)]
    SoftwareCatalog {
        #[command(subcommand)]
        action: SoftwareCatalogActions,
    },
    /// Manage static analysis
    ///
    /// Manage static analysis custom rulesets and custom rules for code security.
    ///
    /// COMMANDS:
    ///   custom-rulesets get     Get a custom ruleset
    ///   custom-rulesets update  Update a custom ruleset from a JSON file
    ///   custom-rulesets delete  Delete a custom ruleset
    ///   custom-rules get        Get a custom rule
    ///   custom-rules create     Create a custom rule from a JSON file
    ///   custom-rules delete     Delete a custom rule
    ///   custom-rules revisions  List revisions for a custom rule
    ///
    /// EXAMPLES:
    ///   pup static-analysis custom-rulesets get my-ruleset
    ///   pup static-analysis custom-rules create --ruleset my-ruleset --file rule.json
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
    /// Manage tag policies for governance and compliance
    ///
    /// Create, list, get, update, and delete tag policies. Tag policies enforce
    /// required tag keys and allowed values across your Datadog resources, and
    /// provide compliance scoring to measure adherence.
    ///
    /// COMMANDS:
    ///   list              List all tag policies
    ///   get <id>          Get a tag policy by ID
    ///   create --file     Create a tag policy from JSON
    ///   update <id> -f    Update a tag policy
    ///   delete <id>       Delete a tag policy
    ///   score             Get the overall tag policy compliance score
    ///
    /// EXAMPLES:
    ///   pup tag-policies list
    ///   pup tag-policies list --include-score
    ///   pup tag-policies list --filter-source api
    ///   pup tag-policies get pol-abc123 --include-score
    ///   pup tag-policies create --file policy.json
    ///   pup tag-policies score pol-abc123
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    #[command(name = "tag-policies", verbatim_doc_comment)]
    TagPolicies {
        #[command(subcommand)]
        action: TagPoliciesActions,
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
    ///   • Execute workflows via API trigger
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
    ///   Workflow CRUD (`workflows get/create/update/delete`),
    ///   `workflows run`, and `workflows instances *` accept OAuth2
    ///   (`pup auth login`) or DD_API_KEY + DD_APP_KEY.
    ///   `workflows connections *` requires DD_API_KEY + DD_APP_KEY
    ///   pending server-side OAuth enablement on the action-connections
    ///   API.
    #[command(verbatim_doc_comment)]
    Workflows {
        #[command(subcommand)]
        action: WorkflowActions,
    },
    #[command(flatten)]
    Generated(generated::GeneratedCommand),
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
        /// Install one extension from a GitHub release archive (without pup- prefix)
        #[arg(long, conflicts_with_all = ["local", "name", "all"])]
        extension: Option<String>,
        /// Install all extensions found in a GitHub release archive
        #[arg(long, conflicts_with_all = ["local", "extension", "name", "description"])]
        all: bool,
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
    /// List extensions available from a remote GitHub repository
    ListRemote {
        /// Source: GitHub owner/repo
        source: String,
        /// Filter to one extension (without pup- prefix)
        #[arg(long)]
        extension: Option<String>,
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
            help = "Maximum number of monitors to return (1-1000)"
        )]
        limit: i32,
        #[arg(long, default_value_t = 0, help = "Page number (0-indexed)")]
        page: i64,
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
        #[arg(long, allow_hyphen_values = true, help = "Sort order")]
        sort: Option<String>,
    },
    /// Delete a monitor
    Delete { monitor_id: i64 },
    /// Diff a candidate JSON definition against the live monitor
    Diff {
        monitor_id: i64,
        file: String,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Restrict the diff to these field paths (dot-notation, comma-separated or repeated)"
        )]
        only: Vec<String>,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Exclude these field paths from the diff (dot-notation, comma-separated or repeated)"
        )]
        ignore: Vec<String>,
    },
}

// ---- MS Teams ----
#[derive(Subcommand)]
enum MsTeamsActions {
    /// Manage tenant-based handles
    Handles {
        #[command(subcommand)]
        action: MsTeamsHandleActions,
    },
    /// Get a channel by tenant, team, and channel name
    #[command(name = "channel-get")]
    ChannelGet {
        tenant_name: String,
        team_name: String,
        channel_name: String,
    },
    /// Manage Workflows webhook handles
    Workflows {
        #[command(subcommand)]
        action: MsTeamsWorkflowActions,
    },
}

#[derive(Subcommand)]
enum MsTeamsHandleActions {
    /// List tenant-based handles
    List,
    /// Get a tenant-based handle
    Get { handle_id: String },
    /// Create a tenant-based handle from JSON
    Create {
        #[arg(long, help = "JSON file with handle data (required)")]
        file: String,
    },
    /// Update a tenant-based handle
    Update {
        handle_id: String,
        #[arg(long, help = "JSON file with handle data (required)")]
        file: String,
    },
    /// Delete a tenant-based handle
    Delete { handle_id: String },
}

#[derive(Subcommand)]
enum MsTeamsWorkflowActions {
    /// List Workflows webhook handles
    List,
    /// Get a Workflows webhook handle
    Get { handle_id: String },
    /// Create a Workflows webhook handle from JSON
    Create {
        #[arg(long, help = "JSON file with handle data (required)")]
        file: String,
    },
    /// Update a Workflows webhook handle
    Update {
        handle_id: String,
        #[arg(long, help = "JSON file with handle data (required)")]
        file: String,
    },
    /// Delete a Workflows webhook handle
    Delete { handle_id: String },
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
        #[arg(
            long,
            value_delimiter = ',',
            help = "Log indexes to aggregate, comma-separated or repeated"
        )]
        index: Vec<String>,
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
        #[arg(
            long,
            allow_hyphen_values = true,
            default_value = "-timestamp",
            help = "Sort order"
        )]
        sort: String,
        #[arg(long, help = "Storage tier: indexes, online-archives, or flex")]
        storage: Option<String>,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Log indexes to search, comma-separated or repeated"
        )]
        index: Vec<String>,
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
        #[arg(
            long,
            allow_hyphen_values = true,
            default_value = "-timestamp",
            help = "Sort order"
        )]
        sort: String,
        #[arg(long, help = "Storage tier: indexes, online-archives, or flex")]
        storage: Option<String>,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Log indexes to search, comma-separated or repeated"
        )]
        index: Vec<String>,
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
        #[arg(
            long,
            value_delimiter = ',',
            help = "Log indexes to search, comma-separated or repeated"
        )]
        index: Vec<String>,
        #[arg(
            long,
            allow_hyphen_values = true,
            default_value = "count",
            help = "Sort groups by aggregation (count,cardinality,pc75,pc90,pc95,pc98,pc99,sum,min,max)"
        )]
        sort: String,
        #[arg(
            long,
            help = "Return a timeseries with this bucket size (e.g. 60s, 5m, 1h); enables timeseries mode"
        )]
        interval: Option<String>,
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
    /// Manage logs saved views (experimental raw API)
    #[command(name = "saved-views")]
    SavedViews {
        #[command(subcommand)]
        action: LogSavedViewActions,
    },
}

#[derive(Subcommand)]
enum LogSavedViewActions {
    /// List logs saved views
    List,
    /// Get logs saved view details
    Get { view_id: String },
    /// Create a logs saved view from JSON
    Create {
        #[arg(long, help = "JSON file with saved view request body (required)")]
        file: String,
    },
    /// Delete a logs saved view
    Delete { view_id: String },
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
        #[arg(
            long,
            help = "Filter incidents using Datadog incidents search syntax (e.g., 'state:resolved', 'severity:SEV-2', 'state:(active OR stable)'). Defaults to 'state:active'."
        )]
        query: Option<String>,
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
    /// Print dashboard URL, optionally scoped to a live time window
    Url {
        id: String,
        #[arg(long = "from", default_value = "now-1w")]
        from_ts: String,
        #[arg(long = "to", default_value = "now")]
        to_ts: String,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        live: bool,
    },
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
    /// Edit widgets embedded in a dashboard, and discover widget schemas
    Widgets {
        #[command(subcommand)]
        action: DashboardWidgetActions,
    },
    /// Manage annotations on dashboard pages
    Annotations {
        #[command(subcommand)]
        action: AnnotationsActions,
    },
}

// ---- Dashboard widget subcommands (embedded widgets in a dashboard) ----

#[derive(Subcommand)]
enum DashboardWidgetActions {
    /// List all widgets in a dashboard (index, id, type, title, layout)
    List {
        /// Dashboard ID
        dash_id: String,
    },
    /// Show one widget's full JSON
    Get {
        /// Dashboard ID
        dash_id: String,
        /// Widget id (Widget.id from the dashboard JSON)
        #[arg(long, conflicts_with = "index", required_unless_present = "index")]
        widget_id: Option<i64>,
        /// 0-based position of the widget in the dashboard widgets array
        #[arg(long)]
        index: Option<usize>,
    },
    /// Append a widget from a JSON file ('-' for stdin)
    Add {
        /// Dashboard ID
        dash_id: String,
        /// Path to widget JSON file, or '-' to read from stdin
        #[arg(long)]
        file: String,
    },
    /// Replace one widget from a JSON file ('-' for stdin)
    Update {
        /// Dashboard ID
        dash_id: String,
        /// Widget id (Widget.id from the dashboard JSON)
        #[arg(long, conflicts_with = "index", required_unless_present = "index")]
        widget_id: Option<i64>,
        /// 0-based position of the widget in the dashboard widgets array
        #[arg(long)]
        index: Option<usize>,
        /// Path to widget JSON file, or '-' to read from stdin
        #[arg(long)]
        file: String,
    },
    /// Delete one widget from a dashboard
    Remove {
        /// Dashboard ID
        dash_id: String,
        /// Widget id (Widget.id from the dashboard JSON)
        #[arg(long, conflicts_with = "index", required_unless_present = "index")]
        widget_id: Option<i64>,
        /// 0-based position of the widget in the dashboard widgets array
        #[arg(long)]
        index: Option<usize>,
    },
    /// List all supported widget type strings
    Types,
    /// Emit a ready-to-edit skeleton JSON for a widget type
    ///
    /// Pass the type string (e.g. timeseries) to get a skeleton you can
    /// fill in and pass to `pup dashboards widgets add --file`.
    /// Run `pup dashboards widgets types` for the full list of types.
    Schema {
        /// Widget type string, e.g. timeseries, note, query_value
        #[arg(help = "Widget type string (see `pup dashboards widgets types`)")]
        r#type: String,
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
        #[arg(long, help = "Probe location as TYPE:METHOD or TYPE:METHOD(arg_types)")]
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
    /// Query v2 timeseries data via TimeseriesFormulaQueryRequest JSON body (SDK PR #1564)
    ///
    /// Provide a JSON file matching the TimeseriesFormulaQueryRequest schema.
    /// To query across organisations, include `cross_org_uuids` inside each
    /// query object in the file (SDK PR #1564 added this field to
    /// MetricsTimeseriesQuery and MetricsScalarQuery).
    ///
    /// Example body (timeseries_request.json):
    ///   {
    ///     "data": {
    ///       "attributes": {
    ///         "formulas": [{"formula": "a"}],
    ///         "from": 1700000000000,
    ///         "interval": 5000,
    ///         "queries": [{"data_source": "metrics", "query": "avg:system.cpu.user{*}", "name": "a",
    ///                       "cross_org_uuids": ["<org-uuid>"]}],
    ///         "to": 1700003600000
    ///       },
    ///       "type": "timeseries_request"
    ///     }
    ///   }
    Timeseries {
        #[arg(long, help = "JSON file with TimeseriesFormulaQueryRequest body")]
        file: String,
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
        #[arg(
            long,
            help = "Lookback window in seconds (SDK PR #1593; default 3600, max 2592000)"
        )]
        window_seconds: Option<i64>,
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
    /// List all SLOs with optional API-backed filters
    List {
        #[arg(long, help = "Filter by SLO name or API-supported search string")]
        query: Option<String>,
        #[arg(long, help = "Filter by a single SLO tag")]
        tags_query: Option<String>,
        #[arg(long, help = "Filter by SLO numerator/denominator query")]
        metrics_query: Option<String>,
        #[arg(long, help = "Number of SLOs to return")]
        limit: Option<i64>,
        #[arg(long, help = "Pagination offset")]
        offset: Option<i64>,
    },
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
        #[arg(
            long,
            default_value_t = false,
            help = "Disable partial results (wait for full indexing before returning)"
        )]
        no_allow_partial: bool,
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
    /// Manage Synthetics downtimes
    Downtime {
        #[command(subcommand)]
        action: SyntheticsDowntimeActions,
    },
}

#[derive(Subcommand)]
enum SyntheticsDowntimeActions {
    /// List all Synthetics downtimes
    List {
        /// Comma-separated list of Synthetics test public IDs to filter by
        #[arg(long = "filter-test-ids")]
        filter_test_ids: Option<String>,
        /// If set to true, return only currently active downtimes
        #[arg(long = "filter-active")]
        filter_active: Option<String>,
    },
    /// Create a new Synthetics downtime (body from JSON file)
    Create {
        #[arg(long, help = "JSON file with downtime definition (required)")]
        file: String,
    },
    /// Delete a Synthetics downtime by ID
    Delete {
        /// Downtime ID to delete
        downtime_id: String,
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
        #[arg(long, allow_hyphen_values = true, help = "Sort order")]
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
    /// Get a specific full result for an API Synthetic test
    #[command(name = "get-result")]
    GetResult {
        /// Public ID of the test
        public_id: String,
        /// Result ID
        result_id: String,
        /// Look up by event ID instead of result ID
        #[arg(long = "event-id")]
        event_id: Option<String>,
        /// Look up by timestamp (seconds)
        #[arg(long)]
        timestamp: Option<i64>,
    },
    /// Get a specific full result for a browser Synthetic test
    #[command(name = "get-browser-result")]
    GetBrowserResult {
        /// Public ID of the test
        public_id: String,
        /// Result ID
        result_id: String,
        /// Look up by event ID instead of result ID
        #[arg(long = "event-id")]
        event_id: Option<String>,
        /// Look up by timestamp (seconds)
        #[arg(long)]
        timestamp: Option<i64>,
    },
    /// List the latest API test result summaries for a test
    #[command(name = "list-latest-results")]
    ListLatestResults {
        /// Public ID of the test
        public_id: String,
        /// Start timestamp in milliseconds
        #[arg(long = "from-ts")]
        from_ts: Option<i64>,
        /// End timestamp in milliseconds
        #[arg(long = "to-ts")]
        to_ts: Option<i64>,
        /// Filter by status (passed, failed, no_data)
        #[arg(long)]
        status: Option<String>,
        /// Filter by run type (scheduled, fast, ci, triggered)
        #[arg(long = "run-type")]
        run_type: Option<String>,
        /// Filter by location (private location ID or public location). Can be repeated.
        #[arg(long = "probe-dc")]
        probe_dc: Option<Vec<String>>,
        /// Filter by device ID. Can be repeated.
        #[arg(long = "device-id")]
        device_id: Option<Vec<String>>,
    },
    /// List the latest browser test result summaries for a test
    #[command(name = "list-latest-browser-results")]
    ListLatestBrowserResults {
        /// Public ID of the test
        public_id: String,
        /// Start timestamp in milliseconds
        #[arg(long = "from-ts")]
        from_ts: Option<i64>,
        /// End timestamp in milliseconds
        #[arg(long = "to-ts")]
        to_ts: Option<i64>,
        /// Filter by status (passed, failed, no_data)
        #[arg(long)]
        status: Option<String>,
        /// Filter by run type (scheduled, fast, ci, triggered)
        #[arg(long = "run-type")]
        run_type: Option<String>,
        /// Filter by location. Can be repeated.
        #[arg(long = "probe-dc")]
        probe_dc: Option<Vec<String>>,
        /// Filter by device ID. Can be repeated.
        #[arg(long = "device-id")]
        device_id: Option<Vec<String>>,
    },
    /// Poll for results given a list of result IDs (typical CI/CD use)
    #[command(name = "poll-results")]
    PollResults {
        /// One or more result IDs
        #[arg(required = true)]
        result_ids: Vec<String>,
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
    /// Manage flaky test management policies
    Policies {
        #[command(subcommand)]
        action: TestOptimizationFlakyTestsPoliciesActions,
    },
}

#[derive(Subcommand)]
enum TestOptimizationFlakyTestsPoliciesActions {
    /// Get flaky tests management policies.
    /// The API uses a POST body to specify which service/repo policies to retrieve.
    Get {
        #[arg(
            long,
            help = "JSON file with request body specifying which policies to retrieve (e.g. {\"data\":{\"type\":\"get_request\"}})"
        )]
        file: String,
    },
    /// Update flaky tests management policies (body from JSON file)
    Update {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
}

// ---- Events ----
#[derive(Subcommand)]
enum EventActions {
    /// Post an event
    Post {
        #[arg(help = "Event title")]
        title: String,
        #[arg(help = "Event message body; reads from stdin when omitted")]
        message: Option<String>,
        #[arg(
            long,
            visible_alias = "date_happened",
            help = "POSIX timestamp when the event occurred"
        )]
        date_happened: Option<i64>,
        #[arg(long, help = "User to post the event as")]
        handle: Option<String>,
        #[arg(long, value_enum, default_value = "normal", help = "Event priority")]
        priority: commands::events::EventPriorityArg,
        #[arg(long, visible_alias = "related_event_id", help = "Parent event ID")]
        related_event_id: Option<i64>,
        #[arg(long, help = "Comma-separated event tags")]
        tags: Option<String>,
        #[arg(
            long,
            default_value = "",
            help = "Host to associate with the event; defaults to the local hostname when it can be determined"
        )]
        host: String,
        #[arg(
            long,
            visible_alias = "no_host",
            help = "Do not associate a host with the event; overrides --host"
        )]
        no_host: bool,
        #[arg(
            long = "device-name",
            visible_aliases = ["device_name", "device"],
            help = "Device name to associate with the event"
        )]
        device: Option<String>,
        #[arg(
            long = "source-type-name",
            visible_aliases = ["source_type_name", "type"],
            help = "Event source type name"
        )]
        event_type: Option<String>,
        #[arg(
            long,
            visible_alias = "aggregation_key",
            help = "Key used to aggregate related events"
        )]
        aggregation_key: Option<String>,
        #[arg(
            long,
            visible_alias = "alert_type",
            value_enum,
            help = "Event alert type"
        )]
        alert_type: Option<commands::events::EventAlertTypeArg>,
    },
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

// ---- DBM ----
#[derive(Subcommand)]
enum DbmActions {
    /// Manage DBM query samples
    Samples {
        #[command(subcommand)]
        action: DbmSamplesActions,
    },
}

#[derive(Subcommand)]
enum DbmSamplesActions {
    /// Search DBM query samples
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
        #[arg(long, default_value_t = 10, help = "Maximum number of samples")]
        limit: i32,
        #[arg(
            long,
            allow_hyphen_values = true,
            default_value = "desc",
            help = "Sort order: asc, desc, timestamp, or -timestamp"
        )]
        sort: String,
    },
}

// ---- DDSQL ----
#[derive(Subcommand)]
enum DdsqlActions {
    /// Execute DDSQL query and return columnar table data
    Table {
        #[arg(
            long,
            allow_hyphen_values = true,
            help = "DDSQL query string, or use --query - to read from stdin"
        )]
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
        #[arg(
            long,
            allow_hyphen_values = true,
            help = "DDSQL query string, or use --query - to read from stdin"
        )]
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
    /// Print DDSQL reference guidance from the editor tooling
    Spec,
    /// Discover DDSQL tables and columns
    Schema {
        #[command(subcommand)]
        action: DdsqlSchemaActions,
    },
}

#[derive(Subcommand)]
enum DdsqlSchemaActions {
    /// List DDSQL tables visible to your org
    Tables {
        #[arg(long, help = "Case-insensitive substring filter for table names")]
        query: Option<String>,
        #[arg(
            long,
            default_value_t = 100,
            help = "Maximum number of tables to return"
        )]
        limit: usize,
        #[arg(long, default_value_t = 0, help = "Number of tables to skip")]
        offset: usize,
    },
    /// Show columns for a DDSQL table
    Columns {
        #[arg(
            long,
            help = "Table ID, for example public.aws.ec2_instance or reference_tables.my_table"
        )]
        table_id: String,
        #[arg(
            long,
            default_value_t = 100,
            help = "Maximum number of columns to return"
        )]
        limit: usize,
        #[arg(long, default_value_t = 0, help = "Number of columns to skip")]
        offset: usize,
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

// ---- Tag Policies ----
#[derive(Subcommand)]
enum TagPoliciesActions {
    /// List all tag policies
    List {
        #[arg(long, default_value_t = false, help = "Include disabled policies")]
        include_disabled: bool,
        #[arg(long, default_value_t = false, help = "Include soft-deleted policies")]
        include_deleted: bool,
        #[arg(
            long,
            default_value_t = false,
            help = "Include compliance score in response"
        )]
        include_score: bool,
        #[arg(long, help = "Filter by policy source: api, terraform, ui")]
        filter_source: Option<String>,
    },
    /// Get a tag policy by ID
    Get {
        policy_id: String,
        #[arg(
            long,
            default_value_t = false,
            help = "Include compliance score in response"
        )]
        include_score: bool,
    },
    /// Create a tag policy from a JSON file
    Create {
        #[arg(long, help = "JSON file with TagPolicyCreateRequest body")]
        file: String,
    },
    /// Update a tag policy from a JSON file
    Update {
        policy_id: String,
        #[arg(long, short, help = "JSON file with TagPolicyUpdateRequest body")]
        file: String,
    },
    /// Delete a tag policy
    Delete {
        policy_id: String,
        #[arg(
            long,
            default_value_t = false,
            help = "Permanently delete (default: soft delete)"
        )]
        hard_delete: bool,
    },
    /// Get the compliance score for a tag policy
    Score {
        policy_id: String,
        #[arg(long, help = "Start of scoring window (Unix ms timestamp)")]
        ts_start: Option<i64>,
        #[arg(long, help = "End of scoring window (Unix ms timestamp)")]
        ts_end: Option<i64>,
    },
}

// ---- Users ----
#[derive(Subcommand)]
enum UserActions {
    /// List users
    List {
        #[arg(long, default_value_t = 10, help = "Results per page (max 100)")]
        page_size: i64,
        #[arg(long, default_value_t = 0, help = "Page number (0-indexed)")]
        page_number: i64,
    },
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
    /// Manage service accounts
    #[command(name = "service-accounts")]
    ServiceAccounts {
        #[command(subcommand)]
        action: ServiceAccountActions,
    },
}

#[derive(Subcommand)]
enum UserRoleActions {
    /// List roles
    List,
}

#[derive(Subcommand)]
enum ServiceAccountActions {
    /// Create a service account from a JSON file
    Create {
        #[arg(long)]
        file: String,
    },
    /// Manage application keys for a service account
    #[command(name = "app-keys")]
    AppKeys {
        #[command(subcommand)]
        action: ServiceAccountAppKeyActions,
    },
}

#[derive(Subcommand)]
enum ServiceAccountAppKeyActions {
    /// List application keys for a service account
    List { service_account_id: String },
    /// Get an application key for a service account
    Get {
        service_account_id: String,
        app_key_id: String,
    },
    /// Create an application key for a service account
    Create {
        service_account_id: String,
        #[arg(long)]
        file: String,
    },
    /// Update an application key for a service account
    Update {
        service_account_id: String,
        app_key_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete an application key from a service account
    Delete {
        service_account_id: String,
        app_key_id: String,
    },
}

// ---- Processes ----
#[derive(Subcommand)]
enum ProcessesActions {
    /// List running processes
    List {
        #[arg(long, help = "Search processes by name or command")]
        search: Option<String>,
        #[arg(long, help = "Comma-separated list of tags to filter by")]
        tags: Option<String>,
        #[arg(long, help = "Maximum number of results per page")]
        page_limit: Option<i32>,
    },
}

// ---- LogsRestriction ----
#[derive(Subcommand)]
enum LogsRestrictionActions {
    /// List all restriction queries
    List,
    /// Get a restriction query by ID
    Get { query_id: String },
    /// Create a restriction query from a JSON file
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update a restriction query from a JSON file
    Update {
        query_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete a restriction query
    Delete { query_id: String },
    /// Manage roles for a restriction query
    Roles {
        #[command(subcommand)]
        action: LogsRestrictionRoleActions,
    },
}

#[derive(Subcommand)]
enum LogsRestrictionRoleActions {
    /// List roles for a restriction query
    List { query_id: String },
    /// Add a role to a restriction query from a JSON file
    Add {
        query_id: String,
        #[arg(long)]
        file: String,
    },
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
            allow_hyphen_values = true,
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
    /// Diff a candidate JSON definition against the live workflow
    Diff {
        workflow_id: String,
        file: String,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Restrict the diff to these field paths (dot-notation, comma-separated or repeated)"
        )]
        only: Vec<String>,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Exclude these field paths from the diff (dot-notation, comma-separated or repeated)"
        )]
        ignore: Vec<String>,
    },
    /// Delete a workflow
    Delete { workflow_id: String },
    /// Execute a workflow via API trigger
    ///
    /// The workflow must have an API trigger configured. Accepts OAuth2
    /// (`pup auth login`, requires the `workflows_run` scope) or
    /// DD_API_KEY + DD_APP_KEY.
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
    /// Manage action connections
    Connections {
        #[command(subcommand)]
        action: WorkflowConnectionActions,
    },
}

#[derive(Subcommand)]
enum WorkflowConnectionActions {
    /// Get an action connection by ID
    Get { connection_id: String },
    /// Create an action connection from a JSON file
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update an action connection from a JSON file
    Update {
        connection_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete an action connection
    Delete { connection_id: String },
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
        #[arg(
            long,
            allow_hyphen_values = true,
            default_value = "status",
            help = "Sort field"
        )]
        sort: String,
        #[arg(long, default_value_t = 100, help = "Maximum hosts")]
        count: i64,
        #[arg(long, help = "Starting offset for paginated results")]
        start: Option<i64>,
        #[arg(long, help = "Include data for muted hosts")]
        include_muted_hosts_data: bool,
        #[arg(long, help = "Include host metadata (agent version, cpu, etc.)")]
        include_hosts_metadata: bool,
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
    /// Migrate a service catalog YAML file to v3 schema
    ///
    /// Detects the current schema version (v1, v2, v2.1, v2.2) and converts
    /// it to v3 format. Tries the Datadog convert API first; falls back to
    /// local migration if the API fails or rejects the input.
    ///
    /// After migration, validates the output against the official v3 JSON schemas
    /// fetched from GitHub, then prompts where to write the result.
    ///
    /// EXAMPLES:
    ///   pup idp migrate-schema service.datadog.yaml
    ///   pup idp migrate-schema ./catalog/checkout-api.yaml
    ///   pup idp migrate-schema
    #[command(verbatim_doc_comment)]
    MigrateSchema {
        /// Path to the YAML file (optional — auto-discovers *.datadog.yaml if omitted)
        file: Option<String>,
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

// ---- Docs AI ----
#[derive(Subcommand)]
enum DocsActions {
    /// Ask the Datadog Docs AI a natural-language question
    Ask {
        /// The question to ask
        question: String,
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
    // -- Query & Investigate --
    /// Search and analyze security findings (posture, vulnerabilities, misconfigs)
    Findings {
        #[command(subcommand)]
        action: SecurityFindingActions,
    },
    /// Search and analyze real-time security detections
    Signals {
        #[command(subcommand)]
        action: SecuritySignalActions,
    },
    /// List entity risk scores
    #[command(name = "risk-scores")]
    RiskScores {
        #[command(subcommand)]
        action: SecurityRiskScoreActions,
    },

    // -- Configure --
    /// Manage detection rules
    Rules {
        #[command(subcommand)]
        action: SecurityRuleActions,
    },
    /// Manage suppression rules
    Suppressions {
        #[command(subcommand)]
        action: SecuritySuppressionActions,
    },
    /// Manage security content packs
    #[command(name = "content-packs")]
    ContentPacks {
        #[command(subcommand)]
        action: SecurityContentPackActions,
    },
    /// Explore indicators of compromise (IoCs)
    Iocs {
        #[command(subcommand)]
        action: SecurityIocActions,
    },
    /// Manage ASM WAF custom rules
    #[command(name = "asm-custom-rules")]
    AsmCustomRules {
        #[command(subcommand)]
        action: AsmCustomRuleActions,
    },
    /// Manage ASM WAF exclusion filters
    #[command(name = "asm-exclusions")]
    AsmExclusions {
        #[command(subcommand)]
        action: AsmExclusionActions,
    },
    /// Manage resource restriction policies
    ///
    /// Restriction policies live at `/api/v2/restriction_policy/{resource}`
    /// where `{resource}` is `<type>:<id>` (ex: `dashboard:abc-123`,
    /// `monitor:12345`). The server accepts OAuth2 or DD_API_KEY +
    /// DD_APP_KEY.
    ///
    /// The required OAuth scope depends on the resource type embedded in
    /// the resource ID. The server enforces the same permission a user would
    /// need to view/edit the underlying resource (ex: `dashboards_read` for
    /// a `dashboard:*` GET, `monitors_write` for a `monitor:*` POST).
    ///
    /// Common types covered by pup's default OAuth scopes today: dashboard,
    /// monitor, slo, workflow, notebook, security-rule, logs-archive,
    /// rum-application, reference-table, case-management-project,
    /// on-call-*, status-page, integration-*. Other resource types (ex:
    /// connection, app-builder-app, obs-pipelines-*, spreadsheet,
    /// feature-flag, agent-builder-agent, product-analytics-*) require
    /// scopes pup does not yet request; for those, use DD_API_KEY +
    /// DD_APP_KEY.
    #[command(name = "restriction-policies", verbatim_doc_comment)]
    RestrictionPolicies {
        #[command(subcommand)]
        action: RestrictionPolicyActions,
    },
    /// Export and convert security monitoring resources as Terraform
    Terraform {
        #[command(subcommand)]
        action: SecurityTerraformActions,
    },
}

#[derive(Subcommand)]
enum SecurityTerraformActions {
    /// Export a single resource as Terraform
    Export {
        /// Resource type (suppressions, critical_assets)
        resource_type: String,
        /// Resource ID to export
        resource_id: String,
    },
    /// Bulk export resources as a Terraform zip archive
    #[command(name = "bulk-export")]
    BulkExport {
        /// Resource type (suppressions, critical_assets)
        resource_type: String,
        #[arg(long, help = "JSON file with the bulk export payload (required)")]
        file: String,
        #[arg(
            long = "output-file",
            help = "Path to write the zip archive to (required)"
        )]
        output_file: String,
    },
    /// Convert a JSON resource definition into Terraform
    Convert {
        /// Resource type (suppressions, critical_assets)
        resource_type: String,
        #[arg(long, help = "JSON file with the conversion payload (required)")]
        file: String,
    },
}

#[derive(Subcommand)]
enum AsmCustomRuleActions {
    /// List WAF custom rules
    List,
    /// Get a WAF custom rule
    Get { custom_rule_id: String },
    /// Create a WAF custom rule
    Create {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update a WAF custom rule
    Update {
        custom_rule_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete a WAF custom rule
    Delete { custom_rule_id: String },
}

#[derive(Subcommand)]
enum AsmExclusionActions {
    /// List WAF exclusion filters
    List,
    /// Get a WAF exclusion filter
    Get { exclusion_filter_id: String },
    /// Create a WAF exclusion filter
    Create {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update a WAF exclusion filter
    Update {
        exclusion_filter_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete a WAF exclusion filter
    Delete { exclusion_filter_id: String },
}

#[derive(Subcommand)]
enum RestrictionPolicyActions {
    /// Get a restriction policy for a resource
    Get { resource_id: String },
    /// Update (replace) the restriction policy for a resource
    Update {
        resource_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete the restriction policy for a resource
    Delete { resource_id: String },
}

#[derive(Subcommand)]
enum SecurityRuleActions {
    /// List security rules
    List {
        #[arg(long, help = "Filter query")]
        filter: Option<String>,
        #[arg(
            long,
            allow_hyphen_values = true,
            help = "Sort order (name, -name, creation_date, -creation_date, update_date, -update_date, enabled, -enabled, type, -type, highest_severity, -highest_severity, source, -source)"
        )]
        sort: Option<String>,
        #[arg(long, default_value_t = 10, help = "Results per page (max 100)")]
        page_size: i64,
        #[arg(long, default_value_t = 0, help = "Page number (0-indexed)")]
        page_number: i64,
    },
    /// Get rule details
    Get { rule_id: String },
    /// Bulk export security monitoring rules
    #[command(name = "bulk-export")]
    BulkExport {
        /// Rule IDs to export
        rule_ids: Vec<String>,
    },
    /// Convert a JSON rule definition into a Terraform resource
    #[command(name = "to-terraform")]
    ToTerraform {
        #[arg(long, help = "JSON file with the rule conversion payload (required)")]
        file: String,
    },
    /// Bulk convert existing rules to Terraform (returns a ZIP archive)
    #[command(name = "bulk-convert")]
    BulkConvert {
        #[arg(
            long,
            help = "JSON file with SecurityMonitoringRuleConvertBulkPayload body (required)"
        )]
        file: String,
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
        #[arg(
            long,
            allow_hyphen_values = true,
            help = "Sort order: timestamp (ascending) or -timestamp (descending)"
        )]
        sort: Option<String>,
    },
    /// Get log queries for investigating a signal
    #[command(name = "investigation-queries")]
    InvestigationQueries {
        /// Security signal ID
        signal_id: String,
    },
    /// Get suggested remediation actions for a signal
    #[command(name = "suggested-actions")]
    SuggestedActions {
        /// Security signal ID
        signal_id: String,
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
    /// Get the schema (available fields and types) for security findings.
    /// Fetches the latest schema reference from Datadog documentation.
    /// Call this before using `findings analyze` to discover queryable fields.
    #[command(name = "schema")]
    Schema,
    /// Analyze security findings using DDSQL. Workflow: 1) Run `pup security findings schema` to get fields, 2) Query with SQL. Function: dd.security_findings(columns => ARRAY['@field', ...], filter => '@field:value', finding_types => ARRAY['type', ...]). AS clause types: VARCHAR, BIGINT, DECIMAL, BOOLEAN, TIMESTAMP. Notes: 'columns' ordering MUST match the AS clause. Use -@compliance.evaluation:pass filter to exclude passing findings. Prefer ordering by @severity_details.adjusted.score. Use LIMIT to reduce output. Example: SELECT rule_name, finding_type, severity, count(*) as cnt FROM dd.security_findings(columns => ARRAY['@rule.name', '@finding_type', '@severity'], filter => '@status:open @severity:(high OR critical)') AS (rule_name VARCHAR, finding_type VARCHAR, severity VARCHAR) GROUP BY rule_name, finding_type, severity ORDER BY cnt DESC LIMIT 100
    #[command(
        name = "analyze",
        long_about = "Analyze security findings using DDSQL with dd.security_findings().\n\nWorkflow: 1) Call `pup security findings schema` first to get available fields\n         2) Use this command to query with SQL\n\nQueries the current state of all security findings using flexible SQL aggregations,\nfiltering, and grouping.\n\nFunction signature:\n  dd.security_findings(\n    columns => ARRAY['@field1', '@field2', ...],\n    filter  => '@field:value',\n    finding_types => ARRAY['library_vulnerability', ...]\n  )\n\nThe AS clause must declare column names and DDSQL types:\n  AS (col1 VARCHAR, col2 BIGINT, ...)\n\nAvailable types: VARCHAR, BIGINT, DECIMAL, BOOLEAN, TIMESTAMP\n\nQuery structure (filter vs WHERE):\n  - filter => '...'  : Datadog query syntax, pushed down to the backing store.\n                        Use @ prefix: @status:open @severity:(high OR critical)\n                        Supports negation: -@compliance.evaluation:pass\n  - WHERE clause     : Standard SQL, operates on the aliases in your AS clause.\n                        No @ prefix: WHERE severity = 'critical'\n                        Simple conditions are pushed down automatically.\n  - columns => ARRAY : Fields to select. Use @ prefix. Order must match AS clause.\n\nCommon fields (use with @ prefix in columns/filter):\n\n  Filtering & Grouping\n    @severity              string   critical, high, medium, low, info, none, unknown\n    @status                string   open, muted, auto_closed\n    @finding_type          string   misconfiguration, host_and_container_vulnerability,\n                                    library_vulnerability, static_code_vulnerability,\n                                    secret, identity_risk, attack_path, ...\n    @resource_type         string   Type of affected resource\n    @rule.name             string   Name of the detection rule\n\n  Identification\n    @title                 string   Human-readable finding title\n    @resource_name         string   Name of the affected resource\n    @resource_id           string   Unique resource identifier\n\n  Risk Prioritization\n    @is_in_security_inbox                boolean  In the Security Inbox\n    @severity_details.adjusted.score     number   CVSS-scale adjusted score\n    @risk.is_publicly_accessible         boolean  Resource is internet-facing\n    @risk.is_production                  boolean  Resource is in production\n    @risk.has_exploit_available          boolean  Known exploits exist\n    @risk.has_high_exploitability_chance  boolean  EPSS score > 1%\n    @risk.is_exposed_to_attacks          boolean  Attacks already detected\n    @risk.has_sensitive_data             boolean  Resource has sensitive data\n\n  Compliance\n    @compliance.evaluation   string   pass or fail\n\n  Scoping\n    @cloud_resource.cloud_provider   string   aws, azure, gcp, oci\n    @cloud_resource.account          string   Cloud account/subscription/project\n    @cloud_resource.region           string   Cloud region\n    @service.name                    string   Service name\n    @host.name                       string   Host name\n\n  Time\n    @first_seen_at           integer  First detection (ms UTC)\n    @last_seen_at            integer  Most recent detection (ms UTC)\n\n  Run `pup security findings schema` for the full field reference.\n\nIMPORTANT notes:\n  - 'columns =>' ordering MUST match the AS clause column ordering\n  - If querying all findings or misconfigurations, use -@compliance.evaluation:pass\n    filter to exclude passing findings\n  - Prefer ordering by severity score (@severity_details.adjusted.score) when relevant\n  - Use LIMIT to reduce context\n\nExample queries:\n\n  # Open findings by severity\n  pup security findings analyze --query \"\n    SELECT severity, COUNT(*) as cnt\n    FROM dd.security_findings(\n      columns => ARRAY['@severity'],\n      filter => '@status:open'\n    ) AS (severity VARCHAR)\n    GROUP BY severity ORDER BY cnt DESC\"\n\n  # Top 10 critical rules\n  pup security findings analyze --query \"\n    SELECT rule_name, COUNT(*) as cnt\n    FROM dd.security_findings(\n      columns => ARRAY['@rule.name'],\n      filter => '@status:open @severity:critical'\n    ) AS (rule_name VARCHAR)\n    GROUP BY rule_name ORDER BY cnt DESC LIMIT 10\"\n\n  # Critical findings in production with known exploits\n  pup security findings analyze --query \"\n    SELECT title, resource_name, score\n    FROM dd.security_findings(\n      columns => ARRAY['@title', '@resource_name', '@severity_details.adjusted.score'],\n      filter => '@status:open @severity:critical @risk.is_production:true @risk.has_exploit_available:true'\n    ) AS (title VARCHAR, resource_name VARCHAR, score DECIMAL)\n    ORDER BY score DESC LIMIT 20\"\n\n  # Findings by cloud account and region\n  pup security findings analyze --query \"\n    SELECT account, region, COUNT(*) as cnt\n    FROM dd.security_findings(\n      columns => ARRAY['@cloud_resource.account', '@cloud_resource.region'],\n      filter => '@status:open @severity:(high OR critical)'\n    ) AS (account VARCHAR, region VARCHAR)\n    GROUP BY account, region ORDER BY cnt DESC LIMIT 20\"\n\n  # Vulnerabilities only (exclude misconfigs, secrets, etc.)\n  pup security findings analyze --query \"\n    SELECT severity, COUNT(*) as cnt\n    FROM dd.security_findings(\n      columns => ARRAY['@severity'],\n      filter => '@status:open',\n      finding_types => ARRAY['host_and_container_vulnerability', 'library_vulnerability']\n    ) AS (severity VARCHAR)\n    GROUP BY severity ORDER BY cnt DESC\""
    )]
    Analyze {
        /// SQL query using dd.security_findings(columns => ARRAY['@field', ...], filter => '@field:value', finding_types => ARRAY['type', ...]) AS (col TYPE, ...). 'columns' ordering MUST match the AS clause. Run `pup security findings schema` to see fields. Types: VARCHAR, BIGINT, DECIMAL, BOOLEAN, TIMESTAMP
        #[arg(long, short)]
        query: String,

        /// Start time (default: 24h ago). Relative (e.g., 24h, 7d) or ISO 8601.
        #[arg(long, default_value = "24h")]
        from: String,

        /// End time (default: now). ISO 8601 or relative
        #[arg(long, default_value = "now")]
        to: String,

        /// Max rows to return
        #[arg(long, default_value_t = 100)]
        limit: i64,
    },
    /// Mute or unmute security findings (up to 100 per request)
    Mute {
        #[arg(long, help = "JSON file with MuteFindingsRequest body (required)")]
        file: String,
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
enum SecurityIocActions {
    /// List indicators of compromise
    List {
        /// Search/filter query (supports field:value syntax)
        #[arg(long)]
        query: Option<String>,
        /// Number of results per page
        #[arg(long)]
        limit: Option<i32>,
        /// Pagination offset
        #[arg(long)]
        offset: Option<i32>,
        /// Sort column (score, first_seen_ts_epoch, last_seen_ts_epoch, indicator, indicator_type, signal_count, log_count, category, as_type)
        #[arg(long = "sort-column")]
        sort_column: Option<String>,
        /// Sort order (asc, desc)
        #[arg(long = "sort-order")]
        sort_order: Option<String>,
    },
    /// Get a single indicator of compromise
    Get {
        /// Indicator value (e.g. IP, domain, hash)
        indicator: String,
    },
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
            allow_hyphen_values = true,
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

// ---- AuthN Mappings ----
#[derive(Subcommand)]
enum AuthnMappingsActions {
    /// List all AuthN mappings
    List,
    /// Get an AuthN mapping
    Get { mapping_id: String },
    /// Create an AuthN mapping from JSON
    Create {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update an AuthN mapping
    Update {
        mapping_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete an AuthN mapping
    Delete { mapping_id: String },
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

// ---- Change Stories ----
#[derive(Subcommand)]
enum ChangeStoriesActions {
    /// List change stories for a service over a time window
    List {
        #[arg(long, help = "APM service name (required) - no wildcards (*)")]
        service: String,
        #[arg(
            long,
            help = "Start time: 1h, 5min, 2hours, RFC3339, Unix timestamp, or 'now'"
        )]
        from: String,
        #[arg(
            long,
            help = "End time: 1h, 5min, 2hours, RFC3339, Unix timestamp, or 'now'"
        )]
        to: String,
        #[arg(
            long,
            help = "Environment filter (e.g. prod). Omit to include all environments"
        )]
        env: Option<String>,
        #[arg(
            long = "story-types",
            value_delimiter = ',',
            help = "Filter by type(s), comma-separated or repeated. Allowed: deployment, feature_flag, configuration, database, kubernetes, scale, crashloopbackoff, traffic_anomaly, schema, watchdog. Omit for all."
        )]
        story_types: Vec<String>,
        #[arg(
            long = "filter-tags",
            help = "Primary tag (key:value, e.g. datacenter:us1.prod.dog)"
        )]
        filter_tags: Option<String>,
        #[arg(long = "token-limit", help = "Max response tokens (default: 10000)")]
        token_limit: Option<i64>,
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
    /// Manage organization group policies
    Policies {
        #[command(subcommand)]
        action: OrgPoliciesActions,
    },
    /// Manage organization group policy overrides
    #[command(name = "policy-overrides")]
    PolicyOverrides {
        #[command(subcommand)]
        action: OrgPolicyOverridesActions,
    },
    /// List available org group policy config definitions
    #[command(name = "policy-configs")]
    PolicyConfigs {
        #[command(subcommand)]
        action: OrgPolicyConfigsActions,
    },
}

#[derive(Subcommand)]
enum OrgPoliciesActions {
    /// List policies for an org group
    List {
        /// Org group UUID (required)
        #[arg(long = "group-id")]
        group_id: String,
        /// Filter by policy name
        #[arg(long)]
        name: Option<String>,
        /// Page number
        #[arg(long = "page-number")]
        page_number: Option<i64>,
        /// Page size (max 1000)
        #[arg(long = "page-size")]
        page_size: Option<i64>,
        /// Sort (id, -id, name, -name)
        #[arg(long)]
        sort: Option<String>,
    },
    /// Get a policy by ID
    Get {
        /// Policy UUID
        policy_id: String,
    },
    /// Create a policy
    Create {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update a policy
    Update {
        /// Policy UUID
        policy_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete a policy
    Delete {
        /// Policy UUID
        policy_id: String,
    },
}

#[derive(Subcommand)]
enum OrgPolicyOverridesActions {
    /// List policy overrides for an org group
    List {
        /// Org group UUID (required)
        #[arg(long = "group-id")]
        group_id: String,
        /// Filter by policy UUID
        #[arg(long = "policy-id")]
        policy_id: Option<String>,
        /// Page number
        #[arg(long = "page-number")]
        page_number: Option<i64>,
        /// Page size (max 1000)
        #[arg(long = "page-size")]
        page_size: Option<i64>,
        /// Sort (id, -id, org_uuid, -org_uuid)
        #[arg(long)]
        sort: Option<String>,
    },
    /// Get a policy override by ID
    Get {
        /// Override UUID
        override_id: String,
    },
    /// Create a policy override
    Create {
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update a policy override
    Update {
        /// Override UUID
        override_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Delete a policy override
    Delete {
        /// Override UUID
        override_id: String,
    },
}

#[derive(Subcommand)]
enum OrgPolicyConfigsActions {
    /// List available org group policy config definitions
    List,
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
        #[arg(long, default_value_t = 1, help = "Page number (1-indexed)")]
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
        #[arg(
            long,
            name = "project-id",
            help = "Project UUID to assign the case to (optional)"
        )]
        project_id: Option<String>,
        #[arg(long, help = "JSON file with request body (required)", conflicts_with_all = ["title", "type-id", "project-id"])]
        file: Option<String>,
    },
    /// Manage comments on a case
    Comments {
        #[command(subcommand)]
        action: CaseCommentActions,
    },
    /// Get the full timeline for a case (comments, attribute updates, etc.)
    Timeline { case_id: String },
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
    /// Update case description
    #[command(name = "update-description")]
    UpdateDescription {
        case_id: String,
        #[arg(long, help = "New description (required)")]
        description: String,
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
enum CaseCommentActions {
    /// List comments on a case
    List { case_id: String },
    /// Get a single comment by ID
    Get {
        case_id: String,
        #[arg(long, name = "comment-id", help = "Comment UUID (required)")]
        comment_id: String,
    },
    /// Create a comment on a case
    Create {
        case_id: String,
        #[arg(long, help = "Comment body (required)")]
        body: String,
    },
    /// Update a comment's body
    Update {
        case_id: String,
        #[arg(long, name = "comment-id", help = "Comment UUID (required)")]
        comment_id: String,
        #[arg(long, help = "New comment body (required)")]
        body: String,
    },
    /// Delete a comment from a case
    Delete {
        case_id: String,
        #[arg(long, name = "comment-id", help = "Comment UUID (required)")]
        comment_id: String,
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

// ---- Software Catalog ----
#[derive(Subcommand)]
enum SoftwareCatalogActions {
    /// Manage catalog entities
    Entities {
        #[command(subcommand)]
        action: SoftwareCatalogEntityActions,
    },
    /// Manage catalog kinds
    Kinds {
        #[command(subcommand)]
        action: SoftwareCatalogKindActions,
    },
    /// Manage catalog relations
    Relations {
        #[command(subcommand)]
        action: SoftwareCatalogRelationActions,
    },
}

#[derive(Subcommand)]
enum SoftwareCatalogEntityActions {
    /// List catalog entities
    ///
    /// EXAMPLES:
    ///   pup software-catalog entities list --filter shop-
    ///   pup software-catalog entities list --filter-kind service --filter shop-
    ///   pup software-catalog entities list --filter-owner team-claude
    ///   pup software-catalog entities list --filter-ref service:shop-frontend
    #[command(verbatim_doc_comment)]
    List {
        #[arg(long, help = "Filter entities by name (substring match)")]
        filter: Option<String>,
        #[arg(long, help = "Filter entities by kind (e.g. service, datastore)")]
        filter_kind: Option<String>,
        #[arg(long, help = "Filter entities by owner")]
        filter_owner: Option<String>,
        #[arg(
            long,
            help = "Filter entities by reference, e.g. service:shop-frontend"
        )]
        filter_ref: Option<String>,
    },
    /// Create or update entities from a JSON file
    Upsert {
        #[arg(long, help = "JSON file with entity definition (required)")]
        file: String,
    },
    /// Delete an entity
    Delete { entity_id: String },
    /// Preview catalog entities
    Preview,
}

#[derive(Subcommand)]
enum SoftwareCatalogKindActions {
    /// List catalog kinds
    List,
    /// Create or update a kind from a JSON file
    Upsert {
        #[arg(long, help = "JSON file with kind definition (required)")]
        file: String,
    },
    /// Delete a kind
    Delete { kind_id: String },
}

#[derive(Subcommand)]
enum SoftwareCatalogRelationActions {
    /// List catalog relations
    List,
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
        #[arg(long, help = "JSON file with app data (required)")]
        file: String,
    },
    /// Update an App Builder application
    Update {
        /// App ID (UUID)
        #[arg(name = "app-id")]
        app_id: String,
        #[arg(long, help = "JSON file with app data (required)")]
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
            allow_hyphen_values = true,
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
        #[arg(long, help = "JSON file with notebook data (required)")]
        file: String,
    },
    /// Update a notebook (full replace)
    Update {
        notebook_id: i64,
        #[arg(long, help = "JSON file with notebook data (required)")]
        file: String,
    },
    /// Append cells to an existing notebook (reads current notebook first, then appends)
    Edit {
        notebook_id: i64,
        #[arg(
            long,
            help = "JSON file containing an array of cell objects to append (required)"
        )]
        file: String,
    },
    /// Delete a notebook
    Delete { notebook_id: i64 },
    /// Manage annotations on notebook pages
    Annotations {
        #[command(subcommand)]
        action: AnnotationsActions,
    },
}

// ---- RUM ----
#[derive(Subcommand)]
enum RumActions {
    /// Manage RUM applications
    Apps {
        #[command(subcommand)]
        action: RumAppActions,
    },
    /// Aggregate RUM events by facets
    Aggregate {
        #[arg(
            long,
            help = "RUM query filter (e.g. '@type:error @application.name:\"My App\"')"
        )]
        query: Option<String>,
        #[arg(
            long,
            help = "Filter to a specific user by email — prepends @usr.email:<value> to the query"
        )]
        user_email: Option<String>,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(
            long,
            default_value = "count",
            help = "Metrics to compute (comma-separated, e.g. count,avg(@duration))"
        )]
        compute: String,
        #[arg(
            long,
            help = "Fields to group by (comma-separated, e.g. @application.version,@view.name)"
        )]
        group_by: Option<String>,
        #[arg(long, default_value_t = 10, help = "Maximum groups per facet")]
        limit: i32,
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
        #[arg(
            long,
            default_value = "count",
            help = "Aggregation function: count, avg(@duration), sum(@duration), percentile(@duration, 95), etc."
        )]
        compute: String,
        #[arg(long, help = "Group by field (e.g. @test.service)")]
        group_by: Option<String>,
        #[arg(long, default_value_t = 10, help = "Maximum groups")]
        limit: i64,
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
        #[arg(long, default_value = "desc", help = "Sort order: asc or desc")]
        sort: String,
        #[arg(
            long,
            default_value = "pipeline",
            help = "CI event granularity: pipeline, stage, job, or step"
        )]
        level: String,
    },
    /// Aggregate CI/CD events
    Aggregate {
        #[arg(long, help = "Search query (required)")]
        query: String,
        #[arg(long, default_value = "1h", help = "Start time")]
        from: String,
        #[arg(long, default_value = "now", help = "End time")]
        to: String,
        #[arg(
            long,
            default_value = "count",
            help = "Aggregation function: count, avg(@duration), sum(@duration), percentile(@duration, 95), etc."
        )]
        compute: String,
        #[arg(long, help = "Group by field (e.g. @git.branch)")]
        group_by: Option<String>,
        #[arg(long, default_value_t = 10, help = "Maximum groups")]
        limit: i64,
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
        #[arg(
            long,
            allow_hyphen_values = true,
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
    /// Manage escalation policies
    EscalationPolicies {
        #[command(subcommand)]
        action: OnCallEscalationPoliciesActions,
    },
    /// Manage on-call schedules
    Schedules {
        #[command(subcommand)]
        action: OnCallSchedulesActions,
    },
    /// Manage user notification channels
    NotificationChannels {
        #[command(subcommand)]
        action: OnCallNotificationChannelsActions,
    },
    /// Manage user notification rules
    NotificationRules {
        #[command(subcommand)]
        action: OnCallNotificationRulesActions,
    },
    /// Manage on-call pages
    Pages {
        #[command(subcommand)]
        action: OnCallPagesActions,
    },
}

#[derive(Subcommand)]
enum OnCallEscalationPoliciesActions {
    /// Get an escalation policy
    Get { policy_id: String },
    /// Create an escalation policy from a JSON file
    Create {
        #[arg(long, help = "Path to JSON file")]
        file: String,
    },
    /// Update an escalation policy from a JSON file
    Update {
        policy_id: String,
        #[arg(long, help = "Path to JSON file")]
        file: String,
    },
    /// Delete an escalation policy
    Delete { policy_id: String },
}

#[derive(Subcommand)]
enum OnCallSchedulesActions {
    /// Get an on-call schedule
    Get { schedule_id: String },
    /// Create an on-call schedule from a JSON file
    Create {
        #[arg(long, help = "Path to JSON file")]
        file: String,
    },
    /// Update an on-call schedule from a JSON file
    Update {
        schedule_id: String,
        #[arg(long, help = "Path to JSON file")]
        file: String,
    },
    /// Delete an on-call schedule
    Delete { schedule_id: String },
}

#[derive(Subcommand)]
enum OnCallNotificationChannelsActions {
    /// List notification channels for a user
    List { user_id: String },
    /// Get a notification channel for a user
    Get { user_id: String, channel_id: String },
    /// Create a notification channel for a user from a JSON file
    Create {
        user_id: String,
        #[arg(long, help = "Path to JSON file")]
        file: String,
    },
    /// Delete a notification channel for a user
    Delete { user_id: String, channel_id: String },
}

#[derive(Subcommand)]
enum OnCallNotificationRulesActions {
    /// List notification rules for a user
    List { user_id: String },
    /// Get a notification rule for a user
    Get { user_id: String, rule_id: String },
    /// Create a notification rule for a user from a JSON file
    Create {
        user_id: String,
        #[arg(long, help = "Path to JSON file")]
        file: String,
    },
    /// Update a notification rule for a user from a JSON file
    Update {
        user_id: String,
        rule_id: String,
        #[arg(long, help = "Path to JSON file")]
        file: String,
    },
    /// Delete a notification rule for a user
    Delete { user_id: String, rule_id: String },
}

#[derive(Subcommand)]
enum OnCallPagesActions {
    /// List on-call pages, optionally filtered by team handle and/or responder user id
    List {
        #[arg(long, help = "Filter by team handle (server-side)")]
        team: Option<String>,
        #[arg(long, help = "Filter by responder user id (client-side)")]
        responder: Option<String>,
        #[arg(
            long,
            allow_hyphen_values = true,
            value_parser = ["created_at", "-created_at"],
            default_value = "-created_at",
            help = "Sort field (created_at or -created_at; defaults to newest first)"
        )]
        sort: String,
        #[arg(
            long,
            default_value_t = 1000,
            value_parser = clap::value_parser!(u32).range(1..=1000),
            help = "Results per page (1-1000; endpoint pagination is unsupported)"
        )]
        page_size: u32,
    },
    /// Create an on-call page from a JSON file
    Create {
        #[arg(long, help = "Path to JSON file")]
        file: String,
    },
    /// Get an on-call page by ID
    Get { page_id: String },
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
        #[arg(
            long,
            allow_hyphen_values = true,
            default_value = "name",
            help = "Sort: name, -name, email, -email, handle, -handle, manager_name, -manager_name"
        )]
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

// ---- Agentless Scanning ----
#[derive(Subcommand)]
enum AgentlessScanningActions {
    /// Manage AWS agentless scan options and on-demand tasks
    Aws {
        #[command(subcommand)]
        action: AgentlessScanningAwsActions,
    },
    /// Manage Azure agentless scan options
    Azure {
        #[command(subcommand)]
        action: AgentlessScanningAzureActions,
    },
    /// Manage GCP agentless scan options
    Gcp {
        #[command(subcommand)]
        action: AgentlessScanningGcpActions,
    },
}

#[derive(Subcommand)]
enum AgentlessScanningAwsActions {
    /// List AWS scan options
    List,
    /// Get AWS scan options for an account
    Get { account_id: String },
    /// Activate AWS scan options from JSON
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update AWS scan options for an account
    Update {
        account_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete AWS scan options for an account
    Delete { account_id: String },
    /// Manage AWS on-demand scan tasks
    OnDemand {
        #[command(subcommand)]
        action: AgentlessScanningAwsOnDemandActions,
    },
}

#[derive(Subcommand)]
enum AgentlessScanningAwsOnDemandActions {
    /// List AWS on-demand tasks
    List,
    /// Get an AWS on-demand task
    Get { task_id: String },
    /// Create an AWS on-demand task from JSON
    Create {
        #[arg(long)]
        file: String,
    },
}

#[derive(Subcommand)]
enum AgentlessScanningAzureActions {
    /// List Azure scan options
    List,
    /// Get Azure scan options for a subscription
    Get { subscription_id: String },
    /// Activate Azure scan options from JSON
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update Azure scan options for a subscription
    Update {
        subscription_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete Azure scan options for a subscription
    Delete { subscription_id: String },
}

#[derive(Subcommand)]
enum AgentlessScanningGcpActions {
    /// List GCP scan options
    List,
    /// Get GCP scan options for a project
    Get { project_id: String },
    /// Activate GCP scan options from JSON
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update GCP scan options for a project
    Update {
        project_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete GCP scan options for a project
    Delete { project_id: String },
}

// ---- CSM Threats ----
#[derive(Subcommand)]
enum CsmThreatsActions {
    /// Manage CSM Threats agent policies
    AgentPolicies {
        #[command(subcommand)]
        action: CsmThreatsAgentPolicyActions,
    },
    /// Manage CSM Threats agent rules
    AgentRules {
        #[command(subcommand)]
        action: CsmThreatsAgentRuleActions,
    },
    /// Manage workload security backend detection rules
    BackendRules {
        #[command(subcommand)]
        action: CsmThreatsBackendRuleActions,
    },
    /// Manage CSM Threats policy
    Policy {
        #[command(subcommand)]
        action: CsmThreatsPolicyActions,
    },
}

#[derive(Subcommand)]
enum CsmThreatsAgentPolicyActions {
    /// List agent policies
    List,
    /// Get an agent policy
    Get { policy_id: String },
    /// Create an agent policy from JSON
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update an agent policy
    Update {
        policy_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete an agent policy
    Delete { policy_id: String },
}

#[derive(Subcommand)]
enum CsmThreatsAgentRuleActions {
    /// List agent rules
    List {
        #[arg(long, help = "Filter by agent policy ID")]
        policy_id: Option<String>,
    },
    /// Get an agent rule
    Get {
        rule_id: String,
        #[arg(long, help = "Filter by agent policy ID")]
        policy_id: Option<String>,
    },
    /// Create an agent rule from JSON
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update an agent rule
    Update {
        rule_id: String,
        #[arg(long)]
        file: String,
        #[arg(long, help = "Agent policy ID")]
        policy_id: Option<String>,
    },
    /// Delete an agent rule
    Delete {
        rule_id: String,
        #[arg(long, help = "Agent policy ID")]
        policy_id: Option<String>,
    },
}

#[derive(Subcommand)]
enum CsmThreatsBackendRuleActions {
    /// List workload security backend rules
    List {
        #[arg(long, help = "Additional query filter")]
        query: Option<String>,
    },
    /// Get a backend rule
    Get { rule_id: String },
    /// Create a backend rule from JSON
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update a backend rule
    Update {
        rule_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete a backend rule
    Delete { rule_id: String },
    /// Validate a backend rule from JSON
    Validate {
        #[arg(long)]
        file: String,
    },
}

#[derive(Subcommand)]
enum CsmThreatsPolicyActions {
    /// Download the CSM threats policy file
    Download,
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
    /// Manage fleet tracers
    Tracers {
        #[command(subcommand)]
        action: FleetTracerActions,
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
    /// List tracers for a specific agent
    Tracers {
        agent_key: String,
        #[arg(long)]
        page_size: Option<i64>,
        #[arg(long, help = "Page number (0-indexed)")]
        page_number: Option<i64>,
        #[arg(long, help = "Sort by attribute (e.g. service, language, hostname)")]
        sort_attribute: Option<String>,
        #[arg(long, default_value_t = false, help = "Sort descending")]
        sort_descending: bool,
    },
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

#[derive(Subcommand)]
enum FleetTracerActions {
    /// List tracers across the fleet.
    ///
    /// Returns telemetry-derived service names, language, tracer version, and runtime IDs.
    /// Note: service names here come from the SDK telemetry pipeline and may differ from
    /// span-derived names in APM (e.g. pup apm services list).
    List {
        #[arg(
            long,
            help = "Filter query (e.g. env:prod, hostname:my-host, service:web-api)"
        )]
        filter: Option<String>,
        #[arg(long)]
        page_size: Option<i64>,
        #[arg(long, help = "Page number (0-indexed)")]
        page_number: Option<i64>,
        #[arg(long, help = "Sort by attribute (e.g. service, language, hostname)")]
        sort_attribute: Option<String>,
        #[arg(long, default_value_t = false, help = "Sort descending")]
        sort_descending: bool,
    },
}

// ---- Data Deletion ----
#[derive(Subcommand)]
enum DataDeletionActions {
    /// Manage deletion requests
    Requests {
        #[command(subcommand)]
        action: DataDeletionRequestActions,
    },
}

#[derive(Subcommand)]
enum DataDeletionRequestActions {
    /// List data deletion requests
    List {
        #[arg(long, help = "Filter by product (e.g., logs, rum)")]
        product: Option<String>,
        #[arg(long, help = "Filter by query")]
        query: Option<String>,
        #[arg(long, help = "Filter by status")]
        status: Option<String>,
    },
    /// Create a data deletion request from a JSON file
    Create {
        #[arg(long, help = "Product to target (e.g., logs, rum)")]
        product: String,
        #[arg(long)]
        file: String,
    },
    /// Cancel a data deletion request
    Cancel { request_id: String },
}

// ---- Datasets ----
#[derive(Subcommand)]
enum DatasetsActions {
    /// List all datasets
    List,
    /// Get a dataset by ID
    Get { dataset_id: String },
    /// Create a dataset from a JSON file
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update a dataset from a JSON file
    Update {
        dataset_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete a dataset
    Delete { dataset_id: String },
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
        #[arg(
            long,
            help = "Filter by issue state: OPEN, ACKNOWLEDGED, RESOLVED, IGNORED, EXCLUDED"
        )]
        state: Option<String>,
        #[arg(long, help = "Filter by team UUID assignee")]
        team: Option<String>,
        #[arg(long, help = "Filter by user UUID assignee")]
        assignee: Option<String>,
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
    /// Manage traffic allocations for a feature flag in an environment
    Allocations {
        #[command(subcommand)]
        action: FeatureFlagAllocActions,
    },
    /// Manage exposure schedules for a feature flag
    Exposure {
        #[command(subcommand)]
        action: FeatureFlagExposureActions,
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

#[derive(Subcommand)]
enum FeatureFlagAllocActions {
    /// Create allocations for a feature flag in an environment
    Create {
        #[arg(help = "Feature flag ID (UUID)")]
        feature_flag_id: String,
        #[arg(help = "Environment ID (UUID)")]
        environment_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
    /// Update (overwrite) allocations for a feature flag in an environment
    Update {
        #[arg(help = "Feature flag ID (UUID)")]
        feature_flag_id: String,
        #[arg(help = "Environment ID (UUID)")]
        environment_id: String,
        #[arg(long, help = "JSON file with request body (required)")]
        file: String,
    },
}

#[derive(Subcommand)]
enum FeatureFlagExposureActions {
    /// Start an exposure schedule
    Start {
        #[arg(help = "Exposure schedule ID (UUID)")]
        exposure_schedule_id: String,
    },
    /// Stop an exposure schedule
    Stop {
        #[arg(help = "Exposure schedule ID (UUID)")]
        exposure_schedule_id: String,
    },
    /// Pause an exposure schedule
    Pause {
        #[arg(help = "Exposure schedule ID (UUID)")]
        exposure_schedule_id: String,
    },
    /// Resume an exposure schedule
    Resume {
        #[arg(help = "Exposure schedule ID (UUID)")]
        exposure_schedule_id: String,
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
    /// Manage Microsoft Teams integration
    #[command(name = "ms-teams")]
    MsTeams {
        #[command(subcommand)]
        action: MsTeamsActions,
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
    /// Manage Datadog cost and billing data (projected costs, attribution, cloud cost configs)
    Datadog {
        #[command(subcommand)]
        action: CostDatadogActions,
    },
    /// Manage Cloud Cost Management features (custom costs, budgets, commitments, tags)
    #[command(name = "ccm")]
    Ccm {
        #[command(subcommand)]
        action: CostCcmActions,
    },
    /// Manage OCI (Oracle Cloud Infrastructure) cost configs
    #[command(name = "oci-configs")]
    OciConfigs {
        #[command(subcommand)]
        action: CostOciConfigsActions,
    },
    /// Manage Cloud Cost Management anomalies
    Anomalies {
        #[command(subcommand)]
        action: CostAnomaliesActions,
    },
}

#[derive(Subcommand)]
enum CostDatadogActions {
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

#[derive(Subcommand)]
enum CostCcmActions {
    /// Manage custom cost files
    #[command(name = "custom-costs")]
    CustomCosts {
        #[command(subcommand)]
        action: CostCcmCustomCostsActions,
    },
    /// Manage tag descriptions
    #[command(name = "tag-descriptions")]
    TagDescriptions {
        #[command(subcommand)]
        action: CostCcmTagDescActions,
    },
    /// Query tag metadata and coverage analytics
    #[command(name = "tag-metadata")]
    TagMetadata {
        #[command(subcommand)]
        action: CostCcmTagMetadataActions,
    },
    /// List cost tags
    Tags {
        #[command(subcommand)]
        action: CostCcmTagsActions,
    },
    /// Manage cost tag keys
    #[command(name = "tag-keys")]
    TagKeys {
        #[command(subcommand)]
        action: CostCcmTagKeysActions,
    },
    /// Manage cost budgets
    Budgets {
        #[command(subcommand)]
        action: CostCcmBudgetsActions,
    },
    /// Query commitment programs (reserved instances, savings plans)
    Commitments {
        #[command(subcommand)]
        action: CostCcmCommitmentsActions,
    },
}

#[derive(Subcommand)]
enum CostCcmCustomCostsActions {
    /// List custom cost files
    List {
        #[arg(long, help = "Page size (default: 100)")]
        page_size: Option<i64>,
        #[arg(long, help = "Filter by status (UPLOADING, SUCCESS, FAILED)")]
        status: Option<String>,
        #[arg(
            long,
            allow_hyphen_values = true,
            help = "Sort key (prefix with '-' for descending, e.g. '-created_at')"
        )]
        sort: Option<String>,
    },
    /// Get a custom cost file by ID
    Get {
        #[arg(help = "File ID (UUID)")]
        file_id: String,
    },
    /// Upload a custom cost file
    Upload {
        #[arg(long, help = "Path to file to upload (required)")]
        file: String,
        #[arg(long, help = "File format version (default: 1.0)")]
        version: Option<String>,
    },
    /// Delete a custom cost file by ID
    Delete {
        #[arg(help = "File ID (UUID)")]
        file_id: String,
    },
}

#[derive(Subcommand)]
enum CostCcmTagDescActions {
    /// List tag descriptions for the organization
    List {
        #[arg(long, help = "Filter by cloud provider (aws, azure, gcp)")]
        cloud: Option<String>,
    },
    /// Get the description for a specific tag key
    Get {
        #[arg(long, help = "Tag key (required)")]
        tag_key: String,
        #[arg(long, help = "Cloud provider (aws, azure, gcp)")]
        cloud: Option<String>,
    },
    /// AI-generate a description for a tag key
    Generate {
        #[arg(long, help = "Tag key (required)")]
        tag_key: String,
    },
    /// Create or update a tag description
    Upsert {
        #[arg(long, help = "Tag key (required)")]
        tag_key: String,
        #[arg(long, help = "Description text (required)")]
        description: String,
        #[arg(long, help = "Cloud provider (aws, azure, gcp); omit for all clouds")]
        cloud: Option<String>,
    },
    /// Delete a tag description
    Delete {
        #[arg(long, help = "Tag key (required)")]
        tag_key: String,
        #[arg(
            long,
            help = "Cloud provider; omit to delete all org descriptions for this key"
        )]
        cloud: Option<String>,
    },
}

#[derive(Subcommand)]
enum CostCcmTagMetadataActions {
    /// List tag metadata for a month
    List {
        #[arg(long, help = "Month in YYYY-MM format (required)")]
        month: String,
        #[arg(long, help = "Cloud provider (aws, azure, gcp, oci)")]
        provider: Option<String>,
        #[arg(long, help = "Cost metric (e.g. aws.cost.net.amortized)")]
        metric: Option<String>,
        #[arg(long, help = "Tag key filter")]
        tag_key: Option<String>,
        #[arg(long, help = "Return daily granularity instead of monthly")]
        daily: bool,
    },
    /// List tag sources for a month
    #[command(name = "tag-sources")]
    TagSources {
        #[arg(long, help = "Month in YYYY-MM format (required)")]
        month: String,
        #[arg(long, help = "Cloud provider (aws, azure, gcp, oci)")]
        provider: Option<String>,
    },
    /// List available cost metrics for a month
    Metrics {
        #[arg(long, help = "Month in YYYY-MM format (required)")]
        month: String,
        #[arg(long, help = "Cloud provider (aws, azure, gcp, oci)")]
        provider: Option<String>,
    },
    /// List orchestrators active in a month
    Orchestrators {
        #[arg(long, help = "Month in YYYY-MM format (required)")]
        month: String,
        #[arg(long, help = "Cloud provider (aws, azure, gcp, oci)")]
        provider: Option<String>,
    },
    /// Get billing currency for a month
    Currency {
        #[arg(long, help = "Month in YYYY-MM format (required)")]
        month: String,
        #[arg(long, help = "Cloud provider (aws, azure, gcp, oci)")]
        provider: Option<String>,
    },
}

#[derive(Subcommand)]
enum CostCcmTagsActions {
    /// List cost tags
    List {
        #[arg(long, help = "Cost metric filter (e.g. aws.cost.net.amortized)")]
        metric: Option<String>,
        #[arg(long, help = "Match filter for tag values")]
        r#match: Option<String>,
        #[arg(long = "tag", help = "Tag filter (key:value, repeatable)", action = clap::ArgAction::Append)]
        tags: Vec<String>,
    },
}

#[derive(Subcommand)]
enum CostCcmTagKeysActions {
    /// List cost tag keys
    List {
        #[arg(long, help = "Cost metric filter")]
        metric: Option<String>,
        #[arg(long = "tag", help = "Tag filter (key:value, repeatable)", action = clap::ArgAction::Append)]
        tags: Vec<String>,
    },
    /// Get details for a specific tag key
    Get {
        #[arg(help = "Tag key")]
        key: String,
        #[arg(long, help = "Cost metric filter")]
        metric: Option<String>,
    },
}

#[derive(Subcommand)]
enum CostCcmBudgetsActions {
    /// List all budgets
    List,
    /// Get a budget by ID
    Get {
        #[arg(help = "Budget ID (UUID)")]
        budget_id: String,
        #[arg(long, help = "Start time for cost data (e.g. 30d, 2024-01)")]
        start: Option<String>,
        #[arg(long, help = "End time for cost data (e.g. now, 2024-03)")]
        end: Option<String>,
        #[arg(long, help = "Include actual costs")]
        actual: bool,
        #[arg(long, help = "Include forecast costs")]
        forecast: bool,
    },
    /// Create or update a budget from a JSON file
    Upsert {
        #[arg(long, help = "JSON file with budget body (required)")]
        file: String,
    },
    /// Delete a budget by ID
    Delete {
        #[arg(help = "Budget ID (UUID)")]
        budget_id: String,
    },
    /// Validate a budget JSON file without saving
    Validate {
        #[arg(long, help = "JSON file with budget body (required)")]
        file: String,
    },
}

#[derive(Subcommand)]
enum CostCcmCommitmentsActions {
    /// Get utilization scalar metrics for commitment programs
    Utilization {
        #[arg(long, help = "Cloud provider: aws, azure (required)")]
        provider: String,
        #[arg(
            long,
            help = "Cloud product: EC2, RDS, ElastiCache, VirtualMachines, etc. (required)"
        )]
        product: String,
        #[arg(long, help = "Start time (required, e.g. 30d, 2024-01-01)")]
        from: String,
        #[arg(long, help = "End time (required, e.g. now, 2024-01-31)")]
        to: String,
        #[arg(long, help = "Commitment type: RI or SP (default: RI)")]
        commitment_type: Option<String>,
        #[arg(long, help = "Tag filter (key:value syntax)")]
        filter_by: Option<String>,
    },
    /// Get coverage scalar metrics for commitment programs
    Coverage {
        #[arg(long, help = "Cloud provider: aws, azure (required)")]
        provider: String,
        #[arg(long, help = "Cloud product (required)")]
        product: String,
        #[arg(long, help = "Start time (required)")]
        from: String,
        #[arg(long, help = "End time (required)")]
        to: String,
        #[arg(long, help = "Commitment type: RI or SP (default: RI)")]
        commitment_type: Option<String>,
        #[arg(long, help = "Tag filter (key:value syntax)")]
        filter_by: Option<String>,
    },
    /// Get savings scalar metrics for commitment programs
    Savings {
        #[arg(long, help = "Cloud provider: aws, azure (required)")]
        provider: String,
        #[arg(long, help = "Cloud product (required)")]
        product: String,
        #[arg(long, help = "Start time (required)")]
        from: String,
        #[arg(long, help = "End time (required)")]
        to: String,
        #[arg(long, help = "Commitment type: RI or SP (default: RI)")]
        commitment_type: Option<String>,
        #[arg(long, help = "Tag filter (key:value syntax)")]
        filter_by: Option<String>,
    },
    /// Get on-demand hot spots (uncovered spend) for commitment programs
    Hotspots {
        #[arg(long, help = "Cloud provider: aws, azure (required)")]
        provider: String,
        #[arg(long, help = "Cloud product (required)")]
        product: String,
        #[arg(long, help = "Start time (required)")]
        from: String,
        #[arg(long, help = "End time (required)")]
        to: String,
        #[arg(long, help = "Commitment type: RI or SP (default: RI)")]
        commitment_type: Option<String>,
        #[arg(long, help = "Tag filter (key:value syntax)")]
        filter_by: Option<String>,
    },
    /// Get utilization timeseries for commitment programs
    #[command(name = "utilization-ts")]
    UtilizationTs {
        #[arg(long, help = "Cloud provider: aws, azure (required)")]
        provider: String,
        #[arg(long, help = "Cloud product (required)")]
        product: String,
        #[arg(long, help = "Start time (required)")]
        from: String,
        #[arg(long, help = "End time (required)")]
        to: String,
        #[arg(long, help = "Commitment type: RI or SP (default: RI)")]
        commitment_type: Option<String>,
        #[arg(long, help = "Tag filter (key:value syntax)")]
        filter_by: Option<String>,
    },
    /// Get coverage timeseries for commitment programs
    #[command(name = "coverage-ts")]
    CoverageTs {
        #[arg(long, help = "Cloud provider: aws, azure (required)")]
        provider: String,
        #[arg(long, help = "Cloud product (required)")]
        product: String,
        #[arg(long, help = "Start time (required)")]
        from: String,
        #[arg(long, help = "End time (required)")]
        to: String,
        #[arg(long, help = "Commitment type: RI or SP (default: RI)")]
        commitment_type: Option<String>,
        #[arg(long, help = "Tag filter (key:value syntax)")]
        filter_by: Option<String>,
    },
    /// Get savings timeseries for commitment programs
    #[command(name = "savings-ts")]
    SavingsTs {
        #[arg(long, help = "Cloud provider: aws, azure (required)")]
        provider: String,
        #[arg(long, help = "Cloud product (required)")]
        product: String,
        #[arg(long, help = "Start time (required)")]
        from: String,
        #[arg(long, help = "End time (required)")]
        to: String,
        #[arg(long, help = "Commitment type: RI or SP (default: RI)")]
        commitment_type: Option<String>,
        #[arg(long, help = "Tag filter (key:value syntax)")]
        filter_by: Option<String>,
    },
    /// List individual commitments (reserved instances, savings plans)
    List {
        #[arg(long, help = "Cloud provider: aws, azure (required)")]
        provider: String,
        #[arg(long, help = "Cloud product (required)")]
        product: String,
        #[arg(long, help = "Start time (required)")]
        from: String,
        #[arg(long, help = "End time (required)")]
        to: String,
        #[arg(long, help = "Commitment type: RI or SP (default: RI)")]
        commitment_type: Option<String>,
        #[arg(long, help = "Tag filter (key:value syntax)")]
        filter_by: Option<String>,
    },
}

// ---- Cost OCI Configs ----
#[derive(Subcommand)]
enum CostOciConfigsActions {
    /// List OCI cost configs
    List,
}

// ---- Cost Anomalies ----
#[derive(Subcommand)]
enum CostAnomaliesActions {
    /// List detected cost anomalies
    List,
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

// ---- Model Lab ----
#[derive(Subcommand)]
enum ModelLabActions {
    /// Manage Model Lab projects
    Projects {
        #[command(subcommand)]
        action: ModelLabProjectActions,
    },
    /// Manage Model Lab runs
    Runs {
        #[command(subcommand)]
        action: ModelLabRunActions,
    },
}

#[derive(Subcommand)]
enum ModelLabProjectActions {
    /// List Model Lab projects
    List {
        #[arg(long, help = "Filter query string")]
        filter: Option<String>,
        #[arg(long, help = "Filter by tags (comma-separated)")]
        filter_tags: Option<String>,
        #[arg(
            long,
            allow_hyphen_values = true,
            help = "Sort field (e.g. name, created_at)"
        )]
        sort: Option<String>,
        #[arg(long)]
        page_size: Option<i64>,
        #[arg(long, help = "Page number (0-indexed)")]
        page_number: Option<i64>,
    },
    /// Get a Model Lab project by ID
    Get { project_id: i64 },
    /// Star a Model Lab project
    Star { project_id: i64 },
    /// Unstar a Model Lab project
    Unstar { project_id: i64 },
    /// List artifacts for a Model Lab project
    Artifacts { project_id: i64 },
    /// List facet keys for Model Lab projects
    #[command(name = "facet-keys")]
    FacetKeys,
    /// List facet values for a Model Lab project facet
    #[command(name = "facet-values")]
    FacetValues {
        #[arg(help = "Facet type (tag)")]
        facet_type: String,
        #[arg(help = "Facet name")]
        facet_name: String,
    },
}

#[derive(Subcommand)]
enum ModelLabRunActions {
    /// List Model Lab runs
    List {
        #[arg(long, help = "Filter query string")]
        filter: Option<String>,
        #[arg(long, help = "Filter by project ID")]
        filter_project_id: Option<i64>,
        #[arg(
            long,
            help = "Filter by status: pending, running, completed, failed, killed, unresponsive, paused"
        )]
        filter_status: Option<String>,
        #[arg(long, help = "Filter by tags (comma-separated)")]
        filter_tags: Option<String>,
        #[arg(long, help = "Filter by params")]
        filter_params: Option<String>,
        #[arg(long, help = "Filter by parent run ID")]
        filter_parent_run_id: Option<String>,
        #[arg(long, default_value_t = false, help = "Show pinned runs first")]
        pinned_first: bool,
        #[arg(long, default_value_t = false, help = "Include pinned runs")]
        include_pinned: bool,
        #[arg(long, allow_hyphen_values = true, help = "Sort field")]
        sort: Option<String>,
        #[arg(long)]
        page_size: Option<i64>,
        #[arg(long, help = "Page number (0-indexed)")]
        page_number: Option<i64>,
    },
    /// Get a Model Lab run by ID
    Get { run_id: i64 },
    /// Delete a Model Lab run
    Delete { run_id: i64 },
    /// Pin a Model Lab run
    Pin { run_id: i64 },
    /// Unpin a Model Lab run
    Unpin { run_id: i64 },
    /// List artifacts for a Model Lab run
    Artifacts {
        run_id: i64,
        #[arg(long, help = "Filter by artifact path prefix")]
        path: Option<String>,
    },
    /// List facet keys for Model Lab runs
    #[command(name = "facet-keys")]
    FacetKeys {
        #[arg(long, help = "Project ID to scope facet keys")]
        filter_project_id: i64,
    },
    /// List facet values for a Model Lab run facet
    #[command(name = "facet-values")]
    FacetValues {
        #[arg(long, help = "Project ID to scope facet values")]
        filter_project_id: i64,
        #[arg(help = "Facet type (parameter, attribute, tag, metric)")]
        facet_type: String,
        #[arg(help = "Facet name")]
        facet_name: String,
    },
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
    /// Manage APM service remapping rules
    #[command(name = "service-remapping")]
    ServiceRemapping {
        #[command(subcommand)]
        action: ApmServiceRemappingActions,
    },
    /// Manage APM customer sampling rules (per-service per-resource head-based sampling rates).
    /// Backed by Remote Config product APM_TRACING with provenance=customer.
    /// Rules show on traces with `_dd.p.dm:-11` and `ingestion_reason:remote_rule`.
    #[command(name = "sampling-rules")]
    SamplingRules {
        #[command(subcommand)]
        action: ApmSamplingRulesActions,
    },
    /// Manage APM adaptive sampling — onboard services and configure the monthly allotment.
    /// Datadog auto-tunes per-resource sampling rates to fit the configured byte/percent budget.
    /// Generated rules show on traces with `_dd.p.dm:-12` and `ingestion_reason:adaptive_rule`.
    #[command(name = "adaptive-sampling")]
    AdaptiveSampling {
        #[command(subcommand)]
        action: ApmAdaptiveSamplingActions,
    },
    /// View APM service instance configuration
    #[command(name = "service-config")]
    ServiceConfig {
        #[command(subcommand)]
        action: ApmServiceConfigActions,
    },
    /// View APM service library configuration
    #[command(name = "service-library-config")]
    ServiceLibraryConfig {
        #[command(subcommand)]
        action: ApmServiceLibraryConfigActions,
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
        name: String,
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
    /// List instrumentation errors for a host or org-wide
    List {
        #[arg(long, help = "Hostname to query (omit for org-wide results)")]
        hostname: Option<String>,
        #[arg(long, help = "Time window (e.g. 4h, 24h, 1h30m)")]
        timeframe: Option<String>,
        #[arg(
            long,
            help = "Filter by result (success, error, abort, unknown; comma-separated)"
        )]
        result: Option<String>,
    },
}

#[derive(Subcommand)]
enum ApmServiceRemappingActions {
    /// List all service remapping rules
    List,
    /// Create a service remapping rule
    Create {
        #[arg(long, help = "Rule name")]
        name: String,
        #[arg(
            long,
            help = "Filter query (e.g. 'service:my-svc', 'peer.service:*.shopify.com')"
        )]
        filter: String,
        #[arg(
            long,
            value_name = "TYPE",
            help = "Entity type: 0 = instrumented service, 1 = inferred entity (from outbound calls)"
        )]
        rule_type: i64,
        #[arg(
            long,
            help = "New service name — static string or template (e.g. 'new-name', '{{service|^(.+?)\\..*$}}')"
        )]
        value: String,
    },
    /// Get a service remapping rule by ID
    Get {
        #[arg(help = "Rule ID")]
        id: String,
    },
    /// Update a service remapping rule
    Update {
        #[arg(help = "Rule ID")]
        id: String,
        #[arg(long, help = "Rule name")]
        name: String,
        #[arg(
            long,
            help = "Filter query (e.g. 'service:my-svc', 'peer.service:*.shopify.com')"
        )]
        filter: String,
        #[arg(
            long,
            value_name = "TYPE",
            help = "Entity type: 0 = instrumented service, 1 = inferred entity"
        )]
        rule_type: i64,
        #[arg(
            long,
            help = "New service name — static string or template (e.g. 'new-name', '{{service|^(.+?)\\..*$}}')"
        )]
        value: String,
        #[arg(long, help = "Current rule version (from list/get output)")]
        version: i64,
    },
    /// Delete a service remapping rule
    Delete {
        #[arg(help = "Rule ID")]
        id: String,
        #[arg(help = "Rule version (from list output)")]
        version: i64,
    },
}

#[derive(Subcommand)]
enum ApmSamplingRulesActions {
    /// List sampling rules. With `--service` + `--env`, narrows to that target.
    List {
        #[arg(long, help = "Filter by service name (must be combined with --env)")]
        service: Option<String>,
        #[arg(long, help = "Filter by environment (must be combined with --service)")]
        env: Option<String>,
    },
    /// Get a sampling rule config by ID
    Get {
        #[arg(help = "Config ID")]
        id: String,
    },
    /// Create a customer sampling rule for (service, env, resource).
    /// Rate is between 0.0 and 1.0. Anything > 1e-6 is honored.
    Create {
        #[arg(long, help = "Service name (required)")]
        service: String,
        #[arg(
            long,
            help = "Environment (required, must match DD_ENV on the service)"
        )]
        env: String,
        #[arg(
            long,
            help = "Resource glob — `*` matches all resources for the service, or e.g. 'GET /api/users'"
        )]
        resource: String,
        #[arg(long, help = "Sample rate between 0.0 and 1.0", value_parser = parse_sample_rate)]
        sample_rate: f64,
    },
    /// Update an existing sampling rule by ID (replaces all attributes)
    Update {
        #[arg(help = "Config ID")]
        id: String,
        #[arg(long, help = "Service name")]
        service: String,
        #[arg(long, help = "Environment")]
        env: String,
        #[arg(long, help = "Resource glob")]
        resource: String,
        #[arg(long, help = "Sample rate between 0.0 and 1.0", value_parser = parse_sample_rate)]
        sample_rate: f64,
    },
    /// Delete a sampling rule by ID
    Delete {
        #[arg(help = "Config ID")]
        id: String,
    },
}

#[derive(Subcommand)]
enum ApmAdaptiveSamplingActions {
    /// Get the onboarding status. With `--service` and `--env`, returns one entry; otherwise lists all.
    #[command(name = "onboarding-status")]
    OnboardingStatus {
        #[arg(long, help = "Filter by service name (optional)")]
        service: Option<String>,
        #[arg(long, help = "Filter by environment (optional)")]
        env: Option<String>,
    },
    /// Onboard a (service, env) pair to adaptive sampling
    Onboard {
        #[arg(long, help = "Service name (required)")]
        service: String,
        #[arg(long, help = "Environment (required)")]
        env: String,
    },
    /// Offboard a (service, env) pair from adaptive sampling
    Offboard {
        #[arg(long, help = "Service name (required)")]
        service: String,
        #[arg(long, help = "Environment (required)")]
        env: String,
    },
    /// Read the org's adaptive sampling allotment configuration
    #[command(name = "get-allotment")]
    GetAllotment,
    /// Set the org's adaptive sampling allotment. Provide exactly one of --bytes or --percent.
    #[command(name = "set-allotment")]
    SetAllotment {
        #[arg(
            long,
            conflicts_with = "percent",
            help = "Monthly target in bytes (strategy=fixed_target)"
        )]
        bytes: Option<i64>,
        #[arg(
            long,
            conflicts_with = "bytes",
            help = "Percent of total monthly allotment (strategy=percent_total)"
        )]
        percent: Option<f64>,
    },
    /// Check whether the configured allotment is sufficient for current ingestion
    Check,
    /// Preview the allotment Datadog would compute for a strategy without applying it.
    /// Provide exactly one of --bytes or --percent.
    Preview {
        #[arg(
            long,
            conflicts_with = "percent",
            help = "Monthly target in bytes (strategy=fixed_target)"
        )]
        bytes: Option<i64>,
        #[arg(
            long,
            conflicts_with = "bytes",
            help = "Percent of total monthly allotment (strategy=percent_total)"
        )]
        percent: Option<f64>,
    },
}

#[derive(Subcommand)]
enum ApmServiceConfigActions {
    /// Get service instance configuration.
    ///
    /// Note: service_name and env reflect what the SDK telemetry pipeline reports
    /// at runtime and may differ from values in the Service Catalog.
    Get {
        #[arg(long, help = "Service name (required)")]
        service_name: String,
        #[arg(long, help = "Environment filter")]
        env: Option<String>,
        #[arg(
            long,
            help = "Comma-separated list of service instance IDs to filter by"
        )]
        service_instance_ids: Option<String>,
    },
}

#[derive(Subcommand)]
enum ApmServiceLibraryConfigActions {
    /// Get library configuration for a service.
    ///
    /// Note: service_name, env, and language_name reflect what the SDK telemetry
    /// pipeline reports at runtime and may differ from values in the Service Catalog,
    /// which aggregates data from multiple sources (APM spans, USM, infrastructure
    /// tags, manual definitions).
    Get {
        #[arg(long, help = "Service name (required)")]
        service_name: String,
        #[arg(long, help = "Environment filter")]
        env: Option<String>,
        #[arg(long, help = "Language filter (python, java, go, ruby, dotnet, etc.)")]
        language: Option<String>,
        #[arg(
            long,
            default_value_t = false,
            help = "Only show configs with mixed values across instances"
        )]
        mixed: bool,
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

// ---- Network ----
// `List` is a stub (the top-level network monitors API isn't wired yet);
// `Flows`, `Devices`, and `Interfaces` are implemented.
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
    /// Manage LLM Observability annotation queues
    #[command(name = "annotation-queues")]
    AnnotationQueues {
        #[command(subcommand)]
        action: LlmObsAnnotationQueuesActions,
    },
    /// Manage LLM Observability custom evaluator configs
    #[command(name = "eval-config")]
    EvalConfig {
        #[command(subcommand)]
        action: LlmObsEvalConfigActions,
    },
    /// List LLM Observability evaluators
    Evals {
        #[command(subcommand)]
        action: LlmObsEvalsActions,
    },
    /// Explore Topic Discovery (Patterns): span clusters and their topic hierarchy
    Patterns {
        #[command(subcommand)]
        action: LlmObsPatternsActions,
    },
    /// Triage LLM Observability Agent Insights
    #[command(name = "agent-insights")]
    AgentInsights {
        #[command(subcommand)]
        action: LlmObsAgentInsightsActions,
    },
    /// Get canonical model pricing rate cards for cost math
    #[command(name = "model-pricing")]
    ModelPricing {
        #[arg(long, help = "Model provider, e.g. anthropic, openai, aws, azure")]
        provider: Option<String>,
        #[arg(long, help = "Model name or partial model query")]
        model: Option<String>,
        #[arg(long, help = "Maximum number of models to return per page")]
        limit: Option<u32>,
        #[arg(long, help = "Pagination cursor from a prior call")]
        cursor: Option<String>,
    },
}

#[derive(Subcommand)]
enum LlmObsAgentInsightsActions {
    /// List Agent Insights as compact triage cards
    List {
        #[arg(long, help = "Filter by ML application")]
        ml_app: Option<String>,
        #[arg(
            long,
            help = "Filter by lifecycle status: for_review, in_progress, completed, or ignored (server default: for_review)"
        )]
        status: Option<String>,
        #[arg(
            long,
            help = "Filter by insight type, e.g. tool_call_retry_loop (from an insight's insight_type)"
        )]
        insight_type: Option<String>,
        #[arg(long, help = "Maximum number of insights to return per page")]
        limit: Option<u32>,
        #[arg(long, help = "Pagination cursor from a prior call")]
        cursor: Option<String>,
    },
    /// Get one Agent Insight by ID, with its full payload and feedback targets
    Get { insight_id: String },
    /// Move an Agent Insight to a new lifecycle status
    #[command(name = "update-status")]
    UpdateStatus {
        insight_id: String,
        #[arg(
            long,
            help = "New lifecycle status: for_review, in_progress, completed, or ignored (required)"
        )]
        status: String,
    },
    /// Submit usefulness feedback for an Agent Insight's feedback targets
    #[command(name = "submit-feedback")]
    SubmitFeedback {
        insight_id: String,
        #[arg(
            long,
            help = "Feedback as \"target_key=usefulness[=reasoning]\", where target_key is a key from the insight's feedback_targets (these may contain colons, e.g. suggested_evaluator:my-eval) and usefulness is useful, somewhat_useful, or not_useful. Repeat for up to 25 targets; omit targets you cannot judge."
        )]
        feedback: Vec<String>,
    },
}

#[derive(Subcommand)]
enum LlmObsPatternsActions {
    /// Manage Topic Discovery configurations
    Configs {
        #[command(subcommand)]
        action: LlmObsPatternConfigsActions,
    },
    /// Inspect Topic Discovery runs for a config
    Runs {
        #[command(subcommand)]
        action: LlmObsPatternRunsActions,
    },
    /// Get the topic hierarchy discovered by a run
    Topics {
        #[arg(long, help = "Pattern config ID (required)")]
        config_id: String,
        #[arg(long, help = "Specific run ID (defaults to the latest completed run)")]
        run_id: Option<String>,
    },
    /// Get the topic hierarchy with clustering point span IDs inlined on leaf topics
    #[command(name = "topics-with-points")]
    TopicsWithPoints {
        #[arg(long, help = "Pattern config ID (required)")]
        config_id: String,
        #[arg(long, help = "Specific run ID (defaults to the latest completed run)")]
        run_id: Option<String>,
        #[arg(
            long,
            help = "Inline per-span duration, cost, token counts, and evaluations (heavier response)"
        )]
        include_metrics: bool,
    },
    /// Get a page of clustering points (spans) for a single topic
    Points {
        #[arg(long, help = "Topic ID (required)")]
        topic_id: String,
        #[arg(
            long,
            help = "Max points per page (server default applies when omitted)"
        )]
        page_size: Option<u32>,
        #[arg(long, help = "Pagination cursor (next_page_token) from a prior call")]
        page_token: Option<String>,
    },
}

#[derive(Subcommand)]
enum LlmObsPatternConfigsActions {
    /// List all Topic Discovery configs for the org
    List,
    /// Get the most-recently-modified Topic Discovery config for the org
    Get,
}

#[derive(Subcommand)]
enum LlmObsPatternRunsActions {
    /// List completed runs for a config, newest first
    List {
        #[arg(long, help = "Pattern config ID (required)")]
        config_id: String,
    },
    /// Get status and per-activity progress of the most recent run for a config
    Status {
        #[arg(long, help = "Pattern config ID (required)")]
        config_id: String,
    },
}

#[derive(Subcommand)]
enum LlmObsProjectsActions {
    /// Create a new LLM Obs project
    Create {
        #[arg(long, help = "JSON file with project body (required)")]
        file: String,
    },
    /// List LLM Obs projects (filter by ID or name to resolve a single project)
    List {
        #[arg(long, help = "Filter by project ID")]
        filter_id: Option<String>,
        #[arg(long, help = "Filter by project name")]
        filter_name: Option<String>,
        #[arg(long, help = "Maximum number of projects to return per page")]
        limit: Option<u32>,
        #[arg(long, help = "Pagination cursor from a prior call")]
        cursor: Option<String>,
    },
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
    /// Submit evaluation-metric events to an experiment
    Submit {
        experiment_id: String,
        #[arg(
            long,
            help = "JSON array of eval-metric events, e.g. '[{\"label\":\"accuracy\",\"metric_type\":\"score\",\"score_value\":0.9}]' (required)"
        )]
        metrics: String,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Optional \"key:value\" tags applied to every submitted metric (comma-separated)"
        )]
        tags: Option<Vec<String>>,
    },
}

#[derive(Subcommand)]
enum LlmObsSpansActions {
    /// Search LLM Observability spans
    Search {
        #[arg(long, help = "Search query string")]
        query: Option<String>,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Filter by \"key:value\" tags (comma-separated, AND)"
        )]
        tags: Option<Vec<String>>,
        #[arg(long, help = "Filter by trace ID")]
        trace_id: Option<String>,
        #[arg(long, help = "Filter by APM trace ID")]
        apm_trace_id: Option<String>,
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
        #[arg(
            long,
            help = "Return only essential fields (span_id, trace_id, name, kind, status, duration, timestamps) — drops tags, llm_info, and content previews"
        )]
        summary: bool,
    },
    /// Get the full trace structure as a span hierarchy tree
    #[command(name = "get-trace")]
    GetTrace {
        #[arg(long, help = "Trace ID (required)")]
        trace_id: String,
        #[arg(long, help = "Include full span tree structure")]
        include_tree: bool,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time: relative (1h, 30m), RFC3339, or Unix ms"
        )]
        from: String,
        #[arg(
            long,
            default_value = "now",
            help = "End time: relative, RFC3339, or Unix ms"
        )]
        to: String,
    },
    /// Get detailed metadata for one or more spans
    #[command(name = "get-details")]
    GetDetails {
        #[arg(long, help = "Trace ID (required)")]
        trace_id: String,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Span IDs to fetch details for (comma-separated, required)"
        )]
        span_ids: Vec<String>,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time: relative (1h, 30m), RFC3339, or Unix ms"
        )]
        from: String,
        #[arg(
            long,
            default_value = "now",
            help = "End time: relative, RFC3339, or Unix ms"
        )]
        to: String,
    },
    /// Get content fields for a span (input, output, messages, documents, metadata)
    #[command(name = "get-content")]
    GetContent {
        #[arg(long, help = "Trace ID (required)")]
        trace_id: String,
        #[arg(long, help = "Span ID (required)")]
        span_id: String,
        #[arg(
            long,
            help = "Content field to retrieve: input, output, expected_output, messages, documents, metadata"
        )]
        field: String,
        #[arg(long, help = "JSONPath within the field")]
        path: Option<String>,
        #[arg(long, help = "Maximum tokens to return")]
        max_tokens: Option<u32>,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time: relative (1h, 30m), RFC3339, or Unix ms"
        )]
        from: String,
        #[arg(
            long,
            default_value = "now",
            help = "End time: relative, RFC3339, or Unix ms"
        )]
        to: String,
    },
    /// Find all error spans within a trace
    #[command(name = "find-errors")]
    FindErrors {
        #[arg(long, help = "Trace ID (required)")]
        trace_id: String,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time: relative (1h, 30m), RFC3339, or Unix ms"
        )]
        from: String,
        #[arg(
            long,
            default_value = "now",
            help = "End time: relative, RFC3339, or Unix ms"
        )]
        to: String,
    },
    /// Expand children of spans for progressive tree exploration
    Expand {
        #[arg(long, help = "Trace ID (required)")]
        trace_id: String,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Span IDs to expand (comma-separated, required)"
        )]
        span_ids: Vec<String>,
        #[arg(long, help = "Maximum depth to expand")]
        max_depth: Option<u32>,
        #[arg(long, help = "Filter expanded spans by kind (llm, agent, tool, etc.)")]
        filter_kind: Option<String>,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time: relative (1h, 30m), RFC3339, or Unix ms"
        )]
        from: String,
        #[arg(
            long,
            default_value = "now",
            help = "End time: relative, RFC3339, or Unix ms"
        )]
        to: String,
    },
    /// Get the chronological agent execution loop for a trace
    #[command(name = "get-agent-loop")]
    GetAgentLoop {
        #[arg(long, help = "Trace ID (required)")]
        trace_id: String,
        #[arg(long, help = "Starting span ID")]
        span_id: Option<String>,
        #[arg(long, help = "Maximum content length per step")]
        max_content_length: Option<u32>,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time: relative (1h, 30m), RFC3339, or Unix ms"
        )]
        from: String,
        #[arg(
            long,
            default_value = "now",
            help = "End time: relative, RFC3339, or Unix ms"
        )]
        to: String,
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
    /// List LLM Obs datasets for a project (filter by ID or name to resolve one dataset)
    List {
        #[arg(long, help = "Project ID (required)")]
        project_id: String,
        #[arg(long, help = "Filter by dataset ID")]
        filter_id: Option<String>,
        #[arg(long, help = "Filter by dataset name")]
        filter_name: Option<String>,
        #[arg(long, help = "Maximum number of datasets to return per page")]
        limit: Option<u32>,
        #[arg(long, help = "Pagination cursor from a prior call")]
        cursor: Option<String>,
    },
    /// Batch insert, update, and delete records in a dataset
    BatchUpdate {
        #[arg(long, help = "Project ID (required)")]
        project_id: String,
        #[arg(long, help = "Dataset ID (required)")]
        dataset_id: String,
        #[arg(long, help = "JSON file with batch update body (required)")]
        file: String,
    },
    /// Clone a dataset into a new dataset
    Clone {
        #[arg(long, help = "Project ID (required)")]
        project_id: String,
        #[arg(long, help = "Dataset ID to clone (required)")]
        dataset_id: String,
        #[arg(long, help = "JSON file with clone body (required)")]
        file: String,
    },
    /// Restore a dataset to a previous version
    Restore {
        #[arg(long, help = "Project ID (required)")]
        project_id: String,
        #[arg(long, help = "Dataset ID (required)")]
        dataset_id: String,
        #[arg(long, help = "JSON file with restore version body (required)")]
        file: String,
    },
    /// Read ALL dataset records, paging past the preview endpoint's response-size cap
    RecordsAll {
        #[arg(long, help = "Dataset ID (required)")]
        dataset_id: String,
        #[arg(long, help = "Records per page (default 100)")]
        limit: Option<u32>,
    },
    /// Read dataset records (structure-preserving previews + schema summary)
    Records {
        #[arg(long, help = "Project ID (required)")]
        project_id: String,
        #[arg(long, help = "Dataset ID (required)")]
        dataset_id: String,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Fetch specific record IDs (comma-separated)"
        )]
        record_ids: Option<Vec<String>>,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Filter by key:value tags (comma-separated, AND)"
        )]
        tags: Option<Vec<String>>,
        #[arg(long, help = "Canonical record ID (all versions of one record)")]
        canonical_id: Option<String>,
        #[arg(long, help = "Historical dataset version (defaults to current)")]
        dataset_version: Option<i64>,
        #[arg(long, default_value = "10", help = "Max records per page")]
        limit: u32,
        #[arg(long, help = "Pagination cursor from a prior call")]
        cursor: Option<String>,
        #[arg(long, help = "Compute the schema summary aggregate (default true)")]
        compute_schema: Option<bool>,
    },
    /// Add records to a dataset (previews by default; pass --confirm to write)
    #[command(name = "records-add")]
    RecordsAdd {
        #[arg(long, help = "Project ID (required)")]
        project_id: String,
        #[arg(long, help = "Dataset ID (required)")]
        dataset_id: String,
        #[arg(
            long,
            help = "JSON file with an array of records, each with optional input, expected_output, metadata, tags (required)"
        )]
        file: String,
        #[arg(
            long,
            help = "Actually insert the records; without this the endpoint returns a preview and writes nothing"
        )]
        confirm: bool,
        #[arg(
            long,
            help = "Bump the dataset version on insert (default true; set false for in-place edits)"
        )]
        create_new_version: Option<bool>,
    },
    /// Fetch up to 3 records with untrimmed input/expected_output/metadata
    #[command(name = "records-full")]
    RecordsFull {
        #[arg(long, help = "Project ID (required)")]
        project_id: String,
        #[arg(long, help = "Dataset ID (required)")]
        dataset_id: String,
        #[arg(
            long,
            value_delimiter = ',',
            help = "1-3 record IDs to fetch (comma-separated, required)"
        )]
        record_ids: Vec<String>,
    },
}

#[derive(Subcommand)]
enum LlmObsAnnotationQueuesActions {
    /// Create an annotation queue
    Create {
        #[arg(long, help = "JSON file with annotation queue body (required)")]
        file: String,
    },
    /// List annotation queues
    List {
        #[arg(long, help = "Filter by project ID")]
        project_id: Option<String>,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Filter by queue IDs (comma-separated)"
        )]
        queue_ids: Option<Vec<String>>,
    },
    /// Update an annotation queue
    Update {
        #[arg(help = "Annotation queue ID")]
        queue_id: String,
        #[arg(long, help = "JSON file with annotation queue update body (required)")]
        file: String,
    },
    /// Delete an annotation queue
    Delete {
        #[arg(help = "Annotation queue ID")]
        queue_id: String,
    },
    /// Manage interactions within an annotation queue
    Interactions {
        #[command(subcommand)]
        action: LlmObsAnnotationQueueInteractionsActions,
    },
}

#[derive(Subcommand)]
enum LlmObsAnnotationQueueInteractionsActions {
    /// Add interactions to an annotation queue
    Add {
        #[arg(help = "Annotation queue ID")]
        queue_id: String,
        #[arg(long, help = "JSON file with interactions body (required)")]
        file: String,
    },
    /// Remove interactions from an annotation queue
    Delete {
        #[arg(help = "Annotation queue ID")]
        queue_id: String,
        #[arg(long, help = "JSON file with interactions to delete (required)")]
        file: String,
    },
    /// List annotated interactions for an annotation queue
    List {
        #[arg(help = "Annotation queue ID")]
        queue_id: String,
    },
}

#[derive(Subcommand)]
enum LlmObsEvalConfigActions {
    /// Get a custom evaluator config by name
    Get {
        #[arg(help = "Evaluator name")]
        eval_name: String,
    },
    /// Create or update a custom evaluator config by name
    Update {
        #[arg(help = "Evaluator name")]
        eval_name: String,
        #[arg(long, help = "JSON file with evaluator config body (required)")]
        file: String,
    },
    /// Delete a custom evaluator config by name
    Delete {
        #[arg(help = "Evaluator name")]
        eval_name: String,
    },
}

#[derive(Subcommand)]
enum LlmObsEvalsActions {
    /// List all evaluators configured for this org
    List,
    /// List evaluators for a specific ML app
    #[command(name = "list-by-ml-app")]
    ListByMlApp {
        #[arg(long, help = "ML app name (required)")]
        ml_app: String,
    },
    /// Get full evaluator configuration (span filters, sampling, scope) via MCP endpoint
    #[command(name = "get-evaluator")]
    GetEvaluator {
        #[arg(help = "Evaluator name (required)")]
        eval_name: String,
    },
    /// Get pass/fail rates and score distributions for an evaluator over a time window
    #[command(name = "get-aggregate-stats")]
    GetAggregateStats {
        #[arg(help = "Evaluator name (required)")]
        eval_name: String,
        #[arg(long, help = "Filter to a specific ML app")]
        ml_app: Option<String>,
        #[arg(
            long,
            default_value = "1h",
            help = "Start time: relative (1h, 30m), RFC3339, or Unix ms"
        )]
        from: String,
        #[arg(
            long,
            default_value = "now",
            help = "End time: relative, RFC3339, or Unix ms"
        )]
        to: String,
    },
    /// Create or fully replace an LLM-judge evaluator config (full replace semantics)
    #[command(name = "create-or-update")]
    CreateOrUpdate {
        #[arg(help = "Evaluator name (required)")]
        eval_name: String,
        #[arg(long, help = "JSON file with evaluator config body (required)")]
        file: String,
    },
    /// Delete an evaluator by name
    Delete {
        #[arg(help = "Evaluator name (required)")]
        eval_name: String,
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

// ---- Scorecards ----
#[derive(Subcommand)]
enum ScorecardsActions {
    /// List all scorecards
    List,
    /// Manage scorecard rules
    Rules {
        #[command(subcommand)]
        action: ScorecardsRulesActions,
    },
    /// Manage scorecard outcomes
    Outcomes {
        #[command(subcommand)]
        action: ScorecardsOutcomesActions,
    },
    /// Manage scorecard campaigns
    Campaigns {
        #[command(subcommand)]
        action: ScorecardsCampaignsActions,
    },
}

#[derive(Subcommand)]
enum ScorecardsRulesActions {
    /// List scorecard rules
    List,
    /// Create a scorecard rule from a JSON file
    Create {
        #[arg(long, help = "Path to JSON file with rule definition")]
        file: String,
    },
    /// Update a scorecard rule from a JSON file
    Update {
        #[arg(help = "Rule ID to update")]
        rule_id: String,
        #[arg(long, help = "Path to JSON file with updated rule definition")]
        file: String,
    },
    /// Delete a scorecard rule
    Delete {
        #[arg(help = "Rule ID to delete")]
        rule_id: String,
    },
}

#[derive(Subcommand)]
enum ScorecardsOutcomesActions {
    /// List scorecard outcomes
    List,
    /// Create or update outcomes in batch from a JSON file
    #[command(name = "batch-create")]
    BatchCreate {
        #[arg(long, help = "Path to JSON file with outcomes batch")]
        file: String,
    },
}

#[derive(Subcommand)]
enum ScorecardsCampaignsActions {
    /// List all scorecard campaigns
    List,
    /// Get a scorecard campaign by ID
    Get {
        #[arg(help = "Campaign ID to retrieve")]
        campaign_id: String,
    },
    /// Create a scorecard campaign from a JSON file
    Create {
        #[arg(long, help = "Path to JSON file with campaign definition")]
        file: String,
    },
    /// Update a scorecard campaign from a JSON file
    Update {
        #[arg(help = "Campaign ID to update")]
        campaign_id: String,
        #[arg(long, help = "Path to JSON file with updated campaign definition")]
        file: String,
    },
    /// Delete a scorecard campaign
    Delete {
        #[arg(help = "Campaign ID to delete")]
        campaign_id: String,
    },
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
    ///   pup traces search --live --query="service:api"
    ///   pup traces search --live --cursor="<cursor from previous next_action>"
    ///
    /// LIVE SEARCH:
    ///   --live pins the end of the window to "now", so the query lands in
    ///   Datadog's live (recent, unsampled) trace buffer instead of the
    ///   indexed store — useful for viewing very recent traces that haven't
    ///   gone through ingestion sampling yet. Combine with the default
    ///   -timestamp sort and page backwards through older spans with
    ///   --cursor (returned as `next_action` in agent mode).
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
        #[arg(
            long,
            default_value = "now",
            help = "End time (ignored when --live is set, which always ends at now)"
        )]
        to: String,
        #[arg(
            long,
            default_value_t = 50,
            help = "Maximum number of spans to return (1-1000)"
        )]
        limit: i32,
        #[arg(
            long,
            allow_hyphen_values = true,
            default_value = "-timestamp",
            help = "Sort order: timestamp or -timestamp"
        )]
        sort: String,
        #[arg(
            long,
            help = "Pagination cursor from a prior call's next_action, to page backwards through older spans"
        )]
        cursor: Option<String>,
        #[arg(
            long,
            help = "Live search: always end the window at now, querying Datadog's live (recent, unsampled) trace buffer"
        )]
        live: bool,
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
    /// Manage span-based metrics
    ///
    /// Create, list, get, update, and delete metrics generated from span data.
    ///
    /// EXAMPLES:
    ///   pup traces metrics list
    ///   pup traces metrics get my-metric-id
    ///   pup traces metrics create --file metric.json
    ///
    /// AUTHENTICATION:
    ///   Requires either OAuth2 authentication or API keys.
    ///   list/get work with the default apm_read scope. create/update/delete
    ///   require apm_generate_metrics, which is also requested by default.
    #[command(verbatim_doc_comment)]
    Metrics {
        #[command(subcommand)]
        action: SpansMetricsActions,
    },
}

#[derive(Subcommand)]
enum SpansMetricsActions {
    /// List all span-based metrics
    List,
    /// Get a span-based metric by ID
    Get { metric_id: String },
    /// Create a span-based metric from a JSON file
    Create {
        #[arg(long)]
        file: String,
    },
    /// Update a span-based metric from a JSON file
    Update {
        metric_id: String,
        #[arg(long)]
        file: String,
    },
    /// Delete a span-based metric
    Delete { metric_id: String },
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

// ---- Agent ----
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

// ---- Annotations ----
#[derive(Subcommand)]
enum AnnotationsActions {
    /// List annotations for a page within a time window
    List {
        #[arg(long, help = "Page ID (dashboard or notebook page)")]
        page_id: String,
        #[arg(long, help = "Start of time window (Unix epoch seconds)")]
        start: i64,
        #[arg(long, help = "End of time window (Unix epoch seconds)")]
        end: i64,
        #[arg(long, help = "Optional widget ID to filter results")]
        widget_id: Option<String>,
    },
    /// Get all annotations on a page grouped by widget
    GetPage {
        #[arg(long, help = "Page ID (dashboard or notebook page)")]
        page_id: String,
        #[arg(long, help = "Start of time window (Unix epoch seconds)")]
        start: i64,
        #[arg(long, help = "End of time window (Unix epoch seconds)")]
        end: i64,
    },
    /// Create a new annotation from a JSON file
    Create {
        #[arg(long, help = "JSON file with AnnotationCreateRequest body (required)")]
        file: String,
    },
    /// Update an existing annotation
    Update {
        /// Annotation UUID
        annotation_id: uuid::Uuid,
        #[arg(long, help = "JSON file with AnnotationUpdateRequest body (required)")]
        file: String,
    },
    /// Delete an annotation by ID
    Delete {
        /// Annotation UUID
        annotation_id: uuid::Uuid,
    },
}

/// Shared dispatch for annotation subcommands. Annotations attach to a page
/// (`dashboard:<id>` or `notebook:<id>`), so the same actions are exposed under
/// both `pup dashboards annotations` and `pup notebooks annotations`.
async fn run_annotations(cfg: &config::Config, action: AnnotationsActions) -> anyhow::Result<()> {
    match action {
        AnnotationsActions::List {
            page_id,
            start,
            end,
            widget_id,
        } => commands::annotations::list(cfg, &page_id, start, end, widget_id).await,
        AnnotationsActions::GetPage {
            page_id,
            start,
            end,
        } => commands::annotations::get_page(cfg, &page_id, start, end).await,
        AnnotationsActions::Create { file } => commands::annotations::create(cfg, &file).await,
        AnnotationsActions::Update {
            annotation_id,
            file,
        } => commands::annotations::update(cfg, annotation_id, &file).await,
        AnnotationsActions::Delete { annotation_id } => {
            commands::annotations::delete(cfg, annotation_id).await
        }
    }
}

// ---- Skills ----
#[cfg(not(target_arch = "wasm32"))]
#[derive(Subcommand)]
enum SkillsActions {
    /// List available skills, agents, and extensions
    List {
        /// Filter by type: skill, agent, extension
        #[arg(long = "type", name = "type")]
        entry_type: Option<String>,
    },
    /// Install skills, agents, and extensions for one or more platforms
    Install {
        /// Target platform (auto-detected from environment if omitted).
        #[arg(value_enum)]
        platform: Option<skills::SkillsPlatform>,
        /// Install a specific skill, agent, or extension by name
        #[arg(long)]
        name: Option<String>,
        /// Override install directory
        #[arg(long)]
        dir: Option<String>,
        /// Filter by type: skill, agent, extension
        #[arg(long = "type", name = "type")]
        entry_type: Option<String>,
        /// Install into the current project instead of the user-global location
        #[arg(long)]
        project: bool,
    },
    /// Show where skills/agents/extensions would be installed
    Path {
        /// Target platform (auto-detected from environment if omitted).
        #[arg(value_enum)]
        platform: Option<skills::SkillsPlatform>,
        /// Show project-local install paths instead of the user-global default
        #[arg(long)]
        project: bool,
    },
    /// Interact with Datadog's hosted skills over the onboarding API
    ///
    /// Unlike the other `skills` subcommands (which install the statically
    /// bundled skills onto a local assistant), `remote` fetches skills live
    /// from Datadog and records onboarding sessions — an assistant runs `pup`
    /// instead of raw HTTP calls. `list` and `get` are public and work
    /// unauthenticated; `sessions create` requires authentication.
    ///
    /// EXAMPLES:
    ///   pup skills remote list --tag aws
    ///   pup skills remote get aws-integration-setup --intent install
    ///   pup skills remote sessions create --session-id run-123 \
    ///     --skill-id aws-integration-setup --summary "Set up AWS" --status completed
    #[command(verbatim_doc_comment)]
    Remote {
        #[command(subcommand)]
        action: SkillsRemoteActions,
    },
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Subcommand)]
enum SkillsRemoteActions {
    /// List the hosted skills
    List {
        /// Only list skills carrying this tag (repeat to require multiple tags)
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Org ID hint for feature-flag evaluation
        #[arg(long)]
        org_id: Option<i64>,
    },
    /// Fetch a skill's instructions as markdown
    Get {
        /// Skill ID (as returned by `skills remote list`)
        skill_id: String,
        /// Caller intent: explore, install, or reference
        #[arg(long)]
        intent: Option<String>,
        /// Session ID to correlate this fetch with a recorded session
        #[arg(long)]
        session_id: Option<String>,
        /// Org ID hint for feature-flag evaluation
        #[arg(long)]
        org_id: Option<i64>,
    },
    /// Record session outcomes
    Sessions {
        #[command(subcommand)]
        action: SkillsRemoteSessionsActions,
    },
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Subcommand)]
enum SkillsRemoteSessionsActions {
    /// Record a session outcome
    Create {
        /// Session (run) ID
        #[arg(long)]
        session_id: String,
        /// Skill ID touched in this session (repeat for multiple; required unless status is in_progress)
        #[arg(long = "skill-id")]
        skill_ids: Vec<String>,
        /// Short summary of the session outcome
        #[arg(long)]
        summary: String,
        /// Session status: in_progress, completed, failed, or abandoned
        #[arg(long)]
        status: String,
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
    /// Manage custom rulesets
    #[command(name = "custom-rulesets")]
    CustomRulesets {
        #[command(subcommand)]
        action: StaticAnalysisCustomRulesetActions,
    },
    /// Manage custom rules within a ruleset
    #[command(name = "custom-rules")]
    CustomRules {
        #[command(subcommand)]
        action: StaticAnalysisCustomRuleActions,
    },
}

#[derive(Subcommand)]
enum StaticAnalysisCustomRulesetActions {
    /// Get a custom ruleset by name
    Get {
        #[arg(help = "Ruleset name")]
        name: String,
    },
    /// Update a custom ruleset from a JSON file
    Update {
        #[arg(help = "Ruleset name")]
        name: String,
        #[arg(long, help = "Path to JSON file with ruleset definition")]
        file: String,
    },
    /// Delete a custom ruleset
    Delete {
        #[arg(help = "Ruleset name")]
        name: String,
    },
}

#[derive(Subcommand)]
enum StaticAnalysisCustomRuleActions {
    /// Get a custom rule by name
    Get {
        #[arg(help = "Ruleset name")]
        ruleset: String,
        #[arg(help = "Rule name")]
        rule: String,
    },
    /// Create a custom rule from a JSON file
    Create {
        #[arg(help = "Ruleset name")]
        ruleset: String,
        #[arg(long, help = "Path to JSON file with rule definition")]
        file: String,
    },
    /// Delete a custom rule
    Delete {
        #[arg(help = "Ruleset name")]
        ruleset: String,
        #[arg(help = "Rule name")]
        rule: String,
    },
    /// List revisions for a custom rule
    Revisions {
        #[arg(help = "Ruleset name")]
        ruleset: String,
        #[arg(help = "Rule name")]
        rule: String,
    },
    /// Get a specific revision of a custom rule
    Revision {
        #[arg(help = "Ruleset name")]
        ruleset: String,
        #[arg(help = "Rule name")]
        rule: String,
        #[arg(help = "Revision ID")]
        revision_id: String,
    },
}

// ---- Kafka (Experimental) ----
#[derive(Subcommand)]
enum KafkaActions {
    /// Get Kafka topic configuration versions
    #[command(name = "topic-configs")]
    TopicConfigs {
        #[arg(long, help = "Kafka cluster ID")]
        kafka_cluster_id: String,
        #[arg(long, help = "Kafka topic name")]
        topic: String,
    },
    /// Get Kafka broker configuration versions
    #[command(name = "broker-configs")]
    BrokerConfigs {
        #[arg(long, help = "Kafka cluster ID")]
        kafka_cluster_id: String,
        #[arg(long, help = "Broker ID")]
        broker_id: String,
    },
    /// Get Kafka client (producer/consumer) configuration
    #[command(name = "client-configs")]
    ClientConfigs {
        #[arg(long, help = "Kafka cluster ID")]
        kafka_cluster_id: String,
        /// Service to fetch configs for in the form `name:producer` or
        /// `name:consumer`. May be repeated.
        #[arg(long = "service", value_name = "NAME:TYPE", required = true)]
        services: Vec<String>,
    },
    /// Read messages from a Kafka cluster / topic via the Datadog Agent
    ///
    /// Dispatches a read-messages action via Remote Config and polls for the
    /// agent's response. Requires the `data_streams_monitoring_capture_messages`
    /// permission and a Datadog Agent connected to the target cluster.
    /// Rate-limited to 10 calls/minute per user.
    #[command(name = "read-messages", verbatim_doc_comment)]
    ReadMessages {
        #[arg(long, help = "Kafka cluster ID")]
        cluster: String,
        #[arg(long, help = "Kafka topic name")]
        topic: String,
        #[arg(long, help = "Bootstrap servers (host:port,...)")]
        bootstrap_servers: String,
        #[arg(long, help = "Partition to read from (omit for all)")]
        partition: Option<i32>,
        #[arg(long, default_value_t = 0, help = "Offset to start reading from")]
        start_offset: i64,
        #[arg(long, help = "Start timestamp (unix ms); overrides start_offset")]
        start_timestamp: Option<i64>,
        #[arg(long, default_value_t = 10, help = "Max messages to return (<=100)")]
        n_messages_retrieved: u32,
        #[arg(long, default_value_t = 1000, help = "Max messages to scan (<=10000)")]
        max_scanned_messages: u32,
        /// Optional jq-style filter expression evaluated agent-side against each
        /// deserialized message. The message context exposes top-level fields
        /// .key, .value, .headers, .topic, .partition, .offset, and .timestamp;
        /// navigate nested fields with dotted paths (e.g. .value.user.country).
        /// Supported operators: ==, !=, >, <, >=, <=, contains. Combine with
        /// ' and ' / ' or ' (or has higher precedence; or is split first).
        /// String literals must be quoted with " or '. Numeric literals are
        /// parsed as int/float. A bare path (no operator) is an existence
        /// check — true when the field resolves to a non-null value.
        /// Examples:
        ///   '.value.status == "failed"'
        ///   '.value.amount > 100'
        ///   '.headers.tenant == "acme" and .value.priority >= 5'
        ///   '.value.tags contains "urgent"'
        ///   '.value.error' (existence)
        #[arg(long, verbatim_doc_comment)]
        filter: Option<String>,
        #[arg(long, help = "Consumer group ID to use")]
        consumer_group_id: Option<String>,
    },
    /// Get all Schema Registry versions for a subject on a cluster
    #[command(name = "subject-schemas")]
    SubjectSchemas {
        #[arg(long, help = "Kafka cluster ID")]
        kafka_cluster_id: String,
        #[arg(long, help = "Schema Registry subject name")]
        subject: String,
    },
}

// ---- Auth ----
#[derive(Subcommand)]
enum AuthActions {
    /// Login via OAuth2
    Login {
        /// Comma-separated OAuth scopes to request (e.g. dashboards_read,metrics_read).
        /// Replaces the entire scope set -- profile/config/default scopes are not
        /// included unless you list them too. Passed through as-is: pup does not
        /// validate scope names, the OAuth server does (rejects unknown ones).
        /// Conflicts with --extra-scopes. See also --extra-scopes to add to the
        /// normal scope set instead of replacing it.
        #[arg(long, value_name = "SCOPES", conflicts_with = "extra_scopes")]
        scopes: Option<String>,
        /// Comma-separated OAuth scopes to add on top of the normal scope set
        /// (defaults, or profile/config scopes, or read-only scopes with --read-only)
        /// instead of replacing it. Useful for opting into a high-privilege scope pup
        /// doesn't request by default (e.g. user_access_manage, needed for
        /// `pup logs-restriction create/update/delete`) without typing out every
        /// other scope you still want. Conflicts with --scopes.
        #[arg(long, value_name = "SCOPES", conflicts_with = "scopes")]
        extra_scopes: Option<String>,
        /// Request only read-only scopes (excludes write, manage, and org-level scopes).
        /// Shorthand: --ro
        #[arg(long, alias = "ro", visible_alias = "ro")]
        read_only: bool,
        /// Datadog site or literal host to authenticate against.
        /// Standard sites: datadoghq.eu, us3.datadoghq.com, etc.
        /// Vanity/SAML SSO host: mycompany.datadoghq.com (replaces the old --subdomain flag).
        /// Custom gateway: mygateway.example.com or mygateway.example.com:8443.
        /// Overrides DD_SITE env var and config file. Defaults to datadoghq.com.
        #[arg(long, value_name = "SITE")]
        site: Option<String>,
        /// Pin the OAuth callback to one specific port from the DCR redirect allowlist
        /// [8000, 8080, 8888, 9000] instead of scanning. Useful when forwarding a single port
        /// over SSH. If the chosen port is busy login fails — no fallback. Other values are
        /// rejected because the OAuth server will only accept a port that was registered as a
        /// redirect URI during Dynamic Client Registration. Falls back to
        /// PUP_OAUTH_CALLBACK_PORT if unset.
        #[arg(long, value_name = "PORT")]
        callback_port: Option<u16>,
        /// Hint the target org by UUID (sent as `dd_oid`). Skips the org switcher when the
        /// existing browser session already matches and pre-routes SAML/SSO routing for
        /// first-time logins. The value is persisted with the session so subsequent
        /// `pup auth login` invocations re-emit it automatically. The server validates
        /// the UUID; malformed values are rejected there.
        #[arg(long, value_name = "UUID")]
        org_uuid: Option<String>,
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

/// Guidance returned in the agent schema for LLMs that author shell scripts
/// or runbooks the user will execute outside the agent session. Agent mode
/// wraps responses in a `{status, data, metadata}` envelope; outside agent
/// mode, output is raw. Without `--no-agent`, a script tested in-session
/// silently breaks when the user runs it.
fn build_script_authoring_guidance() -> serde_json::Value {
    serde_json::json!({
        "summary": "Agent mode wraps JSON responses in a {status, data, metadata} envelope. Outside agent mode, pup emits the raw payload. Scripts written without --no-agent will see different shapes depending on who runs them.",
        "rule": "When authoring a script, alias, runbook, or any pup command that the user (or CI) will run outside this agent session, append --no-agent so the output format matches what they will see.",
        "examples": [
            "# Agent runs interactively (envelope wrapped):",
            "pup monitors list --tag='env:prod'",
            "",
            "# Agent writes a script for the user (raw output, parity with their shell):",
            "pup --no-agent monitors list --tag='env:prod' | jq '.[].name'"
        ],
        "detection": "Agent mode is on when any of: --agent flag, FORCE_AGENT_MODE=1, or an agent env var (CLAUDECODE, CURSOR_AGENT, CODEX, etc.) is set."
    })
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
                "name": "--no-agent",
                "type": "bool",
                "default": "false",
                "description": "Disable agent mode (overrides auto-detection and --agent)"
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
        "Don't pipe large JSON responses through multiple jq transforms; use query filters at the API level",
        "Don't author scripts for the user without --no-agent; the envelope wrapping in agent mode won't appear when they run it (see script_authoring)"
    ]));

    root.insert("script_authoring".into(), build_script_authoring_guidance());

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
                "name": "--no-agent",
                "type": "bool",
                "default": "false",
                "description": "Disable agent mode (overrides auto-detection and --agent)"
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
        "Don't pipe large JSON responses through multiple jq transforms; use query filters at the API level",
        "Don't author scripts for the user without --no-agent; the envelope wrapping in agent mode won't appear when they run it (see script_authoring)"
    ]));

    root.insert("script_authoring".into(), build_script_authoring_guidance());

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
        || name.ends_with("-add")
        || name == "remove"
        || name == "install"
        || name == "assign"
        || name == "unassign"
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
        || name.ends_with("-create")
        || name == "submit"
        || name.starts_with("submit-")
        || name == "post"
        || name == "send"
        || name == "import"
        || name == "register"
        || name == "unregister"
        || name.contains("delete")
        || name == "patch"
        || name.starts_with("patch-")
        || name == "run"
        || name == "enable"
        || name == "disable"
        || name == "edit"
        || name == "upsert"
        || name == "upload"
        || name == "publish"
        || name == "unpublish"
        || name == "comment"
        || name == "start"
        || name == "stop"
        || name == "pause"
        || name == "resume"
        || name == "generate"
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

    // Prefer long_about so agents see full context (grouping, examples, field references)
    if let Some(desc) = cmd.get_long_about().or_else(|| cmd.get_about()) {
        obj.insert("description".into(), serde_json::json!(desc.to_string()));
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
    fn events_post_is_classified_as_write() {
        // Regression: the "post" verb must count as a write so read-only mode
        // blocks it and the agent schema does not advertise it as read-only.
        assert!(is_write_command_name("post"));

        let schema = get_schema();
        let commands = schema["commands"].as_array().unwrap();
        let cmd = find_command(commands, &["events", "post"]).expect("events post not found");
        assert_eq!(cmd["read_only"].as_bool(), Some(false));
    }

    #[test]
    fn records_add_is_classified_as_write() {
        // "records-add" inserts dataset records, so read-only mode must block it
        // and the agent schema must not advertise it as read-only.
        assert!(is_write_command_name("records-add"));
        // Read-only siblings must stay read-only.
        assert!(!is_write_command_name("records"));
        assert!(!is_write_command_name("records-all"));
        assert!(!is_write_command_name("records-full"));

        let schema = get_schema();
        let commands = schema["commands"].as_array().unwrap();
        let cmd = find_command(commands, &["llm-obs", "datasets", "records-add"])
            .expect("llm-obs datasets records-add not found");
        assert_eq!(cmd["read_only"].as_bool(), Some(false));
        let full = find_command(commands, &["llm-obs", "datasets", "records-full"])
            .expect("llm-obs datasets records-full not found");
        assert_eq!(full["read_only"].as_bool(), Some(true));
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

    #[test]
    fn global_flags_include_no_agent() {
        let schema = get_schema();
        let flags = schema["global_flags"]
            .as_array()
            .expect("global_flags missing");
        assert!(
            flags
                .iter()
                .any(|f| f["name"].as_str() == Some("--no-agent")),
            "global_flags must include --no-agent"
        );
    }

    #[test]
    fn no_agent_flag_in_scoped_schema() {
        let cmd = Cli::command();
        let monitors_cmd = cmd
            .get_subcommands()
            .find(|s| s.get_name() == "monitors")
            .expect("monitors subcommand not found");
        let schema = build_agent_schema_scoped(&cmd, monitors_cmd, &["monitors"]);
        let flags = schema["global_flags"]
            .as_array()
            .expect("global_flags missing");
        assert!(
            flags
                .iter()
                .any(|f| f["name"].as_str() == Some("--no-agent")),
            "scoped schema global_flags must include --no-agent"
        );
    }

    /// Assert that a `script_authoring` JSON block has the full contract:
    /// summary + rule + examples + detection, with rule mentioning `--no-agent`.
    /// Shared between the top-level and scoped schema tests.
    fn assert_script_authoring_contract(block: &serde_json::Value) {
        assert!(
            block.is_object(),
            "script_authoring must be an object: {block}"
        );
        let summary = block["summary"]
            .as_str()
            .expect("script_authoring.summary must be a string");
        assert!(
            !summary.is_empty(),
            "script_authoring.summary must not be empty"
        );
        let rule = block["rule"]
            .as_str()
            .expect("script_authoring.rule must be a string");
        assert!(
            rule.contains("--no-agent"),
            "script_authoring.rule must mention --no-agent: {rule}"
        );
        assert!(
            block["examples"].is_array(),
            "script_authoring.examples must be an array"
        );
        let detection = block["detection"]
            .as_str()
            .expect("script_authoring.detection must be a string");
        assert!(
            !detection.is_empty(),
            "script_authoring.detection must not be empty"
        );
    }

    /// Top-level schema must surface the script-authoring guidance so that
    /// LLMs reading `pup --help` in agent mode know to pass `--no-agent`
    /// when writing scripts the user will run later. Without this, an
    /// agent's script gets the envelope wrapping the user won't see.
    #[test]
    fn schema_includes_script_authoring_guidance() {
        let schema = get_schema();
        assert_script_authoring_contract(&schema["script_authoring"]);
    }

    /// Scoped (per-domain) schema must also include the guidance so an
    /// agent that only ran `pup logs --help` still gets the warning.
    #[test]
    fn scoped_schema_includes_script_authoring_guidance() {
        let cmd = Cli::command();
        let logs_cmd = cmd
            .get_subcommands()
            .find(|s| s.get_name() == "logs")
            .expect("logs subcommand not found");
        let schema = build_agent_schema_scoped(&cmd, logs_cmd, &["logs"]);
        assert_script_authoring_contract(&schema["script_authoring"]);
    }

    /// The anti-patterns array should include a pointer to the new
    /// script_authoring section so LLMs that scan anti_patterns first
    /// are still led to the full guidance. Match the actual phrasing
    /// (`see script_authoring`) so an unrelated future entry that
    /// merely contains the word doesn't accidentally satisfy this test.
    #[test]
    fn schema_anti_patterns_reference_script_authoring() {
        let schema = get_schema();
        let anti = schema["anti_patterns"]
            .as_array()
            .expect("anti_patterns missing");
        assert!(
            anti.iter().any(|v| v
                .as_str()
                .is_some_and(|s| s.contains("see script_authoring"))),
            "anti_patterns must reference script_authoring so LLMs find it"
        );
    }

    /// Same as above for the scoped schema — agents that only ever call
    /// `pup logs --help` should still get pointed at script_authoring.
    #[test]
    fn scoped_schema_anti_patterns_reference_script_authoring() {
        let cmd = Cli::command();
        let logs_cmd = cmd
            .get_subcommands()
            .find(|s| s.get_name() == "logs")
            .expect("logs subcommand not found");
        let schema = build_agent_schema_scoped(&cmd, logs_cmd, &["logs"]);
        let anti = schema["anti_patterns"]
            .as_array()
            .expect("scoped anti_patterns missing");
        assert!(
            anti.iter().any(|v| v
                .as_str()
                .is_some_and(|s| s.contains("see script_authoring"))),
            "scoped anti_patterns must reference script_authoring"
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

/// Whether a command is exempt from the read-only write guard because it only
/// touches local state, never the Datadog API. `auth`/`alias` are always local.
/// `skills` installs bundled files locally and is exempt too — except
/// `skills remote`, which calls the onboarding API and can write.
pub(crate) fn is_read_only_exempt(matches: &clap::ArgMatches) -> bool {
    match get_top_level_subcommand_name(matches).as_deref() {
        Some("auth") | Some("alias") => true,
        Some("skills") => !matches!(
            matches.subcommand().and_then(|(_, m)| m.subcommand()),
            Some(("remote", _))
        ),
        _ => false,
    }
}

/// Resolve OAuth scopes for `pup auth login`.
///
/// Priority: CLI --scopes (full replacement) > config profile scopes > config
/// top-level scopes > defaults, then --extra-scopes are appended on top of
/// whichever of those was chosen (clap enforces --scopes and --extra-scopes
/// are mutually exclusive, so this never has to reconcile both against a
/// replacement).
///
/// All explicit scopes (--scopes, --extra-scopes, or config) are passed through
/// as-is, unvalidated -- the OAuth server is the source of truth for which
/// scopes this client can request and rejects the rest with `invalid_scope`.
/// This is a deliberate escape hatch: it lets a user opt into a scope pup
/// doesn't request by default (e.g. high-privilege scopes like
/// user_access_manage, intentionally excluded from default_scopes()) the
/// moment it's enabled server-side, without waiting on a pup release.
///
/// In --read-only mode, config/default scopes are filtered to read-only-safe
/// scopes before --extra-scopes are appended; --scopes is passed through as-is
/// regardless (user intent is explicit).
#[cfg(not(target_arch = "wasm32"))]
fn resolve_login_scopes(
    cli_scopes: Option<&str>,
    extra_scopes: Option<&str>,
    org: Option<&str>,
    read_only: bool,
) -> Vec<String> {
    use crate::auth::types::{default_scopes, read_only_scopes};

    if let Some(raw) = cli_scopes {
        return crate::config::parse_scopes(raw);
    }

    // Load from config file (per-org profile, then top-level scopes)
    let mut base = if let Some(configured) = crate::config::load_configured_scopes(org) {
        if read_only {
            configured
                .into_iter()
                .filter(|s| read_only_scopes().contains(&s.as_str()))
                .collect()
        } else {
            configured
        }
    } else if read_only {
        // Default scopes, filtered for read-only if needed
        read_only_scopes().into_iter().map(String::from).collect()
    } else {
        default_scopes().into_iter().map(String::from).collect()
    };

    if let Some(raw) = extra_scopes {
        for scope in crate::config::parse_scopes(raw) {
            if !base.contains(&scope) {
                base.push(scope);
            }
        }
    }

    base
}

#[cfg(target_arch = "wasm32")]
fn resolve_login_scopes(
    _cli_scopes: Option<&str>,
    _extra_scopes: Option<&str>,
    _org: Option<&str>,
    _read_only: bool,
) -> Vec<String> {
    crate::auth::types::default_scopes()
        .into_iter()
        .map(String::from)
        .collect()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod resolve_login_scopes_tests {
    use super::resolve_login_scopes;
    use crate::test_utils::ENV_LOCK;

    /// Points PUP_CONFIG_DIR at a directory with no config file, so
    /// `load_configured_scopes` returns None and defaults/CLI scopes are exercised.
    fn with_no_config_file<T>(f: impl FnOnce() -> T) -> T {
        let _g = ENV_LOCK.blocking_lock();
        std::env::set_var(
            "PUP_CONFIG_DIR",
            "/tmp/pup_test_resolve_login_scopes_nonexistent",
        );
        let result = f();
        std::env::remove_var("PUP_CONFIG_DIR");
        result
    }

    #[test]
    fn cli_scopes_pass_through_unfiltered() {
        // Includes a scope pup doesn't know about -- the OAuth server validates,
        // not pup, so it should NOT be dropped client-side.
        let got = with_no_config_file(|| {
            resolve_login_scopes(
                Some("logs_read_config,user_access_manage,some_future_scope"),
                None,
                None,
                false,
            )
        });
        assert_eq!(
            got,
            vec![
                "logs_read_config",
                "user_access_manage",
                "some_future_scope"
            ]
            .into_iter()
            .map(String::from)
            .collect::<Vec<String>>()
        );
    }

    #[test]
    fn cli_scopes_pass_through_even_in_read_only_mode() {
        let got =
            with_no_config_file(|| resolve_login_scopes(Some("monitors_write"), None, None, true));
        assert_eq!(got, vec!["monitors_write".to_string()]);
    }

    #[test]
    fn no_cli_or_config_scopes_falls_back_to_defaults() {
        let got = with_no_config_file(|| resolve_login_scopes(None, None, None, false));
        let want: Vec<String> = crate::auth::types::default_scopes()
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn no_cli_or_config_scopes_read_only_falls_back_to_read_only_scopes() {
        let got = with_no_config_file(|| resolve_login_scopes(None, None, None, true));
        let want: Vec<String> = crate::auth::types::read_only_scopes()
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn extra_scopes_append_to_defaults() {
        let got = with_no_config_file(|| {
            resolve_login_scopes(
                None,
                Some("user_access_manage,some_future_scope"),
                None,
                false,
            )
        });
        let mut want: Vec<String> = crate::auth::types::default_scopes()
            .into_iter()
            .map(String::from)
            .collect();
        want.push("user_access_manage".to_string());
        want.push("some_future_scope".to_string());
        assert_eq!(got, want);
    }

    #[test]
    fn extra_scopes_append_to_read_only_defaults() {
        let got = with_no_config_file(|| {
            resolve_login_scopes(None, Some("user_access_manage"), None, true)
        });
        let mut want: Vec<String> = crate::auth::types::read_only_scopes()
            .into_iter()
            .map(String::from)
            .collect();
        want.push("user_access_manage".to_string());
        assert_eq!(got, want);
    }

    #[test]
    fn extra_scopes_does_not_duplicate_scope_already_in_base() {
        // logs_read_config is already a default scope -- adding it again via
        // --extra-scopes should be a no-op, not a duplicate entry.
        let got = with_no_config_file(|| {
            resolve_login_scopes(None, Some("logs_read_config"), None, false)
        });
        let want: Vec<String> = crate::auth::types::default_scopes()
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(got, want);
    }
}

/// Resolve the OAuth callback port. CLI flag wins over `PUP_OAUTH_CALLBACK_PORT`;
/// when both are unset, returns `None` so the callback server scans the DCR
/// allowlist as before. The port must be one of `DCR_REDIRECT_PORTS` — those
/// are the redirect URIs registered with the OAuth server during DCR, so any
/// other port would be rejected by the authorize endpoint anyway. The flag
/// just lets the user *pick which one* to pin, for SSH-forwarding setups that
/// can only forward a single port.
fn resolve_callback_port(cli: Option<u16>) -> anyhow::Result<Option<u16>> {
    if let Some(port) = cli {
        return validate_callback_port(port, "--callback-port").map(Some);
    }
    match std::env::var("PUP_OAUTH_CALLBACK_PORT") {
        Ok(s) if s.is_empty() => Ok(None),
        Ok(s) => {
            let port: u16 = s.parse().map_err(|e| {
                anyhow::anyhow!(
                    "invalid PUP_OAUTH_CALLBACK_PORT={s:?}: {e} (expected one of {:?})",
                    crate::auth::dcr::DCR_REDIRECT_PORTS
                )
            })?;
            validate_callback_port(port, "PUP_OAUTH_CALLBACK_PORT").map(Some)
        }
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            anyhow::bail!("PUP_OAUTH_CALLBACK_PORT contains non-UTF8 bytes; refusing to proceed")
        }
    }
}

fn validate_callback_port(port: u16, source: &str) -> anyhow::Result<u16> {
    if !crate::auth::dcr::DCR_REDIRECT_PORTS.contains(&port) {
        anyhow::bail!(
            "{source} value {port} is not in the DCR redirect allowlist {:?}; \
             the OAuth server will reject any other port",
            crate::auth::dcr::DCR_REDIRECT_PORTS
        );
    }
    Ok(port)
}

fn parse_sample_rate(s: &str) -> anyhow::Result<f64> {
    let rate: f64 = s
        .parse()
        .map_err(|_| anyhow::anyhow!("--sample-rate must be a number between 0.0 and 1.0"))?;
    if !(0.0..=1.0).contains(&rate) {
        anyhow::bail!("--sample-rate {rate} is out of range; must be between 0.0 and 1.0");
    }
    Ok(rate)
}

#[cfg(test)]
mod resolve_callback_port_tests {
    use super::resolve_callback_port;
    use crate::test_utils::ENV_LOCK;

    #[test]
    fn cli_flag_takes_precedence_over_env() {
        let _g = ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_OAUTH_CALLBACK_PORT", "9000");
        let got = resolve_callback_port(Some(8000)).unwrap();
        std::env::remove_var("PUP_OAUTH_CALLBACK_PORT");
        assert_eq!(got, Some(8000));
    }

    #[test]
    fn env_used_when_flag_unset() {
        let _g = ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_OAUTH_CALLBACK_PORT", "8888");
        let got = resolve_callback_port(None).unwrap();
        std::env::remove_var("PUP_OAUTH_CALLBACK_PORT");
        assert_eq!(got, Some(8888));
    }

    #[test]
    fn returns_none_when_neither_set() {
        let _g = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_OAUTH_CALLBACK_PORT");
        assert_eq!(resolve_callback_port(None).unwrap(), None);
    }

    #[test]
    fn empty_env_treated_as_unset() {
        let _g = ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_OAUTH_CALLBACK_PORT", "");
        let got = resolve_callback_port(None).unwrap();
        std::env::remove_var("PUP_OAUTH_CALLBACK_PORT");
        assert_eq!(got, None);
    }

    #[test]
    fn malformed_env_errors() {
        let _g = ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_OAUTH_CALLBACK_PORT", "not-a-port");
        let got = resolve_callback_port(None);
        std::env::remove_var("PUP_OAUTH_CALLBACK_PORT");
        assert!(got.is_err(), "malformed env should error, got: {got:?}");
    }

    #[test]
    fn out_of_range_env_errors() {
        let _g = ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_OAUTH_CALLBACK_PORT", "70000");
        let got = resolve_callback_port(None);
        std::env::remove_var("PUP_OAUTH_CALLBACK_PORT");
        assert!(got.is_err(), "out-of-range port should error, got: {got:?}");
    }

    #[test]
    fn cli_port_outside_dcr_allowlist_rejected() {
        // We only let the user pick *which* of the four DCR-registered ports
        // to pin. Anything else would be rejected by the OAuth server (the
        // redirect URI wasn't registered) and leave the user staring at a
        // confusing OAuth error after consenting in the browser.
        let _g = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_OAUTH_CALLBACK_PORT");
        let got = resolve_callback_port(Some(7777));
        assert!(
            got.is_err(),
            "non-allowlist port should be rejected, got: {got:?}"
        );
        let msg = format!("{}", got.unwrap_err());
        assert!(
            msg.contains("DCR redirect allowlist") && msg.contains("7777"),
            "error should name the bad port and the allowlist: {msg}"
        );
    }

    #[test]
    fn cli_port_zero_rejected() {
        // Port 0 means "OS pick an ephemeral port" to bind(2). Caught by the
        // allowlist check (0 isn't in DCR_REDIRECT_PORTS) — but pin the
        // behavior so a future allowlist relaxation doesn't accidentally
        // unblock it.
        let _g = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_OAUTH_CALLBACK_PORT");
        let got = resolve_callback_port(Some(0));
        assert!(got.is_err(), "port 0 should be rejected, got: {got:?}");
    }

    #[test]
    fn env_port_zero_rejected() {
        let _g = ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_OAUTH_CALLBACK_PORT", "0");
        let got = resolve_callback_port(None);
        std::env::remove_var("PUP_OAUTH_CALLBACK_PORT");
        assert!(got.is_err(), "env port 0 should be rejected, got: {got:?}");
    }

    #[test]
    fn site_flag_accepts_valid_hosts() {
        use crate::config::validate_site;
        for good in [
            "datadoghq.com",
            "us3.datadoghq.com",
            "mycompany.datadoghq.com",
            "mygateway.example.com",
            "gw.example.com:8443",
            "host-1.example.com",
        ] {
            assert!(validate_site(good).is_ok(), "{good:?} should be accepted");
        }
    }

    #[test]
    fn site_flag_rejects_url_smuggling_attempts() {
        use crate::config::validate_site;
        // These values would redirect API or OAuth calls to an attacker-controlled
        // host if allowed through URL construction unchecked.
        for bad in [
            "evil.com/path",
            "a#b",
            "user@host",
            "dd staging",
            "dd_staging", // underscore not in DNS label charset
            "../../etc",
            "",
        ] {
            let err = validate_site(bad).unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("invalid characters") || msg.contains("empty") || msg.contains("dot"),
                "{bad:?} should be rejected, got: {msg}"
            );
        }
    }

    #[test]
    fn each_dcr_redirect_port_is_accepted() {
        // The four DCR-registered ports must all parse successfully via both
        // the CLI flag and the env var path. If a port gets dropped from the
        // allowlist this test will fail loudly rather than letting a silent
        // backend mismatch surface only at OAuth callback time.
        let _g = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_OAUTH_CALLBACK_PORT");
        for &port in crate::auth::dcr::DCR_REDIRECT_PORTS {
            assert_eq!(
                resolve_callback_port(Some(port)).unwrap(),
                Some(port),
                "CLI port {port} should be accepted"
            );
            std::env::set_var("PUP_OAUTH_CALLBACK_PORT", port.to_string());
            let env_got = resolve_callback_port(None).unwrap();
            std::env::remove_var("PUP_OAUTH_CALLBACK_PORT");
            assert_eq!(env_got, Some(port), "env port {port} should be accepted");
        }
    }
}

/// Resolve the effective output format. An explicit `--output` flag wins and is
/// validated — a malformed value is a hard error rather than being silently
/// ignored. When the flag is absent, the format already resolved from env/config
/// in `Config::from_env` is kept, so `DD_OUTPUT` / `PUP_OUTPUT` (and the format an
/// extension inherits from its parent) survive.
fn resolve_output_format(
    flag: Option<&str>,
    resolved: config::OutputFormat,
) -> anyhow::Result<config::OutputFormat> {
    match flag {
        Some(s) => s
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid --output value: {e}")),
        None => Ok(resolved),
    }
}

#[cfg(test)]
mod resolve_output_format_tests {
    use super::resolve_output_format;
    use crate::config::OutputFormat;

    #[test]
    fn explicit_flag_overrides_resolved() {
        let got = resolve_output_format(Some("table"), OutputFormat::Json).unwrap();
        assert_eq!(got, OutputFormat::Table);
    }

    #[test]
    fn absent_flag_keeps_resolved() {
        // No --output: the env/config-resolved format (e.g. an inherited
        // PUP_OUTPUT) must survive instead of being clobbered by a default.
        let got = resolve_output_format(None, OutputFormat::Yaml).unwrap();
        assert_eq!(got, OutputFormat::Yaml);
    }

    #[test]
    fn invalid_explicit_flag_errors() {
        let got = resolve_output_format(Some("tabel"), OutputFormat::Json);
        assert!(
            got.is_err(),
            "a malformed --output value must be a hard error"
        );
    }
}

async fn main_inner() -> anyhow::Result<()> {
    // In agent mode, intercept --help to return a JSON schema instead of plain text.
    let args: Vec<String> = std::env::args().collect();
    let has_help = args.iter().any(|a| a == "--help" || a == "-h");
    let has_agent_flag = args.iter().any(|a| a == "--agent");
    let has_no_agent_flag = args.iter().any(|a| a == "--no-agent");
    if has_help && !has_no_agent_flag && (useragent::is_agent_mode() || has_agent_flag) {
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
                    parsed.globals.apply_to(&mut cfg)?;
                    #[cfg(not(feature = "browser"))]
                    {
                        let interactive = std::io::stdin().is_terminal() && !cfg.agent_mode;
                        let trusted_sites = config::configured_trusted_sites();
                        cfg.ensure_site_trusted(
                            parsed.globals.trust_site,
                            interactive,
                            &trusted_sites,
                        )?;
                    }
                    let exit_code = extensions::exec_extension(&ext_path, &parsed.ext_args, &cfg)?;
                    std::process::exit(exit_code);
                }
            }
        }
    }

    // --- Alias expansion (before clap parsing) ---
    // If the first positional arg matches a stored alias, rewrite args so
    // that clap sees the expanded command instead of the alias name.
    #[cfg(not(target_arch = "wasm32"))]
    let args = {
        let parsed = extensions::parse_extension_args(&args);
        if let Some(ref candidate) = parsed.candidate {
            commands::alias::expand(&args, candidate)
        } else {
            args
        }
    };

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
    let matches = cmd.get_matches_from(&args);
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
    if let Commands::Docs { action } = cli.command {
        match action {
            DocsActions::Ask { question } => {
                commands::docs::ask(&question, &mut std::io::stdout()).await?;
            }
        }
        return Ok(());
    }

    let mut cfg = config::Config::from_env()?;

    // Apply flag overrides. `--output` is only applied when explicitly set so the
    // format resolved from DD_OUTPUT / PUP_OUTPUT in `from_env` survives when no
    // flag is passed (extensions rely on inheriting the parent's format).
    cfg.output_format = resolve_output_format(cli.output.as_deref(), cfg.output_format)?;
    if cli.yes {
        cfg.auto_approve = true;
    }
    cfg.agent_mode = !cli.no_agent && (cli.agent || useragent::is_agent_mode());
    if cfg.agent_mode {
        cfg.auto_approve = true;
    }
    // Apply --org flag (higher priority than DD_ORG env var / config file).
    // Site for this org and access token are also resolved here.
    if let Some(org) = cli.org {
        #[cfg(all(not(feature = "browser"), not(target_arch = "wasm32")))]
        config::apply_org_override(&mut cfg, org)?;
        #[cfg(any(feature = "browser", target_arch = "wasm32"))]
        {
            cfg.org = Some(org);
        }
    }

    if cli.read_only {
        cfg.read_only = true;
    }
    if cli.jq.is_some() {
        cfg.jq = cli.jq;
    }
    if cfg.read_only && !is_read_only_exempt(&matches) {
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

    // Compute once, after agent_mode is resolved (agent mode is always non-interactive).
    #[cfg(not(target_arch = "wasm32"))]
    let interactive = std::io::stdin().is_terminal() && !cfg.agent_mode;
    #[cfg(target_arch = "wasm32")]
    let interactive = false;

    // Durable trust allowlist from the config file, sourced once for all gate calls.
    #[cfg(not(feature = "browser"))]
    let trusted_sites = config::configured_trusted_sites();

    // Gate credential dispatch to non-Datadog hosts before any command dispatches.
    // Auth subcommands resolve their own site late (after --site is applied in-arm),
    // so they gate themselves; skip here to avoid a double prompt.
    #[cfg(not(feature = "browser"))]
    if !matches!(cli.command, Commands::Auth { .. }) {
        cfg.ensure_site_trusted(cli.trust_site, interactive, &trusted_sites)?;
    }

    match cli.command {
        // --- Monitors ---
        Commands::Monitors { action } => {
            cfg.validate_auth()?;
            match action {
                MonitorActions::List {
                    name,
                    tags,
                    limit,
                    page,
                } => {
                    commands::monitors::list(&cfg, name, tags, limit, page).await?;
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
                MonitorActions::Search {
                    query,
                    page,
                    per_page,
                    sort,
                } => {
                    commands::monitors::search(&cfg, query, page, per_page, sort).await?;
                }
                MonitorActions::Delete { monitor_id } => {
                    commands::monitors::delete(&cfg, monitor_id).await?;
                }
                MonitorActions::Diff {
                    monitor_id,
                    file,
                    only,
                    ignore,
                } => {
                    commands::monitors::diff(&cfg, monitor_id, &file, &only, &ignore).await?;
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
                    index,
                    storage,
                } => {
                    commands::logs::search(
                        &cfg,
                        commands::logs::SearchArgs {
                            query,
                            from,
                            to,
                            limit,
                            sort,
                            storage,
                            index,
                        },
                    )
                    .await?;
                }
                LogActions::List {
                    query,
                    from,
                    to,
                    limit,
                    sort,
                    storage,
                    index,
                } => {
                    commands::logs::list(
                        &cfg,
                        commands::logs::SearchArgs {
                            query,
                            from,
                            to,
                            limit,
                            sort,
                            storage,
                            index,
                        },
                    )
                    .await?;
                }
                LogActions::Query {
                    query,
                    from,
                    to,
                    limit,
                    sort,
                    storage,
                    index,
                    timezone: _,
                } => {
                    commands::logs::query(
                        &cfg,
                        commands::logs::SearchArgs {
                            query,
                            from,
                            to,
                            limit,
                            sort,
                            storage,
                            index,
                        },
                    )
                    .await?;
                }
                LogActions::Aggregate {
                    query,
                    from,
                    to,
                    compute,
                    group_by,
                    limit,
                    storage,
                    index,
                    sort,
                    interval,
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
                            index,
                            storage,
                            sort,
                            interval,
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
                LogActions::SavedViews { action } => match action {
                    LogSavedViewActions::List => {
                        commands::logs::saved_views_list(&cfg).await?;
                    }
                    LogSavedViewActions::Get { view_id } => {
                        commands::logs::saved_views_get(&cfg, &view_id).await?;
                    }
                    LogSavedViewActions::Create { file } => {
                        commands::logs::saved_views_create(&cfg, &file).await?;
                    }
                    LogSavedViewActions::Delete { view_id } => {
                        commands::logs::saved_views_delete(&cfg, &view_id).await?;
                    }
                },
            }
        }
        // --- Incidents ---
        Commands::Incidents { action } => {
            cfg.validate_auth()?;
            match action {
                IncidentActions::List { query, limit } => {
                    commands::incidents::list(&cfg, query, limit).await?;
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
                DashboardActions::Annotations { action } => run_annotations(&cfg, action).await?,
                DashboardActions::List => commands::dashboards::list(&cfg).await?,
                DashboardActions::Get { id } => commands::dashboards::get(&cfg, &id).await?,
                DashboardActions::Url {
                    id,
                    from_ts,
                    to_ts,
                    live,
                } => commands::dashboards::url(&cfg, &id, &from_ts, &to_ts, live).await?,
                DashboardActions::Create { file } => {
                    commands::dashboards::create(&cfg, &file).await?;
                }
                DashboardActions::Update { id, file } => {
                    commands::dashboards::update(&cfg, &id, &file).await?;
                }
                DashboardActions::Delete { id } => commands::dashboards::delete(&cfg, &id).await?,
                DashboardActions::Widgets { action } => match action {
                    DashboardWidgetActions::List { dash_id } => {
                        commands::dashboards::widget_list(&cfg, &dash_id).await?;
                    }
                    DashboardWidgetActions::Get {
                        dash_id,
                        widget_id,
                        index,
                    } => {
                        commands::dashboards::widget_get(&cfg, &dash_id, widget_id, index).await?;
                    }
                    DashboardWidgetActions::Add { dash_id, file } => {
                        commands::dashboards::widget_add(&cfg, &dash_id, &file).await?;
                    }
                    DashboardWidgetActions::Update {
                        dash_id,
                        widget_id,
                        index,
                        file,
                    } => {
                        commands::dashboards::widget_update(
                            &cfg, &dash_id, widget_id, index, &file,
                        )
                        .await?;
                    }
                    DashboardWidgetActions::Remove {
                        dash_id,
                        widget_id,
                        index,
                    } => {
                        commands::dashboards::widget_remove(&cfg, &dash_id, widget_id, index)
                            .await?;
                    }
                    DashboardWidgetActions::Types => {
                        commands::dashboards::widget_types(&cfg)?;
                    }
                    DashboardWidgetActions::Schema { r#type } => {
                        commands::dashboards::widget_schema(&cfg, &r#type)?;
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
                MetricActions::List {
                    filter,
                    from,
                    tag_filter,
                } => {
                    commands::metrics::list(&cfg, filter, from, tag_filter).await?;
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
                    MetricTagActions::List {
                        metric_name,
                        window_seconds,
                        ..
                    } => {
                        commands::metrics::tags_list(&cfg, &metric_name, window_seconds).await?;
                    }
                },
                MetricActions::Timeseries { file } => {
                    commands::metrics::query_timeseries(&cfg, &file).await?;
                }
            }
        }
        // --- SLOs ---
        Commands::Slos { action } => {
            cfg.validate_auth()?;
            match action {
                SloActions::List {
                    query,
                    tags_query,
                    metrics_query,
                    limit,
                    offset,
                } => {
                    commands::slos::list(&cfg, query, tags_query, metrics_query, limit, offset)
                        .await?;
                }
                SloActions::Get { id } => commands::slos::get(&cfg, &id).await?,
                SloActions::Create { file } => commands::slos::create(&cfg, &file).await?,
                SloActions::Update { id, file } => {
                    commands::slos::update(&cfg, &id, &file).await?;
                }
                SloActions::Delete { id } => commands::slos::delete(&cfg, &id).await?,
                SloActions::Status { id, from, to } => {
                    let from_ts = util_ext::parse_time_to_unix_millis(&from)? / 1000;
                    let to_ts = util_ext::parse_time_to_unix_millis(&to)? / 1000;
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
                    no_allow_partial,
                } => {
                    commands::symdb::search(
                        &cfg,
                        &service,
                        query.as_deref().unwrap_or(""),
                        version.as_deref(),
                        &view,
                        !no_allow_partial,
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
                    SyntheticsTestActions::GetResult {
                        public_id,
                        result_id,
                        event_id,
                        timestamp,
                    } => {
                        commands::synthetics::tests_get_result(
                            &cfg, &public_id, &result_id, event_id, timestamp,
                        )
                        .await?;
                    }
                    SyntheticsTestActions::GetBrowserResult {
                        public_id,
                        result_id,
                        event_id,
                        timestamp,
                    } => {
                        commands::synthetics::tests_get_browser_result(
                            &cfg, &public_id, &result_id, event_id, timestamp,
                        )
                        .await?;
                    }
                    SyntheticsTestActions::ListLatestResults {
                        public_id,
                        from_ts,
                        to_ts,
                        status,
                        run_type,
                        probe_dc,
                        device_id,
                    } => {
                        commands::synthetics::tests_list_latest_results(
                            &cfg, &public_id, from_ts, to_ts, status, run_type, probe_dc, device_id,
                        )
                        .await?;
                    }
                    SyntheticsTestActions::ListLatestBrowserResults {
                        public_id,
                        from_ts,
                        to_ts,
                        status,
                        run_type,
                        probe_dc,
                        device_id,
                    } => {
                        commands::synthetics::tests_list_latest_browser_results(
                            &cfg, &public_id, from_ts, to_ts, status, run_type, probe_dc, device_id,
                        )
                        .await?;
                    }
                    SyntheticsTestActions::PollResults { result_ids } => {
                        commands::synthetics::tests_poll_results(&cfg, result_ids).await?;
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
                SyntheticsActions::Downtime { action } => match action {
                    SyntheticsDowntimeActions::List {
                        filter_test_ids,
                        filter_active,
                    } => {
                        commands::synthetics::downtime_list(&cfg, filter_test_ids, filter_active)
                            .await?;
                    }
                    SyntheticsDowntimeActions::Create { file } => {
                        commands::synthetics::downtime_create(&cfg, &file).await?;
                    }
                    SyntheticsDowntimeActions::Delete { downtime_id } => {
                        commands::synthetics::downtime_delete(&cfg, &downtime_id).await?;
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
                    TestOptimizationFlakyTestsActions::Policies { action } => match action {
                        TestOptimizationFlakyTestsPoliciesActions::Get { file } => {
                            commands::test_optimization::flaky_tests_management_policies_get(
                                &cfg, &file,
                            )
                            .await?;
                        }
                        TestOptimizationFlakyTestsPoliciesActions::Update { file } => {
                            commands::test_optimization::flaky_tests_management_policies_update(
                                &cfg, &file,
                            )
                            .await?;
                        }
                    },
                },
            }
        }
        // --- Events ---
        Commands::Events { action } => {
            if !matches!(&action, EventActions::Post { .. }) {
                cfg.validate_auth()?;
            }
            match action {
                EventActions::Post {
                    title,
                    message,
                    date_happened,
                    handle,
                    priority,
                    related_event_id,
                    tags,
                    host,
                    no_host,
                    device,
                    event_type,
                    aggregation_key,
                    alert_type,
                } => {
                    commands::events::post(
                        &cfg,
                        commands::events::PostOptions {
                            title,
                            message,
                            date_happened,
                            handle,
                            priority,
                            related_event_id,
                            tags,
                            host: commands::events::resolve_host(host, no_host),
                            device,
                            event_type,
                            aggregation_key,
                            alert_type,
                        },
                    )
                    .await?;
                }
                EventActions::List { from, to, tags, .. } => {
                    let start = util_ext::parse_time_to_unix_millis(&from)? / 1000;
                    let end = util_ext::parse_time_to_unix_millis(&to)? / 1000;
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
        // --- Tag Policies ---
        Commands::TagPolicies { action } => {
            cfg.validate_auth()?;
            match action {
                TagPoliciesActions::List {
                    include_disabled,
                    include_deleted,
                    include_score,
                    filter_source,
                } => {
                    commands::tag_policies::list(
                        &cfg,
                        include_disabled,
                        include_deleted,
                        include_score,
                        filter_source,
                    )
                    .await?;
                }
                TagPoliciesActions::Get {
                    policy_id,
                    include_score,
                } => {
                    commands::tag_policies::get(&cfg, &policy_id, include_score).await?;
                }
                TagPoliciesActions::Create { file } => {
                    commands::tag_policies::create(&cfg, &file).await?;
                }
                TagPoliciesActions::Update { policy_id, file } => {
                    commands::tag_policies::update(&cfg, &policy_id, &file).await?;
                }
                TagPoliciesActions::Delete {
                    policy_id,
                    hard_delete,
                } => {
                    commands::tag_policies::delete(&cfg, &policy_id, hard_delete).await?;
                }
                TagPoliciesActions::Score {
                    policy_id,
                    ts_start,
                    ts_end,
                } => {
                    commands::tag_policies::score(&cfg, &policy_id, ts_start, ts_end).await?;
                }
            }
        }
        // --- Users ---
        Commands::Users { action } => {
            cfg.validate_auth()?;
            match action {
                UserActions::List {
                    page_size,
                    page_number,
                } => commands::users::list(&cfg, page_size, page_number).await?,
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
                UserActions::ServiceAccounts { action } => match action {
                    ServiceAccountActions::Create { file } => {
                        commands::users::service_accounts_create(&cfg, &file).await?;
                    }
                    ServiceAccountActions::AppKeys { action } => match action {
                        ServiceAccountAppKeyActions::List { service_account_id } => {
                            commands::users::service_account_app_keys_list(
                                &cfg,
                                &service_account_id,
                            )
                            .await?;
                        }
                        ServiceAccountAppKeyActions::Get {
                            service_account_id,
                            app_key_id,
                        } => {
                            commands::users::service_account_app_keys_get(
                                &cfg,
                                &service_account_id,
                                &app_key_id,
                            )
                            .await?;
                        }
                        ServiceAccountAppKeyActions::Create {
                            service_account_id,
                            file,
                        } => {
                            commands::users::service_account_app_keys_create(
                                &cfg,
                                &service_account_id,
                                &file,
                            )
                            .await?;
                        }
                        ServiceAccountAppKeyActions::Update {
                            service_account_id,
                            app_key_id,
                            file,
                        } => {
                            commands::users::service_account_app_keys_update(
                                &cfg,
                                &service_account_id,
                                &app_key_id,
                                &file,
                            )
                            .await?;
                        }
                        ServiceAccountAppKeyActions::Delete {
                            service_account_id,
                            app_key_id,
                        } => {
                            commands::users::service_account_app_keys_delete(
                                &cfg,
                                &service_account_id,
                                &app_key_id,
                            )
                            .await?;
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
                        start,
                        include_muted_hosts_data,
                        include_hosts_metadata,
                    } => {
                        commands::infrastructure::hosts_list(
                            &cfg,
                            filter,
                            sort,
                            count,
                            start,
                            include_muted_hosts_data,
                            include_hosts_metadata,
                        )
                        .await?;
                    }
                    InfraHostActions::Get { hostname } => {
                        commands::infrastructure::hosts_get(&cfg, &hostname).await?;
                    }
                },
            }
        }
        // --- IDP (Internal Developer Portal) ---
        Commands::Idp { action } => match action {
            IdpActions::Assist { entity } => {
                cfg.validate_auth()?;
                commands::idp::assist(&cfg, &entity).await?;
            }
            IdpActions::Find { query } => {
                cfg.validate_auth()?;
                commands::idp::find(&cfg, &query).await?;
            }
            IdpActions::Owner { entity } => {
                cfg.validate_auth()?;
                commands::idp::owner(&cfg, &entity).await?;
            }
            IdpActions::Deps { entity } => {
                cfg.validate_auth()?;
                commands::idp::deps(&cfg, &entity).await?;
            }
            IdpActions::Register { file } => {
                cfg.validate_auth()?;
                commands::idp::register(&cfg, &file).await?;
            }
            IdpActions::MigrateSchema { file } => {
                commands::idp::migrate_schema(&cfg, file).await?;
            }
        },
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
                    SecurityRuleActions::List {
                        filter,
                        sort,
                        page_size,
                        page_number,
                    } => {
                        commands::security::rules_list(&cfg, filter, sort, page_size, page_number)
                            .await?
                    }
                    SecurityRuleActions::Get { rule_id } => {
                        commands::security::rules_get(&cfg, &rule_id).await?;
                    }
                    SecurityRuleActions::BulkExport { rule_ids } => {
                        commands::security::rules_bulk_export(&cfg, rule_ids).await?;
                    }
                    SecurityRuleActions::ToTerraform { file } => {
                        commands::security::rules_to_terraform(&cfg, &file).await?;
                    }
                    SecurityRuleActions::BulkConvert { file } => {
                        commands::security::rules_bulk_convert(&cfg, &file).await?;
                    }
                },
                SecurityActions::Signals { action } => match action {
                    SecuritySignalActions::List {
                        query,
                        from,
                        to,
                        limit,
                        sort,
                    } => {
                        commands::security::signals_search(&cfg, query, from, to, limit, sort)
                            .await?;
                    }
                    SecuritySignalActions::InvestigationQueries { signal_id } => {
                        commands::security::signals_investigation_queries(&cfg, &signal_id).await?;
                    }
                    SecuritySignalActions::SuggestedActions { signal_id } => {
                        commands::security::signals_suggested_actions(&cfg, &signal_id).await?;
                    }
                },
                SecurityActions::Findings { action } => match action {
                    SecurityFindingActions::Search { query, limit } => {
                        commands::security::findings_search(&cfg, query, limit).await?;
                    }
                    SecurityFindingActions::Schema => {
                        commands::security::findings_schema(&cfg).await?;
                    }
                    SecurityFindingActions::Analyze {
                        query,
                        from,
                        to,
                        limit,
                    } => {
                        commands::security::findings_analyze(&cfg, &query, &from, &to, limit)
                            .await?;
                    }
                    SecurityFindingActions::Mute { file } => {
                        commands::security::findings_mute(&cfg, &file).await?;
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
                SecurityActions::Iocs { action } => match action {
                    SecurityIocActions::List {
                        query,
                        limit,
                        offset,
                        sort_column,
                        sort_order,
                    } => {
                        commands::security::iocs_list(
                            &cfg,
                            query,
                            limit,
                            offset,
                            sort_column,
                            sort_order,
                        )
                        .await?;
                    }
                    SecurityIocActions::Get { indicator } => {
                        commands::security::iocs_get(&cfg, &indicator).await?;
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
                SecurityActions::AsmCustomRules { action } => match action {
                    AsmCustomRuleActions::List => {
                        commands::security::asm_custom_rules_list(&cfg).await?;
                    }
                    AsmCustomRuleActions::Get { custom_rule_id } => {
                        commands::security::asm_custom_rules_get(&cfg, &custom_rule_id).await?;
                    }
                    AsmCustomRuleActions::Create { file } => {
                        commands::security::asm_custom_rules_create(&cfg, &file).await?;
                    }
                    AsmCustomRuleActions::Update {
                        custom_rule_id,
                        file,
                    } => {
                        commands::security::asm_custom_rules_update(&cfg, &custom_rule_id, &file)
                            .await?;
                    }
                    AsmCustomRuleActions::Delete { custom_rule_id } => {
                        commands::security::asm_custom_rules_delete(&cfg, &custom_rule_id).await?;
                    }
                },
                SecurityActions::AsmExclusions { action } => match action {
                    AsmExclusionActions::List => {
                        commands::security::asm_exclusions_list(&cfg).await?;
                    }
                    AsmExclusionActions::Get {
                        exclusion_filter_id,
                    } => {
                        commands::security::asm_exclusions_get(&cfg, &exclusion_filter_id).await?;
                    }
                    AsmExclusionActions::Create { file } => {
                        commands::security::asm_exclusions_create(&cfg, &file).await?;
                    }
                    AsmExclusionActions::Update {
                        exclusion_filter_id,
                        file,
                    } => {
                        commands::security::asm_exclusions_update(
                            &cfg,
                            &exclusion_filter_id,
                            &file,
                        )
                        .await?;
                    }
                    AsmExclusionActions::Delete {
                        exclusion_filter_id,
                    } => {
                        commands::security::asm_exclusions_delete(&cfg, &exclusion_filter_id)
                            .await?;
                    }
                },
                SecurityActions::RestrictionPolicies { action } => match action {
                    RestrictionPolicyActions::Get { resource_id } => {
                        commands::security::restriction_policy_get(&cfg, &resource_id).await?;
                    }
                    RestrictionPolicyActions::Update { resource_id, file } => {
                        commands::security::restriction_policy_update(&cfg, &resource_id, &file)
                            .await?;
                    }
                    RestrictionPolicyActions::Delete { resource_id } => {
                        commands::security::restriction_policy_delete(&cfg, &resource_id).await?;
                    }
                },
                SecurityActions::Terraform { action } => match action {
                    SecurityTerraformActions::Export {
                        resource_type,
                        resource_id,
                    } => {
                        commands::security::terraform_export(&cfg, &resource_type, &resource_id)
                            .await?;
                    }
                    SecurityTerraformActions::BulkExport {
                        resource_type,
                        file,
                        output_file,
                    } => {
                        commands::security::terraform_bulk_export(
                            &cfg,
                            &resource_type,
                            &file,
                            &output_file,
                        )
                        .await?;
                    }
                    SecurityTerraformActions::Convert {
                        resource_type,
                        file,
                    } => {
                        commands::security::terraform_convert(&cfg, &resource_type, &file).await?;
                    }
                },
            }
        }
        // --- AuthN Mappings ---
        Commands::AuthnMappings { action } => {
            cfg.validate_auth()?;
            match action {
                AuthnMappingsActions::List => {
                    commands::authn_mappings::list(&cfg).await?;
                }
                AuthnMappingsActions::Get { mapping_id } => {
                    commands::authn_mappings::get(&cfg, &mapping_id).await?;
                }
                AuthnMappingsActions::Create { file } => {
                    commands::authn_mappings::create(&cfg, &file).await?;
                }
                AuthnMappingsActions::Update { mapping_id, file } => {
                    commands::authn_mappings::update(&cfg, &mapping_id, &file).await?;
                }
                AuthnMappingsActions::Delete { mapping_id } => {
                    commands::authn_mappings::delete(&cfg, &mapping_id).await?;
                }
            }
        }
        // --- Organizations ---
        Commands::Organizations { action } => {
            cfg.validate_auth()?;
            match action {
                OrgActions::List => commands::organizations::list(&cfg).await?,
                OrgActions::Get => commands::organizations::get(&cfg).await?,
                OrgActions::Policies { action } => match action {
                    OrgPoliciesActions::List {
                        group_id,
                        name,
                        page_number,
                        page_size,
                        sort,
                    } => {
                        commands::organizations::policies_list(
                            &cfg,
                            &group_id,
                            name,
                            page_number,
                            page_size,
                            sort,
                        )
                        .await?;
                    }
                    OrgPoliciesActions::Get { policy_id } => {
                        commands::organizations::policies_get(&cfg, &policy_id).await?;
                    }
                    OrgPoliciesActions::Create { file } => {
                        commands::organizations::policies_create(&cfg, &file).await?;
                    }
                    OrgPoliciesActions::Update { policy_id, file } => {
                        commands::organizations::policies_update(&cfg, &policy_id, &file).await?;
                    }
                    OrgPoliciesActions::Delete { policy_id } => {
                        commands::organizations::policies_delete(&cfg, &policy_id).await?;
                    }
                },
                OrgActions::PolicyOverrides { action } => match action {
                    OrgPolicyOverridesActions::List {
                        group_id,
                        policy_id,
                        page_number,
                        page_size,
                        sort,
                    } => {
                        commands::organizations::policy_overrides_list(
                            &cfg,
                            &group_id,
                            policy_id,
                            page_number,
                            page_size,
                            sort,
                        )
                        .await?;
                    }
                    OrgPolicyOverridesActions::Get { override_id } => {
                        commands::organizations::policy_overrides_get(&cfg, &override_id).await?;
                    }
                    OrgPolicyOverridesActions::Create { file } => {
                        commands::organizations::policy_overrides_create(&cfg, &file).await?;
                    }
                    OrgPolicyOverridesActions::Update { override_id, file } => {
                        commands::organizations::policy_overrides_update(&cfg, &override_id, &file)
                            .await?;
                    }
                    OrgPolicyOverridesActions::Delete { override_id } => {
                        commands::organizations::policy_overrides_delete(&cfg, &override_id)
                            .await?;
                    }
                },
                OrgActions::PolicyConfigs { action } => match action {
                    OrgPolicyConfigsActions::List => {
                        commands::organizations::policy_configs_list(&cfg).await?;
                    }
                },
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
        // --- Change Stories ---
        Commands::ChangeStories { action } => {
            cfg.validate_auth()?;
            match action {
                ChangeStoriesActions::List {
                    service,
                    env,
                    from,
                    to,
                    story_types,
                    filter_tags,
                    token_limit,
                } => {
                    commands::change_stories::list(
                        &cfg,
                        service,
                        env,
                        from,
                        to,
                        story_types,
                        filter_tags,
                        token_limit,
                    )
                    .await?;
                }
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
                    query,
                    page_size,
                    page_number,
                } => {
                    commands::cases::search(&cfg, query, page_size, page_number).await?;
                }
                CaseActions::Get { case_id } => commands::cases::get(&cfg, &case_id).await?,
                CaseActions::Create {
                    title,
                    type_id,
                    priority,
                    description,
                    project_id,
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
                            project_id.as_deref(),
                        )
                        .await?;
                    }
                }
                CaseActions::Comments { action } => match action {
                    CaseCommentActions::List { case_id } => {
                        commands::cases::comments_list(&cfg, &case_id).await?;
                    }
                    CaseCommentActions::Get {
                        case_id,
                        comment_id,
                    } => {
                        commands::cases::comments_get(&cfg, &case_id, &comment_id).await?;
                    }
                    CaseCommentActions::Create { case_id, body } => {
                        commands::cases::comments_create(&cfg, &case_id, &body).await?;
                    }
                    CaseCommentActions::Update {
                        case_id,
                        comment_id,
                        body,
                    } => {
                        commands::cases::comments_update(&cfg, &case_id, &comment_id, &body)
                            .await?;
                    }
                    CaseCommentActions::Delete {
                        case_id,
                        comment_id,
                    } => {
                        commands::cases::comments_delete(&cfg, &case_id, &comment_id).await?;
                    }
                },
                CaseActions::Timeline { case_id } => {
                    commands::cases::timeline(&cfg, &case_id).await?;
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
                CaseActions::UpdateDescription {
                    case_id,
                    description,
                } => {
                    commands::cases::update_description(&cfg, &case_id, &description).await?;
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
        // --- Software Catalog ---
        Commands::SoftwareCatalog { action } => {
            cfg.validate_auth()?;
            match action {
                SoftwareCatalogActions::Entities { action } => match action {
                    SoftwareCatalogEntityActions::List {
                        filter,
                        filter_kind,
                        filter_owner,
                        filter_ref,
                    } => {
                        commands::software_catalog::entities_list(
                            &cfg,
                            filter,
                            filter_kind,
                            filter_owner,
                            filter_ref,
                        )
                        .await?;
                    }
                    SoftwareCatalogEntityActions::Upsert { file } => {
                        commands::software_catalog::entities_upsert(&cfg, &file).await?;
                    }
                    SoftwareCatalogEntityActions::Delete { entity_id } => {
                        commands::software_catalog::entities_delete(&cfg, &entity_id).await?;
                    }
                    SoftwareCatalogEntityActions::Preview => {
                        commands::software_catalog::entities_preview(&cfg).await?;
                    }
                },
                SoftwareCatalogActions::Kinds { action } => match action {
                    SoftwareCatalogKindActions::List => {
                        commands::software_catalog::kinds_list(&cfg).await?;
                    }
                    SoftwareCatalogKindActions::Upsert { file } => {
                        commands::software_catalog::kinds_upsert(&cfg, &file).await?;
                    }
                    SoftwareCatalogKindActions::Delete { kind_id } => {
                        commands::software_catalog::kinds_delete(&cfg, &kind_id).await?;
                    }
                },
                SoftwareCatalogActions::Relations { action } => match action {
                    SoftwareCatalogRelationActions::List => {
                        commands::software_catalog::relations_list(&cfg).await?;
                    }
                },
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
                NotebookActions::Annotations { action } => run_annotations(&cfg, action).await?,
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
                NotebookActions::Edit { notebook_id, file } => {
                    commands::notebooks::edit(&cfg, notebook_id, &file).await?;
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
                RumActions::Aggregate {
                    query,
                    user_email,
                    from,
                    to,
                    compute,
                    group_by,
                    limit,
                } => {
                    let base_query = query.unwrap_or_default();
                    let effective_query = match user_email {
                        Some(email) => {
                            let email_filter = format!("@usr.email:{email}");
                            if base_query.is_empty() {
                                email_filter
                            } else {
                                format!("{email_filter} {base_query}")
                            }
                        }
                        None => base_query,
                    };
                    commands::rum::aggregate(
                        &cfg,
                        commands::rum::RumAggregateArgs {
                            query: effective_query,
                            from,
                            to,
                            compute: commands::rum::split_rum_compute_args(&compute),
                            group_by: group_by
                                .map(|g| {
                                    g.split(',')
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .collect()
                                })
                                .unwrap_or_default(),
                            limit,
                        },
                    )
                    .await?;
                }
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
                        branch,
                        pipeline_name,
                    } => {
                        commands::cicd::pipelines_list(
                            &cfg,
                            query,
                            from,
                            to,
                            limit,
                            branch,
                            pipeline_name,
                        )
                        .await?;
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
                        query,
                        from,
                        to,
                        compute,
                        group_by,
                        limit,
                    } => {
                        commands::cicd::tests_aggregate(
                            &cfg, query, from, to, compute, group_by, limit,
                        )
                        .await?;
                    }
                },
                CicdActions::Events { action } => match action {
                    CicdEventActions::Search {
                        query,
                        from,
                        to,
                        limit,
                        sort,
                        level,
                    } => {
                        commands::cicd::events_search(&cfg, query, from, to, limit, sort, level)
                            .await?;
                    }
                    CicdEventActions::Aggregate {
                        query,
                        from,
                        to,
                        compute,
                        group_by,
                        limit,
                    } => {
                        commands::cicd::events_aggregate(
                            &cfg, query, from, to, compute, group_by, limit,
                        )
                        .await?;
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
                        sort,
                    } => {
                        commands::cicd::flaky_tests_search(&cfg, query, cursor, limit, sort)
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
                            team_id,
                            page_size,
                            page_number,
                            sort,
                        } => {
                            commands::on_call::memberships_list(
                                &cfg,
                                &team_id,
                                page_size,
                                page_number,
                                sort,
                            )
                            .await?;
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
                OnCallActions::EscalationPolicies { action } => match action {
                    OnCallEscalationPoliciesActions::Get { policy_id } => {
                        commands::on_call::escalation_policies_get(&cfg, &policy_id).await?;
                    }
                    OnCallEscalationPoliciesActions::Create { file } => {
                        commands::on_call::escalation_policies_create(&cfg, &file).await?;
                    }
                    OnCallEscalationPoliciesActions::Update { policy_id, file } => {
                        commands::on_call::escalation_policies_update(&cfg, &policy_id, &file)
                            .await?;
                    }
                    OnCallEscalationPoliciesActions::Delete { policy_id } => {
                        commands::on_call::escalation_policies_delete(&cfg, &policy_id).await?;
                    }
                },
                OnCallActions::Schedules { action } => match action {
                    OnCallSchedulesActions::Get { schedule_id } => {
                        commands::on_call::schedules_get(&cfg, &schedule_id).await?;
                    }
                    OnCallSchedulesActions::Create { file } => {
                        commands::on_call::schedules_create(&cfg, &file).await?;
                    }
                    OnCallSchedulesActions::Update { schedule_id, file } => {
                        commands::on_call::schedules_update(&cfg, &schedule_id, &file).await?;
                    }
                    OnCallSchedulesActions::Delete { schedule_id } => {
                        commands::on_call::schedules_delete(&cfg, &schedule_id).await?;
                    }
                },
                OnCallActions::NotificationChannels { action } => match action {
                    OnCallNotificationChannelsActions::List { user_id } => {
                        commands::on_call::notification_channels_list(&cfg, &user_id).await?;
                    }
                    OnCallNotificationChannelsActions::Get {
                        user_id,
                        channel_id,
                    } => {
                        commands::on_call::notification_channels_get(&cfg, &user_id, &channel_id)
                            .await?;
                    }
                    OnCallNotificationChannelsActions::Create { user_id, file } => {
                        commands::on_call::notification_channels_create(&cfg, &user_id, &file)
                            .await?;
                    }
                    OnCallNotificationChannelsActions::Delete {
                        user_id,
                        channel_id,
                    } => {
                        commands::on_call::notification_channels_delete(
                            &cfg,
                            &user_id,
                            &channel_id,
                        )
                        .await?;
                    }
                },
                OnCallActions::NotificationRules { action } => match action {
                    OnCallNotificationRulesActions::List { user_id } => {
                        commands::on_call::notification_rules_list(&cfg, &user_id).await?;
                    }
                    OnCallNotificationRulesActions::Get { user_id, rule_id } => {
                        commands::on_call::notification_rules_get(&cfg, &user_id, &rule_id).await?;
                    }
                    OnCallNotificationRulesActions::Create { user_id, file } => {
                        commands::on_call::notification_rules_create(&cfg, &user_id, &file).await?;
                    }
                    OnCallNotificationRulesActions::Update {
                        user_id,
                        rule_id,
                        file,
                    } => {
                        commands::on_call::notification_rules_update(
                            &cfg, &user_id, &rule_id, &file,
                        )
                        .await?;
                    }
                    OnCallNotificationRulesActions::Delete { user_id, rule_id } => {
                        commands::on_call::notification_rules_delete(&cfg, &user_id, &rule_id)
                            .await?;
                    }
                },
                OnCallActions::Pages { action } => match action {
                    OnCallPagesActions::List {
                        team,
                        responder,
                        sort,
                        page_size,
                    } => {
                        commands::on_call::pages_list(
                            &cfg,
                            team.as_deref(),
                            responder.as_deref(),
                            page_size,
                            &sort,
                        )
                        .await?;
                    }
                    OnCallPagesActions::Create { file } => {
                        commands::on_call::pages_create(&cfg, &file).await?;
                    }
                    OnCallPagesActions::Get { page_id } => {
                        commands::on_call::pages_get(&cfg, &page_id).await?;
                    }
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
                    FleetAgentActions::Tracers {
                        agent_key,
                        page_size,
                        page_number,
                        sort_attribute,
                        sort_descending,
                    } => {
                        commands::fleet::agents_tracers_list(
                            &cfg,
                            agent_key,
                            page_size,
                            page_number,
                            sort_attribute,
                            sort_descending,
                        )
                        .await?;
                    }
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
                FleetActions::Tracers { action } => match action {
                    FleetTracerActions::List {
                        filter,
                        page_size,
                        page_number,
                        sort_attribute,
                        sort_descending,
                    } => {
                        commands::fleet::tracers_list(
                            &cfg,
                            filter,
                            page_size,
                            page_number,
                            sort_attribute,
                            sort_descending,
                        )
                        .await?;
                    }
                },
            }
        }
        // --- DBM ---
        Commands::Dbm { action } => match action {
            DbmActions::Samples { action } => match action {
                DbmSamplesActions::Search {
                    query,
                    from,
                    to,
                    limit,
                    sort,
                } => {
                    commands::dbm::samples_search(&cfg, query, from, to, limit, sort).await?;
                }
            },
        },
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
        // --- Data Deletion ---
        Commands::DataDeletion { action } => {
            cfg.validate_auth()?;
            match action {
                DataDeletionActions::Requests { action } => match action {
                    DataDeletionRequestActions::List {
                        product,
                        query,
                        status,
                    } => {
                        commands::data_deletion::requests_list(&cfg, product, query, status)
                            .await?;
                    }
                    DataDeletionRequestActions::Create { product, file } => {
                        commands::data_deletion::requests_create(&cfg, &product, &file).await?;
                    }
                    DataDeletionRequestActions::Cancel { request_id } => {
                        commands::data_deletion::requests_cancel(&cfg, &request_id).await?;
                    }
                },
            }
        }
        // --- Datasets ---
        Commands::Datasets { action } => {
            cfg.validate_auth()?;
            match action {
                DatasetsActions::List => {
                    commands::datasets::list(&cfg).await?;
                }
                DatasetsActions::Get { dataset_id } => {
                    commands::datasets::get(&cfg, &dataset_id).await?;
                }
                DatasetsActions::Create { file } => {
                    commands::datasets::create(&cfg, &file).await?;
                }
                DatasetsActions::Update { dataset_id, file } => {
                    commands::datasets::update(&cfg, &dataset_id, &file).await?;
                }
                DatasetsActions::Delete { dataset_id } => {
                    commands::datasets::delete(&cfg, &dataset_id).await?;
                }
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
        // Handled before Config::from_env() so it runs without auth.
        Commands::Docs { .. } => unreachable!(),
        // --- Error Tracking ---
        Commands::ErrorTracking { action } => {
            cfg.validate_auth()?;
            match action {
                ErrorTrackingActions::Issues { action } => match action {
                    ErrorTrackingIssueActions::Search {
                        query,
                        limit,
                        from,
                        to,
                        order_by,
                        track,
                        persona,
                        state,
                        team,
                        assignee,
                    } => {
                        commands::error_tracking::issues_search(
                            &cfg, query, limit, from, to, order_by, track, persona, state, team,
                            assignee,
                        )
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
                FeatureFlagActions::Allocations { action } => match action {
                    FeatureFlagAllocActions::Create {
                        feature_flag_id,
                        environment_id,
                        file,
                    } => {
                        commands::feature_flags::allocations_create(
                            &cfg,
                            &feature_flag_id,
                            &environment_id,
                            &file,
                        )
                        .await?;
                    }
                    FeatureFlagAllocActions::Update {
                        feature_flag_id,
                        environment_id,
                        file,
                    } => {
                        commands::feature_flags::allocations_update(
                            &cfg,
                            &feature_flag_id,
                            &environment_id,
                            &file,
                        )
                        .await?;
                    }
                },
                FeatureFlagActions::Exposure { action } => match action {
                    FeatureFlagExposureActions::Start {
                        exposure_schedule_id,
                    } => {
                        commands::feature_flags::exposure_start(&cfg, &exposure_schedule_id)
                            .await?;
                    }
                    FeatureFlagExposureActions::Stop {
                        exposure_schedule_id,
                    } => {
                        commands::feature_flags::exposure_stop(&cfg, &exposure_schedule_id).await?;
                    }
                    FeatureFlagExposureActions::Pause {
                        exposure_schedule_id,
                    } => {
                        commands::feature_flags::exposure_pause(&cfg, &exposure_schedule_id)
                            .await?;
                    }
                    FeatureFlagExposureActions::Resume {
                        exposure_schedule_id,
                    } => {
                        commands::feature_flags::exposure_resume(&cfg, &exposure_schedule_id)
                            .await?;
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
                IntegrationActions::MsTeams { action } => match action {
                    MsTeamsActions::Handles { action } => match action {
                        MsTeamsHandleActions::List => {
                            commands::ms_teams::handles_list(&cfg).await?;
                        }
                        MsTeamsHandleActions::Get { handle_id } => {
                            commands::ms_teams::handles_get(&cfg, &handle_id).await?;
                        }
                        MsTeamsHandleActions::Create { file } => {
                            commands::ms_teams::handles_create(&cfg, &file).await?;
                        }
                        MsTeamsHandleActions::Update { handle_id, file } => {
                            commands::ms_teams::handles_update(&cfg, &handle_id, &file).await?;
                        }
                        MsTeamsHandleActions::Delete { handle_id } => {
                            commands::ms_teams::handles_delete(&cfg, &handle_id).await?;
                        }
                    },
                    MsTeamsActions::ChannelGet {
                        tenant_name,
                        team_name,
                        channel_name,
                    } => {
                        commands::ms_teams::channel_get_by_name(
                            &cfg,
                            &tenant_name,
                            &team_name,
                            &channel_name,
                        )
                        .await?;
                    }
                    MsTeamsActions::Workflows { action } => match action {
                        MsTeamsWorkflowActions::List => {
                            commands::ms_teams::workflows_list(&cfg).await?;
                        }
                        MsTeamsWorkflowActions::Get { handle_id } => {
                            commands::ms_teams::workflows_get(&cfg, &handle_id).await?;
                        }
                        MsTeamsWorkflowActions::Create { file } => {
                            commands::ms_teams::workflows_create(&cfg, &file).await?;
                        }
                        MsTeamsWorkflowActions::Update { handle_id, file } => {
                            commands::ms_teams::workflows_update(&cfg, &handle_id, &file).await?;
                        }
                        MsTeamsWorkflowActions::Delete { handle_id } => {
                            commands::ms_teams::workflows_delete(&cfg, &handle_id).await?;
                        }
                    },
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
                CostActions::Datadog { action } => match action {
                    CostDatadogActions::Projected => commands::cost::projected(&cfg).await?,
                    CostDatadogActions::ByOrg {
                        start_month,
                        end_month,
                        ..
                    } => {
                        commands::cost::by_org(&cfg, start_month, end_month).await?;
                    }
                    CostDatadogActions::Attribution { start, fields, .. } => {
                        commands::cost::attribution(&cfg, start, fields).await?;
                    }
                    CostDatadogActions::AwsConfig { action } => match action {
                        CostCloudConfigActions::List => {
                            commands::cost::aws_config_list(&cfg).await?
                        }
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
                    CostDatadogActions::AzureConfig { action } => match action {
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
                    CostDatadogActions::GcpConfig { action } => match action {
                        CostCloudConfigActions::List => {
                            commands::cost::gcp_config_list(&cfg).await?
                        }
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
                },
                CostActions::Ccm { action } => match action {
                    CostCcmActions::CustomCosts { action } => match action {
                        CostCcmCustomCostsActions::List {
                            page_size,
                            status,
                            sort,
                        } => {
                            commands::cost_ccm::custom_costs_list(&cfg, page_size, status, sort)
                                .await?;
                        }
                        CostCcmCustomCostsActions::Get { file_id } => {
                            commands::cost_ccm::custom_costs_get(&cfg, &file_id).await?;
                        }
                        CostCcmCustomCostsActions::Upload { file, version } => {
                            commands::cost_ccm::custom_costs_upload(&cfg, &file, version).await?;
                        }
                        CostCcmCustomCostsActions::Delete { file_id } => {
                            commands::cost_ccm::custom_costs_delete(&cfg, &file_id).await?;
                        }
                    },
                    CostCcmActions::TagDescriptions { action } => match action {
                        CostCcmTagDescActions::List { cloud } => {
                            commands::cost_ccm::tag_desc_list(&cfg, cloud).await?;
                        }
                        CostCcmTagDescActions::Get { tag_key, cloud } => {
                            commands::cost_ccm::tag_desc_get(&cfg, &tag_key, cloud).await?;
                        }
                        CostCcmTagDescActions::Generate { tag_key } => {
                            commands::cost_ccm::tag_desc_generate(&cfg, &tag_key).await?;
                        }
                        CostCcmTagDescActions::Upsert {
                            tag_key,
                            description,
                            cloud,
                        } => {
                            commands::cost_ccm::tag_desc_upsert(
                                &cfg,
                                &tag_key,
                                &description,
                                cloud,
                            )
                            .await?;
                        }
                        CostCcmTagDescActions::Delete { tag_key, cloud } => {
                            commands::cost_ccm::tag_desc_delete(&cfg, &tag_key, cloud).await?;
                        }
                    },
                    CostCcmActions::TagMetadata { action } => match action {
                        CostCcmTagMetadataActions::List {
                            month,
                            provider,
                            metric,
                            tag_key,
                            daily,
                        } => {
                            commands::cost_ccm::tag_meta_list(
                                &cfg, &month, provider, metric, tag_key, daily,
                            )
                            .await?;
                        }
                        CostCcmTagMetadataActions::TagSources { month, provider } => {
                            commands::cost_ccm::tag_meta_sources(&cfg, &month, provider).await?;
                        }
                        CostCcmTagMetadataActions::Metrics { month, provider } => {
                            commands::cost_ccm::tag_meta_metrics(&cfg, &month, provider).await?;
                        }
                        CostCcmTagMetadataActions::Orchestrators { month, provider } => {
                            commands::cost_ccm::tag_meta_orchestrators(&cfg, &month, provider)
                                .await?;
                        }
                        CostCcmTagMetadataActions::Currency { month, provider } => {
                            commands::cost_ccm::tag_meta_currency(&cfg, &month, provider).await?;
                        }
                    },
                    CostCcmActions::Tags { action } => match action {
                        CostCcmTagsActions::List {
                            metric,
                            r#match,
                            tags,
                        } => {
                            commands::cost_ccm::tags_list(&cfg, metric, r#match, tags).await?;
                        }
                    },
                    CostCcmActions::TagKeys { action } => match action {
                        CostCcmTagKeysActions::List { metric, tags } => {
                            commands::cost_ccm::tag_keys_list(&cfg, metric, tags).await?;
                        }
                        CostCcmTagKeysActions::Get { key, metric } => {
                            commands::cost_ccm::tag_keys_get(&cfg, &key, metric).await?;
                        }
                    },
                    CostCcmActions::Budgets { action } => match action {
                        CostCcmBudgetsActions::List => {
                            commands::cost_ccm::budgets_list(&cfg).await?;
                        }
                        CostCcmBudgetsActions::Get {
                            budget_id,
                            start,
                            end,
                            actual,
                            forecast,
                        } => {
                            commands::cost_ccm::budgets_get(
                                &cfg, &budget_id, start, end, actual, forecast,
                            )
                            .await?;
                        }
                        CostCcmBudgetsActions::Upsert { file } => {
                            commands::cost_ccm::budgets_upsert(&cfg, &file).await?;
                        }
                        CostCcmBudgetsActions::Delete { budget_id } => {
                            commands::cost_ccm::budgets_delete(&cfg, &budget_id).await?;
                        }
                        CostCcmBudgetsActions::Validate { file } => {
                            commands::cost_ccm::budgets_validate(&cfg, &file).await?;
                        }
                    },
                    CostCcmActions::Commitments { action } => match action {
                        CostCcmCommitmentsActions::Utilization {
                            provider,
                            product,
                            from,
                            to,
                            commitment_type,
                            filter_by,
                        } => {
                            commands::cost_ccm::commitments_utilization(
                                &cfg,
                                &provider,
                                &product,
                                &from,
                                &to,
                                commitment_type,
                                filter_by,
                            )
                            .await?;
                        }
                        CostCcmCommitmentsActions::Coverage {
                            provider,
                            product,
                            from,
                            to,
                            commitment_type,
                            filter_by,
                        } => {
                            commands::cost_ccm::commitments_coverage(
                                &cfg,
                                &provider,
                                &product,
                                &from,
                                &to,
                                commitment_type,
                                filter_by,
                            )
                            .await?;
                        }
                        CostCcmCommitmentsActions::Savings {
                            provider,
                            product,
                            from,
                            to,
                            commitment_type,
                            filter_by,
                        } => {
                            commands::cost_ccm::commitments_savings(
                                &cfg,
                                &provider,
                                &product,
                                &from,
                                &to,
                                commitment_type,
                                filter_by,
                            )
                            .await?;
                        }
                        CostCcmCommitmentsActions::Hotspots {
                            provider,
                            product,
                            from,
                            to,
                            commitment_type,
                            filter_by,
                        } => {
                            commands::cost_ccm::commitments_hotspots(
                                &cfg,
                                &provider,
                                &product,
                                &from,
                                &to,
                                commitment_type,
                                filter_by,
                            )
                            .await?;
                        }
                        CostCcmCommitmentsActions::UtilizationTs {
                            provider,
                            product,
                            from,
                            to,
                            commitment_type,
                            filter_by,
                        } => {
                            commands::cost_ccm::commitments_utilization_ts(
                                &cfg,
                                &provider,
                                &product,
                                &from,
                                &to,
                                commitment_type,
                                filter_by,
                            )
                            .await?;
                        }
                        CostCcmCommitmentsActions::CoverageTs {
                            provider,
                            product,
                            from,
                            to,
                            commitment_type,
                            filter_by,
                        } => {
                            commands::cost_ccm::commitments_coverage_ts(
                                &cfg,
                                &provider,
                                &product,
                                &from,
                                &to,
                                commitment_type,
                                filter_by,
                            )
                            .await?;
                        }
                        CostCcmCommitmentsActions::SavingsTs {
                            provider,
                            product,
                            from,
                            to,
                            commitment_type,
                            filter_by,
                        } => {
                            commands::cost_ccm::commitments_savings_ts(
                                &cfg,
                                &provider,
                                &product,
                                &from,
                                &to,
                                commitment_type,
                                filter_by,
                            )
                            .await?;
                        }
                        CostCcmCommitmentsActions::List {
                            provider,
                            product,
                            from,
                            to,
                            commitment_type,
                            filter_by,
                        } => {
                            commands::cost_ccm::commitments_list(
                                &cfg,
                                &provider,
                                &product,
                                &from,
                                &to,
                                commitment_type,
                                filter_by,
                            )
                            .await?;
                        }
                    },
                },
                CostActions::OciConfigs { action } => match action {
                    CostOciConfigsActions::List => commands::cost::oci_configs_list(&cfg).await?,
                },
                CostActions::Anomalies { action } => match action {
                    CostAnomaliesActions::List => commands::cost::anomalies_list(&cfg).await?,
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
        // --- Model Lab ---
        Commands::ModelLab { action } => {
            cfg.validate_auth()?;
            match action {
                ModelLabActions::Projects { action } => match action {
                    ModelLabProjectActions::List {
                        filter,
                        filter_tags,
                        sort,
                        page_size,
                        page_number,
                    } => {
                        commands::model_lab::projects_list(
                            &cfg,
                            filter,
                            filter_tags,
                            sort,
                            page_size,
                            page_number,
                        )
                        .await?;
                    }
                    ModelLabProjectActions::Get { project_id } => {
                        commands::model_lab::projects_get(&cfg, project_id).await?;
                    }
                    ModelLabProjectActions::Star { project_id } => {
                        commands::model_lab::projects_star(&cfg, project_id).await?;
                    }
                    ModelLabProjectActions::Unstar { project_id } => {
                        commands::model_lab::projects_unstar(&cfg, project_id).await?;
                    }
                    ModelLabProjectActions::Artifacts { project_id } => {
                        commands::model_lab::projects_artifacts(&cfg, project_id).await?;
                    }
                    ModelLabProjectActions::FacetKeys => {
                        commands::model_lab::projects_facet_keys(&cfg).await?;
                    }
                    ModelLabProjectActions::FacetValues {
                        facet_type,
                        facet_name,
                    } => {
                        commands::model_lab::projects_facet_values(&cfg, &facet_type, facet_name)
                            .await?;
                    }
                },
                ModelLabActions::Runs { action } => match action {
                    ModelLabRunActions::List {
                        filter,
                        filter_project_id,
                        filter_status,
                        filter_tags,
                        filter_params,
                        filter_parent_run_id,
                        pinned_first,
                        include_pinned,
                        sort,
                        page_size,
                        page_number,
                    } => {
                        commands::model_lab::runs_list(
                            &cfg,
                            filter,
                            filter_project_id,
                            filter_status,
                            filter_tags,
                            filter_params,
                            filter_parent_run_id,
                            pinned_first,
                            include_pinned,
                            sort,
                            page_size,
                            page_number,
                        )
                        .await?;
                    }
                    ModelLabRunActions::Get { run_id } => {
                        commands::model_lab::runs_get(&cfg, run_id).await?;
                    }
                    ModelLabRunActions::Delete { run_id } => {
                        commands::model_lab::runs_delete(&cfg, run_id).await?;
                    }
                    ModelLabRunActions::Pin { run_id } => {
                        commands::model_lab::runs_pin(&cfg, run_id).await?;
                    }
                    ModelLabRunActions::Unpin { run_id } => {
                        commands::model_lab::runs_unpin(&cfg, run_id).await?;
                    }
                    ModelLabRunActions::Artifacts { run_id, path } => {
                        commands::model_lab::runs_artifacts(&cfg, run_id, path).await?;
                    }
                    ModelLabRunActions::FacetKeys { filter_project_id } => {
                        commands::model_lab::runs_facet_keys(&cfg, filter_project_id).await?;
                    }
                    ModelLabRunActions::FacetValues {
                        filter_project_id,
                        facet_type,
                        facet_name,
                    } => {
                        commands::model_lab::runs_facet_values(
                            &cfg,
                            filter_project_id,
                            &facet_type,
                            facet_name,
                        )
                        .await?;
                    }
                },
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
                        name,
                        env,
                        from,
                        to,
                        ..
                    } => {
                        commands::apm::services_resources(&cfg, service, name, env, from, to)
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
                        result,
                    } => {
                        commands::apm::troubleshooting_list(&cfg, hostname, timeframe, result)
                            .await?;
                    }
                },
                ApmActions::ServiceRemapping { action } => match action {
                    ApmServiceRemappingActions::List => {
                        commands::apm::service_remapping_list(&cfg).await?;
                    }
                    ApmServiceRemappingActions::Create {
                        name,
                        filter,
                        rule_type,
                        value,
                    } => {
                        commands::apm::service_remapping_create(
                            &cfg, name, filter, rule_type, value,
                        )
                        .await?;
                    }
                    ApmServiceRemappingActions::Get { id } => {
                        commands::apm::service_remapping_get(&cfg, id).await?;
                    }
                    ApmServiceRemappingActions::Update {
                        id,
                        name,
                        filter,
                        rule_type,
                        value,
                        version,
                    } => {
                        commands::apm::service_remapping_update(
                            &cfg, id, name, filter, rule_type, value, version,
                        )
                        .await?;
                    }
                    ApmServiceRemappingActions::Delete { id, version } => {
                        commands::apm::service_remapping_delete(&cfg, id, version).await?;
                    }
                },
                ApmActions::SamplingRules { action } => match action {
                    ApmSamplingRulesActions::List { service, env } => {
                        commands::apm::sampling_rules_list(&cfg, service, env).await?;
                    }
                    ApmSamplingRulesActions::Get { id } => {
                        commands::apm::sampling_rules_get(&cfg, id).await?;
                    }
                    ApmSamplingRulesActions::Create {
                        service,
                        env,
                        resource,
                        sample_rate,
                    } => {
                        commands::apm::sampling_rules_create(
                            &cfg,
                            service,
                            env,
                            resource,
                            sample_rate,
                        )
                        .await?;
                    }
                    ApmSamplingRulesActions::Update {
                        id,
                        service,
                        env,
                        resource,
                        sample_rate,
                    } => {
                        commands::apm::sampling_rules_update(
                            &cfg,
                            id,
                            service,
                            env,
                            resource,
                            sample_rate,
                        )
                        .await?;
                    }
                    ApmSamplingRulesActions::Delete { id } => {
                        commands::apm::sampling_rules_delete(&cfg, id).await?;
                    }
                },
                ApmActions::AdaptiveSampling { action } => match action {
                    ApmAdaptiveSamplingActions::OnboardingStatus { service, env } => {
                        commands::apm::adaptive_sampling_onboarding_status(&cfg, service, env)
                            .await?;
                    }
                    ApmAdaptiveSamplingActions::Onboard { service, env } => {
                        commands::apm::adaptive_sampling_onboard(&cfg, service, env).await?;
                    }
                    ApmAdaptiveSamplingActions::Offboard { service, env } => {
                        commands::apm::adaptive_sampling_offboard(&cfg, service, env).await?;
                    }
                    ApmAdaptiveSamplingActions::GetAllotment => {
                        commands::apm::adaptive_sampling_get_allotment(&cfg).await?;
                    }
                    ApmAdaptiveSamplingActions::SetAllotment { bytes, percent } => {
                        if bytes.is_none() && percent.is_none() {
                            anyhow::bail!("must provide --bytes or --percent");
                        }
                        commands::apm::adaptive_sampling_set_allotment(&cfg, bytes, percent)
                            .await?;
                    }
                    ApmAdaptiveSamplingActions::Check => {
                        commands::apm::adaptive_sampling_check(&cfg).await?;
                    }
                    ApmAdaptiveSamplingActions::Preview { bytes, percent } => {
                        if bytes.is_none() && percent.is_none() {
                            anyhow::bail!("must provide --bytes or --percent");
                        }
                        commands::apm::adaptive_sampling_preview(&cfg, bytes, percent).await?;
                    }
                },
                ApmActions::ServiceConfig { action } => match action {
                    ApmServiceConfigActions::Get {
                        service_name,
                        env,
                        service_instance_ids,
                    } => {
                        commands::apm::service_config_get(
                            &cfg,
                            service_name,
                            env,
                            service_instance_ids,
                        )
                        .await?;
                    }
                },
                ApmActions::ServiceLibraryConfig { action } => match action {
                    ApmServiceLibraryConfigActions::Get {
                        service_name,
                        env,
                        language,
                        mixed,
                    } => {
                        commands::apm::service_library_config_get(
                            &cfg,
                            service_name,
                            env,
                            language,
                            mixed,
                        )
                        .await?;
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
                DdsqlActions::Spec => {
                    commands::ddsql::spec(&cfg).await?;
                }
                DdsqlActions::Schema { action } => match action {
                    DdsqlSchemaActions::Tables {
                        query,
                        limit,
                        offset,
                    } => {
                        commands::ddsql::schema_tables(&cfg, query.as_deref(), limit, offset)
                            .await?;
                    }
                    DdsqlSchemaActions::Columns {
                        table_id,
                        limit,
                        offset,
                    } => {
                        commands::ddsql::schema_columns(&cfg, &table_id, limit, offset).await?;
                    }
                },
            }
        }
        // --- Investigations ---
        Commands::Investigations { action } => {
            cfg.validate_auth()?;
            match action {
                InvestigationActions::List {
                    page_limit,
                    page_offset,
                    monitor_id,
                } => {
                    commands::investigations::list(&cfg, page_limit, page_offset, monitor_id)
                        .await?;
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
        // --- Network ---
        Commands::Network { action } => match action {
            NetworkActions::List => {
                anyhow::bail!(
                    "the bare `pup network list` is not implemented; use `pup network flows`, `pup network devices`, or `pup network interfaces`"
                )
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
        // --- Scorecards ---
        Commands::Scorecards { action } => {
            cfg.validate_auth()?;
            match action {
                ScorecardsActions::List => {
                    commands::scorecards::list_scorecards(&cfg).await?;
                }
                ScorecardsActions::Rules { action } => match action {
                    ScorecardsRulesActions::List => {
                        commands::scorecards::rules_list(&cfg).await?;
                    }
                    ScorecardsRulesActions::Create { file } => {
                        commands::scorecards::rules_create(&cfg, &file).await?;
                    }
                    ScorecardsRulesActions::Update { rule_id, file } => {
                        commands::scorecards::rules_update(&cfg, &rule_id, &file).await?;
                    }
                    ScorecardsRulesActions::Delete { rule_id } => {
                        commands::scorecards::rules_delete(&cfg, &rule_id).await?;
                    }
                },
                ScorecardsActions::Outcomes { action } => match action {
                    ScorecardsOutcomesActions::List => {
                        commands::scorecards::outcomes_list(&cfg).await?;
                    }
                    ScorecardsOutcomesActions::BatchCreate { file } => {
                        commands::scorecards::outcomes_batch_create(&cfg, &file).await?;
                    }
                },
                ScorecardsActions::Campaigns { action } => match action {
                    ScorecardsCampaignsActions::List => {
                        commands::scorecards::campaigns_list(&cfg).await?;
                    }
                    ScorecardsCampaignsActions::Get { campaign_id } => {
                        commands::scorecards::campaigns_get(&cfg, &campaign_id).await?;
                    }
                    ScorecardsCampaignsActions::Create { file } => {
                        commands::scorecards::campaigns_create(&cfg, &file).await?;
                    }
                    ScorecardsCampaignsActions::Update { campaign_id, file } => {
                        commands::scorecards::campaigns_update(&cfg, &campaign_id, &file).await?;
                    }
                    ScorecardsCampaignsActions::Delete { campaign_id } => {
                        commands::scorecards::campaigns_delete(&cfg, &campaign_id).await?;
                    }
                },
            }
        }
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
                    cursor,
                    live,
                } => {
                    commands::traces::search(&cfg, query, from, to, limit, sort, cursor, live)
                        .await?;
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
                TracesActions::Metrics { action } => match action {
                    SpansMetricsActions::List => {
                        commands::traces::metrics_list(&cfg).await?;
                    }
                    SpansMetricsActions::Get { metric_id } => {
                        commands::traces::metrics_get(&cfg, &metric_id).await?;
                    }
                    SpansMetricsActions::Create { file } => {
                        commands::traces::metrics_create(&cfg, &file).await?;
                    }
                    SpansMetricsActions::Update { metric_id, file } => {
                        commands::traces::metrics_update(&cfg, &metric_id, &file).await?;
                    }
                    SpansMetricsActions::Delete { metric_id } => {
                        commands::traces::metrics_delete(&cfg, &metric_id).await?;
                    }
                },
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
        // --- Api ---
        Commands::Api {
            endpoint,
            method,
            field,
            header,
            include,
            input,
            raw_field,
            silent,
            verbose,
        } => {
            commands::api::run(
                &cfg,
                &endpoint,
                &method,
                &header,
                &field,
                &raw_field,
                input.as_deref(),
                include,
                silent,
                verbose,
            )
            .await?;
        }
        // --- Format ---
        // No auth required: this only renders JSON the caller already has.
        #[cfg(not(target_arch = "wasm32"))]
        Commands::Format {
            input,
            count,
            command,
            next_action,
        } => {
            commands::format::run(&cfg, input.as_deref(), count, command, next_action)?;
        }
        // --- Skills ---
        #[cfg(not(target_arch = "wasm32"))]
        Commands::Skills { action } => match action {
            SkillsActions::List { entry_type } => commands::skills::list(&cfg, entry_type)?,
            SkillsActions::Install {
                platform,
                name,
                dir,
                entry_type,
                project,
            } => commands::skills::install(
                &cfg,
                platform.map(|p| p.as_canonical().to_string()),
                name,
                dir,
                entry_type,
                project,
            )?,
            SkillsActions::Path { platform, project } => {
                commands::skills::path(platform.map(|p| p.as_canonical().to_string()), project)?
            }
            // list/get hit public routes (no validate_auth here); sessions create
            // requires auth and validates it itself.
            SkillsActions::Remote { action } => match action {
                SkillsRemoteActions::List { tags, org_id } => {
                    commands::skills_remote::list(&cfg, org_id, &tags).await?;
                }
                SkillsRemoteActions::Get {
                    skill_id,
                    intent,
                    session_id,
                    org_id,
                } => {
                    commands::skills_remote::get(
                        &cfg,
                        &skill_id,
                        intent.as_deref(),
                        session_id.as_deref(),
                        org_id,
                    )
                    .await?;
                }
                SkillsRemoteActions::Sessions { action } => match action {
                    SkillsRemoteSessionsActions::Create {
                        session_id,
                        skill_ids,
                        summary,
                        status,
                    } => {
                        commands::skills_remote::sessions_create(
                            &cfg,
                            &session_id,
                            &skill_ids,
                            &summary,
                            &status,
                        )
                        .await?;
                    }
                },
            },
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
                StaticAnalysisActions::CustomRulesets { action } => match action {
                    StaticAnalysisCustomRulesetActions::Get { name } => {
                        commands::static_analysis::custom_rulesets_get(&cfg, &name).await?;
                    }
                    StaticAnalysisCustomRulesetActions::Update { name, file } => {
                        commands::static_analysis::custom_rulesets_update(&cfg, &name, &file)
                            .await?;
                    }
                    StaticAnalysisCustomRulesetActions::Delete { name } => {
                        commands::static_analysis::custom_rulesets_delete(&cfg, &name).await?;
                    }
                },
                StaticAnalysisActions::CustomRules { action } => match action {
                    StaticAnalysisCustomRuleActions::Get { ruleset, rule } => {
                        commands::static_analysis::custom_rules_get(&cfg, &ruleset, &rule).await?;
                    }
                    StaticAnalysisCustomRuleActions::Create { ruleset, file } => {
                        commands::static_analysis::custom_rules_create(&cfg, &ruleset, &file)
                            .await?;
                    }
                    StaticAnalysisCustomRuleActions::Delete { ruleset, rule } => {
                        commands::static_analysis::custom_rules_delete(&cfg, &ruleset, &rule)
                            .await?;
                    }
                    StaticAnalysisCustomRuleActions::Revisions { ruleset, rule } => {
                        commands::static_analysis::custom_rule_revisions_list(
                            &cfg, &ruleset, &rule,
                        )
                        .await?;
                    }
                    StaticAnalysisCustomRuleActions::Revision {
                        ruleset,
                        rule,
                        revision_id,
                    } => {
                        commands::static_analysis::custom_rule_revision_get(
                            &cfg,
                            &ruleset,
                            &rule,
                            &revision_id,
                        )
                        .await?;
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
                extra_scopes,
                read_only,
                site,
                callback_port,
                org_uuid,
            } => {
                if let Some(s) = site {
                    cfg.set_site_explicit(s)?;
                }
                // Gate here rather than pre-match: --site is applied above and
                // login opens a browser carrying the real SSO cookie plus DCR
                // client credentials to this host, so it must be checked even
                // when --site was not passed (the from_env site is still the target).
                #[cfg(not(feature = "browser"))]
                cfg.ensure_site_trusted(cli.trust_site, interactive, &trusted_sites)?;
                let is_read_only = read_only || cfg.read_only;
                let resolved = resolve_login_scopes(
                    scopes.as_deref(),
                    extra_scopes.as_deref(),
                    cfg.org.as_deref(),
                    is_read_only,
                );
                let resolved_port = resolve_callback_port(callback_port)?;
                // Coerce empty `--org-uuid ""` to no-hint so callees treat
                // it the same as omitting the flag, not as `dd_oid=`.
                let org_uuid_hint = org_uuid.as_deref().filter(|s| !s.is_empty());
                commands::auth::login(&cfg, resolved, resolved_port, org_uuid_hint).await?
            }
            AuthActions::Logout => commands::auth::logout(&cfg).await?,
            AuthActions::Status { site } => {
                if let Some(s) = site {
                    cfg.set_site_explicit(s)?;
                }
                #[cfg(not(feature = "browser"))]
                cfg.ensure_site_trusted(cli.trust_site, interactive, &trusted_sites)?;
                commands::auth::status(&cfg)?
            }
            #[cfg(debug_assertions)]
            AuthActions::Token => commands::auth::token(&cfg)?,
            AuthActions::Refresh => {
                // Refresh POSTs the stored refresh token to cfg.site; gate it like
                // login/status (the pre-match gate skips all Auth subcommands).
                #[cfg(not(feature = "browser"))]
                cfg.ensure_site_trusted(cli.trust_site, interactive, &trusted_sites)?;
                commands::auth::refresh(&cfg).await?
            }
            AuthActions::List => commands::auth::list(&cfg)?,
            // `auth test` only prints local config (masked keys, no network call),
            // so it sends nothing to the host and needs no trust gate.
            AuthActions::Test => commands::test::run(&cfg)?,
        },
        // --- SavedWidgets (top-level saved/reporting widgets) ---
        Commands::SavedWidgets { action } => {
            cfg.validate_auth()?;
            match action {
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
                    commands::widgets::update(&cfg, &experience_type, &widget_id, &file).await?;
                }
                WidgetActions::Delete {
                    experience_type,
                    widget_id,
                } => {
                    commands::widgets::delete(&cfg, &experience_type, &widget_id).await?;
                }
            }
        }
        // --- Workflows ---
        Commands::Workflows { action } => match action {
            WorkflowActions::Get { workflow_id } => {
                commands::workflows::get(&cfg, &workflow_id).await?;
            }
            WorkflowActions::Create { file } => {
                commands::workflows::create(&cfg, &file).await?;
            }
            WorkflowActions::Update { workflow_id, file } => {
                commands::workflows::update(&cfg, &workflow_id, &file).await?;
            }
            WorkflowActions::Diff {
                workflow_id,
                file,
                only,
                ignore,
            } => {
                commands::workflows::diff(&cfg, &workflow_id, &file, &only, &ignore).await?;
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
                commands::workflows::run(&cfg, &workflow_id, payload, payload_file, wait, &timeout)
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
                    commands::workflows::instance_cancel(&cfg, &workflow_id, &instance_id).await?;
                }
            },
            WorkflowActions::Connections { action } => match action {
                WorkflowConnectionActions::Get { connection_id } => {
                    commands::workflows::connections_get(&cfg, &connection_id).await?;
                }
                WorkflowConnectionActions::Create { file } => {
                    commands::workflows::connections_create(&cfg, &file).await?;
                }
                WorkflowConnectionActions::Update {
                    connection_id,
                    file,
                } => {
                    commands::workflows::connections_update(&cfg, &connection_id, &file).await?;
                }
                WorkflowConnectionActions::Delete { connection_id } => {
                    commands::workflows::connections_delete(&cfg, &connection_id).await?;
                }
            },
        },
        // --- Generated ---
        Commands::Generated(action) => {
            cfg.validate_auth()?;
            generated::run(&cfg, action).await?;
        }
        // --- LLM Observability ---
        Commands::LlmObs { action } => {
            cfg.validate_auth()?;
            match action {
                LlmObsActions::Projects { action } => match action {
                    LlmObsProjectsActions::Create { file } => {
                        commands::llm_obs::projects_create(&cfg, &file).await?;
                    }
                    LlmObsProjectsActions::List {
                        filter_id,
                        filter_name,
                        limit,
                        cursor,
                    } => {
                        commands::llm_obs::projects_list(
                            &cfg,
                            filter_id,
                            filter_name,
                            limit,
                            cursor,
                        )
                        .await?;
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
                        LlmObsExperimentsEventsActions::Submit {
                            experiment_id,
                            metrics,
                            tags,
                        } => {
                            commands::llm_obs::experiments_events_submit(
                                &cfg,
                                &experiment_id,
                                &metrics,
                                tags,
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
                    LlmObsDatasetsActions::List {
                        project_id,
                        filter_id,
                        filter_name,
                        limit,
                        cursor,
                    } => {
                        commands::llm_obs::datasets_list(
                            &cfg,
                            &project_id,
                            filter_id,
                            filter_name,
                            limit,
                            cursor,
                        )
                        .await?;
                    }
                    LlmObsDatasetsActions::BatchUpdate {
                        project_id,
                        dataset_id,
                        file,
                    } => {
                        commands::llm_obs::datasets_batch_update(
                            &cfg,
                            &project_id,
                            &dataset_id,
                            &file,
                        )
                        .await?;
                    }
                    LlmObsDatasetsActions::Clone {
                        project_id,
                        dataset_id,
                        file,
                    } => {
                        commands::llm_obs::datasets_clone(&cfg, &project_id, &dataset_id, &file)
                            .await?;
                    }
                    LlmObsDatasetsActions::Restore {
                        project_id,
                        dataset_id,
                        file,
                    } => {
                        commands::llm_obs::datasets_restore(&cfg, &project_id, &dataset_id, &file)
                            .await?;
                    }
                    LlmObsDatasetsActions::RecordsAll { dataset_id, limit } => {
                        commands::llm_obs::datasets_records_all(&cfg, &dataset_id, limit).await?;
                    }
                    LlmObsDatasetsActions::Records {
                        project_id,
                        dataset_id,
                        record_ids,
                        tags,
                        canonical_id,
                        dataset_version,
                        limit,
                        cursor,
                        compute_schema,
                    } => {
                        commands::llm_obs::datasets_records(
                            &cfg,
                            &project_id,
                            &dataset_id,
                            record_ids,
                            tags,
                            canonical_id,
                            dataset_version,
                            limit,
                            cursor,
                            compute_schema,
                        )
                        .await?;
                    }
                    LlmObsDatasetsActions::RecordsAdd {
                        project_id,
                        dataset_id,
                        file,
                        confirm,
                        create_new_version,
                    } => {
                        commands::llm_obs::datasets_records_add(
                            &cfg,
                            &project_id,
                            &dataset_id,
                            &file,
                            confirm,
                            create_new_version,
                        )
                        .await?;
                    }
                    LlmObsDatasetsActions::RecordsFull {
                        project_id,
                        dataset_id,
                        record_ids,
                    } => {
                        commands::llm_obs::datasets_records_full(
                            &cfg,
                            &project_id,
                            &dataset_id,
                            record_ids,
                        )
                        .await?;
                    }
                },
                LlmObsActions::Spans { action } => match action {
                    LlmObsSpansActions::Search {
                        query,
                        tags,
                        trace_id,
                        apm_trace_id,
                        span_id,
                        span_kind,
                        span_name,
                        ml_app,
                        root_spans_only,
                        from,
                        to,
                        limit,
                        cursor,
                        summary,
                    } => {
                        commands::llm_obs::spans_search(
                            &cfg,
                            query,
                            tags,
                            trace_id,
                            apm_trace_id,
                            span_id,
                            span_kind,
                            span_name,
                            ml_app,
                            root_spans_only,
                            from,
                            to,
                            limit,
                            cursor,
                            summary,
                        )
                        .await?;
                    }
                    LlmObsSpansActions::GetTrace {
                        trace_id,
                        include_tree,
                        from,
                        to,
                    } => {
                        commands::llm_obs::spans_get_trace(&cfg, &trace_id, include_tree, from, to)
                            .await?;
                    }
                    LlmObsSpansActions::GetDetails {
                        trace_id,
                        span_ids,
                        from,
                        to,
                    } => {
                        commands::llm_obs::spans_get_span_details(
                            &cfg, &trace_id, span_ids, from, to,
                        )
                        .await?;
                    }
                    LlmObsSpansActions::GetContent {
                        trace_id,
                        span_id,
                        field,
                        path,
                        max_tokens,
                        from,
                        to,
                    } => {
                        commands::llm_obs::spans_get_span_content(
                            &cfg, &trace_id, &span_id, &field, path, max_tokens, from, to,
                        )
                        .await?;
                    }
                    LlmObsSpansActions::FindErrors { trace_id, from, to } => {
                        commands::llm_obs::spans_find_error_spans(&cfg, &trace_id, from, to)
                            .await?;
                    }
                    LlmObsSpansActions::Expand {
                        trace_id,
                        span_ids,
                        max_depth,
                        filter_kind,
                        from,
                        to,
                    } => {
                        commands::llm_obs::spans_expand_spans(
                            &cfg,
                            &trace_id,
                            span_ids,
                            max_depth,
                            filter_kind,
                            from,
                            to,
                        )
                        .await?;
                    }
                    LlmObsSpansActions::GetAgentLoop {
                        trace_id,
                        span_id,
                        max_content_length,
                        from,
                        to,
                    } => {
                        commands::llm_obs::spans_get_agent_loop(
                            &cfg,
                            &trace_id,
                            span_id,
                            max_content_length,
                            from,
                            to,
                        )
                        .await?;
                    }
                },
                LlmObsActions::AnnotationQueues { action } => match action {
                    LlmObsAnnotationQueuesActions::Create { file } => {
                        commands::llm_obs::annotation_queues_create(&cfg, &file).await?;
                    }
                    LlmObsAnnotationQueuesActions::List {
                        project_id,
                        queue_ids,
                    } => {
                        commands::llm_obs::annotation_queues_list(&cfg, project_id, queue_ids)
                            .await?;
                    }
                    LlmObsAnnotationQueuesActions::Update { queue_id, file } => {
                        commands::llm_obs::annotation_queues_update(&cfg, &queue_id, &file).await?;
                    }
                    LlmObsAnnotationQueuesActions::Delete { queue_id } => {
                        commands::llm_obs::annotation_queues_delete(&cfg, &queue_id).await?;
                    }
                    LlmObsAnnotationQueuesActions::Interactions { action } => match action {
                        LlmObsAnnotationQueueInteractionsActions::Add { queue_id, file } => {
                            commands::llm_obs::annotation_queue_interactions_add(
                                &cfg, &queue_id, &file,
                            )
                            .await?;
                        }
                        LlmObsAnnotationQueueInteractionsActions::Delete { queue_id, file } => {
                            commands::llm_obs::annotation_queue_interactions_delete(
                                &cfg, &queue_id, &file,
                            )
                            .await?;
                        }
                        LlmObsAnnotationQueueInteractionsActions::List { queue_id } => {
                            commands::llm_obs::annotation_queue_interactions_list(&cfg, &queue_id)
                                .await?;
                        }
                    },
                },
                LlmObsActions::EvalConfig { action } => match action {
                    LlmObsEvalConfigActions::Get { eval_name } => {
                        commands::llm_obs::eval_config_get(&cfg, &eval_name).await?;
                    }
                    LlmObsEvalConfigActions::Update { eval_name, file } => {
                        commands::llm_obs::eval_config_update(&cfg, &eval_name, &file).await?;
                    }
                    LlmObsEvalConfigActions::Delete { eval_name } => {
                        commands::llm_obs::eval_config_delete(&cfg, &eval_name).await?;
                    }
                },
                LlmObsActions::Evals { action } => match action {
                    LlmObsEvalsActions::List => {
                        commands::llm_obs::evals_list(&cfg).await?;
                    }
                    LlmObsEvalsActions::ListByMlApp { ml_app } => {
                        commands::llm_obs::evals_list_by_ml_app(&cfg, &ml_app).await?;
                    }
                    LlmObsEvalsActions::GetEvaluator { eval_name } => {
                        commands::llm_obs::evals_get_evaluator(&cfg, &eval_name).await?;
                    }
                    LlmObsEvalsActions::GetAggregateStats {
                        eval_name,
                        ml_app,
                        from,
                        to,
                    } => {
                        commands::llm_obs::evals_get_aggregate_stats(
                            &cfg, &eval_name, ml_app, from, to,
                        )
                        .await?;
                    }
                    LlmObsEvalsActions::CreateOrUpdate { eval_name, file } => {
                        commands::llm_obs::evals_create_or_update(&cfg, &eval_name, &file).await?;
                    }
                    LlmObsEvalsActions::Delete { eval_name } => {
                        commands::llm_obs::evals_delete(&cfg, &eval_name).await?;
                    }
                },
                LlmObsActions::Patterns { action } => match action {
                    LlmObsPatternsActions::Configs { action } => match action {
                        LlmObsPatternConfigsActions::List => {
                            commands::llm_obs::patterns_configs_list(&cfg).await?;
                        }
                        LlmObsPatternConfigsActions::Get => {
                            commands::llm_obs::patterns_configs_get(&cfg).await?;
                        }
                    },
                    LlmObsPatternsActions::Runs { action } => match action {
                        LlmObsPatternRunsActions::List { config_id } => {
                            commands::llm_obs::patterns_runs_list(&cfg, &config_id).await?;
                        }
                        LlmObsPatternRunsActions::Status { config_id } => {
                            commands::llm_obs::patterns_runs_status(&cfg, &config_id).await?;
                        }
                    },
                    LlmObsPatternsActions::Topics { config_id, run_id } => {
                        commands::llm_obs::patterns_topics(&cfg, &config_id, run_id).await?;
                    }
                    LlmObsPatternsActions::TopicsWithPoints {
                        config_id,
                        run_id,
                        include_metrics,
                    } => {
                        commands::llm_obs::patterns_topics_with_points(
                            &cfg,
                            &config_id,
                            run_id,
                            include_metrics,
                        )
                        .await?;
                    }
                    LlmObsPatternsActions::Points {
                        topic_id,
                        page_size,
                        page_token,
                    } => {
                        commands::llm_obs::patterns_points(&cfg, &topic_id, page_size, page_token)
                            .await?;
                    }
                },
                LlmObsActions::AgentInsights { action } => match action {
                    LlmObsAgentInsightsActions::List {
                        ml_app,
                        status,
                        insight_type,
                        limit,
                        cursor,
                    } => {
                        commands::llm_obs::agent_insights_list(
                            &cfg,
                            ml_app,
                            status,
                            insight_type,
                            limit,
                            cursor,
                        )
                        .await?;
                    }
                    LlmObsAgentInsightsActions::Get { insight_id } => {
                        commands::llm_obs::agent_insights_get(&cfg, &insight_id).await?;
                    }
                    LlmObsAgentInsightsActions::UpdateStatus { insight_id, status } => {
                        commands::llm_obs::agent_insights_update_status(&cfg, &insight_id, &status)
                            .await?;
                    }
                    LlmObsAgentInsightsActions::SubmitFeedback {
                        insight_id,
                        feedback,
                    } => {
                        commands::llm_obs::agent_insights_submit_feedback(
                            &cfg,
                            &insight_id,
                            feedback,
                        )
                        .await?;
                    }
                },
                LlmObsActions::ModelPricing {
                    provider,
                    model,
                    limit,
                    cursor,
                } => {
                    commands::llm_obs::model_pricing(&cfg, provider, model, limit, cursor).await?;
                }
            }
        }
        // --- Profiling ---
        Commands::Profiling => {
            println!(
                "Profiling is not supported in pup yet. Use the Datadog MCP server instead:\n\nDocs: https://docs.datadoghq.com/bits_ai/mcp_server\nEnable profiling toolset: https://mcp.datadoghq.com/api/unstable/mcp-server/mcp?toolsets=core,profiling"
            );
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
                extension,
                all,
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
                        extension,
                        all,
                        tag,
                        local,
                        link,
                        name,
                        force,
                        description,
                    },
                )?;
            }
            ExtensionActions::ListRemote { source, extension } => {
                commands::extension::list_remote(&cfg, source, extension)?;
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
        // --- Processes ---
        Commands::Processes { action } => {
            cfg.validate_auth()?;
            match action {
                ProcessesActions::List {
                    search,
                    tags,
                    page_limit,
                } => {
                    commands::processes::list(&cfg, search, tags, page_limit).await?;
                }
            }
        }
        // --- LogsRestriction ---
        Commands::LogsRestriction { action } => {
            cfg.validate_auth()?;
            match action {
                LogsRestrictionActions::List => {
                    commands::logs_restriction::list(&cfg).await?;
                }
                LogsRestrictionActions::Get { query_id } => {
                    commands::logs_restriction::get(&cfg, &query_id).await?;
                }
                LogsRestrictionActions::Create { file } => {
                    commands::logs_restriction::create(&cfg, &file).await?;
                }
                LogsRestrictionActions::Update { query_id, file } => {
                    commands::logs_restriction::update(&cfg, &query_id, &file).await?;
                }
                LogsRestrictionActions::Delete { query_id } => {
                    commands::logs_restriction::delete(&cfg, &query_id).await?;
                }
                LogsRestrictionActions::Roles { action } => match action {
                    LogsRestrictionRoleActions::List { query_id } => {
                        commands::logs_restriction::roles_list(&cfg, &query_id).await?;
                    }
                    LogsRestrictionRoleActions::Add { query_id, file } => {
                        commands::logs_restriction::roles_add(&cfg, &query_id, &file).await?;
                    }
                },
            }
        }
        // --- Agentless Scanning ---
        Commands::AgentlessScanning { action } => {
            cfg.validate_auth()?;
            match action {
                AgentlessScanningActions::Aws { action } => match action {
                    AgentlessScanningAwsActions::List => {
                        commands::agentless_scanning::aws_scan_options_list(&cfg).await?;
                    }
                    AgentlessScanningAwsActions::Get { account_id } => {
                        commands::agentless_scanning::aws_scan_options_get(&cfg, &account_id)
                            .await?;
                    }
                    AgentlessScanningAwsActions::Create { file } => {
                        commands::agentless_scanning::aws_scan_options_create(&cfg, &file).await?;
                    }
                    AgentlessScanningAwsActions::Update { account_id, file } => {
                        commands::agentless_scanning::aws_scan_options_update(
                            &cfg,
                            &account_id,
                            &file,
                        )
                        .await?;
                    }
                    AgentlessScanningAwsActions::Delete { account_id } => {
                        commands::agentless_scanning::aws_scan_options_delete(&cfg, &account_id)
                            .await?;
                    }
                    AgentlessScanningAwsActions::OnDemand { action } => match action {
                        AgentlessScanningAwsOnDemandActions::List => {
                            commands::agentless_scanning::aws_on_demand_list(&cfg).await?;
                        }
                        AgentlessScanningAwsOnDemandActions::Get { task_id } => {
                            commands::agentless_scanning::aws_on_demand_get(&cfg, &task_id).await?;
                        }
                        AgentlessScanningAwsOnDemandActions::Create { file } => {
                            commands::agentless_scanning::aws_on_demand_create(&cfg, &file).await?;
                        }
                    },
                },
                AgentlessScanningActions::Azure { action } => match action {
                    AgentlessScanningAzureActions::List => {
                        commands::agentless_scanning::azure_scan_options_list(&cfg).await?;
                    }
                    AgentlessScanningAzureActions::Get { subscription_id } => {
                        commands::agentless_scanning::azure_scan_options_get(
                            &cfg,
                            &subscription_id,
                        )
                        .await?;
                    }
                    AgentlessScanningAzureActions::Create { file } => {
                        commands::agentless_scanning::azure_scan_options_create(&cfg, &file)
                            .await?;
                    }
                    AgentlessScanningAzureActions::Update {
                        subscription_id,
                        file,
                    } => {
                        commands::agentless_scanning::azure_scan_options_update(
                            &cfg,
                            &subscription_id,
                            &file,
                        )
                        .await?;
                    }
                    AgentlessScanningAzureActions::Delete { subscription_id } => {
                        commands::agentless_scanning::azure_scan_options_delete(
                            &cfg,
                            &subscription_id,
                        )
                        .await?;
                    }
                },
                AgentlessScanningActions::Gcp { action } => match action {
                    AgentlessScanningGcpActions::List => {
                        commands::agentless_scanning::gcp_scan_options_list(&cfg).await?;
                    }
                    AgentlessScanningGcpActions::Get { project_id } => {
                        commands::agentless_scanning::gcp_scan_options_get(&cfg, &project_id)
                            .await?;
                    }
                    AgentlessScanningGcpActions::Create { file } => {
                        commands::agentless_scanning::gcp_scan_options_create(&cfg, &file).await?;
                    }
                    AgentlessScanningGcpActions::Update { project_id, file } => {
                        commands::agentless_scanning::gcp_scan_options_update(
                            &cfg,
                            &project_id,
                            &file,
                        )
                        .await?;
                    }
                    AgentlessScanningGcpActions::Delete { project_id } => {
                        commands::agentless_scanning::gcp_scan_options_delete(&cfg, &project_id)
                            .await?;
                    }
                },
            }
        }
        // --- CSM Threats ---
        Commands::CsmThreats { action } => {
            cfg.validate_auth()?;
            match action {
                CsmThreatsActions::AgentPolicies { action } => match action {
                    CsmThreatsAgentPolicyActions::List => {
                        commands::csm_threats::agent_policies_list(&cfg).await?;
                    }
                    CsmThreatsAgentPolicyActions::Get { policy_id } => {
                        commands::csm_threats::agent_policies_get(&cfg, &policy_id).await?;
                    }
                    CsmThreatsAgentPolicyActions::Create { file } => {
                        commands::csm_threats::agent_policies_create(&cfg, &file).await?;
                    }
                    CsmThreatsAgentPolicyActions::Update { policy_id, file } => {
                        commands::csm_threats::agent_policies_update(&cfg, &policy_id, &file)
                            .await?;
                    }
                    CsmThreatsAgentPolicyActions::Delete { policy_id } => {
                        commands::csm_threats::agent_policies_delete(&cfg, &policy_id).await?;
                    }
                },
                CsmThreatsActions::AgentRules { action } => match action {
                    CsmThreatsAgentRuleActions::List { policy_id } => {
                        commands::csm_threats::agent_rules_list(&cfg, policy_id).await?;
                    }
                    CsmThreatsAgentRuleActions::Get { rule_id, policy_id } => {
                        commands::csm_threats::agent_rules_get(&cfg, &rule_id, policy_id).await?;
                    }
                    CsmThreatsAgentRuleActions::Create { file } => {
                        commands::csm_threats::agent_rules_create(&cfg, &file).await?;
                    }
                    CsmThreatsAgentRuleActions::Update {
                        rule_id,
                        file,
                        policy_id,
                    } => {
                        commands::csm_threats::agent_rules_update(&cfg, &rule_id, &file, policy_id)
                            .await?;
                    }
                    CsmThreatsAgentRuleActions::Delete { rule_id, policy_id } => {
                        commands::csm_threats::agent_rules_delete(&cfg, &rule_id, policy_id)
                            .await?;
                    }
                },
                CsmThreatsActions::BackendRules { action } => match action {
                    CsmThreatsBackendRuleActions::List { query } => {
                        commands::csm_threats::backend_rules_list(&cfg, query).await?;
                    }
                    CsmThreatsBackendRuleActions::Get { rule_id } => {
                        commands::csm_threats::backend_rules_get(&cfg, &rule_id).await?;
                    }
                    CsmThreatsBackendRuleActions::Create { file } => {
                        commands::csm_threats::backend_rules_create(&cfg, &file).await?;
                    }
                    CsmThreatsBackendRuleActions::Update { rule_id, file } => {
                        commands::csm_threats::backend_rules_update(&cfg, &rule_id, &file).await?;
                    }
                    CsmThreatsBackendRuleActions::Delete { rule_id } => {
                        commands::csm_threats::backend_rules_delete(&cfg, &rule_id).await?;
                    }
                    CsmThreatsBackendRuleActions::Validate { file } => {
                        commands::csm_threats::backend_rules_validate(&cfg, &file).await?;
                    }
                },
                CsmThreatsActions::Policy { action } => match action {
                    CsmThreatsPolicyActions::Download => {
                        commands::csm_threats::policy_download(&cfg).await?;
                    }
                },
            }
        }
        // --- Kafka (Experimental) ---
        Commands::Kafka { action } => {
            cfg.validate_auth()?;
            match action {
                KafkaActions::TopicConfigs {
                    kafka_cluster_id,
                    topic,
                } => {
                    commands::kafka::topic_configs(&cfg, &kafka_cluster_id, &topic).await?;
                }
                KafkaActions::BrokerConfigs {
                    kafka_cluster_id,
                    broker_id,
                } => {
                    commands::kafka::broker_configs(&cfg, &kafka_cluster_id, &broker_id).await?;
                }
                KafkaActions::ClientConfigs {
                    kafka_cluster_id,
                    services,
                } => {
                    let mut parsed = Vec::with_capacity(services.len());
                    for s in services {
                        let (name, kind) = s.split_once(':').ok_or_else(|| {
                            anyhow::anyhow!(
                                "--service must be in the form NAME:producer|consumer (got {s:?})"
                            )
                        })?;
                        if kind != "producer" && kind != "consumer" {
                            anyhow::bail!(
                                "--service type must be 'producer' or 'consumer' (got {kind:?})"
                            );
                        }
                        parsed.push((name.to_string(), kind.to_string()));
                    }
                    commands::kafka::client_configs(&cfg, &kafka_cluster_id, parsed).await?;
                }
                KafkaActions::SubjectSchemas {
                    kafka_cluster_id,
                    subject,
                } => {
                    commands::kafka::subject_schemas(&cfg, &kafka_cluster_id, &subject).await?;
                }
                KafkaActions::ReadMessages {
                    cluster,
                    topic,
                    bootstrap_servers,
                    partition,
                    start_offset,
                    start_timestamp,
                    n_messages_retrieved,
                    max_scanned_messages,
                    filter,
                    consumer_group_id,
                } => {
                    commands::kafka::read_messages(
                        &cfg,
                        &cluster,
                        &topic,
                        &bootstrap_servers,
                        partition,
                        start_offset,
                        start_timestamp,
                        n_messages_retrieved,
                        max_scanned_messages,
                        filter,
                        consumer_group_id,
                    )
                    .await?;
                }
            }
        }
    }

    Ok(())
}

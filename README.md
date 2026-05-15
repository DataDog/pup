# :dog2: Give Your Agent a Puppy: Introducing Pup CLI

[![CI](https://github.com/DataDog/pup/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/DataDog/pup/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-stable-orange?logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

Every AI agent needs a loyal companion. Meet Pup — the CLI that gives your agents full access to Datadog's observability platform (because even autonomous agents need good tooling, not just tricks).

## What is Pup?

A comprehensive, AI-agent-ready CLI covering a wide range of Datadog product domains. We've unleashed the full power of Datadog's APIs so your agents can fetch metrics, sniff out errors, and track down issues without barking up the wrong API tree.

AI agents are the fastest-growing interface for infrastructure management. Companies like Vercel and AWS are racing to make their platforms agent-accessible, but we're leading the pack. Pup makes Datadog a great choice for AI-native workflows by exposing the API surface in a way agents can navigate without barking up the wrong tree.

## Why Your Agent Will Love It

- :paw_prints: **Well-trained**: Self-discoverable commands (no need to chase documentation)
- :guide_dog: **Obedient**: Structured JSON/YAML output for easy parsing
- :service_dog: **On a leash**: OAuth2 + PKCE for scoped access (no more long-lived keys running wild)
- :dog: **Knows all the tricks**: Monitors, logs, metrics, RUM, security and more!

## Try It (Humans Welcome Too!)

```bash
# Give your agent credentials (house-training, basically)
pup auth login

# Now they can fetch data like a good pup
pup monitors list --tags="team:api-platform"         # Fetch monitors
pup logs search --query="status:error" --from="1h"   # Sniff out errors
pup metrics query --query="avg:system.cpu.user{*}"   # Track the metrics tail
```

:dog: **TL;DR**: We built a comprehensive CLI so AI agents can use Datadog like a pro. Give your agent a pup. They're housetrained, loyal, and know way more tricks than you'd expect.

*P.S. No actual puppies were harmed in the making of this CLI. Just a lot of Rust code and API endpoints.*

## API Coverage

Pup covers most major Datadog product surfaces. See
[docs/COMMANDS.md](docs/COMMANDS.md) for the canonical command reference, or run
`pup --help` (or `pup agent schema` for machine-readable output) for the live
list of commands as built.

💡 **Tip:** Use Ctrl/Cmd+F to search for specific APIs. [Request features via GitHub Issues](https://github.com/DataDog/pup/issues).

---

<details>
<summary><b>📊 Core Observability</b></summary>

| API Domain | Status | Pup Commands | Notes |
|------------|--------|--------------|-------|
| Metrics | ✅ | `metrics search`, `metrics query`, `metrics list`, `metrics get` | V1 and V2 APIs supported |
| Logs | ✅ | `logs search`, `logs list`, `logs aggregate` | V1 and V2 APIs supported |
| Events | ✅ | `events list`, `events search`, `events get` | Infrastructure event management |
| RUM | ✅ | `rum apps`, `rum sessions`, `rum metrics`, `rum retention-filters`, `rum playlists`, `rum heatmaps` | Apps, sessions, metrics, retention filters, replay playlists, heatmaps |
| APM Services | ✅ | `apm services`, `apm entities`, `apm dependencies`, `apm flow-map` | Services stats, operations, resources; entity queries; dependencies; flow visualization |
| Traces | ✅ | `traces search`, `traces aggregate`, `traces metrics` | Span search/aggregation and span-based metric definitions |
| Profiling | ⏳ | `profiling` | Not supported in pup yet. Use the Datadog MCP server: https://docs.datadoghq.com/bits_ai/mcp_server. Enable with: https://mcp.datadoghq.com/api/unstable/mcp-server/mcp?toolsets=core,profiling |
| Database Monitoring | ✅ | `dbm samples search` | DBM query sample search |
| Session Replay | ❌ | - | Not yet implemented |

</details>

<details>
<summary><b>🔔 Monitoring & Alerting</b></summary>

| API Domain | Status | Pup Commands | Notes |
|------------|--------|--------------|-------|
| Monitors | ✅ | `monitors list`, `monitors get`, `monitors delete`, `monitors search` | Full CRUD support with advanced search |
| Dashboards | ✅ | `dashboards list`, `dashboards get`, `dashboards delete`, `dashboards url` | Full management capabilities |
| SLOs | ✅ | `slos list`, `slos get`, `slos delete`, `slos status` | Full CRUD plus V2 status query |
| Synthetics | ✅ | `synthetics tests`, `synthetics locations`, `synthetics suites` | Tests, locations, and V2 suites management |
| Downtimes | ✅ | `downtime list`, `downtime get`, `downtime cancel` | Full downtime management |
| Notebooks | ✅ | `notebooks list`, `notebooks get`, `notebooks delete` | Investigation notebooks supported |
| Status Pages | ✅ | `status-pages pages`, `status-pages components`, `status-pages degradations` | **New** — Pages, components, and degradation management |
| Powerpacks | ❌ | - | Not yet implemented |
| Workflow Automation | ✅ | `workflows get`, `workflows create`, `workflows update`, `workflows delete`, `workflows run`, `workflows instances` | Full CRUD plus run and instance management (list, get, cancel) |
| Local Runbooks | ✅ | `runbooks list`, `runbooks describe`, `runbooks run`, `runbooks import`, `runbooks validate` | **New** — YAML-defined multi-step runbooks with pup/shell/http/workflow step types, variable interpolation, and reusable templates |

</details>

<details>
<summary><b>🔒 Security & Compliance</b></summary>

| API Domain | Status | Pup Commands | Notes |
|------------|--------|--------------|-------|
| Security Monitoring | ✅ | `security rules`, `security signals`, `security findings`, `security content-packs`, `security risk-scores` | Rules, signals, findings, content packs, entity risk scores |
| Cloud Security | ✅ | `security findings analyze`, `security findings schema` | DDSQL analytics for misconfigurations, identity risks, and all Cloud Security finding types |
| Application Security | ✅ | `security findings analyze`, `security asm-custom-rules`, `security asm-exclusions` | API findings via DDSQL, WAF custom rules and exclusion filters |
| Static Analysis | ✅ | `static-analysis ast`, `static-analysis custom-rulesets`, `static-analysis sca`, `static-analysis coverage` | Code security analysis |
| Audit Logs | ✅ | `audit-logs list`, `audit-logs search` | Full audit log search and listing |
| Data Governance | ✅ | `data-governance scanner-rules list` | Sensitive data scanner rules |
| CSM Threats | ✅ | `csm-threats` | Cloud Security Management threat rules and agent rules |
| Sensitive Data Scanner | ✅ | `data-governance scanner-rules list` | Listed via Data Governance row above |
| Agentless Scanning | ✅ | `agentless-scanning aws list/get/create/update/delete`, `agentless-scanning gcp list`, `agentless-scanning azure list` | Cloud agentless scanning configuration for AWS, GCP, and Azure |
| Logs Restriction | ✅ | `logs-restriction list`, `logs-restriction get`, `logs-restriction create`, `logs-restriction update`, `logs-restriction delete` | Log restriction queries for fine-grained log access control |
| Data Deletion | ✅ | `data-deletion requests list`, `data-deletion requests create`, `data-deletion requests cancel` | GDPR/compliance data deletion request management |

</details>

<details>
<summary><b>☁️ Infrastructure & Cloud</b></summary>

| API Domain | Status | Pup Commands | Notes |
|------------|--------|--------------|-------|
| Infrastructure | ✅ | `infrastructure hosts list`, `infrastructure hosts get` | Host inventory management |
| Tags | ✅ | `tags list`, `tags get`, `tags add`, `tags update`, `tags delete` | Host tag operations |
| Network | ✅ | `network flows list`, `network devices`, `network interfaces` | Network flows, device inventory, interface tags |
| Cloud (AWS) | ✅ | `cloud aws list`, `cloud aws cloud-auth persona-mappings` | AWS integration management with persona mapping CRUD |
| Cloud (GCP) | ✅ | `cloud gcp list` | GCP integration management |
| Cloud (Azure) | ✅ | `cloud azure list` | Azure integration management |
| Cloud (OCI) | ✅ | `cloud oci` | Oracle Cloud tenancy configs and products |
| Containers | ✅ | `containers list`, `containers images list` | Containers |
| Processes | ✅ | `processes list` | Process inventory query |

</details>

<details>
<summary><b>🚨 Incident & Operations</b></summary>

| API Domain | Status | Pup Commands | Notes |
|------------|--------|--------------|-------|
| Incidents | ✅ | `incidents list`, `incidents get`, `incidents attachments`, `incidents settings`, `incidents handles`, `incidents postmortem-templates` | Incident management with settings, handles, and postmortem templates |
| On-Call (Teams) | ✅ | `on-call teams` (CRUD, memberships with roles) | Full team management system with admin/member roles |
| Case Management | ✅ | `cases` (create, search, assign, archive, projects, jira, servicenow, move) | Complete case management with Jira/ServiceNow linking |
| Error Tracking | ✅ | `error-tracking issues search`, `error-tracking issues get` | Error issue search and details |
| Service Catalog | ✅ | `service-catalog list`, `service-catalog get` | Service registry management |
| Scorecards | ✅ | `scorecards list`, `scorecards get` | Service quality scores |
| Fleet Automation | ✅ | `fleet agents`, `fleet deployments`, `fleet schedules` | Agent management, deployments, schedules (Preview) |
| HAMR | ✅ | `hamr connections get`, `hamr connections create` | **New** — High Availability Multi-Region connections |
| Investigations | ✅ | `investigations list`, `investigations get`, `investigations trigger` | Bits AI SRE investigation management |
| Change Management | ✅ | `change-management create`, `change-management get`, `change-management update`, `change-management create-branch`, `change-management decisions` | Change request management with decisions and branching |
| Incident Services/Teams | ✅ | `incidents services`, `incidents teams` | Service and team CRUD scoped to incident management |
| Live Debugger | ✅ | `debugger probes list`, `debugger probes get`, `debugger probes create`, `debugger probes delete`, `debugger probes watch` | Remote log probe management for Live Debugger |
| Software Catalog | ✅ | `software-catalog entities list`, `software-catalog entities upsert`, `software-catalog kinds list`, `software-catalog relations list` | Software Catalog entity and kind management (next-gen catalog) |

</details>

<details>
<summary><b>🔧 CI/CD & Development</b></summary>

| API Domain | Status | Pup Commands | Notes |
|------------|--------|--------------|-------|
| CI Visibility | ✅ | `cicd pipelines list`, `cicd events list` | CI/CD pipeline visibility and events |
| Test Optimization | ✅ | `cicd tests`, `cicd flaky-tests`, `test-optimization` | Test events, flaky test management, and Test Optimization API |
| DORA Metrics | ✅ | `cicd dora` | DORA deployment patching |
| Code Coverage | ✅ | `code-coverage branch-summary`, `code-coverage commit-summary` | Branch and commit-level coverage summaries |
| Deployment Gates | ✅ | `deployment-gates gates`, `deployment-gates evaluations`, `deployment-gates rules` | Deployment gate CRUD, evaluation triggers, and rule management |

</details>

<details>
<summary><b>👥 Organization & Access</b></summary>

| API Domain | Status | Pup Commands | Notes |
|------------|--------|--------------|-------|
| Users | ✅ | `users list`, `users get`, `users roles`, `users seats` | User and role management with seat assignment |
| Organizations | ✅ | `organizations get`, `organizations list` | Organization settings management |
| API Keys | ✅ | `api-keys list`, `api-keys get`, `api-keys create`, `api-keys delete` | Full API key CRUD |
| App Keys | ✅ | `app-keys list`, `app-keys get`, `app-keys create`, `app-keys update`, `app-keys delete` | Full application key CRUD |
| Service Accounts | ✅ | - | Managed via users commands |
| Roles | ❌ | - | Only list via users |
| AuthN Mappings | ✅ | `authn-mappings list`, `authn-mappings get`, `authn-mappings create`, `authn-mappings update`, `authn-mappings delete` | SAML/IdP attribute-to-role authentication mappings |

</details>

<details>
<summary><b>⚙️ Platform & Configuration</b></summary>

| API Domain | Status | Pup Commands | Notes |
|------------|--------|--------------|-------|
| Usage Metering | ✅ | `usage summary`, `usage hourly` | Usage and billing metrics |
| Cost Management | ✅ | `costs datadog projected`, `costs datadog attribution`, `costs datadog by-org`, `costs datadog aws-config`, `costs datadog azure-config`, `costs datadog gcp-config`, `costs ccm custom-costs`, `costs ccm tag-descriptions`, `costs ccm tag-metadata`, `costs ccm tags`, `costs ccm tag-keys`, `costs ccm budgets`, `costs ccm commitments` | Cost attribution, cloud cost configs (AWS/Azure/GCP), and Cloud Cost Management (custom costs, tag descriptions, budgets, commitment programs) |
| Product Analytics | ✅ | `product-analytics events send`, `product-analytics query` | Server-side product analytics events and queries |
| Integrations | ✅ | `integrations slack`, `integrations pagerduty`, `integrations webhooks`, `integrations jira`, `integrations servicenow`, `integrations google-chat`, `integrations ms-teams` | Third-party integrations including Jira, ServiceNow, Google Chat, and Microsoft Teams |
| Feature Flags | ✅ | `feature-flags flags`, `feature-flags environments`, `feature-flags allocations`, `feature-flags exposure`, `feature-flags enable`, `feature-flags disable` | Feature flag management with environment, allocation, and exposure control |
| Data Streams (Kafka) | ✅ | `kafka topic-configs`, `kafka broker-configs`, `kafka client-configs`, `kafka read-messages` | **Experimental** — Kafka cluster inspection via Datadog |
| Restricted Datasets | ✅ | `datasets list`, `datasets get`, `datasets create`, `datasets update`, `datasets delete` | Restricted dataset management for data access control |
| Observability Pipelines | ✅ | `obs-pipelines list`, `obs-pipelines get`, `obs-pipelines create`, `obs-pipelines update`, `obs-pipelines delete`, `obs-pipelines validate` | Full pipeline CRUD and validation |
| LLM Observability | ✅ | `llm-obs projects`, `llm-obs experiments`, `llm-obs datasets` | **New** — LLM Obs projects, experiments, and dataset management |
| Reference Tables | ✅ | `reference-tables list`, `reference-tables get`, `reference-tables create`, `reference-tables batch-query` | **New** — Reference table management for log enrichment |
| Miscellaneous | ✅ | `misc ip-ranges`, `misc status` | IP ranges and status |
| App Builder | ✅ | `app-builder list`, `app-builder get`, `app-builder create`, `app-builder update`, `app-builder delete`, `app-builder publish` | Low-code app management with publish/unpublish and batch delete |
| Key Management | ✅ | `api-keys`, `app-keys` | API key and application key CRUD (listed in Organization & Access above) |
| IP Allowlist | ❌ | - | Not yet implemented |

</details>

## Installation

### Homebrew (macOS/Linux)

```bash
brew tap datadog-labs/pack
brew install datadog-labs/pack/pup
```

### Build from Source

```bash
git clone https://github.com/DataDog/pup.git && cd pup
cargo build --release
cp target/release/pup /usr/local/bin/pup
```

### Manual Download

Download pre-built binaries from the [latest release](https://github.com/DataDog/pup/releases/latest).

## Authentication

Pup supports two authentication methods. **OAuth2 is preferred** and will be used automatically if you've logged in.

### OAuth2 Authentication (Preferred)

OAuth2 provides secure, browser-based authentication with automatic token refresh.

```bash
# Set your Datadog site (optional)
export DD_SITE="datadoghq.com"  # Defaults to datadoghq.com

# Login via browser
pup auth login

# Use any command - OAuth tokens are used automatically
pup monitors list

# Check status
pup auth status

# Logout
pup auth logout
```

**Token Storage**: Tokens are stored securely in your system's keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service). Set `DD_TOKEN_STORAGE=file` to use file-based storage instead.

**Note**: OAuth2 requires Dynamic Client Registration (DCR) to be enabled on your Datadog site. If DCR is not available yet, use API key authentication.

See [docs/OAUTH2.md](docs/OAUTH2.md) for detailed OAuth2 documentation.

### API Key Authentication (Fallback)

If OAuth2 tokens are not available, Pup automatically falls back to API key authentication.

```bash
export DD_API_KEY="your-datadog-api-key"
export DD_APP_KEY="your-datadog-application-key"
export DD_SITE="datadoghq.com"  # Optional, defaults to datadoghq.com

# Use any command - API keys are used automatically
pup monitors list
```

### Bearer Token Authentication (WASM / Headless)

For WASM builds or environments without keychain access, use a pre-obtained bearer token:

```bash
export DD_ACCESS_TOKEN="your-oauth-access-token"
export DD_SITE="datadoghq.com"

pup monitors list
```

API key authentication (`DD_API_KEY` + `DD_APP_KEY`) also works in WASM. See the [WASM](#wasm) section below.

### Authentication Priority

Pup checks for authentication in this order:
1. **`DD_ACCESS_TOKEN`** - Stateless bearer token (highest priority)
2. **OAuth2 tokens** (from `pup auth login`) - Used if valid tokens exist
3. **API keys** (from `DD_API_KEY` and `DD_APP_KEY`) - Used if OAuth tokens not available

## Usage

### Authentication

```bash
# OAuth2 login (recommended)
pup auth login

# Check authentication status
pup auth status

# Refresh access token
pup auth refresh

# Logout
pup auth logout
```

### Test Connection

```bash
pup auth test
```

### Monitors

```bash
# List all monitors
pup monitors list

# Get specific monitor
pup monitors get 12345678

# Delete monitor
pup monitors delete 12345678 --yes
```

### Metrics

```bash
# Search metrics using classic query syntax (v1 API)
pup metrics search --query="avg:system.cpu.user{*}" --from="1h"

# Query time-series data (v2 API)
pup metrics query --query="avg:system.cpu.user{*}" --from="1h"

# List available metrics
pup metrics list --filter="system.*"
```

### Dashboards

```bash
# List all dashboards
pup dashboards list

# Get dashboard details
pup dashboards get abc-123-def

# Print a live 1 week dashboard URL
pup dashboards url abc-123-def --from=now-1w --to=now --live=true

# Delete dashboard
pup dashboards delete abc-123-def --yes
```

### SLOs

```bash
# List all SLOs
pup slos list

# Get SLO details
pup slos get abc-123

# Delete SLO
pup slos delete abc-123 --yes
```

### Incidents

```bash
# List all incidents
pup incidents list

# Get incident details
pup incidents get abc-123-def
```

## Global Flags

- `-o, --output`: Output format (json, table, yaml) - default: json
- `-y, --yes`: Skip confirmation prompts for destructive operations

## Environment Variables

- `DD_ACCESS_TOKEN`: Bearer token for stateless auth (highest priority)
- `DD_API_KEY`: Datadog API key (optional if using OAuth2 or DD_ACCESS_TOKEN)
- `DD_APP_KEY`: Datadog Application key (optional if using OAuth2 or DD_ACCESS_TOKEN)
- `DD_SITE`: Datadog site (default: datadoghq.com)
- `DD_AUTO_APPROVE`: Auto-approve destructive operations (true/false)
- `DD_TOKEN_STORAGE`: Token storage backend (keychain or file, default: auto-detect)

## Agent Mode

When pup is invoked by an AI coding agent, it automatically switches to **agent mode** which returns structured JSON responses optimized for machine consumption (including metadata, error details, and hints). Agent mode also auto-approves confirmation prompts.

Agent mode is **auto-detected** when any of these environment variables are set to `1` or `true`:

| Variable | Agent |
|----------|-------|
| `CLAUDE_CODE` or `CLAUDECODE` | Claude Code |
| `CURSOR_AGENT` | Cursor |
| `CODEX` or `OPENAI_CODEX` | OpenAI Codex |
| `AIDER` | Aider |
| `CLINE` | Cline |
| `WINDSURF_AGENT` | Windsurf |
| `GITHUB_COPILOT` | GitHub Copilot |
| `AMAZON_Q` or `AWS_Q_DEVELOPER` | Amazon Q |
| `GEMINI_CODE_ASSIST` | Gemini Code Assist |
| `SRC_CODY` | Sourcegraph Cody |
| `PI_CODING_AGENT` | pi.dev |
| `FORCE_AGENT_MODE` | Any agent (manual override) |

You can also enable it explicitly with the `--agent` flag or by setting `FORCE_AGENT_MODE=1`:

```bash
# Auto-detected (e.g., running inside Claude Code)
pup monitors list

# Explicit flag
pup monitors list --agent

# Environment variable override
FORCE_AGENT_MODE=1 pup monitors list
```

If you are integrating pup into an AI agent workflow, make sure the appropriate environment variable is set so responses are optimized for your agent. Without it, pup defaults to human-friendly output.

## WASM

Pup compiles to WebAssembly via the `wasm32-wasip2` target for use in WASI-compatible runtimes such as Wasmtime, Wasmer, and Cloudflare Workers.

### Building

```bash
# Install the WASI target
rustup target add wasm32-wasip2

# Build for WASI
cargo build --target wasm32-wasip2 --no-default-features --features wasi --release
```

### Authentication

The WASM build supports **stateless authentication** — keychain storage and browser-based OAuth login are not available. Use either `DD_ACCESS_TOKEN` or API keys:

```bash
# Option 1: Bearer token
DD_ACCESS_TOKEN="your-token" DD_SITE="datadoghq.com" wasmtime run target/wasm32-wasip2/release/pup.wasm -- monitors list

# Option 2: API keys
DD_API_KEY="your-api-key" DD_APP_KEY="your-app-key" wasmtime run target/wasm32-wasip2/release/pup.wasm -- monitors list
```

The `pup auth status` command works in WASM and reports which credentials are configured. The `login`, `logout`, and `refresh` subcommands return guidance to use `DD_ACCESS_TOKEN`.

### Limitations

- No local token storage (keychain/file) — use `DD_ACCESS_TOKEN` or API keys
- No browser-based OAuth login flow
- Networking relies on the host runtime's networking capabilities

### Running with Wasmtime

```bash
# Run directly
wasmtime run --env DD_ACCESS_TOKEN="your-token" target/wasm32-wasip2/release/pup.wasm -- monitors list

# Or with API keys
wasmtime run --env DD_API_KEY="key" --env DD_APP_KEY="key" target/wasm32-wasip2/release/pup.wasm -- --help
```

## Runbooks

`pup runbooks` is a local execution engine for YAML-defined operational procedures. Runbooks live in `~/.config/pup/runbooks/` and encode multi-step tasks — from deployment gates to incident triage — using `pup`, shell, HTTP, Datadog Workflow, and interactive confirmation steps.

```bash
# List available runbooks
pup runbooks list

# Inspect a runbook's steps
pup runbooks describe incident-triage

# Run a runbook, passing required variables
pup runbooks run deploy-service --arg SERVICE=payments --arg VERSION=1.2.3

# Dry-run (show steps without executing)
pup runbooks run deploy-service --dry-run

# Import a runbook from a file
pup runbooks import ./my-runbook.yaml

# Validate a runbook file without running it
pup runbooks validate ./my-runbook.yaml
```

### Runbook Features

- **Step types**: `pup` (Datadog commands), `shell`, `http`, `datadog-workflow`, `confirm`
- **Variable interpolation**: `{{VAR_NAME}}` in any field, passed via `--arg KEY=VALUE`
- **Reusable templates**: Store shared step definitions in `_templates/` and reference them with `template: <name>`
- **HTTP steps**: Full method support (GET/POST/PUT/PATCH/DELETE) with `body`, `headers`, `content_type`, and `body_file`
- **Failure handling**: `on_failure: fail|warn|ignore` and `optional: true` per step
- **Conditional execution**: `when: on_success|on_failure|always`
- **Polling**: `poll.interval`, `poll.timeout`, `poll.until` for long-running operations
- **Output capture**: `capture: VAR_NAME` stores stdout for use in later steps
- **Timestamped output**: Every step shows start time, elapsed duration, and labeled stdout/stderr

See `docs/examples/runbooks/` for ready-to-use examples and [docs/EXAMPLES.md](docs/EXAMPLES.md) for full reference.

## Agent Skills

Pup ships a set of skills and domain agents embedded in the binary, installable to any AI coding assistant. Run `pup skills list` to see what's available in the version you have installed.

```bash
# Install all skills and agents for the auto-detected platform
pup skills install

# Install for a specific platform (positional arg)
pup skills install claude
pup skills install cursor
pup skills install codex
pup skills install opencode
pup skills install pi

# Install for every supported platform at once
pup skills install all

# By default installs go to the user-global directory; --project keeps them local
pup skills install claude --project

# List available skills and agents
pup skills list
pup skills list --type=skill
pup skills list --type=agent

# Install a specific skill by name
pup skills install claude --name dd-monitors
```

For Claude Code, skills install to `~/.claude/skills/` (or `.claude/skills/` with `--project`) and agents install to `~/.claude/agents/` (native subagent format). For Cursor, Codex, and opencode, everything installs as `SKILL.md` under that tool's skills directory (e.g. `~/.cursor/skills/`, `~/.codex/skills/`, `~/.config/opencode/skills/`).

Pup ships plugin manifest files for several AI coding assistants:

```
# Claude Code
/plugin marketplace add DataDog/pup

# Codex (reads .codex-plugin/plugin.json from the repo, or marketplace.json from ~/.agents/plugins/)
```

## ACP Server

`pup acp serve` turns pup into a local AI agent server, letting coding tools talk directly to Datadog Bits AI. It supports two protocols:

- **[ACP](https://agentcommunicationprotocol.dev/)** — Agent Communication Protocol for ACP-native clients
- **OpenAI-compatible** — `POST /chat/completions` for [opencode](https://opencode.ai), Cursor, and any `@ai-sdk/openai-compatible` client

```bash
# Start the server (auto-discovers your first Datadog AI agent)
pup acp serve

# Or target a specific agent
pup acp serve --agent-id <uuid> --port 9099
```

Point any OpenAI-compatible client at `http://127.0.0.1:9099` to start asking questions about your Datadog environment.

**opencode** (`~/Library/Application Support/opencode/opencode.jsonc`):
```jsonc
{
  "provider": {
    "datadog": {
      "name": "Datadog AI",
      "npm": "@ai-sdk/openai-compatible",
      "models": { "datadog-ai": { "name": "Datadog AI Agent" } },
      "options": { "baseURL": "http://127.0.0.1:9099" }
    }
  }
}
```

See [docs/EXAMPLES.md#acp-server](docs/EXAMPLES.md) for full usage details.

## Development

```bash
# Run tests
cargo test

# Build
cargo build --release

# Lint
cargo clippy -- -D warnings

# Format check
cargo fmt --check

# Build WASM
rustup target add wasm32-wasip2
cargo build --target wasm32-wasip2 --no-default-features --features wasi

# Run without building
cargo run -- monitors list
```

## License

Apache License 2.0 - see LICENSE for details.

## Documentation

For detailed documentation, see [CLAUDE.md](CLAUDE.md).

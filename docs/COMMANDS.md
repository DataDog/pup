# Command Reference

Complete reference for the command groups in Pup. Run `pup --help` (or `pup
agent schema --compact`) for the live, exhaustive list as built into your
binary — the table below is curated documentation and may lag the source.

## Command Pattern

```bash
pup <domain> <action> [options]           # Simple commands
pup <domain> <subgroup> <action> [options] # Nested commands
```

## Status Legend

- ✅ **WORKING** - Command compiles and runs (requires valid auth)
- ⚠️ **API BLOCKED** - Implementation correct, waiting for API client library updates
- ⏳ **PLACEHOLDER** - Skeleton implementation, API endpoints pending

## Command Index

| Domain | Subcommands | File | Status |
|--------|-------------|------|--------|
| acp | serve | src/commands/acp.rs | ✅ |
| auth | login, logout, status, refresh | src/commands/auth.rs | ✅ |
| metrics | query, list, search, timeseries, metadata, tags, submit | src/commands/metrics.rs | ✅ |
| logs | search, list, aggregate, saved-views (list, get, create, delete) | src/commands/logs.rs | ✅ |
| traces | metrics (list, get, create, update, delete) | src/commands/traces.rs | ✅ |
| monitors | list, get, create, update, delete, search, diff | src/commands/monitors.rs | ✅ |
| dashboards | list, get, delete, url, annotations (list, get-page, create, update, delete) | src/commands/dashboards.rs, src/commands/annotations.rs | ✅ |
| dbm | samples (search) | src/commands/dbm.rs | ✅ |
| ddsql | table, time-series, spec, schema (tables, columns) | src/commands/ddsql.rs | ✅ |
| debugger | probes (list, get, create, delete, watch) | src/commands/debugger.rs | ✅ |
| slos | list, get, delete, status | src/commands/slos.rs | ✅ |
| incidents | list, get, attachments, settings, handles, postmortem-templates | src/commands/incidents.rs | ✅ |
| rum | apps, metrics, retention-filters, sessions, playlists, heatmaps | src/commands/rum.rs | ✅ |
| cicd | pipelines, events, tests, dora, flaky-tests | src/commands/cicd.rs | ✅ |
| static-analysis | custom-rulesets (get, update, delete), custom-rules (get, create, delete, revisions, revision) | src/commands/static_analysis.rs | ✅ |
| downtime | list, get, cancel | src/commands/downtime.rs | ✅ |
| tags | list, get, add, update, delete | src/commands/tags.rs | ✅ |
| events | post, list, search, get | src/commands/events.rs | ✅ |
| on-call | teams (CRUD, memberships), pages (newest-first list, get, create) | src/commands/on_call.rs | ✅ |
| audit-logs | list, search | src/commands/audit_logs.rs | ✅ |
| api-keys | list, get, create, delete | src/commands/api_keys.rs | ✅ |
| app-keys | list, get, create, update, delete | src/commands/app_keys.rs | ✅ |
| infrastructure | hosts (list, get) | src/commands/infrastructure.rs | ✅ |
| synthetics | tests, locations, suites, downtime | src/commands/synthetics.rs | ✅ |
| symdb | search | src/commands/symdb.rs | ✅ |
| logs-restriction | list, get, create, update, delete, roles (list, add) | src/commands/logs_restriction.rs | ✅ |
| processes | list | src/commands/processes.rs | ✅ |
| users | list, get, roles, service-accounts (create, app-keys CRUD) | src/commands/users.rs | ✅ |
| notebooks | list, get, delete, annotations (list, get-page, create, update, delete) | src/commands/notebooks.rs, src/commands/annotations.rs | ✅ |
| security | rules, signals, findings, content-packs, risk-scores | src/commands/security.rs | ✅ |
| organizations | get, list | src/commands/organizations.rs | ✅ |
| service-catalog | list, get | src/commands/service_catalog.rs | ✅ |
| idp | assist, find, owner, deps, register | src/commands/idp.rs | ✅ |
| error-tracking | issues (search, get) | src/commands/error_tracking.rs | ✅ |
| scorecards | rules (list, create, update, delete), outcomes (list, batch-create) | src/commands/scorecards.rs | ✅ |
| usage | summary, hourly | src/commands/usage.rs | ✅ |
| apm | services (list, stats, operations, resources), entities (list), dependencies (list), flow-map, troubleshooting (list), service-config (get), service-library-config (get) | src/commands/apm.rs | ✅ |
| containers | list, images (list) | src/commands/containers.rs | ✅ |
| costs | datadog (projected, attribution, by-org, aws-config, azure-config, gcp-config), ccm (custom-costs, tag-descriptions, tag-metadata, tags, tag-keys, budgets, commitments) | src/commands/cost.rs, src/commands/cost_ccm.rs | ✅ |
| product-analytics | events send | src/commands/product_analytics.rs | ✅ |
| profiling | none | n/a | ⏳ |
| datasets | list, get, create, update, delete | src/commands/datasets.rs | ✅ |
| data-deletion | requests (list, create, cancel) | src/commands/data_deletion.rs | ✅ |
| data-governance | scanner-rules (list) | src/commands/data_governance.rs | ✅ |
| obs-pipelines | list, get, create, update, delete, validate | src/commands/obs_pipelines.rs | ✅ |
| llm-obs | projects (create, list), experiments (create, list, update, delete, summary, events (list, get, submit), metric-values, dimension-values), datasets (create, list, batch-update, clone, restore, records, records-add, records-all, records-full), spans (search), patterns (configs (list, get), runs (list, status), topics, topics-with-points, points), agent-insights (list, get, update-status, submit-feedback), model-pricing | src/commands/llm_obs.rs | ✅ |
| reference-tables | list, get, create, batch-query | src/commands/reference_tables.rs | ✅ |
| network | flows list, devices (list, get, interfaces, tags), interfaces (list, update) | src/commands/network.rs | ✅ |
| cloud | aws, gcp, azure, oci | src/commands/cloud.rs | ✅ |
| integrations | slack, pagerduty, webhooks, jira, servicenow | src/commands/integrations.rs | ✅ |
| misc | ip-ranges, status | src/commands/misc.rs | ✅ |
| cases | create, get, search, assign, archive, projects, jira, servicenow, move | src/commands/cases.rs | ✅ |
| status-pages | pages, components, degradations | src/commands/status_pages.rs | ✅ |
| code-coverage | branch-summary, commit-summary | src/commands/code_coverage.rs | ✅ |
| hamr | connections (get, create) | src/commands/hamr.rs | ✅ |
| fleet | agents (list, get, versions, tracers), deployments (list, get, configure, upgrade, cancel), schedules (list, get, create, update, delete, trigger), tracers (list), clusters (list), instrumented-pods (list) | src/commands/fleet.rs | ✅ |
| skills | list, install, path (positional `<platform>`: claude/cursor/codex/opencode/windsurf/gemini/pi/devin/all; `--name`, `--type`, `--project` for project-local scope) | src/commands/skills.rs | ✅ |
| runbooks | list, describe, run, import, validate | src/commands/runbooks.rs | ✅ |
| workflows | get, create, update, diff, delete, run, instances (list, get, cancel), connections (get, create, update, delete) | src/commands/workflows.rs | ✅ |
| investigations | list, get, trigger | src/commands/investigations.rs | ✅ |
| change-requests | create, get, update, create-branch, decisions (update, delete) | src/commands/change_management.rs | ✅ |
| change-stories | list | src/commands/change_stories.rs | ✅ |
| app-builder | list, get, create, update, delete, delete-batch, publish, unpublish | src/commands/app_builder.rs | ✅ |

**Note:** RUM command is fully operational. Apps and sessions work completely. Metrics and retention-filters support list/get operations (create/update/delete operations pending due to complex API type structures).

**Auth note:** All workflow commands require `DD_API_KEY` + `DD_APP_KEY`. OAuth2 bearer tokens are not supported for workflow operations.

**Profiling note:** `pup profiling` has no subcommands yet. Use the Datadog MCP server instead: https://docs.datadoghq.com/bits_ai/mcp_server. Enable profiling in the MCP toolset with: https://mcp.datadoghq.com/api/unstable/mcp-server/mcp?toolsets=core,profiling

## Common Patterns

### List Operations
```bash
pup <domain> list [--flags]
pup monitors list --tags="env:production"
pup dashboards list
```

### Get Operations
```bash
pup <domain> get <id>
pup monitors get 12345678
pup slos get abc-123-def
```

### Search/Query
```bash
pup logs search --query="status:error" --from="1h"
pup logs search --query="service:api" --from="7d" --storage="flex"
pup logs query --query="service:api" --index="main,security" --from="1h"
pup logs saved-views create --file=saved-view.json
pup dbm samples search --query="dbm_type:activity service:orders env:prod" --from="1h" --limit=10
pup metrics search --query="avg:system.cpu.user{*}" --from="1h"
pup metrics query --query="avg:system.cpu.user{*}" --from="1h"
pup metrics tags list system.cpu.user --window-seconds=3600
pup metrics timeseries --file=request.json
pup events search --query="@user.id:12345"
```

### Create/Update/Delete
```bash
pup <domain> create [--flags]
pup <domain> update <id> [--flags]
pup <domain> delete <id> [--yes]
pup events post --tags="version:1,application:web" --no_host --type=my_apps --aggregation_key=application:web --alert_type=info "Something big happened!" "And let me tell you all about it here!"
```

### Nested Commands
```bash
pup rum apps list
pup rum metrics get <id>
pup cicd pipelines list
pup security rules list
pup infrastructure hosts list
```

## Domain Categories

### Data & Observability
- **metrics** - Time-series metrics (query, list, get, search)
- **logs** - Log search and analysis (search, list, aggregate, saved views)
- **dbm** - Database Monitoring query samples (samples search)
- **traces** - APM spans metrics (list, get, create, update, delete)
- **rum** - Real User Monitoring (apps, metrics, retention-filters, sessions)
- **events** - Infrastructure events (post, list, search, get)
- **ddsql** - DDSQL queries and discovery (table, time-series, spec, schema)
- **symdb** - Symbol Database queries (search scopes, probe locations)

### Monitoring & Alerting
- **monitors** - Monitor management (list, get, delete)
- **dashboards** - Dashboard management (list, get, delete, url)
- **slos** - Service Level Objectives (list, get, delete, status)
- **synthetics** - Synthetic monitoring (tests, locations, suites, downtime)
- **notebooks** - Investigation notebooks (list, get, delete)
- **downtime** - Monitor downtime (list, get, cancel)
- **status-pages** - Status pages with components and degradations

### Infrastructure & Performance
- **infrastructure** - Host inventory (hosts list, hosts get)
- **network** - Network monitoring (flows list, devices list/get/interfaces/tags, interfaces list/update)
- **tags** - Host tag management (list, get, add, update, delete)
- **profiling** - Placeholder that points users to the Datadog MCP server for profiler data

### Security & Compliance
- **security** - Security monitoring (rules, signals, findings, content-packs, risk-scores)
  - `pup security findings mute --file <body.json>` — Mute or unmute up to 100 findings (stable, SDK #1519/#1660)
  - `pup security rules bulk-convert --file <payload.json>` — Bulk convert existing rules to Terraform ZIP archive (SDK #1675)
- **static-analysis** - Code security (custom-rulesets, custom-rules)
- **audit-logs** - Audit trail (list, search)
- **data-governance** - Sensitive data scanning (scanner-rules list)

### Cloud & Integrations
- **cloud** - Cloud providers (aws, gcp, azure, oci)
- **integrations** - Third-party integrations (slack, pagerduty, webhooks, jira, servicenow)

### Development & Quality
- **cicd** - CI/CD visibility (pipelines, events, tests, dora, flaky-tests)
- **code-coverage** - Code coverage summaries (branch, commit)
- **error-tracking** - Error management (issues search, issues get); search supports `--state`, `--team`, `--assignee` filters
- **scorecards** - Service quality (rules, outcomes)
- **service-catalog** - Service registry (list, get)
- **idp** - Service Catalog agent access (assist, find, owner, deps, register)
- **debugger** - Live Debugger (probes list, get, create, delete, watch)

### Operations & Incident Response
- **incidents** - Incident management (list, get, attachments, settings, handles, postmortem-templates)
- **on-call** - Team management (create, update, delete teams; manage memberships with roles) and pages (newest-first list, get, create)
- **cases** - Case management (create, search, assign, archive, unarchive, update, projects, jira, servicenow, move)
- **hamr** - High Availability Multi-Region connections
- **fleet** - Fleet Automation (agents, deployments, schedules, tracers, clusters, instrumented-pods)
- **runbooks** - Local runbook execution engine (list, describe, run, import, validate)
- **workflows** - Workflow Automation (get, create, update, diff, delete, run, instances, connections)
- **investigations** - Bits AI SRE investigations (list, get, trigger)
- **change-requests** - Change request management (create, get, update, create-branch, decisions)
- **change-stories** - Change events for a service (deployments, feature flags, config, k8s, watchdog) over time window

### Organization & Access
- **users** - User management (list, get, roles)
- **organizations** - Org settings (get, list)
- **api-keys** - API key management (list, get, create, delete)
- **app-keys** - Application key management (list, get, create, update, delete)

### Cost & Usage
- **usage** - Usage and billing (summary, hourly)
- **costs** - Cost management: `datadog` subgroup (projected, attribution, by-org, aws-config, azure-config, gcp-config), `ccm` subgroup (custom-costs, tag-descriptions, tag-metadata, tags, tag-keys, budgets, commitments), `oci-configs` subgroup (list), and `anomalies` subgroup (list)

### Configuration & Data Management
- **obs-pipelines** - Observability pipelines (list, get, create, update, delete, validate)
- **llm-obs** - LLM Observability (projects, experiments, datasets, spans, agent insights, model pricing)
- **reference-tables** - Reference tables for log enrichment (list, get, create, batch-query)
- **misc** - Miscellaneous (ip-ranges, status)
- **product-analytics** - Product analytics events (send, query scalar/timeseries)
- **app-builder** - Low-code app management (list, get, create, update, delete, publish, unpublish)

## Global Flags

Available on all commands:

```bash
--config string      Config file path (default: ~/.config/pup/config.yaml)
--site string        Datadog site (default: datadoghq.com)
--output string      Output format: json, yaml, table (default: json)
--jq string          Filter/transform output with a jq expression (applied before formatting)
--verbose            Enable verbose logging
--yes                Skip confirmation prompts
--read-only          Block all write operations (create, update, delete)
```

### `--jq` filtering

`--jq` applies a [jq](https://jqlang.github.io/jq/) expression to the raw JSON response
**before** output formatting, so it works with every `-o` format:

```bash
# Extract a single field across all monitors
pup monitors list --jq '.[].name'

# Select matching records and then format as a table
pup monitors list --jq '.[] | select(.name | endswith("prod"))' -o table

# Compose with other jq features
pup logs search --query="status:error" --jq '.data | length'
```

**Cardinality:** the jq expression may produce a stream of values.
- 0 outputs → `null`
- 1 output → the value (unwrapped)
- 2+ outputs → an array

**Agent mode — filter target:** `--jq` runs on the **raw response payload**, which
is the value that appears under `.data` in agent mode. Write expressions against the
payload (e.g. `.[]`), **not** against the envelope (`.data[]` will not work):

```bash
# correct — targets the payload array
pup monitors list --agent --jq '.[0]'

# wrong — .data does not exist in the payload --jq sees
pup monitors list --agent --jq '.data[0]'
```

**Agent mode — metadata:** when `--jq` is active, `metadata.count` and
`metadata.truncated` are omitted from the envelope because they describe the
pre-filter data, not the filtered result.

**Limitation:** commands that print output directly (e.g. `pup auth login`, some runbook
steps) bypass `format_and_print` and do not honor `--jq`.

## Recent Enhancements

### v0.64.x — Error Tracking Issue Filters (SDK PRs #1568, #1480)

- **error-tracking issues search** — new optional filter flags:
  - `--state <STATE>` — filter by issue state: `OPEN`, `ACKNOWLEDGED`, `RESOLVED`, `IGNORED`, `EXCLUDED`
  - `--team <UUID>` — filter by team UUID assignee
  - `--assignee <UUID>` — filter by user UUID assignee
  - These flags are independent of the existing `--track`/`--persona` mutual exclusion

### v0.34.1 — ACP Server (Datadog AI Agent Integration)

- ✅ **acp** (new) — Local ACP + OpenAI-compatible server that proxies to Datadog Bits AI
  - `serve` — Start the server (default port 9099)
  - `serve --agent-id <uuid>` — Target a specific Datadog Bits AI agent (auto-discovers if omitted)
  - `serve --port 8080 --host 0.0.0.0` — Custom bind address
  - Implements [Agent Communication Protocol (ACP)](https://agentcommunicationprotocol.dev/) at `POST /runs` and `POST /runs/stream`
  - Also exposes OpenAI-compatible `POST /chat/completions` and `GET /models` for tools like [opencode](https://opencode.ai)
  - Requires OAuth2 (`pup auth login`) with `notebooks_read` + `notebooks_write` scopes

### v0.33.4 — IDP Commands for Service Catalog

- ✅ **idp** (new) — Agent-native access to the Datadog Service Catalog
  - `assist <entity>` — full context: owner, on-call, health, dependencies, metadata gaps, and suggested next actions
  - `find <query>` — search entities by name (defaults to `kind:service`)
  - `owner <entity>` — ownership + on-call responders for an entity
  - `deps <entity>` — upstream/downstream service dependencies
  - `register <file>` — POST a `service.datadog.yaml` to the Service Definitions API

### v0.28.0 — New Command Groups and Full Pipeline Implementation

- ✅ **llm-obs** (new) — LLM Observability: projects (create, list), experiments (create, list, update, delete, summary, events (list, get, submit), metric-values, dimension-values), datasets (create, list, batch-update, clone, restore, records, records-all, records-full), spans (search)
- ✅ **reference-tables** (new) — Reference table management (list, get, create, batch-query)
- ✅ **obs-pipelines** (upgraded from placeholder) — Full CRUD: list, get, create, update, delete, validate
- **costs** — Added cloud cost configs: `aws-config`, `azure-config`, `gcp-config` (list, get, create, delete each)

### v0.27.0 — Major Expansion

- ✅ **status-pages** (new) — Status page management (pages, components, degradations CRUD)
- ✅ **code-coverage** (new) — Code coverage summaries (branch-level and commit-level)
- ✅ **hamr** (new) — High Availability Multi-Region connections
- **integrations** — Added Jira integration (accounts, templates CRUD) and ServiceNow integration (instances, templates, users, assignment groups, business services)
- **cloud** — Added OCI integration (tenancy configs CRUD, products)
- **synthetics** — Added suites management (V2 API: search, get, create, update, delete)
- **synthetics** — Added downtime management (V2 API: list, create, delete)
- **security** — Added content packs (list, activate, deactivate), bulk rule export, and entity risk scores
- **incidents** — Added global settings, handles, and postmortem template management
- **cases** — Added Jira/ServiceNow issue linking, case project moves, and notification rules
- **cicd** — Added DORA deployment patching and flaky tests management
- **slos** — Added SLO status query (V2 API)
- **rum** — Replaced playlist/heatmap placeholders with working RUM Replay API implementations

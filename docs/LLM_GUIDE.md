# LLM Agent Guide for Pup CLI

This guide helps AI coding agents understand and effectively use the Pup CLI tool. It covers output behavior, discovery commands, query syntax, and common workflows.

For the machine-readable runtime reference (embedded in the binary), run `pup agent schema` (JSON).

## Output Is the Same for Agents and Humans

Pup has no separate agent mode. Whether a person or an AI coding agent runs it:

- JSON output (the default) is the raw Datadog API response body, with no wrapper.
- `--jq` filters that same body.
- `--help` prints standard text help.
- Errors print `Error: <message>` to stderr, nothing to stdout, and exit non-zero.
- Destructive operations prompt for confirmation unless `--yes` (or `DD_AUTO_APPROVE=true`) is set. When stdin is not a terminal, prompts such as the untrusted-site gate fail closed instead of waiting.

Commands you run in an agent session therefore behave exactly like the ones the user runs in their own shell or CI, so scripts you write need no special flags.

Pup detects AI coding agents (`CLAUDECODE`, `CLAUDE_CODE`, `CURSOR_AGENT`, `CODEX`, `GEMINI_CLI`, `AGENT`, and others) only to add an `ai-agent <name>` token to the User-Agent header for telemetry. The legacy `--agent` and `--no-agent` flags are accepted and ignored.

## Discovery Commands (Recommended First Steps)

Use `pup agent schema` for a machine-readable command reference:

```bash
pup agent schema              # Full JSON schema
pup agent schema --compact    # Minimal schema (names + flags only, fewer tokens)
pup agent schema logs aggregate   # One command, plus its response shape (also: logs.aggregate)
pup agent schema logs             # One domain, with a response shape for each subcommand
pup agent schema --search cache   # Find leaf commands by path or description
```

The full schema contains `version`, `auth`, `global_flags`, `commands`, `query_syntax`, `time_formats`, `workflows`, `best_practices`, and `anti_patterns`.

A path lookup prints minified JSON with:

- `output`: how commands write output — raw JSON body on stdout, what `--jq`
  sees, how errors are reported, and which commands print non-JSON text.
- `returns` on each leaf command: `documented` (`true` when the shape is
  hand-verified), a JSON Schema for the response `body`, a `jq_root` expression
  for `--jq`, and, for single-command lookups, a worked `example`.
  Commands with `documented: false` pass the Datadog API body through and do
  not publish a shape yet.

For human-readable help on any command, use `--help` (for example, `pup logs aggregate --help`).

## Authentication

```bash
# OAuth2 (recommended) — opens browser for secure login
pup auth login

# Check auth status
pup auth status

# API keys (legacy) — set environment variables
export DD_API_KEY="your-key"
export DD_APP_KEY="your-key"
export DD_SITE="datadoghq.com"
```

- OAuth2 tokens are stored in the OS keychain and refresh automatically
- Some endpoints require API keys even with OAuth2 (e.g., logs search v1)

## Command Patterns

All commands follow `pup <domain> <action> [flags]` or `pup <domain> <subgroup> <action> [flags]`.

### CRUD operations

```bash
pup <resource> list [--filters]          # List/search resources
pup <resource> get <id>                  # Get details by ID
pup <resource> delete <id> [--yes]       # Delete (--yes to skip confirmation)
pup <resource> create [--body=file.json] # Create from JSON
pup <resource> update <id> [--body=...]  # Update resource
```

### Output formats

```bash
pup monitors list --output=json   # JSON (default, recommended for agents)
pup monitors list --output=table  # Human-readable table
pup monitors list --output=yaml   # YAML
```

## Query Syntax by Domain

### Logs

```
status:error                    # Filter by status
service:web-app                 # Filter by service
@user.id:12345                  # Custom attribute (@ prefix)
host:i-*                        # Wildcard matching
"exact error message"           # Exact phrase matching
status:error AND service:web    # Boolean AND (implicit or explicit)
status:error OR status:warn     # Boolean OR
-status:info                    # Negation
@http.status_code:[400 TO 599] # Numeric range
```

```bash
# Search logs
pup logs search --query="status:error AND service:api" --from=1h --limit=100

# Aggregate logs (counting, statistics)
pup logs aggregate --query="*" --from=1h --compute="count" --group-by="service"
pup logs aggregate --query="service:api" --from=1h --compute="avg(@duration)" --group-by="service"
pup logs aggregate --query="env:prod" --from=30m --compute="percentile(@duration, 99)" --group-by="service"

# Storage tiers
pup logs search --query="*" --from=30d --storage="flex"
pup logs search --query="*" --from=1h --storage="auto"  # Try Flex, fall back to indexed logs

# Specific indexes
pup logs query --query="*" --index="main,security" --from=1h
```

### Metrics

```
<aggregation>:<metric_name>{<filter>} by {<group>}

avg:system.cpu.user{env:prod} by {host}         # CPU by host
sum:trace.servlet.request.hits{service:web}      # Request count
max:system.mem.used{*} by {host}                 # Max memory
```

```bash
pup metrics query --query="avg:system.cpu.user{env:prod} by {host}" --from=1h
pup metrics list --query="system.cpu"
```

### APM / Traces

**CRITICAL: Durations are in NANOSECONDS**
- 1ms = 1,000,000 ns
- 1s = 1,000,000,000 ns

```
service:<name>                  # Filter by service
resource_name:<path>            # Filter by endpoint
@duration:>5000000000           # Duration > 5s (nanoseconds!)
status:error                    # Error spans only
env:production                  # Filter by environment
```

```bash
pup traces search --query="service:api AND @duration:>1000000000" --from=1h
pup apm services list
```

### Monitors

```bash
pup monitors list --tags="env:production" --name="CPU"  # Filter by tags/name
pup monitors search --query="status:Alert"               # Full-text search
pup monitors get 12345678                                 # Get by ID
```

### RUM

```
@type:error                     # Error events
@type:view                      # Page views
@view.loading_time:>3000        # Slow pages (milliseconds)
@session.type:user              # Real users (not synthetic)
```

### Incidents

```bash
pup incidents list --query="status:active"
pup incidents get <incident-id>
```

### IDP entity graph

Use UEG when an answer needs connected context across service identity,
ownership, dependencies, health, source, work, or security. It can return a
selected service and dependency view in one bounded request. Discover the live
schema before filtering on unfamiliar fields or relations:

```bash
pup --read-only idp kinds list
pup --read-only idp kinds describe service
pup --read-only idp entities query 'kind:service AND name:"<service-name>"' \
  --field name,owner,service_health_status,active_incidents_count,alert_monitors_count,breached_slos_count \
  --include owner_teams,systems,upstream_services,downstream_services \
  --relation-limit 3 \
  --timeseries-interval 24h \
  --limit 1
```

Every query needs one unquoted `kind:<kind>` or concrete
`ref:"ref:<kind>:<id>"`. `--field` selects attributes; `--include` expands
declared relations. Honor result cursors, relationship truncation, and null as
unknown. `--timeseries-interval` accepts lookbacks such as `168h` or `7d`.
Use `idp assist` for a fast curated service summary and
`idp owner` for convenient ownership/on-call resolution; use UEG when graph
fidelity, relation counts, pagination, or traversal matters. Treat `find` and
`deps` as narrow legacy helpers. Install `dd-idp` for detailed DSL, recovery,
and recipe guidance:

```bash
pup skills install --name dd-idp
```

## Time Ranges

All `--from` and `--to` flags accept:

| Format | Example |
|--------|---------|
| Relative short | `1h`, `30m`, `7d`, `5s`, `1w` |
| Relative long | `5min`, `2hours`, `3days` |
| With spaces | `"5 minutes"`, `"2 hours"` |
| RFC3339 | `2024-01-01T00:00:00Z` |
| Unix ms | `1704067200000` |
| Keyword | `now` |

## Common Workflows

### Error investigation

```bash
# 1. Get error counts by service
pup logs aggregate --query="status:error" --from=1h --compute="count" --group-by="service"

# 2. Drill into affected service
pup logs search --query="status:error AND service:<name>" --from=1h --limit=20

# 3. Check monitors for that service
pup monitors list --tags="service:<name>"

# 4. Check recent events
pup events list --from=4h
```

### Performance investigation

```bash
# 1. Check service latency
pup metrics query --query="avg:trace.servlet.request.duration{service:<name>} by {resource_name}" --from=1h

# 2. Find slow traces (>5 seconds)
pup traces search --query="service:<name> AND @duration:>5000000000" --from=1h

# 3. Check resource utilization
pup metrics query --query="avg:system.cpu.user{service:<name>} by {host}" --from=1h
```

### Service health overview

```bash
pup slos list
pup monitors list --tags="team:<team_name>"
pup incidents list --query="status:active"
```

## Output and Errors

Command output is the raw API response body. Use `--jq` to select fields from it:

```bash
pup monitors list --tag='env:prod' --jq '.[].name'
```

Run `pup agent schema <command>` to see the body shape and `jq_root` for a command.

Errors go to stderr as `Error: <message>`, with nothing on stdout and a non-zero exit code. A few APIs report errors in the response body with exit 0; `returns.notes` in `pup agent schema <command>` calls these out.

## Best Practices

1. **Always specify `--from`** — most commands default to 1h but be explicit
2. **Start narrow, widen later** — begin with 1h, expand to 24h/7d only if needed
3. **Filter at the API level** — use `--tags`, `--query`, `--name` instead of fetching everything and parsing locally
4. **Use `aggregate` for counts** — don't fetch all logs and count them yourself
5. **APM durations are in nanoseconds** — 1s = 1,000,000,000
6. **Use `--yes` for approved writes** — pup does not auto-approve prompts for agents; pass `--yes` only after the user has authorized the change
7. **Check `pup agent schema`** when unsure about a command's flags
8. **Chain queries** — aggregate first to find patterns, then search for specifics

## Anti-Patterns

1. **Don't omit `--from`** on time-series queries — you'll get unexpected ranges or errors
2. **Don't use `--limit=1000` as a first step** — start small and refine
3. **Don't list all monitors without filters** in large orgs (>10k monitors)
4. **Don't assume durations are in seconds** — APM uses nanoseconds
5. **Don't fetch raw logs to count them** — use `pup logs aggregate --compute=count`
6. **Don't retry 401/403 errors** — re-authenticate or check permissions instead
7. **Don't use `--from=30d`** unless you specifically need a month of data

## Error Reference

| Status | Meaning | Suggested Action |
|--------|---------|------------------|
| 401 | Authentication failed | `pup auth login` or check DD_API_KEY/DD_APP_KEY |
| 403 | Insufficient permissions | Verify API/App key scopes |
| 404 | Resource not found | Check the resource ID |
| 429 | Rate limited | Wait and retry with backoff |
| 5xx | Server error | Retry after a short delay; check https://status.datadoghq.com/ |

## Architecture Reference

### Agent detection

- Implementation: `src/useragent.rs`
- Table-driven detector registry; first match wins
- `detect_agent_info()` returns agent name and detection status
- Used only to add `ai-agent <name>` to the User-Agent header; it does not change behavior

### Schema generation

- Implementation: `src/commands/agent.rs`
- Walks the clap command tree to generate schema
- Schema stays in sync automatically as commands are added
- Subtree schemas filter to a single domain + relevant query syntax

### Output rendering

- Implementation: `src/output.rs` (`src/formatter.rs` exposes the generator-owned contract)
- Renders the raw response as JSON, YAML, table, CSV, or TSV

## File Map

| File | Purpose |
|------|---------|
| `src/useragent.rs` | Agent detection for the User-Agent header (table-driven registry) |
| `src/commands/agent.rs` | Schema generation, `pup agent schema`, `pup agent guide` |
| `src/formatter.rs` | Generator-owned formatting contract |
| `src/output.rs` | Output rendering and printing |
| `src/main.rs` | CLI entry point, command routing, output formatting |

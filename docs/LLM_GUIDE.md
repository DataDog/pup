# LLM Agent Guide for Pup CLI

This guide helps AI coding agents understand and effectively use the Pup CLI tool. It covers the agent operability system, discovery commands, query syntax, and common workflows.

For the machine-readable runtime reference (embedded in the binary), run `pup agent schema` (JSON).

## Agent Mode

Pup auto-detects AI coding agents and switches to **agent mode**, which changes how the CLI behaves. Agent mode is triggered by any of:

| Method | Example |
|--------|---------|
| Auto-detect | `CLAUDECODE=1`, `CLAUDE_CODE=1`, `CURSOR_AGENT=1`, `CODEX=1`, `OPENAI_CODEX=1`, `OPENCODE=1`, `AIDER=1`, `CLINE=1`, `WINDSURF_AGENT=1`, `GITHUB_COPILOT=1`, `AMAZON_Q=1`, `AWS_Q_DEVELOPER=1`, `GEMINI_CODE_ASSIST=1`, `SRC_CODY=1`, `PI_CODING_AGENT=1`, `AGENT=1` |
| Explicit flag | `pup --agent <command>` |
| Environment override | `FORCE_AGENT_MODE=1` |

### What changes in agent mode

| Behavior | Human Mode | Agent Mode |
|----------|-----------|------------|
| `--help` output | Standard text help | Structured JSON schema |
| Confirmation prompts | Interactive stdin | Auto-approved (no hangs) |
| Error format | Human text with suggestions | Structured JSON with error codes |
| API response wrapping | Raw API response | Envelope with metadata (count, truncation, warnings) |

### Verifying agent mode

```bash
# This should return JSON schema (not text) when agent is detected
pup --help

# Force agent mode for testing
FORCE_AGENT_MODE=1 pup --help

# Subtree schema (only logs commands + logs query syntax)
FORCE_AGENT_MODE=1 pup logs --help
```

## Discovery Commands (Recommended First Steps)

### 1. Get full command schema

In agent mode, `--help` returns the complete JSON schema with all commands, flags, query syntax, workflows, best practices, and anti-patterns in a single call:

```bash
pup --help
# Returns: { version, auth, global_flags, commands[], query_syntax, time_formats, workflows, best_practices, anti_patterns }
```

### 2. Get domain-specific schema

```bash
pup logs --help      # Only logs commands + logs query syntax
pup monitors --help  # Only monitors commands
pup metrics --help   # Only metrics commands
```

### 3. Explicit schema commands (work regardless of agent mode)

```bash
pup agent schema              # Full JSON schema
pup agent schema --compact    # Minimal schema (names + flags only, fewer tokens)
```

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
- In agent mode, if auth fails, the error JSON includes `suggestions` with remediation steps

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

## Agent Envelope (Agent Mode Output)

In agent mode, command output is wrapped in a metadata envelope:

```json
{
  "status": "success",
  "data": [ ... ],
  "metadata": {
    "count": 42,
    "truncated": false,
    "command": "monitors list",
    "warnings": [],
    "note": "This envelope (status/data/metadata) only appears in agent mode. If you are writing a script the user will run outside this agent session, append --no-agent so the output format matches what they will see."
  }
}
```

### Authoring scripts the user will run

**The envelope only exists in agent mode.** If you write a script, alias, or runbook that the user (or CI) will run outside this agent session, those callers will not have an agent env var set — so pup will emit the raw payload, not the `{status, data, metadata}` wrapper. A script of yours that does `pup ... | jq '.data[]'` will break when the user runs it.

Append `--no-agent` whenever you produce pup commands the user will execute later:

```bash
# Interactive (agent mode auto-detected — envelope wrapped):
pup monitors list --tag='env:prod'

# In a script you're handing to the user (raw output, parity with their shell):
pup --no-agent monitors list --tag='env:prod' | jq '.[].name'
```

This is also surfaced in the agent schema under the `script_authoring` key (run `pup agent schema | jq '.script_authoring'`) and called out in `anti_patterns`.

Error responses in agent mode:

```json
{
  "status": "error",
  "error_code": 401,
  "error_message": "Authentication failed",
  "operation": "list monitors",
  "suggestions": [
    "Run 'pup auth login' to re-authenticate",
    "Or set DD_API_KEY and DD_APP_KEY environment variables"
  ]
}
```

## Best Practices

1. **Always specify `--from`** — most commands default to 1h but be explicit
2. **Start narrow, widen later** — begin with 1h, expand to 24h/7d only if needed
3. **Filter at the API level** — use `--tags`, `--query`, `--name` instead of fetching everything and parsing locally
4. **Use `aggregate` for counts** — don't fetch all logs and count them yourself
5. **APM durations are in nanoseconds** — 1s = 1,000,000,000
6. **Use `--yes` for automation** — or rely on agent mode auto-approval
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
- `is_agent_mode()` checks `FORCE_AGENT_MODE` first, then agent env vars
- `detect_agent_info()` returns agent name and detection status

### Schema generation

- Implementation: `src/commands/agent.rs`
- Walks the clap command tree to generate schema
- Schema stays in sync automatically as commands are added
- Subtree schemas filter to a single domain + relevant query syntax

### Output envelope

- Implementation: `src/formatter.rs`
- Agent envelope wraps responses with metadata (count, truncation, warnings)
- Structured error formatting for agent consumption
- Only activated when agent mode is true

### Help interception

- `src/main.rs` intercepts `--help`/`-h` before clap processes args
- When agent mode is detected, outputs structured JSON schema instead of text help
- Extracts the domain name for subtree schemas

## File Map

| File | Purpose |
|------|---------|
| `src/useragent.rs` | Agent detection (table-driven registry + FORCE_AGENT_MODE) |
| `src/commands/agent.rs` | Schema generation, `pup agent schema`, `pup agent guide` |
| `src/formatter.rs` | Agent envelope and structured errors |
| `src/config.rs` | `agent_mode` field on Config |
| `src/main.rs` | `--agent` flag, help interception, output formatting |

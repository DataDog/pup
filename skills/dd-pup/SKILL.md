---
name: dd-pup
description: Datadog CLI (Go). OAuth2 auth with token refresh.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,cli,dd-pup,pup
  alwaysApply: "false"
---

# pup (Datadog CLI)

Pup CLI for Datadog API operations. Supports OAuth2 and API key auth.

## Quick Reference

| Task | Command |
|------|---------|
| Search error logs | `pup logs search --query "status:error" --duration 1h` |
| List monitors | `pup monitors list` |
| Mute a monitor | `pup monitors mute --id 123 --duration 1h` |
| Find slow traces | `pup apm traces list --service api --min-duration 500ms` |
| List active incidents | `pup incidents list --status active` |
| Create incident | `pup incidents create --title "Issue" --severity SEV-2` |
| Query metrics | `pup metrics query --query "avg:system.cpu.user{*}"` |
| List hosts | `pup hosts list` |
| Check SLOs | `pup slos list` |
| Who's on call | `pup on-call who --team my-team` |
| Security signals | `pup security signals list --severity critical` |
| Check auth | `pup auth status` |
| Refresh token | `pup auth refresh` |

## Prerequisites

```bash
# Install pup
brew tap datadog-labs/pack
brew install pup
```

## Auth

```bash
pup auth login          # OAuth2 browser flow (recommended)
pup auth status         # Check token validity
pup auth refresh        # Refresh expired token (no browser)
pup auth logout         # Clear credentials
```

**⚠️ Tokens expire (~1 hour)**. If a command fails with 401/403 mid-conversation:

```bash
pup auth refresh        # Try refresh first
pup auth login          # If refresh fails, full re-auth
```

### Headless/CI (no browser)

```bash
# Use env vars or:
export DD_API_KEY=your-api-key
export DD_APP_KEY=your-app-key
export DD_SITE=datadoghq.com    # or datadoghq.eu, etc.
```

## Command Reference

### Monitors
```bash
pup monitors list --limit 10
pup monitors list --tags "env:prod"
pup monitors get --id 12345
pup monitors mute --id 12345 --duration 1h
pup monitors unmute --id 12345
pup monitors create --name "High CPU" --type "metric alert" \
  --query "avg(last_5m):avg:system.cpu.user{*} > 80" \
  --message "CPU high @slack-ops"
```

### Logs
```bash
pup logs search --query "status:error" --duration 1h
pup logs search --query "service:payment-api" --duration 1h --limit 100
pup logs search --query "@http.status_code:5*" --duration 24h
pup logs search --query "env:prod level:error" --duration 1h --json
```

### Metrics
```bash
pup metrics query --query "avg:system.cpu.user{*}" --duration 1h
pup metrics query --query "sum:trace.express.request.hits{service:api}" --duration 1h
pup metrics list --filter "system.*"
```

### APM / Traces
```bash
pup apm services list
pup apm traces list --service my-service --duration 1h
pup apm traces list --service api --min-duration 500ms --duration 1h
pup apm traces list --service api --status error --duration 1h
pup apm traces get abc123def456
```

### Incidents
```bash
pup incidents list --status active
pup incidents list --status resolved --duration 7d
pup incidents create --title "API Degradation" --severity SEV-2
pup incidents update --id abc-123 --status stable
pup incidents resolve --id abc-123
```

### Dashboards
```bash
pup dashboards list
pup dashboards list --tags "team:platform"
pup dashboards get --id abc-123
pup dashboards create --title "My Dashboard" --description "..." --widgets '[...]'
```

### SLOs
```bash
pup slos list
pup slos get --id slo-123
pup slos history --id slo-123 --duration 30d
```

### Synthetics
```bash
pup synthetics list
pup synthetics results --test-id abc-123
pup synthetics trigger --test-id abc-123
```

### On-Call
```bash
pup on-call teams list
pup on-call schedules list
pup on-call who --team platform-team
```

### Hosts / Infrastructure
```bash
pup hosts list --limit 50
pup hosts list --filter "env:prod"
pup hosts mute --hostname web-01 --duration 1h
pup hosts get --hostname web-01
```

### Events
```bash
pup events list --duration 24h
pup events list --tags "source:deploy"
pup events post --title "Deploy started" --text "v1.2.3" --tags "env:prod"
```

### Downtimes
```bash
pup downtime list
pup downtime create --scope "env:staging" --duration 2h --message "Maintenance"
pup downtime cancel --id 12345
```

### Users / Teams
```bash
pup users list
pup teams list
```

### Security
```bash
pup security signals list --duration 24h
pup security signals list --severity critical
```

### Service Catalog
```bash
pup services list
pup services get --name payment-api
```

### Notebooks
```bash
pup notebooks list
pup notebooks get --id 12345
```

### Workflows
```bash
pup workflows list
pup workflows trigger --id workflow-123 --input '{"key": "value"}'
```

## Subcommand Discovery

```bash
pup --help              # List all commands
pup <command> --help    # Command-specific help
```

## Error Handling

| Error | Cause | Fix |
|-------|-------|-----|
| 401 Unauthorized | Token expired | `pup auth refresh` |
| 403 Forbidden | Missing scope | Check app key permissions |
| 404 Not Found | Wrong ID/resource | Verify resource exists |
| Rate limited | Too many requests | Add delays between calls |

## Install

```bash
brew tap datadog-labs/pack
brew install pup
```

### Verify Installation

```bash
which pup
pup --version
```

## Sites

| Site | `DD_SITE` value |
|------|-----------------|
| US1 (default) | `datadoghq.com` |
| US3 | `us3.datadoghq.com` |
| US5 | `us5.datadoghq.com` |
| EU1 | `datadoghq.eu` |
| AP1 | `ap1.datadoghq.com` |
| US1-FED | `ddog-gov.com` |


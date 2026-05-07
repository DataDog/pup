---
name: dd-pup
description: Datadog CLI (pup). OAuth2 auth with token refresh.
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
| Search error logs | `pup logs search --query "status:error" --from 1h` |
| List monitors | `pup monitors list` |
| Create downtime | `pup downtime create --file downtime.json` |
| Find slow traces | `pup traces search --query="@duration:>500000000" --from="1h"` |
| List incidents | `pup incidents list` |
| Query metrics | `pup metrics query --query "avg:system.cpu.user{*}"` |
| List hosts | `pup infrastructure hosts list` |
| Check SLOs | `pup slos list` |
| On-call teams | `pup on-call teams list` |
| Security signals | `pup security signals list --query "*" --from 24h` |
| Inspect runtime values | `pup debugger probes create --service my-svc --env prod --probe-location "com.example.MyClass:myMethod"` or `"com.example.MyClass:myMethod(String, int)"` |
| Find probe-able methods | `pup symdb search --service my-svc --query MyController --view probe-locations` |
| Check auth | `pup auth status` |
| Refresh token | `pup auth refresh` |

## Prerequisites

```bash
# Install pup via Homebrew (recommended)
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
pup monitors get 12345
pup monitors search --query "High CPU"
pup monitors create --file monitor.json
pup monitors update 12345 --file monitor.json
pup monitors delete 12345
```

### Logs
```bash
pup logs search --query "status:error" --from 1h
pup logs search --query "service:payment-api" --from 1h --limit 100
pup logs search --query "@http.status_code:5*" --from 24h
pup logs aggregate --query "service:api" --compute count --from 1h
```

### Metrics
```bash
pup metrics query --query "avg:system.cpu.user{*}" --from 1h
pup metrics query --query "sum:trace.express.request.hits{service:api}" --from 1h
pup metrics list --filter "system.*"
```

### APM / Services
```bash
pup apm services list --env production
pup apm services stats --env production
pup apm services operations --env production --service my-service
pup apm services resources --env production --service my-service --name http.request
pup apm dependencies list --env production
```

### Traces
```bash
# Search traces (duration in nanoseconds: 1s = 1000000000)
pup traces search --query="service:api-gateway" --from="1h"
pup traces search --query="service:api @duration:>1000000000" --from="1h"
pup traces search --query="service:api status:error" --from="1h"
pup traces aggregate --query="service:api" --compute="avg(@duration)" --group-by="resource_name" --from="1h"
```

### Incidents
```bash
pup incidents list
pup incidents list --limit 20
pup incidents get <incident-id>
```

### Dashboards
```bash
pup dashboards list
pup dashboards get abc-123
pup dashboards create --file dashboard.json
pup dashboards update abc-123 --file dashboard.json
pup dashboards delete abc-123
```

### SLOs
```bash
pup slos list
pup slos get slo-123
pup slos status slo-123 --from 30d --to now
pup slos create --file slo.json
```

### Synthetics
```bash
pup synthetics tests list
pup synthetics tests get abc-123
pup synthetics tests search --text "login"
pup synthetics locations list
```

### Downtimes
```bash
pup downtime list
pup downtime get abc-123-def
pup downtime create --file downtime.json
pup downtime cancel abc-123-def
```

### Infrastructure / Hosts
```bash
pup infrastructure hosts list
pup infrastructure hosts list --filter "env:prod"
pup infrastructure hosts list --count 100
pup infrastructure hosts get <host-id>
```

### Events
```bash
pup events list --from 24h
pup events list --tags "source:deploy" --from 24h
pup events search --query "deploy" --from 24h
pup events get <event-id>
```

### Users / Teams
```bash
pup users list
pup users get <user-id>
pup on-call teams list
pup on-call teams get <team-id>
```

### Security
```bash
pup security signals list --query "*" --from 24h
pup security signals list --query "severity:critical" --from 24h
pup security rules list
```

### Live Debugger
```bash
# Check service context (verify env has active instances)
pup debugger context my-svc
pup debugger context my-svc --env prod

# Find probe-able methods in a service
pup symdb search --service my-svc --query MyController --view probe-locations

# Place a log probe with capture expressions
# --probe-location accepts TYPE:METHOD or TYPE:METHOD(arg1, arg2, ...) with optional signature
pup debugger probes create --service my-svc --env prod \
  --probe-location "com.example.MyController:handleRequest" \
  --capture "request.id" --capture "request.headers" \
  --ttl 1h

# With method signature (useful when the method is overloaded)
pup debugger probes create --service my-svc --env prod \
  --probe-location "com.example.MyController:handleRequest(String, HttpHeaders)" \
  --capture "request.id" --ttl 1h

# Watch probe events — compact output
pup debugger probes watch <PROBE_ID> --fields "message,captures,timestamp" --timeout 60 --limit 10 --wait 5

# Watch — template message only
pup debugger probes watch <PROBE_ID> --fields "message" --limit 10

# List and delete probes
pup debugger probes list --service my-svc
pup debugger probes delete <PROBE_ID>
```

### Service Catalog
```bash
pup service-catalog list
pup service-catalog get <service-name>
```

### Notebooks
```bash
pup notebooks list
pup notebooks get 12345
pup notebooks create --file notebook.json
```

### Observability Pipelines
```bash
pup obs-pipelines list
pup obs-pipelines get <pipeline-id>
pup obs-pipelines create --file pipeline.json
pup obs-pipelines update <pipeline-id> --file pipeline.json
pup obs-pipelines delete <pipeline-id>
pup obs-pipelines validate --file pipeline.json
```

### LLM Observability
```bash
pup llm-obs projects list
pup llm-obs projects create --file project.json
pup llm-obs experiments list
pup llm-obs experiments list --filter-project-id <project-id>
pup llm-obs experiments list --filter-dataset-id <dataset-id>
pup llm-obs experiments create --file experiment.json
pup llm-obs experiments update <experiment-id> --file experiment.json
pup llm-obs experiments delete --file delete-request.json
pup llm-obs datasets list --project-id <project-id>
pup llm-obs datasets create --project-id <project-id> --file dataset.json
```

### Reference Tables
```bash
pup reference-tables list
pup reference-tables get <table-id>
pup reference-tables create --file table.json
pup reference-tables batch-query --file query.json
```

### Cost Cloud Configs
```bash
# AWS CUR configs
pup costs datadog aws-config list
pup costs datadog aws-config get <account-id>
pup costs datadog aws-config create --file config.json
pup costs datadog aws-config delete <account-id>

# Azure UC configs
pup costs datadog azure-config list
pup costs datadog azure-config get <account-id>
pup costs datadog azure-config create --file config.json
pup costs datadog azure-config delete <account-id>

# GCP usage cost configs
pup costs datadog gcp-config list
pup costs datadog gcp-config get <account-id>
pup costs datadog gcp-config create --file config.json
pup costs datadog gcp-config delete <account-id>
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
# Homebrew (recommended)
brew tap datadog-labs/pack
brew install pup

# Or build from source
cargo install --git https://github.com/DataDog/pup
```

### Verify Installation

```bash
pup --version
pup auth status
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


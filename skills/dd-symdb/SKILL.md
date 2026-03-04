---
name: dd-symdb
description: Symbol Database - search service symbols, find probe-able methods.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,symdb,symbol-database,live-debugger
  globs: ""
  alwaysApply: "false"
---

# Datadog Symbol Database

Search for classes and methods in instrumented services. Find probe-able method locations for use with the Live Debugger.

## Prerequisites

`pup` must be installed:

```bash
brew tap datadog-labs/pack && brew install pup
```

## Authentication

Authenticate via OAuth2 (recommended) or API keys:

```bash
# OAuth2 (recommended)
pup auth login

# Or use API keys
export DD_API_KEY="key" DD_APP_KEY="key" DD_SITE="datadoghq.com"
```

## Usage

### Names View

One scope name per line.

```bash
pup symdb search --service my-service --query "Controller" --view names
```

### Probe Locations View

`TYPE:METHOD` pairs suitable for `--probe-location` in `pup debugger probes create`. When the search returns CLASS scopes, children (methods) are fetched automatically and concurrently.

```bash
pup symdb search --service my-service --query "VetController" --view probe-locations
```

### Full View

Raw JSON response from the API.

```bash
pup symdb search --service my-service --query "VetController" --view full
```

### Filter by Version

```bash
pup symdb search --service my-service --query "handler" --version "v1.2.3" --view names
```

### Allow Partial Results

```bash
pup symdb search --service my-service --query "Controller" --allow-partial --view names
```

## Combining with Debugger

Find methods then place probes:

```bash
# 1. Find probe-able methods
pup symdb search --service my-service --query "MyController" --view probe-locations

# 2. Place a probe on a discovered method (language auto-detected from symdb)
pup debugger probes create \
  --service my-service \
  --env production \
  --probe-location "com.example.MyController:handleRequest" \
  --template "handleRequest called with id={id}"

# 3. Stream events
pup debugger probes watch <PROBE_ID> --timeout 60 --limit 10
```

## Options

| Flag | Description | Default |
|------|-------------|---------|
| `--service` | Service name (required) | — |
| `--query` | Search query, matches scope names | None (lists all) |
| `--version` | Service version filter | None |
| `--allow-partial` | Allow partial results | `false` |
| `--view` | Output view: `full`, `names`, `probe-locations` | `full` |

## Failure Handling

| Problem | Fix |
|---------|-----|
| No results | Verify the service is instrumented and reporting to Datadog |
| Auth error | Run `pup auth login` or set `DD_API_KEY` + `DD_APP_KEY` + `DD_SITE` |
| Wrong service name | Check exact service name in Datadog APM service catalog |
| Partial results | Add `--allow-partial` to accept incomplete data from the API |
| Stale symbols | Filter with `--version` to target the currently deployed version |

## References

- [Live Debugger Docs](https://docs.datadoghq.com/dynamic_instrumentation/)
- [Symbol Database](https://docs.datadoghq.com/dynamic_instrumentation/symdb/)

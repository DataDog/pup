---
name: dd-apm
description: APM - traces, services, dependencies, performance analysis.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,apm,tracing,performance,distributed-tracing,dd-apm
  globs: "**/ddtrace*,**/datadog*.yaml,**/*trace*"
  alwaysApply: "false"
---

# Datadog APM

Distributed tracing, service maps, and performance analysis.

## Requirements

Datadog Labs Pup should be installed via:

```bash
brew tap datadog-labs/pack && brew install pup
```

## Quick Start

```bash
pup auth login
pup apm services list
pup apm traces list --service api-gateway --duration 1h
```

## Services

### List Services

```bash
pup apm services list
pup apm services list --env production
```

### Service Details

```bash
pup apm services get api-gateway --json
```

### Service Map

```bash
# View dependencies
pup apm service-map --service api-gateway --json
```

## Traces

### Search Traces

```bash
# By service
pup apm traces list --service api-gateway --duration 1h

# Errors only
pup apm traces list --service api-gateway --status error

# Slow traces (>1s)
pup apm traces list --service api-gateway --min-duration 1000ms

# With specific tag
pup apm traces list --query "@http.url:/api/users"
```

### Get Trace Detail

```bash
pup apm traces get <trace_id> --json
```

## Key Metrics

| Metric | What It Measures |
|--------|------------------|
| `trace.http.request.hits` | Request count |
| `trace.http.request.duration` | Latency |
| `trace.http.request.errors` | Error count |
| `trace.http.request.apdex` | User satisfaction |

## ⚠️ Trace Sampling

**Not all traces are kept.** Understand sampling:

| Mode | What's Kept |
|------|-------------|
| **Head-based** | Random % at start |
| **Error/Slow** | All errors, slow traces |
| **Retention** | What's indexed (billed) |

```bash
# Check retention filters
pup apm retention-filters list
```

### Trace Retention Costs

| Retention | Cost |
|-----------|------|
| Indexed spans | $$$ per million |
| Ingested spans | $ per million |

**Best practice:** Only index what you need for search.

## Service Level Objectives

Link APM to SLOs:

```bash
pup slos create \
  --name "API Latency p99 < 200ms" \
  --type metric \
  --numerator "sum:trace.http.request.hits{service:api,@duration:<200000000}" \
  --denominator "sum:trace.http.request.hits{service:api}" \
  --target 99.0
```

## Common Queries

| Goal | Query |
|------|-------|
| Slowest endpoints | `avg:trace.http.request.duration{*} by {resource_name}` |
| Error rate | `sum:trace.http.request.errors{*} / sum:trace.http.request.hits{*}` |
| Throughput | `sum:trace.http.request.hits{*}.as_rate()` |

## Troubleshooting

| Problem | Fix |
|---------|-----|
| No traces | Check ddtrace installed, DD_TRACE_ENABLED=true |
| Missing service | Verify DD_SERVICE env var |
| Traces not linked | Check trace headers propagated |
| High cardinality | Don't tag with user_id/request_id |

## References/Docs

- [APM Setup](https://docs.datadoghq.com/tracing/)
- [Trace Search](https://docs.datadoghq.com/tracing/trace_explorer/)
- [Retention Filters](https://docs.datadoghq.com/tracing/trace_pipeline/trace_retention/)


---
name: dd-apm
description: APM analysis playbook (traces, services, sampling concepts). Query CLI: traces agent. Sampling/span metrics: apm-configuration.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,apm,tracing,performance,distributed-tracing,dd-apm
  globs: "**/ddtrace*,**/datadog*.yaml,**/*trace*"
  alwaysApply: "false"
---

# Datadog APM

Distributed tracing, service maps, and performance analysis. This skill is the analysis playbook. For trace/span search CLI, use the `traces` agent; for sampling rules and span-based metrics, use `apm-configuration`.

## Requirements

Datadog Labs Pup should be installed via:

```bash
brew tap datadog-labs/pack
brew install pup
```

## Quick Start

```bash
pup auth login
pup apm services list --env production
pup traces search --query="service:api-gateway" --from="1h"
```

## Services

### List Services

`--env` is **required** for all `apm services` commands.

```bash
pup apm services list --env production
pup apm services list --env staging
```

### Service Statistics

```bash
pup apm services stats --env production
pup apm services stats --env production --from 4h
```

### Service Operations and Resources

```bash
# List operations for a service
pup apm services operations --env production --service api-gateway

# List resources (endpoints) for an operation
pup apm services resources --env production --service api-gateway --name http.request
```

### Service Dependencies

```bash
pup apm dependencies list --env production
```

### Flow Map

```bash
# View service flow map (--query and --env required)
pup apm flow-map --query "service:api-gateway" --env production
```

## Traces

Traces are searched via the top-level `traces` command (not under `apm`).

**Important:** APM durations are in **nanoseconds**: 1 second = 1,000,000,000 ns.

### Search Traces

```bash
# By service
pup traces search --query="service:api-gateway" --from="1h"

# Errors only
pup traces search --query="service:api-gateway status:error" --from="1h"

# Slow traces (>1 second = 1000000000 ns)
pup traces search --query="service:api-gateway @duration:>1000000000" --from="1h"

# With specific tag
pup traces search --query="service:api @http.url:/api/users" --from="1h"
```

### Aggregate Traces

```bash
# Average duration by resource
pup traces aggregate \
  --query="service:api-gateway" \
  --compute="avg(@duration)" \
  --group-by="resource_name" \
  --from="1h"

# Error count by service
pup traces aggregate \
  --query="status:error" \
  --compute="count" \
  --group-by="service" \
  --from="1h"

# p99 latency
pup traces aggregate \
  --query="service:api-gateway" \
  --compute="percentile(@duration, 99)" \
  --from="1h"
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

### Trace Retention Costs

| Retention | Cost |
|-----------|------|
| Indexed spans | $$$ per million |
| Ingested spans | $ per million |

**Best practice:** Only index what you need for search.

## Service Level Objectives

Link APM to SLOs:

```bash
pup slos create --file slo.json
```

## Common Queries

| Goal | Query |
|------|-------|
| Slowest endpoints | `pup traces aggregate --query="service:api" --compute="avg(@duration)" --group-by="resource_name" --from="1h"` |
| Error rate by service | `pup traces aggregate --query="status:error" --compute="count" --group-by="service" --from="1h"` |
| Throughput | `pup traces aggregate --query="service:api" --compute="count" --group-by="resource_name" --from="1h"` |

## Service Config

Query service instance metadata — instance IDs, hostnames, and config IDs for all
running instances of a service. Returns up to 100 instances.

```bash
# Get instance metadata for a service
pup apm service-config get --service-name my-service

# Filter by environment
pup apm service-config get --service-name my-service --env prod

# Filter by specific instance IDs
pup apm service-config get --service-name my-service --service-instance-ids "id-1,id-2"
```

> **Note on service identity:** `service_name` and `env` come from the SDK telemetry
> pipeline and may differ from values in the Service Catalog.

## Service Library Config

Query the APM tracer configuration deployed across all running instances of a service.
Useful for auditing config drift — finding instances where tracing, profiling, or AppSec
is misconfigured relative to the rest of the fleet.

```bash
# Get tracer config for a service
pup apm service-library-config get --service-name my-service

# Filter by environment
pup apm service-library-config get --service-name my-service --env prod

# Filter by language
pup apm service-library-config get --service-name my-service --env prod --language python

# Only show configs where instances disagree (config drift)
pup apm service-library-config get --service-name my-service --mixed
```

> **Note on service identity:** `service_name`, `env`, and `language_name` come from the
> SDK telemetry pipeline and reflect what the tracer reports at runtime. These may differ
> from values in the Service Catalog, which aggregates data from multiple sources (APM
> spans, USM, infrastructure tags, manual definitions).

## Troubleshooting

| Problem | Fix |
|---------|-----|
| No traces | Check ddtrace installed, DD_TRACE_ENABLED=true |
| Missing service | Verify DD_SERVICE env var |
| Traces not linked | Check trace headers propagated |
| High cardinality | Don't tag with user_id/request_id |
| `--env` required error | Always pass `--env` to `apm services` commands |

## References/Docs

- [APM Setup](https://docs.datadoghq.com/tracing/)
- [Trace Search](https://docs.datadoghq.com/tracing/trace_explorer/)
- [Retention Filters](https://docs.datadoghq.com/tracing/trace_pipeline/trace_retention/)


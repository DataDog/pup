---
name: dd-logs
description: Log management - search, pipelines, archives, and cost control.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,logs,logging,search,dd-logs
  globs: "**/datadog*.yaml,**/*log*"
  alwaysApply: "false"
---

# Datadog Logs

Search, process, and archive logs with cost awareness.

## Prerequisites

Datadog Pup (dd-pup/pup) should already be installed:

```bash
cargo install --git https://github.com/DataDog/pup
```

## Quick Start

```bash
pup auth login
```

## Search Logs

```bash
# Basic search
pup logs search --query="status:error" --from="1h"

# With filters
pup logs search --query="service:api status:error" --from="1h" --limit 100

# JSON output is the default
pup logs search --query="@http.status_code:>=500" --from="1h"
```

### Search Syntax

| Query | Meaning |
|-------|---------|
| `error` | Full-text search |
| `status:error` | Tag equals |
| `@http.status_code:500` | Attribute equals |
| `@http.status_code:>=400` | Numeric range |
| `service:api AND env:prod` | Boolean |
| `@message:*timeout*` | Wildcard |

### Trace IDs in Log Results

Logs that show a linked trace in the Datadog UI may not include `dd.trace_id` /
`dd.span_id` in API results. When a trace ID attribute is remapped for trace
correlation (via JSON preprocessing or a Trace Remapper processor), the source
attribute is removed and the value is stored as an internal attribute that the
Logs Search API does not return. The UI's "trace" link reads that internal
attribute, so UI and API results differ.

**This is expected Datadog Log Management behavior, not a pup bug or an
instrumentation problem.** Do not retry queries or change instrumentation to
"fix" it. Datadog is tracking making these values queryable (support reference
FRLOGSS-4306).

Workarounds until then:

- Emit the trace ID under a separate attribute that is not remapped (e.g.
  `@custom.trace_id`) and query that.
- Pivot the other way: search spans by the log's service/time window via
  `pup traces search`, or use the trace link in the Datadog UI.

## Pipelines

Process logs before indexing:

```bash
# List pipelines
pup obs-pipelines list

# Create pipeline (JSON)
pup obs-pipelines create --file pipeline.json
```

### Common Processors

```json
{
  "name": "API Logs",
  "filter": {"query": "service:api"},
  "processors": [
    {
      "type": "grok-parser",
      "name": "Parse nginx",
      "source": "message",
      "grok": {"match_rules": "%{IPORHOST:client_ip} %{DATA:method} %{DATA:path} %{NUMBER:status}"}
    },
    {
      "type": "status-remapper",
      "name": "Set severity",
      "sources": ["level", "severity"]
    },
    {
      "type": "attribute-remapper",
      "name": "Remap user_id",
      "sources": ["user_id"],
      "target": "usr.id"
    }
  ]
}
```

## ⚠️ Exclusion Filters (Cost Control)

**Index only what matters:**

```json
{
  "name": "Drop debug logs",
  "filter": {"query": "status:debug"},
  "is_enabled": true
}
```

### High-Volume Exclusions

```bash
# Find noisiest log sources
pup logs search --query="*" --from="1h" | jq 'group_by(.service) | map({service: .[0].service, count: length}) | sort_by(-.count)[:10]'
```

| Exclude | Query |
|---------|-------|
| Health checks | `@http.url:"/health" OR @http.url:"/ready"` |
| Debug logs | `status:debug` |
| Static assets | `@http.url:*.css OR @http.url:*.js` |
| Heartbeats | `@message:*heartbeat*` |

## Archives

Store logs cheaply for compliance:

```bash
# List archives
pup logs archives list

# Archive config (S3 example)
{
  "name": "compliance-archive",
  "query": "*",
  "destination": {
    "type": "s3",
    "bucket": "my-logs-archive",
    "path": "/datadog"
  },
  "rehydration_tags": ["team:platform"]
}
```

## Log-Based Metrics

Inspect log-based metrics:

```bash
# List existing log-based metrics
pup logs metrics list
```

**⚠️ Cardinality warning:** Group by bounded values only.

## Sensitive Data

### Scrubbing Rules

```json
{
  "type": "hash-remapper",
  "name": "Hash emails",
  "sources": ["email", "@user.email"]
}
```

### Never Log

```python
# In your app - sanitize before sending
import re

def sanitize_log(message: str) -> str:
    # Remove credit cards
    message = re.sub(r'\b\d{4}[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{4}\b', '[REDACTED]', message)
    # Remove SSNs
    message = re.sub(r'\b\d{3}-\d{2}-\d{4}\b', '[REDACTED]', message)
    return message
```

## Troubleshooting

| Problem | Fix |
|---------|-----|
| Logs not appearing | Check agent, pipeline filters |
| High costs | Add exclusion filters |
| Search slow | Narrow time range, use indexes |
| Missing attributes | Check grok parser |
| `dd.trace_id` missing but UI shows a trace | Expected: remapped trace IDs become internal attributes (see [Trace IDs in Log Results](#trace-ids-in-log-results)) |

## References/Documentation

- [Log Search Syntax](https://docs.datadoghq.com/logs/explorer/search_syntax/)
- [Pipelines](https://docs.datadoghq.com/logs/log_configuration/pipelines/)
- [Exclusion Filters](https://docs.datadoghq.com/logs/indexes/#exclusion-filters)
- [Archives](https://docs.datadoghq.com/logs/archives/)


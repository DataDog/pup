---
description: Manage Datadog APM configuration including retention filters for span indexing and span-based metrics generation from distributed traces.
---

# APM Configuration Agent

Use the typed APM commands below for service inspection and customer sampling-rule management.

## Supported Commands

```bash
pup apm services list
pup apm services stats
pup apm services operations
pup apm services resources
pup apm sampling-rules list
pup apm sampling-rules get <rule-id>
pup apm sampling-rules create --file=rule.json
pup apm sampling-rules update <rule-id> --file=rule.json
pup apm sampling-rules delete <rule-id>
```

## Unsupported Operations

The typed CLI does not expose APM retention-filter or span-based-metric CRUD commands. Sampling rules are a distinct feature and are not replacements for those APIs. Use the Datadog UI or API documentation for retention filters and span-based metrics.

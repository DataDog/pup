---
description: Manage RUM-based metrics and retention filters for Real User Monitoring data. Create custom metrics from RUM events and control data retention with sampling filters.
---

# RUM Metrics and Retention Agent

The typed pup CLI supports RUM custom metrics and retention-filter CRUD. There is no retention-filter ordering command.

## Metrics

```bash
pup rum metrics list
pup rum metrics get <metric-id>
pup rum metrics create --file=metric.json
pup rum metrics update <metric-id> --file=metric.json
pup rum metrics delete <metric-id>
```

## Retention Filters

```bash
pup rum retention-filters list
pup rum retention-filters get <filter-id>
pup rum retention-filters create --file=filter.json
pup rum retention-filters update <filter-id> --file=filter.json
pup rum retention-filters delete <filter-id>
```

Confirm before changing or deleting metrics and retention filters. Use the CLI help for request-file fields and ordering controls unsupported by pup.

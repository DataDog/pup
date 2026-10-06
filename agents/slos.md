---
description: Manage Service Level Objectives including listing, viewing, and deleting SLOs.
---

# Service Level Objectives Agent

Use the typed SLO commands to list, inspect, and check current SLO status:

```bash
pup slos list
pup slos get <slo-id>
pup slos status <slo-id>
```

The typed CLI does not provide a `slos history` subcommand. Do not present current status as historical data; use the Datadog UI or API documentation for SLO history.

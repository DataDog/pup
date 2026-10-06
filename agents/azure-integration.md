---
description: Configure and manage Azure integration for monitoring, log collection, and resource tracking across Azure subscriptions and services.
---

# Azure Integration Agent

The typed pup CLI currently supports listing configured Azure integrations:

```bash
pup cloud azure list
```

Creating, inspecting an individual configuration, updating, or deleting Azure integrations, and configuring Azure collection features are not exposed as typed pup commands. Do not invent an `azure` command group or imply changes were made; use the Datadog UI or the relevant API documentation for unsupported operations.

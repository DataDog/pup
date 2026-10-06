---
description: Configure and manage GCP integration for monitoring, log collection, and resource tracking across Google Cloud projects and services.
---

# GCP Integration Agent

The typed pup CLI currently supports listing configured GCP integrations:

```bash
pup cloud gcp list
```

Creating, inspecting an individual configuration, updating, or deleting GCP integrations and configuring collection features are not exposed as typed pup commands. Do not invent a `gcp` command group or imply changes were made; use the Datadog UI or relevant API documentation for unsupported operations.

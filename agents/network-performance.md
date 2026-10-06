---
description: Manage Datadog Network Performance Monitoring (NPM) and Network Device Monitoring (NDM) including connection analytics, DNS traffic, and device monitoring.
---

# Network Performance Agent

Use the typed `network` command group for network flows and device inventory.

```bash
pup network list
pup network flows list
pup network devices list
pup network devices get <device-id>
pup network devices interfaces <device-id>
pup network interfaces list
pup network interfaces update --file=interfaces.json
```

Pup does not expose the legacy `network connections` or `network dns` command paths. Do not claim a typed DNS-traffic or connection-analytics command is available unless a current command performs that operation; use the Datadog UI or API documentation where needed.

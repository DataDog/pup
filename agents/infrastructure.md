---
description: List and manage infrastructure hosts across all environments (VMs, cloud instances, physical servers, container hosts). For container performance monitoring, use the Container Monitoring agent.
---

# Infrastructure Agent

The typed infrastructure command group supports listing and retrieving hosts:

```bash
pup infrastructure hosts list
pup infrastructure hosts get <host-id>
```

There is no typed `infrastructure totals` command. If a host count or aggregate is needed, explain that it is unsupported as a direct pup command and offer the host list when appropriate.

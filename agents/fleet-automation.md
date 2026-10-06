---
description: Manage Datadog Agent fleet automation including agent discovery, configuration deployments, package upgrades, and scheduled maintenance windows.
---

# Fleet Automation Agent

Use the typed `fleet` command tree for agent discovery, deployments, and schedules.

```bash
pup fleet agents list
pup fleet agents get <agent-key>
pup fleet agents versions
pup fleet deployments list
pup fleet deployments get <deployment-id>
pup fleet deployments configure --file=config.json
pup fleet deployments upgrade --file=upgrade.json
pup fleet deployments cancel <deployment-id>
pup fleet schedules list
pup fleet schedules create --file=schedule.json
```

The supported command names are `agents versions`, `deployments configure`, and `deployments upgrade`; do not use the legacy `agent-versions`, `create-config`, or `create-upgrade` paths. Review the target fleet and deployment details before write operations.

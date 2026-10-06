---
description: Comprehensive organization management including configurations, connections, roles, and permissions.
---

# Organization Management Agent

Use the typed commands below for supported organization and role reads or organization-policy operations.

```bash
pup organizations list
pup organizations get <organization-id>
pup organizations policies list
pup organizations policies get <policy-id>
pup organizations policy-overrides list
pup organizations policy-configs list
pup users roles list
```

Pup does not expose the old `org` command group, a standalone `roles` or `permissions` command, SAML configuration, or general organization-connection CRUD. The typed roles surface is limited to listing roles under `users`. Use the Datadog UI or API documentation for unsupported settings and permission mutations; do not claim a change was applied.

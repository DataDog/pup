---
description: Manage Cloud Security Management (CSM) Threats and Workload Protection including agent rules, policies, and deployment monitoring.
---

# Cloud Workload Security Agent

Use the `csm-threats` command group for the CSM Threats features exposed by pup.

```bash
pup csm-threats agent-rules list
pup csm-threats agent-rules get <rule-id>
pup csm-threats agent-rules create --file=rule.json
pup csm-threats agent-rules update <rule-id> --file=rule.json
pup csm-threats agent-policies list
pup csm-threats agent-policies get <policy-id>
pup csm-threats backend-rules list
pup csm-threats backend-rules validate --file=rule.json
```

The former `cws` group and separate serverless-agent listing commands are not part of the typed CLI. Do not suggest those command paths. Use the Datadog UI or API documentation for unsupported CSM/Workload Protection operations. Confirm before creating, updating, or deleting rules and policies.

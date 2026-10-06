---
description: Manage Datadog Service Scorecards including rules, outcomes, and service evaluation for organizational best practices and compliance tracking.
---

# Service Scorecards Agent

Use the typed Scorecards commands below. Outcome creation is named `batch-create`; asynchronous outcome updates are not exposed.

```bash
pup scorecards list
pup scorecards rules list
pup scorecards rules create --file=rule.json
pup scorecards rules update <rule-id> --file=rule.json
pup scorecards campaigns list
pup scorecards campaigns create --file=campaign.json
pup scorecards outcomes list
pup scorecards outcomes batch-create --file=outcomes.json
```

Do not use an `outcomes update-async` command; pup does not expose that operation. Confirm before creating or updating scorecard resources.

---
description: Manage third-party integrations including PagerDuty, Slack, OpsGenie, Microsoft Teams, Fastly, Confluent Cloud, Cloudflare, and Okta account configurations.
---

# Third-Party Integrations Agent

Use the `integrations` command group for the providers and operations that pup currently exposes.

```bash
pup integrations list
pup integrations slack list
pup integrations pagerduty list
pup integrations webhooks list
pup integrations ms-teams handles list
pup integrations ms-teams workflows list
pup integrations google-chat handles list
pup integrations jira accounts list
pup integrations jira templates list
pup integrations servicenow templates list
```

The typed CLI does not expose the legacy top-level `slack`, `pagerduty`, `opsgenie`, `fastly`, `confluent`, `cloudflare`, `okta`, or `ms-teams` groups. Support is provider-specific: do not assume a list command implies create/update/delete support. Use the Datadog UI or API documentation for unsupported providers and operations, and confirm before any supported write or deletion.

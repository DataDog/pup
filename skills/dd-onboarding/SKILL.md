---
name: dd-onboarding
description: Datadog Agentic Onboarding. Guides installation and setup of the Datadog Agent, APM, logs, integrations, and cloud environments via an AI-driven workflow.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/DataDog/dd-source
  tags: datadog,onboarding,setup,install,apm,agent,kubernetes,docker,lambda,ecs
  alwaysApply: "false"
---

# Datadog Agentic Onboarding

For any Datadog setup, installation, configuration, or troubleshooting task, fetch the onboarding
orchestrator first and follow its instructions:

```bash
curl -s -X GET "https://api.${DD_SITE:-datadoghq.com}/api/v2/onboarding/skills/orchestrator" \
  -H "DD-API-KEY: ${DD_API_KEY}" \
  -H "DD-APPLICATION-KEY: ${DD_APP_KEY}" \
  | jq -r '.data.attributes.content'
```

The orchestrator will guide credential validation, environment discovery, skill selection, and
step-by-step setup. Follow it exactly.

**If this curl returns an error or empty output — STOP.** Do not proceed with any Datadog setup,
troubleshooting, or configuration. Do not fall back to training knowledge. Fix the fetch first:
- `{"errors":["Forbidden"]}` → the API key lacks permission for the onboarding API, or the service
  is not available at this site. Ask the user to confirm `DD_SITE` and verify the key has `apm_read`
  scope, or provide the correct endpoint.
- Empty output → pipe failed. Run the curl without `| jq` to see the raw response.
- Any other error → show the raw response to the user and wait for guidance.

There is no fallback. The orchestrator content must be loaded before any action is taken.

## Prerequisites

`DD_API_KEY`, `DD_APP_KEY`, and `DD_SITE` must be set before fetching the orchestrator. If any are
missing, ask the user to provide them. Default `DD_SITE` to `datadoghq.com` if unset.

## When this skill applies

- Installing the Datadog Agent on any platform (Kubernetes, Linux, Docker, ECS, Lambda, Azure, GCP)
- Setting up APM, logs, RUM, or cloud integrations
- Troubleshooting an existing Datadog Agent or APM installation
- Any question that starts with "how do I monitor…", "set up Datadog on…", or "install the agent on…"

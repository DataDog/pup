---
name: onboarding-summary
description: Generate a live Single Step Instrumentation (SSI) onboarding confirmation report — verifies APM instrumentation is working end-to-end with deep links into the Datadog UI. Only use after agent-install and enable-ssi have both completed successfully.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,apm,kubernetes,ssi,summary,verification
  alwaysApply: "false"
---

# APM Onboarding Summary

## Triggers

Invoke this skill when:
- All steps in `verify-ssi` have passed
- All checks in `troubleshoot-ssi` have been resolved
- The user asks "is everything working?", "show me the status", or "confirm APM is set up"

Do NOT invoke this skill if any verification or troubleshooting check is still failing — resolve those first.

---

## Context to resolve before acting

| Variable | How to resolve |
|---|---|
| `AGENT_NAMESPACE` | Namespace where Datadog Agent is installed |
| `APP_NAMESPACE` | Namespace of the application |
| `APP_LABEL` | Check `spec.selector.matchLabels.app` in the Deployment manifest |
| `CLUSTER_NAME` | `spec.global.clusterName` in `datadog-agent.yaml` |
| `SERVICE_NAME` | `tags.datadoghq.com/service` label on the Deployment |
| `ENV` | `tags.datadoghq.com/env` label on the Deployment |
| `DD_SITE` | `spec.global.site` in `datadog-agent.yaml` |

---

## Prerequisites

### Claude runs

```bash
pup auth status --site <DD_SITE>
```

If valid token — proceed.

ERROR: Not authenticated:

### Claude runs

```bash
pup auth login --site <DD_SITE>
```

> This opens a browser tab for OAuth. Complete the login there — Claude will continue once the command exits.

---

## Collect live confirmation data

Run all of the following. Each populates a row in the final report.

### Claude runs

```bash
# Agent pod count and status
kubectl get pods -n <AGENT_NAMESPACE> \
  -l app.kubernetes.io/component=agent \
  --no-headers

# SSI instrumentation config live in cluster
kubectl get datadogagent datadog -n <AGENT_NAMESPACE> \
  -o jsonpath='{.spec.features.apm.instrumentation}'

# Init container confirmed in app pod spec
kubectl get pod -l app=<APP_LABEL> -n <APP_NAMESPACE> \
  -o jsonpath='{.items[0].spec.initContainers[*].name}'

# Pod confirmed instrumented — init containers in pod spec
kubectl get pod -l app=<APP_LABEL> -n <APP_NAMESPACE> \
  -o jsonpath='{.items[0].spec.initContainers[*].name}'

# Service visible and traced in APM
DD_SITE=<DD_SITE> pup apm services list --env <ENV> --from 1h

# Traces arriving in the last hour
DD_SITE=<DD_SITE> pup traces search --query "service:<SERVICE_NAME>" --from 1h --limit 5
```

---

## Present the report

Fill in every value from live command output. Do not leave any placeholder unfilled. If a value cannot be confirmed, mark that row as failed and link to `troubleshoot-ssi`.

---

**APM onboarding complete**

| Check | Detail | Status |
|---|---|---|
| Datadog Agent | `<N>` pod(s) Running in `<AGENT_NAMESPACE>` | OK |
| SSI enabled | Targeting namespace `<APP_NAMESPACE>`, language `<LANGUAGE>` v`<MAJOR_VERSION>` | OK |
| Init container injected | `datadog-lib-<language>-init` present in pod spec | OK |
| Pod instrumented | `<POD_NAME>` in `pup fleet instrumented-pods list` | OK |
| Tracer reporting | Service `<SERVICE_NAME>`, `<LANGUAGE>`, tracer v`<TRACER_VERSION>` | OK |
| APM service visible | `<SERVICE_NAME>` in env `<ENV>` | OK |
| Traces arriving | `<N>` trace(s) found in the last hour | OK |

---

**Your service in Datadog — click to open:**

Construct each URL by substituting real values. Do not print placeholder URLs.

| View | URL |
|---|---|
| Service overview | `https://app.<DD_SITE>/apm/services/<SERVICE_NAME>?env=<ENV>` |
| Traces explorer | `https://app.<DD_SITE>/apm/traces?query=service:<SERVICE_NAME>%20env:<ENV>` |
| Service map | `https://app.<DD_SITE>/apm/map?env=<ENV>&service=<SERVICE_NAME>` |
| Agent fleet | `https://app.<DD_SITE>/fleet-automation` |

---

## Security constraints

- Never write a raw API key into any file or chat message

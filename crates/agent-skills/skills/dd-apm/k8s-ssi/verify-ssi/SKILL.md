---
name: verify-ssi
description: Verify Single Step Instrumentation (SSI) is working end-to-end on Kubernetes — SSI automatically instruments applications for APM without code changes. Only use after enable-ssi has run.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,apm,kubernetes,ssi,verification,instrumentation
  alwaysApply: "false"
---

# Verify APM SSI on Kubernetes

> **Before doing anything else:** Fully resolve all variables in `## Context to resolve before acting`. Do not begin Step 1 until every variable has a concrete value.

## Triggers

Invoke this skill when the user expresses intent to:
- Confirm SSI is working after enabling APM
- Check whether pods are being instrumented
- Verify the tracer is running and reporting telemetry
- Confirm tracer config is applied correctly

Do NOT invoke this skill if:
- SSI has not been enabled yet — run `enable-ssi` first
- Pods are not being instrumented at all — use `troubleshoot-ssi`

---

## Prerequisites

- [ ] `enable-ssi` is complete
- [ ] Application pods have been restarted since SSI was enabled

### pup-cli: check, install, and authenticate

### Claude runs

```bash
pup --version
```

If not found:

### Claude runs

```bash
brew tap datadog-labs/pack
brew install pup
```

Check auth:
```bash
pup auth status --site <DD_SITE>
```

If not authenticated:

### Claude runs

```bash
pup auth login --site <DD_SITE>
```

> This opens a browser tab for OAuth. Complete the login there — Claude will continue once the command exits.

If valid token — proceed.
ERROR: No browser available — use API key fallback: `export DD_APP_KEY=<your-app-key>`

---

## Context to resolve before acting

| Variable | How to resolve |
|---|---|
| `CLUSTER_NAME` | Check `spec.global.clusterName` in `datadog-agent.yaml`, or `kubectl config current-context` |
| `ENV` | Check `tags.datadoghq.com/env` label on the application Deployment |
| `SERVICE_NAME` | Check `tags.datadoghq.com/service` label on the application Deployment |

---

## Step 1: Confirm Pods are Instrumented

### Claude runs

```bash
kubectl get pod -l app=<APP_LABEL> -n <APP_NAMESPACE> \
  -o jsonpath='{.items[0].spec.initContainers[*].name}'
```

If the output includes `datadog-lib-<language>-init` and `datadog-init-apm-inject` — SSI init containers are injected.

ERROR: Init containers missing — pod was not restarted after SSI was enabled, or namespace targeting is not matching. Restart the pod and recheck.

---

## Step 2: Confirm the Tracer is Reporting Telemetry

### Claude runs

```bash
DD_SITE=<DD_SITE> pup apm services list --env <ENV> --from 1h
```

If `<SERVICE_NAME>` appears in the services list with `isTraced: true` — continue to Step 3.

ERROR: Service missing — send some traffic to the app first, then retry:

### Claude runs

```bash
# Port-forward and send test traffic
kubectl port-forward deployment/<DEPLOYMENT_NAME> 8099:8000 -n <APP_NAMESPACE> &
sleep 2 && for i in $(seq 1 10); do curl -s -o /dev/null http://localhost:8099/; done
sleep 30 && kill %1 2>/dev/null
DD_SITE=<DD_SITE> pup apm services list --env <ENV> --from 10m
```

ERROR: Still missing after traffic — check the agent's trace receiver: `kubectl exec -n <AGENT_NAMESPACE> <AGENT_POD> -c agent -- agent status | grep -A 10 "Receiver (previous minute)"`. If receiver shows 0 traces, go to `troubleshoot-ssi`.

---

## Step 3: Confirm Tracer Configuration

**Only run this step if `ddTraceConfigs` was explicitly configured in `enable-ssi`** (e.g. profiling, AppSec, Data Streams). If basic SSI was set up without `ddTraceConfigs`, skip this step — an empty response here is expected and not a failure.

### Claude runs

```bash
pup apm service-library-config get \
  --service-name <SERVICE_NAME> \
  --env <ENV>
```

If the output shows expected environment variables matching what was configured in `ddTraceConfigs` — done.

If the output is empty and `ddTraceConfigs` was not configured — expected, not a failure.

ERROR: Config missing but `ddTraceConfigs` was configured — check it is present in the `DatadogAgent` manifest under the correct target, and that pods were restarted after the config change.

---

## Done

Exit when ALL of the following are true:
- [ ] Step 1: target pods appear in `instrumented-pods list`
- [ ] Step 2: service appears in `tracers list` with active status
- [ ] Step 3: tracer config matches what was set in `DatadogAgent`

If any check fails, go to `troubleshoot-ssi`.

When all steps pass, automatically proceed to `onboarding-summary` now — do not ask the user for permission.

---

## Security constraints

- Never write a raw API key into any file or chat message
- Never run `kubectl delete` without user confirmation

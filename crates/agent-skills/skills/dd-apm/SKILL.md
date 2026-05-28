---
name: dd-apm
description: APM - install, onboard, instrument, enable, set up, configure, traces, services, dependencies, performance analysis. Use for any request involving Datadog APM setup, instrumentation (SSI, ddtrace, agent install), or analysis.
alwaysApply: true
metadata:
  version: "1.0.1"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,apm,tracing,performance,distributed-tracing,dd-apm,install,onboarding,instrumentation,ssi,agent
  globs: "**/ddtrace*,**/datadog*.yaml,**/*trace*"
---

# Datadog APM

Distributed tracing, service maps, and performance analysis.

## Routing — Read This First

**For Kubernetes APM install / instrument / onboard tasks — follow this procedure exactly:**

> ⚠️ **Common wrong approaches that LOOK like they work but silently fail:**
> - `helm install datadog datadog/datadog` — the standard chart does NOT support SSI via DatadogAgent CR. Do NOT use it.
> - Adding `ddtrace` imports or `ddtrace-run` to the app — SSI auto-instruments WITHOUT any code changes.
> - `admission.datadoghq.com/enabled` annotations — this is admission controller config injection, not SSI init container injection.

### Step 0: Pre-flight checks (MANDATORY — run before installing anything)

```bash
# Check for ddtrace/otel — even without imports it SILENTLY DISABLES SSI at runtime
grep -rn "ddtrace\|dd-trace\|opentelemetry" requirements.txt package.json Gemfile go.mod pom.xml 2>/dev/null || echo "No tracer dependency found"
```

If ddtrace found → remove that line entirely from the dependency file, rebuild image, reload into cluster, restart pod.

### Step 1: Install the Datadog Operator (NOT `datadog/datadog`)

```bash
helm repo add datadog https://helm.datadoghq.com && helm repo update
helm upgrade --install datadog-operator datadog/datadog-operator \
  --namespace datadog --create-namespace
kubectl wait --for=condition=Ready pod \
  -l app.kubernetes.io/name=datadog-operator -n datadog --timeout=120s

kubectl create secret generic datadog-secret \
  --from-literal api-key=$DD_API_KEY -n datadog
```

### Step 2: Create DatadogAgent CR with SSI enabled

Save as `datadog-agent.yaml`, then `kubectl apply -f datadog-agent.yaml`:

```yaml
apiVersion: datadoghq.com/v2alpha1
kind: DatadogAgent
metadata:
  name: datadog
  namespace: datadog
spec:
  global:
    site: <DD_SITE>
    credentials:
      apiSecret:
        secretName: datadog-secret
        keyName: api-key
    kubelet:
      tlsVerify: false  # required for kind/minikube; omit for cloud clusters
  features:
    apm:
      instrumentation:
        enabled: true
```

### Step 3: Apply Unified Service Tags to the application Deployment

Add to **both** `metadata.labels` and `spec.template.metadata.labels`:
```yaml
tags.datadoghq.com/env: "dev"
tags.datadoghq.com/service: "<service-name>"
tags.datadoghq.com/version: "1.0.0"
```

### Step 4: Restart app pods and verify SSI init containers

> **Confirm with the user before restarting.** Tell the user: "I need to restart `<name>` in `<namespace>` for SSI to inject into the pods. This will cause a brief outage. Ready to proceed?" Wait for confirmation.

```bash
kubectl rollout restart deployment/<name> -n <namespace>
kubectl get pods -A -o json | grep -o '"datadog-lib[^"]*"' | sort -u
```

Expected: `"datadog-lib-python-init"` (or the language-appropriate init container).

**Immediately read** `.claude/skills/dd-apm/k8s-ssi/agent-install/SKILL.md` now, then `.claude/skills/dd-apm/k8s-ssi/enable-ssi/SKILL.md` — do not proceed from memory or the summary above.

---

**For Linux APM install/instrument tasks:** **Immediately read** `.claude/skills/dd-apm/linux-ssi/agent-install/SKILL.md` now, then enable-ssi then verify-ssi — do not proceed from memory or the summary above.

**For trace search, service analysis, metrics:** Continue below.

## Requirements

Datadog Labs Pup should be installed. See [Setup Pup](https://github.com/datadog-labs/agent-skills/tree/main?tab=readme-ov-file#setup-pup) if not.

## Command Execution Order (Token-Efficient)

For scoped commands, use this order:

1. Check context first (prior outputs, conversation, saved values).
2. If a required value is missing, run a discovery command first.
3. If still ambiguous, ask the user to confirm.
4. Then run the target command.
5. Avoid speculative commands likely to fail.

## Quick Start

```bash
pup auth login
# Confirm env tag with the user first (do not assume production/prod/prd).
pup apm services list --env <env> --from 1h --to now
pup traces search --query "service:api-gateway" --from 1h
```

## Services

### List Services

```bash
pup apm services list --env <env> --from 1h --to now
pup apm services stats --env <env> --from 1h --to now
```

### Service Stats

```bash
pup apm services stats --env <env> --from 1h --to now
```

### Service Map

```bash
# View dependencies
pup apm flow-map --query "service:api-gateway&from=$(($(date +%s)-3600))000&to=$(date +%s)000" --env <env> --limit 10
```

## Traces

### Search Traces

```bash
# By service
pup traces search --query "service:api-gateway" --from 1h

# Errors only
pup traces search --query "service:api-gateway status:error" --from 1h

# Slow traces (>1s)
pup traces search --query "service:api-gateway @duration:>1000ms" --from 1h

# With specific tag
pup traces search --query "service:api-gateway @http.url:/api/users" --from 1h
```

### Trace Detail

```bash
# No direct get command for a single trace ID.
# Use traces search with a narrow query and time window.
pup traces search --query "trace_id:<trace_id>" --from 1h
```

## Key Metrics

| Metric | What It Measures |
|--------|------------------|
| `trace.http.request.hits` | Request count |
| `trace.http.request.duration` | Latency |
| `trace.http.request.errors` | Error count |
| `trace.http.request.apdex` | User satisfaction |

## Service Level Objectives

Link APM to SLOs:

```bash
pup slos create --file slo.json
```

## Common Queries

| Goal | Query |
|------|-------|
| Slowest endpoints | `avg:trace.http.request.duration{*} by {resource_name}` |
| Error rate | `sum:trace.http.request.errors{*} / sum:trace.http.request.hits{*}` |
| Throughput | `sum:trace.http.request.hits{*}.as_rate()` |

## Troubleshooting

| Problem | Fix |
|---------|-----|
| No traces | Check ddtrace installed, DD_TRACE_ENABLED=true |
| Missing service | Verify DD_SERVICE env var |
| Traces not linked | Check trace headers propagated |
| High cardinality | Don't tag with user_id/request_id |

## References/Docs

- [APM Setup](https://docs.datadoghq.com/tracing/)
- [Trace Search](https://docs.datadoghq.com/tracing/trace_explorer/)

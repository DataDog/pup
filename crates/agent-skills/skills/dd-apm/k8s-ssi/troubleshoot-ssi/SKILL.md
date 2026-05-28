---
name: troubleshoot-ssi
description: Diagnose and fix Single Step Instrumentation (SSI) issues on Kubernetes — SSI automatically instruments applications for APM without code changes. Only use if the agent and SSI are already configured but traces are missing or instrumentation is not working.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,apm,kubernetes,ssi,troubleshooting,instrumentation
  alwaysApply: "false"
---

# Troubleshoot APM SSI on Kubernetes

## Triggers

Invoke this skill when the user expresses intent to:
- Debug why a pod is not being instrumented
- Investigate why traces are not appearing in Datadog
- Diagnose admission webhook or init container injection failures
- Follow up on failed checks from `verify-ssi`
- Report that a specific service or pod has no traces

Do NOT invoke this skill if:
- SSI has not been enabled yet — run `enable-ssi` first

---

## Prerequisites

- [ ] kubectl configured to target cluster — `kubectl config current-context`

### pup-cli: check, install, and authenticate

### Claude runs

```bash
pup --version
```

If not found, install it (OS-aware):

### Claude runs

```bash
if [[ "$(uname)" == "Darwin" ]]; then
  brew tap datadog-labs/pack && brew install pup
else
  PUP_VERSION=$(curl -s https://api.github.com/repos/datadog-labs/pup/releases/latest | grep '"tag_name"' | cut -d'"' -f4)
  curl -L "https://github.com/datadog-labs/pup/releases/download/${PUP_VERSION}/pup_linux_amd64.tar.gz" | tar xz -C /usr/local/bin pup
  chmod +x /usr/local/bin/pup
fi
pup --version
```

Check auth:
```bash
pup auth status
```

If not authenticated:

### Claude runs

```bash
pup auth login
```

> This opens a browser tab for OAuth. Complete the login there — Claude will continue once the command exits.

If no browser available: `export DD_APP_KEY=<your-app-key>`.

---

## Context to resolve before acting

| Variable | How to resolve |
|---|---|
| `AGENT_NAMESPACE` | Namespace where Datadog Agent is installed |
| `APP_NAMESPACE` | Namespace of the application with missing traces |
| `CLUSTER_NAME` | `kubectl config current-context` or `spec.global.clusterName` in `datadog-agent.yaml` |
| `SERVICE_NAME` | `tags.datadoghq.com/service` label on the Deployment, or ask the user |
| `ENV` | `tags.datadoghq.com/env` label on the Deployment, or ask the user |
| `POD_NAME` | `kubectl get pods -n <APP_NAMESPACE>` — use the specific pod the user mentioned |
| `DEPLOYMENT_NAME` | Check `metadata.name` in the Deployment manifest, or ask the user |
| `APP_LABEL` | Check `spec.selector.matchLabels.app` in the Deployment manifest |

---

## How SSI Works — Domain Knowledge

Read this before investigating. It gives you the mental model to reason about novel failures, not just known ones.

**Injection chain:**
1. Admission webhook (registered by Cluster Agent) intercepts pod creation
2. Webhook mutates the pod spec — adds a `datadog-lib-<language>-init` init container
3. Init container downloads the tracer library onto a shared volume
4. `LD_PRELOAD` env var is set pointing to the library `.so` file
5. Application process loads the library automatically on startup via `LD_PRELOAD`

**What each diagnostic layer can see:**
- **pup** — sees what Datadog's backend received. Blind to cluster-side injection failures. If pup shows no instrumented pods, the problem is in the cluster.
- **kubectl** — sees cluster state. Blind to whether data reached Datadog. If kubectl shows the init container but pup shows no traces, the problem is post-injection.

**What healthy looks like:**
- `pup fleet instrumented-pods list` shows the pod with correct language/version
- `pup fleet tracers list` shows the service as active
- `kubectl get pod -o jsonpath='{.spec.initContainers[*].name}'` includes `datadog-lib-<language>-init`

**Known silent failures — SSI produces no error when these occur:**
- **Existing ddtrace or OTel instrumentation** — SSI detects it and silently disables itself
- **Unsupported runtime version** — silently skipped
- **`admission.datadoghq.com/enabled: "false"` annotation** — webhook skips the pod entirely
- **Pod not restarted after SSI enabled** — injection happens at startup; existing pods keep running uninstrumented
- **Pod in Agent namespace** — SSI never instruments its own namespace

**Reasoning shortcuts:**
- No init container → webhook didn't fire → check: namespace targeting, pod-selector, opt-out annotation, webhook registration, pod not restarted
- Init container present + no traces → injection attempted but failed or tracer not reporting → check: existing ddtrace, runtime version, Agent connectivity, DD_SITE mismatch

---

## Step 1: Triage

Run all seven simultaneously and surface them back to the user as the diagnostics you're running. Everything after this is driven by what you find here. Resolve `<NODE_HOSTNAME>` from `kubectl get pod <POD_NAME> -n <APP_NAMESPACE> -o jsonpath='{.spec.nodeName}'` once you have a pod name; if no pod context yet, run the `pup` commands without `--hostname` first.

### Claude runs

```bash
pup traces search --query "service:<SERVICE_NAME>" --from 1h --limit 5
pup fleet instrumented-pods list <CLUSTER_NAME>
pup apm troubleshooting list --hostname <NODE_HOSTNAME> --timeframe 1h
pup apm service-library-config get --service-name <SERVICE_NAME> --env <ENV>
kubectl get pod <POD_NAME> -n <APP_NAMESPACE> \
  -o jsonpath='{.spec.initContainers[*].name}'
kubectl describe pod <POD_NAME> -n <APP_NAMESPACE> | grep -A 10 "Events:"
kubectl get mutatingwebhookconfigurations | grep datadog
```

The last command confirms the Admission Controller webhook is registered cluster-wide — this is the precondition for SSI injection working at all and must be checked even when most other services are being instrumented (any deviation in one webhook config can silently skip a subset of pods).

`pup apm troubleshooting list` surfaces injection errors that Datadog's backend received from the cluster — these point to cluster-side mutation failures that may not be visible from `kubectl describe` alone. `pup apm service-library-config get` shows the runtime SDK config the tracer is operating under; an empty result with `ddTraceConfigs` configured, or unexpected values, points to UST/config-propagation issues.

---

## Step 2: State Your Hypotheses

Before investigating, explicitly state your ranked hypotheses based on triage output. Do not skip this step.

**When the user reports multiple affected services in the same namespace, diagnose each independently.** Two pods can fail injection for entirely different reasons (one opt-out annotation, one missing namespace label, one with pre-existing ddtrace). Do not assume a shared root cause — investigate each service's pod spec, annotations, and runtime separately and surface findings per-service.

| Triage signal | Strong hypothesis |
|---|---|
| Traces arriving + pod in instrumented list | Not a real problem — likely a UI filter or time window. Tell the user and stop |
| No traces + pod NOT in instrumented list + no init container | Injection never happened — investigate: namespace targeting, webhook, pod-selector, opt-out annotation, pod not restarted |
| No traces + pod NOT in instrumented list + init container present | Injection attempted but failed — check `pup apm troubleshooting list` for injection errors |
| No traces + pod in instrumented list + init container present | Tracer injected but not reporting — investigate: connectivity, DD_SITE, API key |
| Pod events show CrashLoopBackOff or init container errors | Init container failure — check existing ddtrace, runtime version |
| Traces arriving but wrong service/env | UST labels missing or misconfigured on the Deployment |

State your top 1-3 hypotheses explicitly: *"Based on triage, I think the most likely cause is X because Y."*

---

## Step 3: Investigate

Use only the tools relevant to your hypotheses. Each observation informs your next action.

---

### Cluster-side investigation tools

**Is the pod in the Agent namespace?**
SSI never instruments pods in the same namespace as the Datadog Agent.
```bash
kubectl get pods -n <AGENT_NAMESPACE>
```

**Were pods restarted after SSI was enabled?**

> **Confirm with the user before restarting.** Tell the user: "Pods must be restarted for SSI to inject into them. I'll restart `<DEPLOYMENT_NAME>` in `<APP_NAMESPACE>`. Ready to proceed?" Wait for confirmation.

### Claude runs

```bash
kubectl rollout restart deployment/<DEPLOYMENT_NAME> -n <APP_NAMESPACE>
kubectl wait --for=condition=Ready pod -l app=<APP_LABEL> -n <APP_NAMESPACE> --timeout=120s
```

### Claude runs

```bash
pup fleet instrumented-pods list <CLUSTER_NAME>
```

**Does the namespace carry the Admission Controller opt-in label?**
When the Admission Controller runs with `mutateUnlabelled: false`, injection happens only in namespaces explicitly labeled `admission.datadoghq.com/mutate-pods=true`. A namespace missing this label silently has SSI skipped for every pod in it — a common cause when most cluster services are instrumented but one namespace's services aren't.

```bash
kubectl get namespace <APP_NAMESPACE> -o jsonpath='{.metadata.labels}'
kubectl get namespace <APP_NAMESPACE> --show-labels
```

Fix: label the namespace, then restart the affected deployments so the AC mutates them on pod recreate.
```bash
kubectl label namespace <APP_NAMESPACE> admission.datadoghq.com/mutate-pods=true
```

**Is namespace targeting filtering the pod out?**
```bash
kubectl get datadogagent datadog -n <AGENT_NAMESPACE> -o yaml | grep -A 15 instrumentation
```
Fix: update `enabledNamespaces` in `datadog-agent.yaml`.

### Claude runs

```bash
kubectl apply -f datadog-agent.yaml
```

**Is a `podSelector` target filtering the pod out?**
If `targets` with `podSelector` is configured, only pods whose labels match the selector are instrumented. Check whether the app pod's labels match any target:
```bash
kubectl get datadogagent datadog -n <AGENT_NAMESPACE> -o yaml | grep -A 20 targets
kubectl get pod <POD_NAME> -n <APP_NAMESPACE> --show-labels
```
Fix: add a matching label to the pod template, or broaden the `podSelector`, then apply and restart.

**Is a pod annotation opting it out — or missing the AC's injection-success annotation?**
Two annotations to look for:
- `admission.datadoghq.com/enabled: "false"` — explicit opt-out, AC skips the pod.
- `admission.datadoghq.com/status: injected` — set by the AC after successful mutation; its **absence** on a running pod is positive evidence the AC never mutated it.

```bash
kubectl get pod <POD_NAME> -n <APP_NAMESPACE> -o jsonpath='{.metadata.annotations}'
kubectl get pod <POD_NAME> -n <APP_NAMESPACE> -o yaml | grep -A 10 annotations
```
Fix: remove an opt-out annotation from the Deployment pod template, then apply and restart.

**Are the expected `DD_*` environment variables present in the running pod?**
SSI injects `DD_SERVICE`, `DD_ENV`, `DD_VERSION`, `DD_TRACE_*`, and `LD_PRELOAD` into the container env when it mutates a pod. Their absence confirms the mutation did not run; their presence with unexpected values points to UST label mismatches or `ddTraceConfigs` issues.

```bash
kubectl exec -n <APP_NAMESPACE> <POD_NAME> -- env | grep -E '^(DD_|LD_PRELOAD)'
kubectl describe pod <POD_NAME> -n <APP_NAMESPACE> | grep -E 'DD_|LD_PRELOAD'
```

### Claude runs

```bash
kubectl apply -f <your-app-deployment.yaml>
```

> **Confirm with the user before restarting.** Tell the user: "I need to restart `<DEPLOYMENT_NAME>` in `<APP_NAMESPACE>` for this change to take effect. Ready to proceed?" Wait for confirmation.

### Claude runs

```bash
kubectl rollout restart deployment/<DEPLOYMENT_NAME> -n <APP_NAMESPACE>
```

**Does the app have existing custom instrumentation?**
SSI silently disables itself when it detects existing tracer code. Scan source files for:
- Python: `import ddtrace`, `ddtrace.patch_all()`
- Node.js: `require('dd-trace')`, `DD.init()`
- Java: `GlobalTracer.register(`, `dd-java-agent`
- .NET: `Tracer.Instance`, `DD.Trace`
- Ruby: `require 'ddtrace'`, `Datadog.configure`
- PHP: `DDTrace\`

Also check dependency manifests: `requirements.txt`, `package.json`, `Gemfile`, `pom.xml`.

Fix: remove the import/package, rebuild image, reload into cluster, restart pod.

**Is the base image Alpine (musl libc)?**
K8s SSI injects `LD_PRELOAD` as an environment variable into the pod — it does not rely on `/etc/ld.so.preload`, so musl/Alpine images are supported. This is not a blocker for Kubernetes SSI.

**Is the runtime version supported?**
```bash
kubectl exec -n <APP_NAMESPACE> <POD_NAME> -- python --version
kubectl exec -n <APP_NAMESPACE> <POD_NAME> -- node --version
kubectl exec -n <APP_NAMESPACE> <POD_NAME> -- java -version
```
Verify against [SSI compatibility matrix](https://docs.datadoghq.com/tracing/trace_collection/automatic_instrumentation/single-step-apm/compatibility/).

**Is the admission webhook registered?**
```bash
kubectl get mutatingwebhookconfigurations | grep datadog
kubectl get pods -n <AGENT_NAMESPACE> -l app=datadog-cluster-agent
kubectl logs -n <AGENT_NAMESPACE> -l app=datadog-cluster-agent --tail=100
```

**Did injection produce errors?**
Get the node hostname first, then query Datadog for injection errors:
```bash
kubectl get pod <POD_NAME> -n <APP_NAMESPACE> -o jsonpath='{.spec.nodeName}'
pup apm troubleshooting list --hostname <NODE_HOSTNAME> --timeframe 1h
```

**Is the Agent sending data to Datadog?**
```bash
kubectl exec -n <AGENT_NAMESPACE> \
  $(kubectl get pod -n <AGENT_NAMESPACE> -l app=datadog-agent -o name | head -1) \
  -- agent status | grep -A 5 "APM Agent"
```

---

### Datadog-side investigation tools

**Is the tracer reporting?**
```bash
pup fleet tracers list --filter "service:<SERVICE_NAME>"
```

**Does APM recognise the service?**
```bash
pup apm services list --env <ENV>
```

**What SDK configuration is the service running with?**
Shows env vars the tracer is configured with (e.g. `DD_TRACE_ENABLED`, `DD_SERVICE`, `DD_ENV`, sampling rules). Empty output is expected if `ddTraceConfigs` was not set in `enable-ssi`; a populated output mismatching what was configured indicates the change didn't propagate.
```bash
pup apm service-library-config get --service-name <SERVICE_NAME> --env <ENV>
```

**Are traces arriving?**
```bash
pup traces search --query "service:<SERVICE_NAME>" --from 1h --limit 10
```

**Which agent is the tracer connected to?**
Use if connectivity between tracer and Agent is suspected.
```bash
pup fleet agents list --filter "hostname:<NODE_HOSTNAME>"
pup fleet agents tracers <AGENT_KEY> --filter "service:<SERVICE_NAME>"
```

---

## Step 4: Reflect Before Concluding

Before applying any fix, answer:
1. What evidence confirms my hypothesis?
2. What evidence would contradict it — and have I checked?
3. Is there a simpler explanation I haven't considered?

If the conclusion doesn't hold up, return to Step 2 with new hypotheses. Keep iterating until you can defend the conclusion against all three questions.

---

## Step 5: Fix

Apply the fix for the confirmed root cause. If the fix requires a code or Dockerfile change, rebuild and reload:

### Claude runs

```bash
docker build -f <DOCKERFILE_PATH> -t <IMAGE_NAME> <BUILD_CONTEXT>
```

[DECISION: cluster type]
- kind (local): load the image into the cluster

### Claude runs

```bash
kind load docker-image <IMAGE_NAME> --name <CLUSTER_NAME>
```

- Registry-based: skip — image will be pulled on next deployment

> **Confirm with the user before restarting.** Tell the user: "I need to restart `<DEPLOYMENT_NAME>` in `<APP_NAMESPACE>` to apply the fix. Ready to proceed?" Wait for confirmation.

### Claude runs

```bash
kubectl rollout restart deployment/<DEPLOYMENT_NAME> -n <APP_NAMESPACE>
kubectl wait --for=condition=Ready pod -l app=<APP_LABEL> -n <APP_NAMESPACE> --timeout=120s
```

---

## Step 6: Verify

Re-run triage to confirm the fix worked:

### Claude runs

```bash
pup traces search --query "service:<SERVICE_NAME>" --from 1h --limit 5
pup fleet instrumented-pods list <CLUSTER_NAME>
```

If traces are arriving and the pod is in the instrumented list — resolved. Automatically proceed to `onboarding-summary` now — do not ask the user for permission.

ERROR: Still not resolved — return to Step 2 with the new triage data and form updated hypotheses.

---

## Security constraints

- Never write a raw API key into any file or chat message
- Never run `kubectl delete` without user confirmation
- Never modify `admissionController` settings directly
- `docker push` to a registry always requires user confirmation

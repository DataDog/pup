---
name: enable-ssi
description: Enable Single Step Instrumentation (SSI) on Kubernetes — automatically instruments applications for APM without code changes. Only use if the Datadog Agent is already running on the cluster — if not, use agent-install first.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,apm,kubernetes,ssi,instrumentation,single-step
  alwaysApply: "false"
---

# Enable APM on Kubernetes via Single Step Instrumentation

> **Before doing anything else:** Fully resolve all variables in `## Context to resolve before acting`. Do not begin Step 0 until every variable has a concrete value.

---

> **Silent failure — check this before any other step:**
>
> If the application has `ddtrace`, `dd-trace`, or any OpenTelemetry SDK in its **dependency manifest** (`requirements.txt`, `package.json`, `Gemfile`, `go.mod`, `pom.xml`) — even with no import statements in code — SSI will silently disable itself at runtime.
>
> The failure is invisible: init containers run and complete, the pod starts healthy, no errors appear in `kubectl` or `pup`, but no traces arrive. The injector detects the user-installed tracer and exits cleanly without logging anything.
>
> ### Claude runs
>
> ```bash
> grep -rE "ddtrace|dd-trace|opentelemetry" \
>   requirements.txt package.json Gemfile go.mod pom.xml 2>/dev/null \
>   || echo "No tracer dependency found"
> ```
>
> If any match — **stop**. Remove the package entirely (not just the import), rebuild the image, reload it into the cluster, and restart the pod before continuing. A package present in the manifest is enough to trigger this even if it is never imported.

---

## Triggers

Invoke this skill when the user expresses intent to:
- Enable APM on a Kubernetes cluster
- Instrument Kubernetes applications with Datadog tracing
- Set up Single Step Instrumentation (SSI)

Do NOT invoke this skill if:
- The Datadog Agent is not yet installed — run `agent-install` first
- The user wants to verify SSI after setup — use `verify-ssi`
- The user wants to enable Profiler, AppSec, or Data Streams — use `dd-apm-k8s-sdk-features`

---

## Prerequisites

> **These are not a reading exercise — actively verify each one before proceeding.**

**Environment**
- [ ] Datadog Agent is installed and healthy — `agent-install` complete
- [ ] Kubernetes v1.20+
- [ ] Linux node pools only — Windows pods require explicit namespace exclusion
- [ ] Cluster is not ECS Fargate — unsupported
- [ ] Not a hardened SELinux environment — unsupported
- [ ] Not a very small VM instance (e.g. t2.micro) — SSI can hit init timeouts
- [ ] No PodSecurity baseline or restricted policy enforced

**Language and runtime**
- [ ] Application language is one of: Java, Python, Ruby, Node.js, .NET, PHP
- [ ] Runtime version is within SSI's supported range — verify against the [SSI compatibility matrix](https://docs.datadoghq.com/tracing/trace_collection/automatic_instrumentation/single-step-apm/compatibility/)
- [ ] Node.js app is not using ESM — SSI does not support ESM
- [ ] Java app is not already using a `-javaagent` JVM flag

**Existing instrumentation** — confirmed clean by the check at the top of this skill. If you skipped that check, go back and run it now.

---

## Context to resolve before acting

> **Discover from the cluster — do not ask the user for information you can find yourself.**

| Variable | How to resolve |
|---|---|
| `AGENT_NAMESPACE` | Same namespace used in `agent-install` (e.g. `datadog`) |
| `APP_NAMESPACE` | Run `kubectl get namespaces --no-headers \| awk '{print $1}' \| grep -vE '^(kube-system\|kube-public\|kube-node-lease\|datadog\|local-path-storage)$'` — instrument all non-system namespaces, or use the namespace(s) the user mentioned |
| `TARGET_LANGUAGES` | Run `kubectl get pods -A -o jsonpath='{.items[*].spec.containers[*].image}'` and infer language from image names, or check Dockerfiles/manifests in the workspace. If uncertain, enable all languages. |
| `DEPLOYMENT_NAME` | Run `kubectl get deployments -A --no-headers` — identify application deployments (exclude system components) |
| `APP_LABEL` | Check `spec.selector.matchLabels` in the Deployment manifest via `kubectl get deployment <DEPLOYMENT_NAME> -n <APP_NAMESPACE> -o yaml` |
| `CLUSTER_NAME` | Check `spec.global.clusterName` in `datadog-agent.yaml`, or `kubectl config current-context` — needed for kind clusters in Step 0 |
| `ENV` | Use `apm-evals` if running in an eval cluster (kind cluster names contain "evalya"). Otherwise use `production` unless the user specifies otherwise. |
| `SERVICE_NAME` | Use the deployment name (e.g. `python-app` → service `python-app`). Do not ask the user. |
| `VERSION` | Use `1.0.0` as the default. Do not ask the user. |

---

## Step 0 (Only if existing instrumentation detected): Remove Manual Instrumentation

Scan all source files for: `import ddtrace`, `from ddtrace`, `require 'ddtrace'`, `require("dd-trace")`, `opentelemetry`, `tracer.trace(`

Also check dependency manifests for `ddtrace` / `dd-trace` / OTel SDK packages.

If found — remove the import/package, then rebuild and reload:

### Claude runs

```bash
docker build -f <DOCKERFILE_PATH> -t <IMAGE_NAME> <BUILD_CONTEXT>
```

[DECISION: how does this cluster get local images?]

Check the repo's setup script (e.g. `create.sh`, `Makefile`, `justfile`) for how images are loaded — do not guess from the cluster name or context. Common patterns:

| What you find in the setup script | Load command |
|---|---|
| `minikube image load` or `minikube cache add` | `minikube -p <PROFILE> image load <IMAGE_NAME>` — profile is the `-p` flag value in the script, NOT necessarily the kubectl context name |
| `kind load docker-image` | `kind load docker-image <IMAGE_NAME> --name <CLUSTER_NAME>` |
| `docker push` to a registry | Push the new image; the cluster will pull on restart — skip local load |
| `k3d image import` | `k3d image import <IMAGE_NAME> -c <CLUSTER_NAME>` |
| No image load step (cloud cluster, always pulls from registry) | Skip — image will be pulled on next deployment |

If the setup script is ambiguous, run the load command it uses exactly as written.

- Registry-based: skip — image will be pulled on next deployment

> **Confirm with the user before restarting.** Tell the user: "I need to restart `<DEPLOYMENT_NAME>` in `<APP_NAMESPACE>` to pick up the rebuilt image. Ready to proceed?" Wait for confirmation.

### Claude runs

```bash
kubectl rollout restart deployment/<DEPLOYMENT_NAME> -n <APP_NAMESPACE>
kubectl wait --for=condition=Ready pod \
  -l app=<APP_LABEL> \
  -n <APP_NAMESPACE> \
  --timeout=120s
```

---

## Step 1: Extend the DatadogAgent Manifest with APM

SSI is configured on the existing `DatadogAgent` resource — do not create a separate manifest.

**Choose targeting scope based on what the user asked for:**
- User asked to instrument **all applications** or didn't specify scope → **use Option A (cluster-wide)**
- User asked for specific namespaces only → use Option B
- User asked to exclude namespaces from cluster-wide → use Option C
- User asked for specific pods/workloads → use Option D

> **Default is cluster-wide (Option A).** If the user said "all my applications", "my whole cluster", or didn't restrict scope, use Option A with no `enabledNamespaces` or `targets`.

Recommended `ddTraceVersions`: `java: "1"`, `python: "2"`, `js: "5"`, `dotnet: "3"`, `ruby: "2"`, `php: "1"`

**Option A — Cluster-wide (default):**
```yaml
features:
  apm:
    instrumentation:
      enabled: true
```

**Option B — Specific namespaces only:**
```yaml
features:
  apm:
    instrumentation:
      enabled: true
      enabledNamespaces:
        - <APP_NAMESPACE>
```

**Option C — Cluster-wide with exclusions:**
```yaml
features:
  apm:
    instrumentation:
      enabled: true
      disabledNamespaces:
        - jenkins
        - kube-system
```

**Option D — Target specific workloads:**
```yaml
features:
  apm:
    instrumentation:
      enabled: true
      targets:
        - name: <TARGET_NAME>
          namespaceSelector:
            matchNames:
              - <APP_NAMESPACE>
          ddTraceVersions:
            <LANGUAGE>: "<MAJOR_VERSION>"
```

> **Note:** `ddTraceVersions` only applies inside a `targets[]` entry (Option D). It is not valid alongside `enabledNamespaces` or at the `instrumentation` level directly.

### Claude runs

```bash
kubectl apply -f datadog-agent.yaml
```

If `datadogagent.datadoghq.com/datadog configured` — continue to Step 2.

ERROR: Validation error — check YAML. `enabledNamespaces` and `disabledNamespaces` cannot both be set.

---

## Step 2: Inform the User About Unified Service Tags

> **Do NOT modify application Deployments without explicit user confirmation.** Applying labels to existing application workloads is a change to customer-managed resources.

Inform the user that adding Unified Service Tags (UST) to their Deployments will enable proper service/env/version tagging in Datadog. This is optional for SSI to work but recommended for full observability:

```yaml
# Add to both metadata.labels and spec.template.metadata.labels
tags.datadoghq.com/env: "<ENV>"
tags.datadoghq.com/service: "<SERVICE_NAME>"
tags.datadoghq.com/version: "<VERSION>"
```

If the user wants you to apply these, get their confirmation first. UST labels are not required for APM traces to flow — SSI works without them.

---

## Step 3: Restart Application Pods

> **Confirm with the user before restarting.** Tell the user: "I need to restart `<DEPLOYMENT_NAME>` in `<APP_NAMESPACE>` for SSI to inject into the pods. This will cause a brief outage. Ready to proceed?" Wait for confirmation.

### Claude runs

```bash
kubectl rollout restart deployment/<DEPLOYMENT_NAME> -n <APP_NAMESPACE>

kubectl wait --for=condition=Ready pod \
  -l app=<APP_LABEL> \
  -n <APP_NAMESPACE> \
  --timeout=120s
```

If pods restart cleanly, init containers named `datadog-lib-<language>-init` will be visible in the pod spec.

ERROR: Pods crash-looping — check for existing custom instrumentation. See `troubleshoot-ssi`.

---

## Done

Exit when ALL of the following are true:
- [ ] `features.apm.instrumentation` is present in the applied `DatadogAgent` manifest
- [ ] User has been informed that they need to restart their application pods
- [ ] User has been informed about Unified Service Tags (UST) and how to apply them if desired
- [ ] Scope confirmed: which workloads are instrumented, which were skipped and why

Automatically proceed to `verify-ssi` now — do not ask the user for permission.

---

## Security constraints

- Never write a raw API key into any file or chat message
- Never use namespace `default` for Datadog resources
- Never modify `admissionController` settings directly — SSI manages this via the Operator
- Do not add APM config to application manifests — configure only via `DatadogAgent`
- Exception: UST labels (`tags.datadoghq.com/*`) on application Deployments are required and intentional
- Never run `kubectl delete` without user confirmation
- `docker push` to a registry always requires user confirmation
- **Never use `kubectl patch` to apply UST labels or any Deployment changes.** Always edit the Deployment YAML file and `kubectl apply -f`. Changes made with `kubectl patch` are transient and will be overwritten on the next rollout.

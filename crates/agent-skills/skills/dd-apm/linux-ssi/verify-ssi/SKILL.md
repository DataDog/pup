---
name: verify-ssi
description: Verify Single Step Instrumentation (SSI) is working end-to-end on Linux hosts — SSI automatically instruments applications for APM without code changes. Only use after enable-ssi has run.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,apm,linux,ssi,verification,instrumentation,ld-preload
  alwaysApply: "false"
---

# Verify APM SSI on Linux

> **Before doing anything else:** Fully resolve all variables in `## Context to resolve before acting`. Do not begin Step 1 until every variable has a concrete value.

## Triggers

Invoke this skill when the user expresses intent to:
- Confirm SSI is working after installing the Datadog Agent on Linux
- Check whether a Linux process is being instrumented
- Verify the tracer is running and reporting telemetry

Do NOT invoke this skill if:
- SSI has not been enabled yet — run `agent-install` first
- Services have not been restarted since the agent was installed — restart them first, then verify

---

## Prerequisites

- [ ] `agent-install` is complete
- [ ] Application services have been restarted since the agent was installed

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
ERROR: No browser available: `export DD_APP_KEY=<your-app-key>`

---

## Context to resolve before acting

| Variable | How to resolve |
|---|---|
| `DD_HOSTNAME` | Hostname as Datadog sees it — from `sudo datadog-agent status` output |
| `SERVICE_NAME` | Expected service name in APM — ask the user |
| `ENV` | Environment tag — ask the user |
| `DD_SITE` | `grep "^site:" /etc/datadog-agent/datadog.yaml` via SSH, or ask the user |
| `SSH_KEY` | Path to SSH private key |
| `SSH_USER` | SSH username |
| `SSH_HOST` | Hostname or IP of the target host |

---

## Step 1: Confirm the Process is Injected

Use `/proc/<pid>/maps` — this is the authoritative check. It shows the actual shared libraries loaded into the running process, which is the only way to confirm the launcher and tracer `.so` files were actually loaded.

### Claude runs

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "pgrep -a -f '<SERVICE_NAME>' | head -5"
```

Use the PID from above:

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo cat /proc/<PID>/maps | grep -E 'launcher|apm-library|datadog'"
```

If the output includes both the launcher (e.g. `launcher.preload.so`) and a language library (e.g. `apm-library-python`) — injection succeeded for this process.

ERROR: Launcher present but no language library — launcher ran but couldn't inject. Check for injection errors:

### Claude runs

```bash
pup apm troubleshooting list --hostname <DD_HOSTNAME> --timeframe 1h
```

ERROR: Neither present — process was not injected. Check `/etc/ld.so.preload`:

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> "cat /etc/ld.so.preload"
```

If empty — install did not set up the launcher. Re-run the install script with `DD_APM_INSTRUMENTATION_ENABLED=host`. If non-empty but the process still isn't injected — the process was started before the launcher was installed. Restart the service and recheck.

---

## Step 2: Confirm the Agent is Receiving Traces

### Claude runs

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo datadog-agent status 2>&1 | grep -A 15 'APM Agent'"
```

Healthy output shows:
- `feature_auto_instrumentation_enabled: true`
- `Receiver (previous minute)` with `> 0` traces

ERROR: `feature_auto_instrumentation_enabled: false` — SSI not active on the agent. Check `apm_config` in `/etc/datadog-agent/datadog.yaml`.

ERROR: `Receiver (previous minute): 0` — agent running but no traces yet. Generate traffic first (see Step 3), then recheck.

---

## Step 3: Confirm the Service is Visible in Datadog

### Claude runs

```bash
DD_SITE=<DD_SITE> pup apm services list --env <ENV> --from 1h
```

If `<SERVICE_NAME>` appears with `isTraced: true` — traces are reaching the Datadog backend.

> **Flask / ddtrace v3 naming note:** With ddtrace >=3.x, Flask spans are emitted as `service:flask` rather than `service:<DD_SERVICE>`. The `DD_SERVICE` value appears as `base_service` on the spans. If you set `DD_SERVICE=my-app`, search for `service:flask` in the APM UI — the service list will show `flask`, not `my-app`. Check the `base_service` tag to confirm it matches your `DD_SERVICE`.

ERROR: Service missing — generate traffic to trigger trace creation:

### Claude runs

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo ss -tlnp 2>/dev/null | grep <PID> || sudo netstat -tlnp 2>/dev/null | grep <PID>"
```

Use the port from above:
```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "for i in \$(seq 1 10); do curl -s -o /dev/null http://localhost:<PORT>/; done"
```

Wait 30 seconds, then retry:
```bash
DD_SITE=<DD_SITE> pup apm services list --env <ENV> --from 10m
DD_SITE=<DD_SITE> pup traces search --query "service:<SERVICE_NAME>" --from 10m --limit 5
```

ERROR: Still missing — check for injection errors and go to `troubleshoot-ssi`:
```bash
pup apm troubleshooting list --hostname <DD_HOSTNAME> --timeframe 1h
```

---

## Done

Exit when ALL of the following are true:
- [ ] Step 1: launcher + language library both visible in `/proc/<PID>/maps`
- [ ] Step 2: agent APM receiver shows `> 0` traces/min
- [ ] Step 3: service appears in `pup apm services list`

If any check fails, go to `troubleshoot-ssi`.

When all steps pass, automatically proceed to `onboarding-summary` now — do not ask the user for permission.

---

## Security constraints

- Never write a raw API key into any file or chat message
- Always confirm before restarting production services

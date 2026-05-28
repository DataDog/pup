---
name: enable-ssi
description: Configure Unified Service Tags and verify Single Step Instrumentation (SSI) injection on Linux hosts — SSI automatically instruments applications for APM without code changes. Only use if the Datadog Agent is already installed.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,apm,linux,ssi,instrumentation,single-step,ld-preload,ust
  alwaysApply: "false"
---

# Configure SSI and Unified Service Tags on Linux

> **Before doing anything else:** Fully resolve all variables in `## Context to resolve before acting`. Do not begin Step 0 until every variable has a concrete value.

## Triggers

Invoke this skill when:
- The Datadog Agent is already installed with SSI (`DD_APM_INSTRUMENTATION_ENABLED=host` was used) and you need to configure Unified Service Tags on the application service
- The user wants to set `DD_SERVICE`, `DD_ENV`, `DD_VERSION` on a running service
- SSI is installed but `/proc/<pid>/maps` doesn't show the language tracer (launcher-only injection)

Do NOT invoke this skill if:
- The Datadog Agent is not yet installed — run `agent-install` first
- SSI packages are missing from `/opt/datadog-packages/` — re-run `agent-install`
- The target is a Kubernetes cluster — use `dd-apm-k8s-enable-ssi` instead

---

## Background

When the install script runs with `DD_APM_INSTRUMENTATION_ENABLED=host`, it:
1. Installs `datadog-apm-inject` and language library packages under `/opt/datadog-packages/`
2. Writes the launcher path into `/etc/ld.so.preload`
3. SSI is now armed — every new process on the host gets the launcher injected at startup

**What SSI does NOT configure automatically:**
- `DD_SERVICE`, `DD_ENV`, `DD_VERSION` — these must be set on the application process for traces to be tagged correctly
- Without `DD_SERVICE`, the tracer auto-detects a service name (often the process name or framework name), which may not match what the user expects

---

## Prerequisites

**Verify SSI is armed:**

### Claude runs

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "cat /etc/ld.so.preload && ls /opt/datadog-packages/ | grep apm"
```

If `/etc/ld.so.preload` contains a path to the launcher, and `/opt/datadog-packages/datadog-apm-inject` exists — SSI is armed.

ERROR: Either missing — run `agent-install` first.

**Check for existing manual instrumentation:**

### Claude runs

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> "
grep -r 'import ddtrace\|from ddtrace\|require .dd-trace.\|opentelemetry' <SOURCE_DIR> 2>/dev/null | head -5 || echo 'No manual instrumentation found'
"
```

ERROR: Manual instrumentation found — SSI silently disables itself when it detects an existing tracer. Remove the manual import/package before proceeding.

**Check base libc:**

### Claude runs

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "ldd --version 2>&1 | head -1"
```

ERROR: musl — SSI requires glibc. No workaround; must use a glibc-based OS.

---

## Context to resolve before acting

| Variable | How to resolve |
|---|---|
| `SERVICE_NAME` | Ask the user — how the service should appear in Datadog APM (e.g. `payment-api`) |
| `ENV` | Ask the user — environment name (e.g. `production`, `staging`, `dev`) |
| `VERSION` | Ask the user or read from the app's version file / git tag |
| `SYSTEMD_SERVICE_NAME` | From `systemctl list-units --type=service --state=running` on the host — the unit running the app |
| `SSH_KEY` | Path to SSH private key |
| `SSH_USER` | SSH username |
| `SSH_HOST` | Hostname or IP of the target host |

---

## Step 0 (Only if existing instrumentation detected): Remove Manual Instrumentation

- Python: `pip uninstall ddtrace`, remove `import ddtrace` / `ddtrace-run` from CMD
- Node.js: `npm uninstall dd-trace`, remove `require('dd-trace')` 
- Java: remove `-javaagent:/path/to/dd-java-agent.jar` JVM flag
- Ruby: `gem uninstall ddtrace`, remove `require 'ddtrace'`
- .NET: remove `Datadog.Trace` NuGet and profiler env vars

After removing, restart the service. **Confirm with the user before restarting.** Tell the user: "I need to restart `<SYSTEMD_SERVICE_NAME>` to remove the old instrumentation. This will cause a brief outage. Ready to proceed?" Wait for confirmation.

---

## Step 1: Set Unified Service Tags on the Application Process

Without UST, traces arrive with an auto-detected service name that may not match user expectations, and won't be tagged with env or version.

**For systemd-managed services** (most common):

### Claude runs

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo systemctl cat <SYSTEMD_SERVICE_NAME>"
```

Add a drop-in override (preserves the original unit file):

### What you need to do in a terminal

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST>
sudo systemctl edit <SYSTEMD_SERVICE_NAME>
```

Add to the editor:

```ini
[Service]
Environment="DD_SERVICE=<SERVICE_NAME>"
Environment="DD_ENV=<ENV>"
Environment="DD_VERSION=<VERSION>"
```

Apply:

### Claude runs

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo systemctl daemon-reload && sudo systemctl show <SYSTEMD_SERVICE_NAME> | grep -E 'DD_SERVICE|DD_ENV|DD_VERSION'"
```

If the UST vars appear in the output — configuration applied.

**For supervisord:**
```ini
# In [program:<name>] section of supervisord.conf
environment=DD_SERVICE="<SERVICE_NAME>",DD_ENV="<ENV>",DD_VERSION="<VERSION>"
```
Reload: `sudo supervisorctl reload`

**For pm2:**
```js
// ecosystem.config.js
env: { DD_SERVICE: "<SERVICE_NAME>", DD_ENV: "<ENV>", DD_VERSION: "<VERSION>" }
```
Reload: `pm2 reload <app>`

---

## Step 2: Restart the Service

**Confirm with the user before restarting.** Tell the user: "I need to restart `<SYSTEMD_SERVICE_NAME>` for SSI to inject into it. This will cause a brief outage. Ready to proceed?" Wait for confirmation.

### Claude runs

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo systemctl restart <SYSTEMD_SERVICE_NAME> && sleep 3 && sudo systemctl is-active <SYSTEMD_SERVICE_NAME>"
```

If `active` is returned — service is running.

ERROR: Returns `failed` — check logs:
```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo journalctl -u <SYSTEMD_SERVICE_NAME> --since '1 minute ago' | tail -30"
```

---

## Step 3: Confirm Injection and UST in the Running Process

### Claude runs

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "pgrep -a -f '<SERVICE_NAME>' | head -3"
```

Use the PID:

```bash
# Authoritative injection check
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo cat /proc/<PID>/maps | grep -E 'launcher|apm-library|datadog'"

# UST vars in process environment
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo cat /proc/<PID>/environ | tr '\0' '\n' | grep -E 'DD_SERVICE|DD_ENV|DD_VERSION'"
```

If both the launcher and language library appear in maps, and UST vars are in environ — SSI and tagging are fully configured.

ERROR: Launcher in maps but no language library — injection attempted but failed. Run:
```bash
pup apm troubleshooting list --hostname <DD_HOSTNAME> --timeframe 15m
```

Go to `troubleshoot-ssi` if errors are present.

---

## Done

Exit when ALL of the following are true:
- [ ] Launcher and language library visible in `/proc/<PID>/maps`
- [ ] `DD_SERVICE`, `DD_ENV`, `DD_VERSION` present in `/proc/<PID>/environ`
- [ ] Service is running and healthy

Automatically proceed to `verify-ssi` now — do not ask the user for permission.

---

## Security constraints

- Never write a raw API key into any file or chat message
- Always confirm with the user before restarting production services
- Do not modify application source code — configure only via environment variables in the service unit

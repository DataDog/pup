---
name: agent-install
description: Install the Datadog Agent on Linux hosts via SSH with Single Step Instrumentation (SSI) enabled — SSI automatically instruments applications for APM without code changes. Only use if no agent is installed yet.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,apm,linux,agent,install,ssi,ssh
  alwaysApply: "false"
---

# Install Datadog Agent on Linux

> **Before doing anything else:** Fully resolve all variables in `## Context to resolve before acting`. Do not begin Step 1 until every variable has a concrete value.

## Triggers

Invoke this skill when the user expresses intent to:
- Install the Datadog Agent on Linux hosts or VMs
- Set up Datadog monitoring on bare-metal or cloud Linux instances
- Prepare Linux hosts for APM onboarding

Do NOT invoke this skill if:
- The Agent is already installed on all hosts — check with `datadog-agent status` first
- The target is a Kubernetes cluster — use `dd-apm-k8s-agent-install` instead

---

## Phase 0: Load Credentials

```bash
[ -f environment ] && source environment
echo "DD_API_KEY set: $([ -n "${DD_API_KEY:-}" ] && echo yes || echo no)"
echo "DD_SITE: ${DD_SITE:-not set}"
```

**If `DD_API_KEY` is already set** — proceed directly to gathering infrastructure info.

**If `DD_API_KEY` is not set** — tell the user:

> Please run the following in this chat to set your credentials (the `!` prefix executes it in this session):
> ```
> ! export DD_API_KEY=your-api-key-here
> ! export DD_SITE=datadoghq.com
> ```

Wait for the user to run the commands, then re-run the check above before continuing.

---

## Phase 1: Gather Infrastructure Info

Only do this phase if the user hasn't already provided the information. If SSH credentials are known, skip to Phase 2.

Ask the user:
1. **Which hosts** need the agent? Get a list of IPs or hostnames.
2. **How do I SSH to them?** Get the SSH user, key path, and any jump host or bastion configuration.
3. **Do any hosts already have the Datadog Agent installed?** If so, skip install for those hosts and go straight to `verify-ssi`.

### Claude runs

Verify SSH works for each host before proceeding:

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> "hostname"
```

If it returns a hostname — proceed.
ERROR: Connection refused or timeout — resolve connectivity before continuing.

Once SSH is confirmed, present a plan to the user before proceeding. For example:

```
Here's what I'm going to do:
  1. Install the Datadog Agent with SSI on: <host1>, <host2>, ...
  2. Verify each agent is running and healthy
  3. Discover services on each host that need restarting for SSI to take effect
  4. After you restart services, verify instrumentation is working

Ready to proceed?
```

Wait for user confirmation before starting installs.

---

## Prerequisites

**Per host — check before installing:**

### Claude runs

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "uname -m && cat /etc/os-release | grep -E '^(ID|VERSION_ID|PRETTY_NAME)='"
```

If architecture is `x86_64` or `aarch64`, and the OS is a supported distribution (Ubuntu 16.04+, Debian 9+, RHEL/CentOS 6-9, Amazon Linux 2/2023, SUSE 12+) — proceed.

ERROR: Architecture is `armv7l` (32-bit ARM) or unsupported OS — stop. Datadog Agent 7 and SSI do not support this configuration.

---

## Context to resolve before acting

| Variable | How to resolve |
|---|---|
| `DD_API_KEY` | Check `echo $DD_API_KEY` first — if set, use it. Otherwise ask the user for their API key from Datadog UI: Organization Settings → API Keys. Never log or print the key. |
| `DD_SITE` | Check `echo $DD_SITE` first — if set, use it. Otherwise ask the user. Default: `datadoghq.com`. Options: `datadoghq.com`, `us3.datadoghq.com`, `us5.datadoghq.com`, `datadoghq.eu`, `ap1.datadoghq.com` |
| `SSH_KEY` | Ask the user for the path to their SSH private key, or check `CLAUDE.md` |
| `SSH_USER` | Ask the user for the SSH username. Default: `root` |
| `SSH_HOST` | Ask the user for the hostname or IP of the target host |
| `SSH_PORT` | Ask the user for the SSH port. Default: `22` |

---

## Phase 2: Install the Datadog Agent with SSI

Run for each host that does not already have the agent installed.

### Claude runs

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "DD_API_KEY=${DD_API_KEY} DD_SITE=${DD_SITE} DD_APM_INSTRUMENTATION_ENABLED=host bash -c \"\$(curl -L https://install.datadoghq.com/scripts/install_script_agent7.sh)\""
```

`DD_APM_INSTRUMENTATION_ENABLED=host` causes the install script to also install `datadog-apm-inject` and language library packages under `/opt/datadog-packages/` in one pass.

If the script completes without errors — proceed to Phase 2.

ERROR: `curl: command not found`:
```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "apt-get install -y curl 2>/dev/null || yum install -y curl"
```

ERROR: Permission error — ensure the SSH user has sudo access. The install script requires root.

ERROR: Script fails with GPG key error — retry; if it persists, check the host's DNS resolution for `keys.datadoghq.com`.

---

## Phase 3: Verify the Agent is Running and Healthy

### Claude runs

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo datadog-agent status 2>&1 | head -40"
```

Healthy output shows:
- `Agent (v7.XX.X)` with `Status: Running`
- `API Keys status: API Key ending with XXXX: Valid`

ERROR: `command not found` — installation did not complete. Re-run Phase 1.

ERROR: `API key invalid` — update and restart:
```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo sed -i 's/^api_key:.*/api_key: <NEW_API_KEY>/' /etc/datadog-agent/datadog.yaml && \
   (sudo systemctl restart datadog-agent 2>/dev/null || sudo service datadog-agent restart)"
```

ERROR: Agent service not running:
```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo systemctl start datadog-agent 2>/dev/null && sudo systemctl enable datadog-agent 2>/dev/null || sudo service datadog-agent start"
```

**Verify APM inject packages are present on disk** (not just registered):
```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "ls /opt/datadog-packages/ && sudo datadog-installer status 2>/dev/null | grep apm | head -10"
```

If `/opt/datadog-packages/datadog-apm-inject` exists — injection is available.

ERROR: Directory missing or empty — `datadog-installer status` may show the package as registered while its directory is actually empty (stale registration). Reinstall:
```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo datadog-installer remove datadog-apm-inject && \
   DD_API_KEY=${DD_API_KEY} DD_SITE=${DD_SITE} DD_APM_INSTRUMENTATION_ENABLED=host bash -c \"\$(curl -L https://install.datadoghq.com/scripts/install_script_agent7.sh)\""
```

**Verify hostname registration** — the Agent must resolve and register its hostname for the host to appear in Datadog. DNS lookup failures are common in containers and minimal VMs:

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo datadog-agent status 2>&1 | grep -iE '^\s+Hostname' | head -3"
```

If `Hostname: <some-name>` is shown — hostname resolved. Record this as `DD_HOSTNAME` for all subsequent steps.

ERROR: `Hostname: (none)` or any DNS resolution error — the agent can't resolve its own FQDN. Fix by setting the hostname explicitly in `datadog.yaml`:

```bash
# Read the actual system hostname
ACTUAL_HOSTNAME=$(ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> "hostname")

# Append to datadog.yaml only if not already set
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "grep -q '^hostname:' /etc/datadog-agent/datadog.yaml || \
   echo \"hostname: ${ACTUAL_HOSTNAME}\" | sudo tee -a /etc/datadog-agent/datadog.yaml"

# Restart the Agent
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo systemctl restart datadog-agent 2>/dev/null || sudo service datadog-agent restart"

# Confirm hostname is now registered
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo datadog-agent status 2>&1 | grep -iE '^\s+Hostname' | head -2"
```

---

## Phase 4: Discover Services That Need Restarting

SSI only injects into processes at startup. Existing processes keep running uninstrumented until restarted. Discover what's running so the user knows what to restart.

### Claude runs

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> \
  "sudo ss -lntp 2>/dev/null || sudo netstat -tlnp 2>/dev/null || cat /proc/net/tcp"
```

For each application-level listener (ignore sshd, systemd, chronyd):

```bash
ssh -o StrictHostKeyChecking=no -i <SSH_KEY> <SSH_USER>@<SSH_HOST> "
# Command line of the process
sudo cat /proc/<PID>/cmdline | tr '\0' ' '
# Service manager (may not be available in all environments)
sudo systemctl status <PID> 2>/dev/null | head -3 || true
# Parent process
PPID=\$(sudo awk '/PPid/ {print \$2}' /proc/<PID>/status)
sudo cat /proc/\$PPID/cmdline | tr '\0' ' '
"
```

Present findings to the user:

```
I found the following application services on <host>:

  Port 8080 — PID 1234 — /usr/bin/python3 /app/server.py
    Managed by: systemd unit flask-app.service

  Port 3000 — PID 5678 — node /app/server.js
    Managed by: supervisord

These services need to be restarted for Datadog SSI to inject into them.
Restart them however is appropriate for your environment, then let me know
and I'll verify the instrumentation.
```

**Do not offer to restart services. Do not restart services unless the user explicitly asks.**

---

## Done

Exit when ALL of the following are true:
- [ ] Agent running on each target host (`datadog-agent status` shows Running, API key valid)
- [ ] `/opt/datadog-packages/datadog-apm-inject` exists on disk on each host
- [ ] User has been informed which services need restarting
- [ ] User has confirmed they are ready to restart services

Automatically proceed to `enable-ssi` (if services need UST labels configured) or `verify-ssi` (if services have already been restarted) — do not ask the user for permission.

---

## Security constraints

- Never write a raw API key into any file or chat message
- Never store `DD_API_KEY` in shell history — pass it inline in the SSH command only
- If the user's API key appears in any output, redact it before displaying
- Always confirm before restarting production services

---
description: Comprehensive monitoring and alerting management including monitors, templates, notification routing, and downtimes. Complete lifecycle from creation to alert delivery to maintenance windows.
---

# Monitoring & Alerting Agent

Use this agent for monitor management and downtime operations through pup. The `dd-monitors` skill is the concise guide for routine monitor CRUD and monitor design best practices; select this agent when the request involves downtime operations or needs the combined monitoring-and-maintenance workflow.

## Monitors

List, search, inspect, create, update, and delete monitors:

```bash
pup monitors list
pup monitors list --tags="env:prod"
pup monitors search --query="production"
pup monitors get 12345
pup monitors create --file=monitor.json
pup monitors update 12345 --file=monitor.json
```

### Monitor deletion safety

Monitor deletion is supported, but destructive. Only delete a monitor when the user explicitly requests deletion and confirms the target monitor. Pup prompts for confirmation by default; `--yes` bypasses that safeguard and must not be used unless the user explicitly authorizes automated deletion. Do not silently rename monitors to mark them for deletion as a substitute for the requested operation.

```bash
pup monitors delete 12345
```

## Downtimes

Pup uses the singular `downtime` command group. List, inspect, create, and cancel downtimes:

```bash
pup downtime list
pup downtime get <downtime-id>
pup downtime create --file=downtime.json
pup downtime cancel <downtime-id>
```

Confirm the scope and time window with the user before creating or canceling a downtime. The typed CLI does not expose a downtime update command.

## Unsupported Operations

The typed pup CLI does not currently provide monitor-template or notification-rule commands. Creating, listing, applying, or managing templates, and configuring or testing notification routing rules, are unsupported through pup. Do not suggest `monitor-templates ...` or `notification-rules ...` command groups; direct users to the Datadog UI or the relevant API documentation instead.

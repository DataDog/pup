# dd-pup — pi extension for the Datadog `pup` CLI

Exposes the [`pup`](https://github.com/datadog-labs/pup) Datadog CLI as
first-class pi tools, so the LLM can query telemetry and manage Datadog
resources directly.

## Install

```bash
# 1. install pup
brew tap datadog-labs/pack
brew install pup
pup auth login

# 2. install the extension (defaults to user-global ~/.pi/agent/extensions)
pup skills install pi

# Install project-local instead (<repo>/.pi/extensions/dd-pup-pi):
pup skills install pi --project
```

pi auto-discovers the extension on next launch (or via `/reload`).
Override the pup binary path with `DD_PUP_BIN` if needed.

## Tools registered for the LLM

| Tool | Purpose |
| --- | --- |
| `pup_run` | Run **any** `pup` subcommand (escape hatch). JSON output enforced. |
| `pup_logs_search` | Search Datadog logs by query + time window. |
| `pup_logs_aggregate` | Counts / distributions / percentiles on logs. |
| `pup_metrics_query` | Time-series metric query (avg/sum/max/min/count). |
| `pup_traces_search` | APM trace search (durations are **nanoseconds**). |
| `pup_monitors_list` | List monitors with tag/name filters. |
| `pup_apm_services` | APM service list / stats per env. |
| `pup_auth_status` | Check or refresh Datadog auth. |

All telemetry tools default to a 1h window and small limits. On a 401/403
the extension transparently runs `pup auth refresh` once and retries.

## Slash commands

- `/pup <subcommand…>` — run pup directly and show output, no LLM round-trip.
- `/pup-auth` — quick menu: status / refresh / login / logout.

## Status widget

A footer line shows the Datadog site and token expiry, e.g.

```
pup: ✓ datadoghq.com (exp 2026-05-12 16:06:02)
```

## Design notes

- `pup_run` is the workhorse — any sub-domain the focused tools don't cover
  (incidents, SLOs, dashboards, downtimes, RUM, security signals, infra
  hosts, on-call, …) is still one tool call away.
- JSON output is auto-injected unless the caller passes `--output` themselves,
  so results are structured (and surfaced in `details.parsed`).
- Outputs are truncated to ~24 KB of text to keep context cheap. The full
  parsed JSON is still attached as tool result `details`.
- Durations in APM/trace queries are documented in the tool descriptions as
  **nanoseconds** so the model stops getting that wrong.

---
name: dd-debugger
description: Live Debugger - inspect runtime argument/variable values in production by placing log probes on methods. Use when asked what values a function receives, what parameters look like at runtime, or to capture live data from running services without redeploying.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,debugger,live-debugger,probes,dd-debugger
  globs: ""
  alwaysApply: "false"
---

# Datadog Live Debugger

Place log probes on running services without redeploying. Create probes with custom templates and conditions, and stream captured events in real time.

## Prerequisites

`pup` must be installed:

```bash
brew tap datadog-labs/pack && brew install pup
```

## Authentication

Authenticate via OAuth2 (recommended) or API keys:

```bash
# OAuth2 (recommended)
pup auth login

# Or use API keys
export DD_API_KEY="key" DD_APP_KEY="key" DD_SITE="datadoghq.com"
```

## Typical Workflow

1. **Find a method** using the `dd-symdb` skill (`pup symdb search --view probe-locations`)
2. **Place a probe** on it
3. **Watch** to see captured events
4. **Delete** the probe when done

```bash
# 1. Create a probe (use dd-symdb skill to find TYPE:METHOD values)
pup debugger probes create \
  --service my-service \
  --env production \
  --probe-location "com.example.MyController:handleRequest" \
  --template "handleRequest called with id={id}" \
  --ttl 1h

# 2. Stream events (waits for new events by default)
pup debugger probes watch <PROBE_ID> --timeout 60 --limit 10

# 3. Clean up
pup debugger probes delete <PROBE_ID>
```

## Probe Management

### List Probes

```bash
# All probes
pup debugger probes list

# Filter by service
pup debugger probes list --service my-service
```

### Get Probe Details

```bash
pup debugger probes get <PROBE_ID>
```

### Create a Log Probe

```bash
pup debugger probes create \
  --service my-service \
  --env staging \
  --probe-location "com.example.MyClass:myMethod"
```

**Options:**

| Flag | Description | Default |
|------|-------------|---------|
| `--service` | Service name (required) | — |
| `--env` | Environment (required) | — |
| `--probe-location` | `TYPE:METHOD` (required) | — |
| `--language` | `java`, `python`, `dotnet`, `go` | Auto-detected from symdb |
| `--template` | Log message template with `{variable}` placeholders | Auto-generated |
| `--condition` | DSL condition to filter captures | None |
| `--no-snapshot` | Disable snapshot capture | `false` |
| `--rate` | Snapshots per second | `1` |
| `--budget` | Max probe hits. Only "total" window supported (hourly/daily not yet available). | `1000` |
| `--ttl` | Probe time-to-live (e.g., `10m`, `1h`, `24h`). Probe auto-expires. | `1h` |

**Templates** use `{variable}` syntax to interpolate method arguments and locals:

```bash
# Capture specific arguments
--template "Processing order={orderId} for user={userId}"

# Use @duration for method execution time
--template "handleRequest took {@duration}ms"
```

**Conditions** filter when the probe fires:

```bash
# Only capture when status is error
--condition "status == 'error'"

# Only capture slow calls
--condition "@duration > 100"
```

```bash
# Create with custom budget and TTL
pup debugger probes create \
  --service my-service \
  --env staging \
  --probe-location "com.example.MyClass:myMethod" \
  --budget 500 --ttl 2h
```

### Delete a Probe

```bash
pup debugger probes delete <PROBE_ID>
```

## Watch Probe Events

Stream log events and status errors from a probe in real time.

```bash
pup debugger probes watch <PROBE_ID>
```

**Options:**

| Flag | Description | Default |
|------|-------------|---------|
| `--timeout` | Exit after N seconds | `120` |
| `--limit` | Exit after N log events | unlimited |
| `--from` | Start time for log query | `now` |
| `--wait` | Wait up to N seconds for the probe to become available | `0` |

**Behavior:**
- Polls for new log events and probe status errors every 1s
- Default `--from` is `now` — only shows new events going forward
- Use `--from 5m` or `--from 1h` to include recent historical events
- Probe status errors (e.g., instrumentation failures) go to stderr
- Exit code 0 if events received, 1 if timed out with no events
- Use `--wait <seconds>` to retry probe existence check (handles create → watch pipeline); default is 0 (fail immediately if not found)

### Extracting fields with jq

Probe snapshots capture method arguments, locals, and return values. Common jq patterns:

```bash
# Extract a specific argument value
pup debugger probes watch <ID> --from 5m --limit 5 \
  | jq -c '{
      arg: .attributes.attributes.debugger.snapshot.captures.return.arguments.<ARG_NAME>.value,
      ts: .attributes.timestamp
    }'

# Get the log template message
pup debugger probes watch <ID> --limit 1 \
  | jq -r .attributes.message

# List all captured argument names
pup debugger probes watch <ID> --limit 1 \
  | jq '.attributes.attributes.debugger.snapshot.captures.return.arguments | keys'
```

**Snapshot structure:**
```
.attributes.attributes.debugger.snapshot.captures.return.arguments  → method arguments
.attributes.attributes.debugger.snapshot.captures.return.locals     → local variables
.attributes.message                                                 → rendered template
.attributes.timestamp                                               → event time
.attributes.attributes.debugger.snapshot.probe.location             → source location
```

## Supported Languages

| Language | `--language` value |
|----------|-------------------|
| Java | `java` |
| Python | `python` |
| .NET | `dotnet` |

## Failure Handling

| Problem | Fix |
|---------|-----|
| "probe not found" | Use `--wait <seconds>` to retry, e.g. `pup debugger probes watch <ID> --wait 10` |
| No events appearing | Check `--from` (default is `now`); probe may need time to instrument |
| Instrumentation errors | Check stderr output from watch for status errors |
| Auth error | Run `pup auth login` or set `DD_API_KEY` + `DD_APP_KEY` + `DD_SITE` |
| Wrong method signature | Use the `dd-symdb` skill to find exact `TYPE:METHOD` values |

## References

- [Live Debugger Docs](https://docs.datadoghq.com/dynamic_instrumentation/)
- [Log Probe Templates](https://docs.datadoghq.com/dynamic_instrumentation/symdb/expressions/)

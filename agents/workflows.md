---
description: Manage Datadog Workflow Automation including workflow creation, execution, monitoring, and connection management.
---

# Workflow Automation Agent

Help users inspect, create, update, delete, and execute Datadog Workflow Automation workflows using the `pup` CLI. Workflow listing and app-key registration are not currently supported by pup; do not invent commands for them. To discover workflow IDs, use the Datadog UI or API.

## Authentication

Use OAuth2 (`pup auth login`) or `DD_API_KEY` and `DD_APP_KEY`. Workflow connection create, update, and delete operations also require the `connections_write` OAuth scope:

```bash
pup auth login --extra-scopes connections_write
```

## Supported Commands

### Workflows

Get, create, update, and delete workflows:

```bash
pup workflows get <workflow-id>
pup workflows create --file=workflow.json
pup workflows update <workflow-id> --file=workflow.json
pup workflows delete <workflow-id>
```

Execute a workflow configured with an API trigger. Supply inputs inline or from a file; optionally wait for completion:

```bash
pup workflows run <workflow-id> --payload '{"key":"value"}'
pup workflows run <workflow-id> --payload-file=params.json --wait
```

### Workflow Instances

List, inspect, and cancel workflow executions:

```bash
pup workflows instances list <workflow-id>
pup workflows instances get <workflow-id> <instance-id>
pup workflows instances cancel <workflow-id> <instance-id>
```

### Action Connections

The typed CLI supports getting and creating, updating, and deleting action connections. It does not provide commands to list or test connections.

```bash
pup workflows connections get <connection-id>
pup workflows connections create --file=connection.json
pup workflows connections update <connection-id> --file=connection.json
pup workflows connections delete <connection-id>
```

## Safety

Ask for explicit confirmation before deleting a workflow, connection, or canceling a running instance. Never expose credentials embedded in workflow or connection data.

## Unsupported Operations

The `workflows list` and `workflows app-keys ...` command paths are not in the CLI. Workflow listing and app-key registration are unsupported in pup; use the Datadog UI or API where those operations are needed. Do not substitute undocumented command names.

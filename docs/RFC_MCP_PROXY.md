# MCP Proxy for Pup — Implementation Plan

## Goal

Let pup delegate commands to the Datadog MCP server instead of calling APIs directly. A generic routing layer: you register "this pup command → this MCP tool" and pup becomes a thin CLI skin over MCP. Single-source tool logic, iterate faster.

## Design

### New module: `src/mcp/`

| File | Purpose |
|------|---------|
| `mod.rs` | Public API: `call_tool()`, `list_tools()` |
| `client.rs` | JSON-RPC 2.0 transport over HTTP (POST to MCP endpoint) |
| `registry.rs` | Static map of pup command paths → MCP tool names + arg translators |

### MCP endpoint

`{api_base_url}/api/unstable/mcp-server/mcp` — reuses pup's existing site logic.

### Auth

Reuse pup's existing bearer token / API keys. The MCP server accepts the same `Authorization: Bearer` and `DD-API-KEY`/`DD-APPLICATION-KEY` headers. No new login flow.

### How a proxied command works

1. Command dispatches in `main.rs` as normal
2. Handler checks if MCP is enabled + this command has a registry entry
3. If yes → `mcp::client::call_tool(cfg, tool_name, args)` via JSON-RPC
4. Extract text content from MCP response, print it
5. If MCP fails or is disabled → existing direct-API implementation runs

### Config

```yaml
# ~/.config/pup/config.yaml  (or PUP_MCP_ENABLED=1)
mcp_enabled: true
```

Default: off. Opt-in while we validate.

## Implementation order

### Step 1: MCP client + auth validation
- `src/mcp/client.rs` — POST JSON-RPC to MCP endpoint, reuse `apply_auth()`
- `src/mcp/mod.rs` — public interface
- `pup mcp list-tools` command to test auth works with existing creds

### Step 2: Registry + security command wiring
- `src/mcp/registry.rs` — command→tool map for the 3 security findings commands
- Modify `security.rs` handlers to try MCP when enabled, fall back on failure

### Step 3: Tests
- Unit tests for registry and client
- Manual validation with `PUP_MCP_ENABLED=1`

## Starting commands

| Pup Command | MCP Tool |
|---|---|
| `pup security findings analyze` | `analyze_security_findings` |
| `pup security findings search` | `search_security_findings` |
| `pup security findings schema` | `security_findings_schema` |

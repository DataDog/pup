# MCP Proxy

Proxies pup commands through the Datadog MCP server (`mcp.{site}`) so tool logic is single-sourced.

## How it works

```
pup mcp list-tools          # discover available MCP tools
pup mcp call <tool> <json>  # call any tool directly
```

Under the hood:

1. **OAuth** — The MCP server has its own OAuth 2.1 flow (DCR + PKCE), separate from pup's main Datadog OAuth. On first use, a browser opens for auth. Tokens are stored at `~/.config/pup/mcp_token_{site}.json`.
2. **Session** — Each invocation initializes a Streamable HTTP session (`initialize` → `notifications/initialized` → poll `tools/list` until tools appear).
3. **Tool call** — `tools/call` via JSON-RPC 2.0 with the session ID.

## Architecture

| File | Purpose |
|------|---------|
| `client.rs` | Streamable HTTP MCP client — session init, JSON-RPC transport, tool polling |
| `oauth.rs` | MCP OAuth 2.1 (DCR + PKCE + token storage + refresh) |
| `registry.rs` | Maps pup command paths → MCP tool names + argument translators |

## Auth flow

The MCP server publishes OAuth metadata at `https://mcp.{site}/.well-known/oauth-authorization-server`:

| Endpoint | URL |
|----------|-----|
| Register (DCR) | `https://mcp.{site}/api/unstable/mcp-server/register` |
| Authorize | `https://mcp.{site}/api/unstable/mcp-server/authorize` |
| Token | `https://mcp.{site}/api/unstable/mcp-server/token` |

This is the same OAuth flow Claude Code uses when you `claude mcp add --transport http`.

## Configuration

| Env var | Default | Purpose |
|---------|---------|---------|
| `PUP_MCP_ENDPOINT` | `https://mcp.{site}/api/unstable/mcp-server/mcp?toolsets=core,security` | Override the MCP endpoint URL |
| `PUP_MCP_TOOLSETS` | `core,security` | Comma-separated toolsets to request |

## Command registry

The registry maps pup commands to MCP tools. Currently registered:

| Pup command | MCP tool |
|-------------|----------|
| `security findings analyze` | `analyze_security_findings` |
| `security findings search` | `search_security_findings` |
| `security findings schema` | `security_findings_schema` |

To add a new mapping, add an entry to `registry.rs`.

## TODO

- [ ] Wire security findings commands to delegate through MCP when enabled
- [ ] Session reuse across calls (avoid re-init per invocation)
- [ ] Token refresh on 401 retry
- [ ] Extend registry to other command domains (logs, monitors, etc.)

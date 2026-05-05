# MCP Proxy

Proxies pup commands through the Datadog MCP server (`mcp.{site}`) so tool logic is single-sourced. Instead of pup maintaining its own API call logic, prompts, and schemas for each command, it delegates to the MCP server — the same server that Claude Code and other AI tools use.

## Quick start

```bash
# List all available MCP tools (opens browser for auth on first run)
pup mcp list-tools

# Call any MCP tool directly
pup mcp call security_findings_schema '{"include_description": false, "telemetry": {"intent": "test"}}'

# Call with different toolsets
PUP_MCP_TOOLSETS=all pup mcp list-tools
```

## Architecture

| File | Purpose |
|------|---------|
| `client.rs` | Streamable HTTP MCP client — session init, JSON-RPC transport, session caching |
| `oauth.rs` | MCP OAuth 2.1 (DCR + PKCE + token storage + refresh) |
| `registry.rs` | Maps pup command paths to MCP tool names + argument translators |

## How a call works

1. User runs `pup mcp call <tool> <json>`
2. Client checks for a cached session (`~/.config/pup/mcp_session_{site}.json`)
3. If cached and valid, reuse it. If not, establish a fresh session
4. Send `tools/call` via JSON-RPC 2.0 with the session ID
5. Extract text content from the MCP response and print it

## Authentication

The MCP server has its own OAuth 2.1 flow, separate from pup's standard Datadog OAuth. This is the same flow Claude Code uses when you `claude mcp add --transport http`. On first use, pup registers a client via DCR, opens the browser for auth, exchanges the code for tokens via PKCE, and stores everything locally. Subsequent calls reuse the stored token, refreshing automatically when expired.

**Token storage:**

| File | Contents |
|------|----------|
| `~/.config/pup/mcp_token_{site}.json` | OAuth access + refresh token (0600 permissions) |
| `~/.config/pup/mcp_client_{site}.json` | DCR client credentials (client_id) |

## Session management

Sessions are cached at `~/.config/pup/mcp_session_{site}.json` with a 10-minute TTL. On reuse, a `tools/list` ping verifies the session is still alive. If rejected, the cache is cleared and a fresh session is established.

## Configuration

| Env var | Default | Purpose |
|---------|---------|---------|
| `PUP_MCP_ENDPOINT` | `https://mcp.{site}/api/unstable/mcp-server/mcp?toolsets=core,security` | Override the full MCP endpoint URL |
| `PUP_MCP_TOOLSETS` | `core,security` | Comma-separated toolsets to request |

## Command registry

The registry (`registry.rs`) maps pup command paths to MCP tool names. Currently registered:

| Pup command | MCP tool |
|-------------|----------|
| `security findings analyze` | `analyze_security_findings` |
| `security findings search` | `search_security_findings` |
| `security findings schema` | `security_findings_schema` |

To add a new mapping, add an entry to the `match` in `registry::lookup()`.

# OAuth2 Authentication Guide

## Overview

Pup supports OAuth2 authentication with PKCE (Proof Key for Code Exchange) for secure, browser-based authentication with Datadog. This is the recommended authentication method as it provides better security and granular access control compared to API keys.

## Features

### 🔒 Security Features

- **PKCE Protection (S256)**: Prevents authorization code interception attacks
- **Dynamic Client Registration (DCR)**: Each CLI installation gets unique credentials
- **CSRF Protection**: State parameter validation prevents cross-site request forgery
- **Secure Token Storage**: Tokens stored in the OS keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service); falls back to a JSON file under `~/.config/pup/` with `0600` permissions when no keychain is available
- **Automatic Token Refresh**: Seamless token refresh before expiration

### 🎯 Key Benefits

1. **No Hardcoded Credentials**: No need to manage long-lived API keys
2. **Granular Revocation**: Revoke access for one installation without affecting others
3. **Scope-Based Permissions**: Request only necessary OAuth scopes
4. **User Context**: Actions performed as the authenticated user
5. **Better Audit Trail**: OAuth tokens provide clearer audit logs

## Quick Start

### 1. Login

```bash
pup auth login                          # default site (datadoghq.com), default org
pup auth login --site datadoghq.eu      # a different Datadog site
pup auth login --org staging-child      # a named session for a second org
```

This will:
1. Register a new OAuth client with Datadog (if first time)
2. Generate PKCE challenge and state parameter
3. Open your browser to Datadog's authorization page
4. Start a local callback server on `http://127.0.0.1:<random-port>/callback`
5. Wait for you to approve the requested scopes
6. Exchange the authorization code for access/refresh tokens
7. Store tokens securely (OS keychain, or JSON file under `~/.config/pup/` with `0600` permissions when no keychain is available)

See [Multi-Site Support](#multi-site-support) and
[Multi-Org Support](#multi-org-support) for managing multiple sessions.

### 2. Check Status

```bash
pup auth status
```

Shows your current authentication status including:
- Whether you're authenticated
- Token expiration time
- Site you're authenticated with

### 3. Refresh Token

```bash
pup auth refresh
```

Manually refresh your access token using the refresh token. This happens automatically when making API calls, but you can force it with this command.

### 4. Logout

```bash
pup auth logout                                # default session
pup auth logout --org staging-child            # one named session, leaves others intact
```

**Side effect on sibling sessions:** logging out the default (unnamed)
session for a site also deletes that site's shared DCR client
credentials. Any named-org sessions on the same site will still hold
valid access tokens, but their next automatic refresh will fail (no
client credentials). The shared credentials are re-registered
automatically by the next `pup auth login` on that site (any org,
named or default), and the sibling sessions can refresh again from
that point on. Logging out a named session (`--org <name>`) does not
touch the shared client credentials.

See [Multi-Org Support](#multi-org-support) for managing multiple named
sessions side-by-side.

## OAuth2 Flow Details

### Step-by-Step Process

```
┌─────────┐                                  ┌──────────┐
│  User   │                                  │ Datadog  │
│   CLI   │                                  │  OAuth   │
└────┬────┘                                  └────┬─────┘
     │                                            │
     │ 1. Check for existing client credentials  │
     │─────────────────────────────────────────> │
     │                                            │
     │ 2. Register new client (if needed - DCR)  │
     │─────────────────────────────────────────> │
     │ <────────────────────────────────────────│
     │        client_id, client_secret           │
     │                                            │
     │ 3. Generate PKCE challenge & state        │
     │─────────────────┐                         │
     │                 │                         │
     │ <───────────────┘                         │
     │                                            │
     │ 4. Start local callback server            │
     │─────────────────┐                         │
     │                 │                         │
     │ <───────────────┘                         │
     │                                            │
     │ 5. Open browser with authorization URL    │
     │─────────────────────────────────────────> │
     │                                            │
     │ 6. User approves scopes                   │
     │                                            │
     │ 7. Redirect to callback with auth code    │
     │ <────────────────────────────────────────│
     │                                            │
     │ 8. Exchange code for tokens (with PKCE)   │
     │─────────────────────────────────────────> │
     │ <────────────────────────────────────────│
     │    access_token, refresh_token            │
     │                                            │
     │ 9. Store tokens securely                  │
     │─────────────────┐                         │
     │                 │                         │
     │ <───────────────┘                         │
     │                                            │
```

### Component Details

#### Dynamic Client Registration (DCR)

Based on RFC 7591, each CLI installation registers as a unique OAuth client:

```json
{
  "client_name": "Datadog Pup CLI",
  "redirect_uris": ["http://127.0.0.1:<port>/callback"],
  "grant_types": ["authorization_code", "refresh_token"],
  "response_types": ["code"],
  "token_endpoint_auth_method": "client_secret_post"
}
```

Response includes:
- `client_id`: Unique client identifier
- `client_secret`: Client secret for token exchange
- Stored in `~/.config/pup/client_<site>.json`

#### PKCE (RFC 7636)

Proof Key for Code Exchange prevents authorization code interception:

1. **Generate Code Verifier**: 128-character random string
2. **Generate Code Challenge**: `BASE64URL(SHA256(code_verifier))`
3. **Include in Authorization**: Send `code_challenge` and `code_challenge_method=S256`
4. **Include in Token Exchange**: Send `code_verifier` to prove possession

#### Token Storage

By default, OAuth tokens and DCR client credentials are stored in your
platform's secure store: macOS Keychain (via Apple's Security framework),
Linux Secret Service (via the `keyring` crate), or Windows Credential
Manager (via the `keyring` crate). When the secure store is unavailable,
pup falls back to JSON files under `~/.config/pup/` with `0600` permissions.

Each per-site entry is read at most once per command (reads are memoized for the
process), so the OS keychain prompts at most once per site even when a command
loads credentials several times.

The storage backend can be overridden via `DD_TOKEN_STORAGE` (env var, takes
precedence) or `token_storage` in `~/.config/pup/config.yaml`:

| Value | macOS | Linux | Windows |
|---|---|---|---|
| `keychain` (default) | Keychain (Security framework). macOS may prompt once per stable app identity (signed Homebrew release); unsigned/dev builds may prompt more often. | Secret Service (GNOME Keyring / KWallet); falls back to `file` if unavailable | WinCred (chunked) |
| `file` | Plaintext JSON: `~/.config/pup/tokens_<site>.json`, `client_<site>.json`, `0600` perms | Same | Same |

**Upgrading from a previous version:** to ensure your stored token uses the
current default backend, run `pup auth logout && pup auth login`.

In secure-store mode each site has one per-site entry holding both
tokens and client credentials (on Windows, sharded across multiple
WinCred records to stay within WinCred's per-record size limit). In
file mode tokens and client credentials are kept in separate files
(`tokens_<site>.json` and `client_<site>.json`). In either mode, when
a site has multiple named-org sessions (see
[Multi-Org Support](#multi-org-support)) all of their tokens live
inside the per-site tokens entry, keyed internally by org name; there
is no separate `tokens_<site>_<org>.json` file.

The token payload is:

```json
{
  "access_token": "<token>",
  "refresh_token": "<token>",
  "token_type": "Bearer",
  "expires_in": 3600,
  "expires_at": "2024-02-04T12:00:00Z",
  "scope": "dashboards_read dashboards_write ..."
}
```

## OAuth Scopes

Pup requests OAuth scopes covering the read/write surface of supported
commands. The list below is illustrative — see
[`src/auth/`](../src/auth/) for the canonical, code-driven scope set:

### Dashboards
- `dashboards_read` - Read dashboards
- `dashboards_write` - Create/update/delete dashboards

### Monitors
- `monitors_read` - Read monitors
- `monitors_write` - Create/update monitors
- `monitors_downtime` - Manage downtimes

### APM/Traces
- `apm_read` - Read APM data and traces

### SLOs
- `slos_read` - Read SLOs
- `slos_write` - Create/update SLOs
- `slos_corrections` - Manage SLO corrections

### Incidents
- `incident_read` - Read incidents
- `incident_write` - Create/update incidents

### Synthetics
- `synthetics_read` - Read synthetic tests
- `synthetics_write` - Create/update/delete synthetic tests

### Security
- `security_monitoring_signals_read` - Read security signals
- `security_monitoring_rules_read` - Read security rules
- `security_monitoring_findings_read` - Read security findings

### RUM
- `rum_apps_read` - Read RUM applications
- `rum_apps_write` - Manage RUM applications

### Infrastructure
- `hosts_read` - Read host information

### Users
- `user_access_read` - Read user access information
- `user_self_profile_read` - Read own user profile

### Cases
- `cases_read` - Read cases
- `cases_write` - Create/update cases

### Events
- `events_read` - Read events

### Logs
- `logs_read_data` - Read log data
- `logs_read_index_data` - Read log index data

### Metrics
- `metrics_read` - Read metrics
- `timeseries_query` - Query timeseries data

### Usage
- `usage_read` - Read usage data

## Token Management

### Automatic Refresh

Tokens are automatically refreshed when:
- Making an API call with an expired token
- Token is within 5 minutes of expiration

The refresh happens transparently in the background.

### Manual Refresh

Force a token refresh:

```bash
pup auth refresh
```

### Token Expiration

Access tokens typically expire after 1 hour. The CLI:
1. Checks expiration before each API call
2. Automatically refreshes if needed
3. Uses the refresh token (valid for 30 days)
4. Re-prompts for login if refresh token expires

## Multi-Site Support

Pup supports all Datadog sites with separate credentials per site:

```bash
# US1 (default)
pup auth login --site datadoghq.com

# EU1
pup auth login --site datadoghq.eu

# US3
pup auth login --site us3.datadoghq.com

# US5
pup auth login --site us5.datadoghq.com

# AP1
pup auth login --site ap1.datadoghq.com

# AP2
pup auth login --site ap2.datadoghq.com

# Gov
pup auth login --site ddog-gov.com

# DD_SITE env var also works on any of the above.
DD_SITE=datadoghq.eu pup auth login
```

Each site maintains separate state:
- Client credentials, shared across orgs on the same site.
- Access/refresh tokens, in a single per-site entry keyed internally by org.

See [Token Storage](#token-storage) for the secure-store-vs-file layout.

## Multi-Org Support

Pup supports multiple Datadog orgs side-by-side via *named sessions*. Each
named session is a `(site, org)` pair, and `--org <name>` (or
`DD_ORG=<name>`) selects which session a command runs against. The flag is
global and works on every subcommand, not just `auth`.

**Recommended pattern if you work with more than one org:** give every
session an explicit `--org <name>` rather than mixing the default
(unnamed) session with named ones. This way `--org <name>` always
appears in your commands and there's no ambiguity about which org a
query targeted. Sharing one default slot across multiple orgs is easy
to get wrong (you re-log into a different org without realizing it),
and as noted in [Logout](#4-logout), logging out the default also
removes the shared DCR client credentials for that site.

### Logging into multiple orgs

```bash
# Two child orgs on the default site (US1).
pup auth login --org prod-child
pup auth login --org staging-child

# A child org on a different site. --site is only needed at login;
# subsequent commands recall it from the session registry.
pup auth login --site ap2.datadoghq.com --org ap2-prod

# A SAML/SSO org with a vanity login page (e.g. acme.datadoghq.com).
# Pass the full host via --site so the OAuth consent page routes to the
# correct tenant. The literal host is used verbatim; --subdomain has been
# removed.
pup auth login --org acme-prod --site acme.datadoghq.com

# A non-Datadog host (an API gateway or proxy) is used verbatim too, but
# because it is not a Datadog-owned domain pup confirms before sending
# credentials. Answer the prompt, pass --trust-site, set PUP_TRUST_SITE=1, or
# add the host to trusted_sites in the config file. See docs/TROUBLESHOOTING.md.
pup auth login --site mygateway.example.com --trust-site

# Pre-target a specific org by UUID (sent as `dd_oid`). Skips the org
# switcher when the existing browser session matches, and pre-routes
# SAML/SSO routing for first-time logins. The UUID is persisted with the
# session and re-emitted on subsequent `pup auth login` invocations for
# the same named session.
pup auth login --org acme-prod --org-uuid 11111111-2222-3333-4444-555555555555
```

### Using a named session

```bash
# Site is recalled from sessions.json; no DD_SITE / --site needed.
pup monitors list --org prod-child
pup logs query --org ap2-prod --query "service:web-store" --limit 10

# DD_ORG env var is equivalent to --org.
DD_ORG=prod-child pup metrics query --query "avg:system.cpu.user{*}"
```

### Inspecting and managing sessions

```bash
# List every stored session (site, org, org_uuid, scopes, expiry, status).
pup auth list

# Refresh a specific named session.
pup auth refresh --org prod-child

# Log out of a single named session (other sessions are untouched).
pup auth logout --org staging-child

# Log out of the default (unnamed) session for the current site.
pup auth logout
```

### Site selection rules

When pup resolves a site for a non-auth command:

1. `DD_SITE` env var (or `site:` in `~/.config/pup/config.yaml`), if set.
2. The site recorded in `~/.config/pup/sessions.json` for the named
   `--org` / `DD_ORG`, when the lookup is unambiguous.
3. Default: `datadoghq.com`.

`pup auth login` and `pup auth status` additionally accept `--site`,
which wins over the above for those two commands. No other subcommand
accepts `--site`.

If multiple sessions share the same org name on different sites, step 2
is skipped (ambiguous) and pup warns to stderr; pass `DD_SITE` to
disambiguate. An unnamed (default) session can't be selected by `--org`
at all -- it has no name to look up.

### Session registry

Named-session metadata lives in `~/.config/pup/sessions.json`. The file
records the `site`, `org`, and (when supplied at login) `org_uuid` for
each session. No tokens or secrets are stored here. The registry is what
enables `--org <name>` to recall the right site on a non-auth command.

## Troubleshooting

### Browser Doesn't Open

If the browser doesn't open automatically:

```
⚠️  Could not open browser automatically
Please open this URL manually: https://datadoghq.com/oauth2/v1/authorize?...
```

Copy and paste the URL into your browser manually.

### Callback Timeout

If you don't complete authorization within 5 minutes:

```
Error: timeout waiting for OAuth callback
```

Run `pup auth login` again to restart the flow.

### Token Expired

If your access token expires and refresh fails:

```
⚠️  Token expired
Run 'pup auth refresh' to refresh or 'pup auth login' to re-authenticate
```

Try `pup auth refresh` first. If that fails, run `pup auth login` to start a new session.

### Port Already in Use

The callback server scans `[8000, 8080, 8888, 9000]` and binds the first one that's free. If all four are busy, login fails with the list above.

### Pinning the Callback Port (SSH workflows)

When `pup auth login` runs inside an SSH-tunneled remote workspace, the operator typically forwards localhost ports to the laptop browser. To avoid forwarding all four candidate ports, pin one of the four DCR-registered ports with `--callback-port` (or `PUP_OAUTH_CALLBACK_PORT`):

```bash
ssh -L 8000:127.0.0.1:8000 workspace-host
PUP_OAUTH_CALLBACK_PORT=8000 pup auth login --org acme
# or per-invocation:
pup auth login --org acme --callback-port 8000
```

The pinned value must be one of `[8000, 8080, 8888, 9000]` — those are the redirect URIs registered with the OAuth server during DCR, so any other port would be rejected at the authorize step regardless. Precedence is `--callback-port` > `PUP_OAUTH_CALLBACK_PORT` > the auto-scan default. When pinned, login fails fast if the port is already in use — there is no fallback, since a silent fallback would orphan the OAuth callback when the browser hits a port that isn't forwarded.

### Invalid State Parameter

If you see a CSRF protection error:

```
Error: state parameter mismatch (CSRF protection)
```

This indicates a potential security issue. Run `pup auth login` again to start a fresh flow.

## Security Considerations

### Client Credentials

- Each installation gets unique `client_id` and `client_secret`
- Stored in `~/.config/pup/client_<site>.json` with `0600` permissions
- Never committed to version control
- Can be revoked individually without affecting other installations

### Tokens

- Access tokens are short-lived (1 hour)
- Refresh tokens are longer-lived (30 days)
- Stored with restricted file permissions
- Never logged or printed to console
- Automatically refreshed before expiration

### PKCE

- Prevents authorization code interception attacks
- Uses S256 (SHA256) code challenge method
- Code verifier is cryptographically random (128 characters)
- Never transmitted in the authorization request

### CSRF Protection

- State parameter is cryptographically random (32 characters)
- Validated on callback to prevent cross-site request forgery
- New state generated for each authorization flow

## Comparison with API Keys

| Feature | OAuth2 | API Keys |
|---------|--------|----------|
| **Setup** | Browser login | Copy/paste keys |
| **Security** | Short-lived tokens | Long-lived keys |
| **Revocation** | Per-installation | Organization-wide |
| **Scopes** | Granular | All or nothing |
| **Audit Trail** | User-specific | Key-specific |
| **Rotation** | Automatic (refresh) | Manual |
| **PKCE Protection** | Yes | N/A |
| **Token Storage** | Secure local files | Environment variables |

## Implementation Details

### File Structure

```
~/.config/pup/
├── client_<site>.json      # DCR client credentials, one per site (shared across orgs)
├── tokens_<site>.json      # OAuth2 tokens, one per site (keyed internally by org)
└── sessions.json           # Named-session registry (site, org, org_uuid; no secrets)
```

On platforms using the secure-store backend (macOS, plus Linux/Windows
when a keychain is available), both `client_<site>.json` and
`tokens_<site>.json` are absent: their contents live together in a
per-site secure-store entry. On Windows, this entry is sharded into
multiple WinCred records (one count record plus one or more chunk
records) to stay within WinCred's per-record size limit.
`sessions.json` is always file-based regardless of backend.

### Code Structure

```
src/auth/
├── mod.rs         # Auth module entry point
├── types.rs       # Shared auth types
├── dcr.rs         # Dynamic Client Registration
├── pkce.rs        # PKCE code verifier/challenge generation
├── storage.rs     # Token and credential storage (keychain + JSON file fallback)
└── callback.rs    # Local callback server
```

## References

- **RFC 6749**: OAuth 2.0 Authorization Framework
- **RFC 7591**: OAuth 2.0 Dynamic Client Registration Protocol
- **RFC 7636**: Proof Key for Code Exchange (PKCE)
- **PR #84**: Original TypeScript implementation reference

## Future Enhancements

- [ ] Automatic token refresh background service
- [ ] Support for custom OAuth scopes
- [ ] OAuth2 device flow for headless environments

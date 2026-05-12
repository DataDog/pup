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
pup auth login
```

This will:
1. Register a new OAuth client with Datadog (if first time)
2. Generate PKCE challenge and state parameter
3. Open your browser to Datadog's authorization page
4. Start a local callback server on `http://127.0.0.1:<random-port>/callback`
5. Wait for you to approve the requested scopes
6. Exchange the authorization code for access/refresh tokens
7. Store tokens securely (OS keychain, or JSON file under `~/.config/pup/` with `0600` permissions when no keychain is available)

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
pup auth logout
```

Clears all stored tokens and client credentials for the current site.

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

Tokens are stored in the OS keychain by default (macOS Keychain, Windows
Credential Manager, Linux Secret Service via the `keyring` crate). When a
keychain is unavailable, pup falls back to a JSON file at
`~/.config/pup/tokens_<site>.json` with `0600` permissions. Set
`DD_TOKEN_STORAGE=file` to force file storage.

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
export DD_SITE="datadoghq.com"
pup auth login

# EU1
export DD_SITE="datadoghq.eu"
pup auth login

# US3
export DD_SITE="us3.datadoghq.com"
pup auth login

# US5
export DD_SITE="us5.datadoghq.com"
pup auth login

# AP1
export DD_SITE="ap1.datadoghq.com"
pup auth login
```

Each site maintains separate:
- Client credentials (`client_<site>.json`)
- Access/refresh tokens (`tokens_<site>.json`)

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

## Comparison with PAT/SAT and API Keys

Pup supports three auth methods, in order of preference:

| Feature | OAuth2 (preferred) | PAT / SAT | API + App Key |
|---------|--------------------|------------|----------------|
| **Setup** | Browser login | Create in Datadog UI | Copy/paste keys |
| **Identity** | Interactive user (per installation) | PAT: interactive user. SAT: service account. | Org-level key |
| **Security** | Short-lived tokens, auto-refresh | Time-limited (24h to 1yr) | Long-lived |
| **Revocation** | Per-installation | Per-token | Organization-wide |
| **Scopes** | Granular, OAuth scopes | Granular, Datadog permissions | All or nothing |
| **Audit Trail** | User-specific | User- or service-specific | Key-specific |
| **Rotation** | Automatic (refresh) | Manual (recreate) | Manual |
| **PKCE Protection** | Yes | N/A | N/A |
| **API coverage in pup** | Most endpoints (expanding to 100%) | Full | Full |
| **Token Storage** | Secure local files | Environment variable | Environment variables |

OAuth2 is the easiest to set up and the safest (short-lived, no local credential management). PAT or SAT is the recommended fallback whenever OAuth is not an option for a particular endpoint or workflow -- use PAT for interactive workflows and SAT for service-account use cases like CI. API + App Key is supported for backward compatibility but mildly discouraged in favor of PAT/SAT, which provide equivalent coverage with scopes and expiration. PATs and SATs are wire-identical; pup accepts `DD_PAT` and `DD_SAT` as interchangeable env vars (the distinction is only surfaced in `pup auth status`).

## Implementation Details

### File Structure

```
~/.config/pup/
├── client_datadoghq_com.json      # DCR client credentials
└── tokens_datadoghq_com.json      # OAuth2 tokens
```

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

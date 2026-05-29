# Architecture & Design Decisions

Technical architecture and design rationale for Pup CLI.

## Technology Choices

### Why Rust?

**Benefits:**
1. **Performance** - Compiled binary with fast startup
2. **Cross-platform** - Single binary for macOS, Linux, Windows
3. **Memory safety** - No garbage collector, zero-cost abstractions
4. **Concurrency** - Async/await with tokio runtime
5. **Strong type system** - Catch errors at compile time with rich enums

### Static Linking for Linux

Linux release binaries are statically linked using musl libc:
- **No glibc dependency** - Works across all Linux distributions, including older systems like Ubuntu 22.04 (Jammy) with glibc 2.35
- **Fully self-contained** - No library compatibility issues
- **Portable** - Same binary works on Debian, Ubuntu, Alpine, CentOS, etc.

Build targets:
- `x86_64-unknown-linux-musl` - Intel/AMD 64-bit processors
- `aarch64-unknown-linux-musl` - ARM 64-bit processors

**Tradeoffs:**
- Steeper learning curve than Go
- Longer compile times
- Smaller ecosystem for some domains

### Why Clap?

**Benefits:**
1. **Industry standard** - Most popular Rust CLI framework
2. **Rich features** - Subcommands, flags, derive macros, help generation
3. **Shell completion** - Built-in bash/zsh/fish completion via clap_complete
4. **Type safety** - Derive-based argument parsing
5. **Documentation** - Excellent docs and examples

### Why Serde + serde_norway?

**Benefits:**
1. **Configuration management** - Handles YAML config files
2. **Serialization** - Unified serialize/deserialize across JSON, YAML
3. **Derive macros** - Zero-boilerplate struct serialization
4. **Performance** - Fast parsing with compile-time code generation

### Why OAuth2 over API Keys?

**OAuth2 advantages:**
1. **Security** - Short-lived tokens (1 hour) vs. long-lived keys
2. **Granular control** - Per-installation revocation
3. **Scope-based** - Request only necessary permissions
4. **User context** - Actions as authenticated user
5. **Auditable** - Better audit trail with user identity

**API Keys still supported:**
- Legacy compatibility
- Simpler for programmatic access
- No browser requirement

## Project Structure

```
pup/
├── src/                    # Rust source code
│   ├── main.rs            # Entry point + clap command registration
│   ├── client.rs          # Datadog API client wrapper
│   ├── config.rs          # Configuration management
│   ├── formatter.rs       # Output formatting (JSON, YAML, table)
│   ├── util.rs            # Time parsing, validation
│   ├── useragent.rs       # User agent + AI agent detection
│   ├── version.rs         # Version information
│   ├── auth/              # Authentication logic
│   │   ├── mod.rs         # Auth module
│   │   ├── pkce.rs        # PKCE code verifier/challenge generation
│   │   ├── dcr.rs         # Dynamic Client Registration
│   │   ├── storage.rs     # Token storage (keychain + JSON file fallback)
│   │   ├── callback.rs    # OAuth callback server
│   │   └── types.rs       # Shared auth types
│   └── commands/          # Command implementations
│       ├── mod.rs         # Command registration
│       ├── metrics.rs     # Metrics domain
│       ├── monitors.rs    # Monitors domain
│       └── ...            # one module per command group
├── Cargo.toml             # Dependencies and metadata
├── tests/                 # Integration tests
│   ├── compare/           # Output comparison tests
│   └── ...
└── docs/                  # Documentation
```

**Design principles:**
- `src/commands/` contains clap command definitions (thin layer)
- `src/client.rs`, `src/auth/`, etc. contain business logic (testable)
- `#[cfg(test)]` modules co-located with source for unit tests

## Authentication Architecture

### Token Storage Strategy

**Priority order:**
1. **OS Keychain** (primary) - Most secure
   - macOS: Keychain (via `keyring` crate with `apple-native` feature)
   - Windows: Credential Manager
   - Linux: Secret Service / Keyring (via `linux-native` feature)
2. **JSON file** (fallback) - When keychain unavailable
   - Location: `~/.config/pup/tokens_<site>.json`
   - Permissions: `0600` (owner read/write only) on Unix
   - The file is plain JSON; do not enable file fallback on shared hosts

### OAuth2 Flow

Based on [RFC 6749](https://tools.ietf.org/html/rfc6749) and [RFC 7636](https://tools.ietf.org/html/rfc7636) (PKCE):

```
1. DCR Registration -> client_id, client_secret
2. PKCE Generation -> verifier, challenge
3. Authorization URL -> user approval
4. Callback Server -> receive code
5. Token Exchange -> access + refresh tokens
6. Secure Storage -> keychain (or `0600` JSON file fallback)
7. Auto Refresh -> before expiration
```

**Why PKCE?**
- Prevents authorization code interception
- Required for public clients (CLI)
- S256 method more secure than "plain"

### Token Refresh Strategy

**Automatic refresh triggers:**
- Token expires within 5 minutes
- API call with expired token
- Manual `pup auth refresh`

**Refresh flow:**
1. Check token expiration
2. Use refresh_token to get new access_token
3. Update stored tokens
4. Retry original API call

**Fallback:**
- If refresh fails, prompt user to re-authenticate

## User Agent & Agent Mode

Custom user agent identifies pup CLI and detects AI coding assistants:

**Format:**
```
pup/v0.1.0 (rust; os darwin; arch arm64)                        # Without agent
pup/v0.1.0 (rust; os darwin; arch arm64; ai-agent claude-code)  # With agent
```

**AI Agent Detection** (`src/useragent.rs`):

Table-driven registry detecting AI coding agents via environment variables. First match wins:
- Claude Code (`CLAUDECODE`, `CLAUDE_CODE`), Cursor (`CURSOR_AGENT`), Codex (`CODEX`, `OPENAI_CODEX`), OpenCode (`OPENCODE`), Aider (`AIDER`), Cline (`CLINE`), Windsurf (`WINDSURF_AGENT`), GitHub Copilot (`GITHUB_COPILOT`), Amazon Q (`AMAZON_Q`, `AWS_Q_DEVELOPER`), Gemini Code Assist (`GEMINI_CODE_ASSIST`), Sourcegraph Cody (`SRC_CODY`), pi.dev (`PI_CODING_AGENT`), Generic Agent (`AGENT`)
- Manual override: `FORCE_AGENT_MODE=1` or `--agent` flag

**Agent Mode Behavior** (when detected):
- `--help` returns structured JSON schema instead of text
- Confirmation prompts auto-approved (prevents stdin hangs)
- API responses wrapped in metadata envelope (count, truncation, warnings)
- Errors returned as structured JSON with suggestions

See [LLM_GUIDE.md](LLM_GUIDE.md) for the complete agent guide.

## API Client Wrapper

### Design Pattern

Thin wrapper around `datadog-api-client` Rust crate:

```rust
pub struct Client {
    config: datadog_api_client::configuration::Configuration,
}

impl Client {
    pub async fn query_metrics(&self, query: &str) -> Result<Response> {
        let api = datadog_api_client::apis::MetricsApi::new(&self.config);
        let resp = api.query_metrics(query).await
            .context("failed to query metrics")?;
        Ok(resp)
    }
}
```

**Benefits:**
- Centralized error handling
- Authentication injection
- Consistent retry logic
- Easy mocking for tests

### Error Handling

**Error wrapping pattern (using anyhow):**
```rust
use anyhow::{Context, Result};

fn query_metrics(query: &str) -> Result<Response> {
    let resp = client.get(url)
        .send()
        .context("failed to query metrics")?;
    Ok(resp)
}
```

**Never expose:**
- API keys in error messages
- OAuth tokens in logs
- User credentials in output

## Command Structure

### Patterns

**Simple commands:**
```
metrics
  |-- query      # pup metrics query
  |-- list       # pup metrics list
  +-- get        # pup metrics get
```

**Nested commands:**
```
rum
  |-- apps
  |   |-- list   # pup rum apps list
  |   +-- get    # pup rum apps get
  |-- metrics
  |   +-- get    # pup rum metrics get
  +-- sessions
      +-- search # pup rum sessions search
```

### Flag Consistency

Global flags available on all commands:
- `--config` - Config file path
- `--site` - Datadog site
- `--output` - Output format (json, yaml, table)
- `--verbose` - Enable debug logging
- `--yes` - Skip confirmations

Domain-specific flags:
- `--from`, `--to` - Time ranges (metrics, logs, traces)
- `--query` - Search query (logs, metrics, events)
- `--tag` - Tag filter (monitors, hosts)
- `--limit` - Result limit

## Output Formatting

### Three Formats

**JSON (default):**
- Machine-readable
- Preserves all fields
- No truncation

**YAML:**
- Human-readable
- Preserves structure
- Good for config files

**Table:**
- Compact display (via `comfy-table` crate)
- Truncates long values
- Good for terminals

## Configuration Management

### Precedence Order

1. **Command-line flags** (highest priority)
2. **Environment variables** (`DD_*`, `PUP_*`)
3. **Config file** (see paths below)
4. **Default values** (lowest priority)

### Config File

Platform-specific locations (checked in this order):
- **Linux**: `~/.config/pup/config.yaml` (or `$XDG_CONFIG_HOME/pup/config.yaml`)
- **macOS**: `~/Library/Application Support/pup/config.yaml`, falling back to `~/.config/pup/config.yaml`

Override with `PUP_CONFIG_DIR=/your/path` (e.g. for testing).

```yaml
# Authentication
site: datadoghq.com

# Output
output: json
verbose: false

# Safety
read_only: false

# Defaults
default_from: 1h
default_to: now
```

## Performance Considerations

### Async Runtime

Pup uses tokio for async I/O:

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // All API calls are async
    let monitors = client.list_monitors().await?;
    Ok(())
}
```

**Rate limiting:**
- Respect Datadog API limits (depends on plan)
- Implement exponential backoff for retries
- Use connection pooling (reqwest default)

### Memory Management

Rust's ownership system ensures:
- No garbage collection pauses
- Predictable memory usage
- Zero-cost abstractions for iterators and streaming

## Security Architecture

### Threat Model

**Protected against:**
1. Token theft (OS keychain primary; file fallback restricted to `0600`)
2. Code interception (PKCE)
3. CSRF attacks (state parameter)
4. Man-in-the-middle (HTTPS only)
5. Credential exposure (never log tokens)

**Not protected against:**
- Compromised OS (keychain access)
- Malicious CLI binary (user must trust source)
- Datadog account compromise (outside scope)

### Security Best Practices

1. **Never commit secrets** - Gitignore tokens, keys
2. **Restrict at-rest tokens** - Keychain primary; file fallback is owner-only (`0600`)
3. **Validate inputs** - Prevent injection attacks
4. **Use HTTPS** - All API calls over TLS
5. **Minimal scopes** - Request only needed permissions

## References

- [Clap Documentation](https://docs.rs/clap)
- [Tokio Documentation](https://tokio.rs)
- [Serde Documentation](https://serde.rs)
- [Datadog API Client Rust](https://crates.io/crates/datadog-api-client)

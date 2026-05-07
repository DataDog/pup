# Contributing Guide

Guidelines for contributing to Pup.

## Getting Started

Pup uses the **fork and pull** model: contributors push changes to their
personal fork and open pull requests to bring those changes into this
repository. Direct pushes to `DataDog/pup` are not permitted.

### 1. Fork and clone

```bash
# Fork via GitHub UI, then clone your fork
git clone https://github.com/<your-username>/pup.git
cd pup

# Add upstream remote and disable accidental pushes to it
git remote add upstream https://github.com/DataDog/pup.git
git remote set-url --push upstream no_push
```

### 2. Build and verify

```bash
cargo build
cargo test
cargo run -- <command>
```

### 3. (Optional) Speed up local builds with sccache + S3

[sccache](https://github.com/mozilla/sccache) can cache `rustc` outputs to an
S3 bucket and is shared across branches, worktrees, and clean checkouts. This
is opt-in; it is not wired into CI.

**Install:**

```bash
# macOS
brew install sccache

# cargo (any platform)
cargo install sccache --locked
```

**Configure** (edit the placeholders, then add to your shell rc):

```bash
# --- AWS credentials ---
# Provide via any method sccache/AWS SDK supports: `aws sso login`, a profile
# in ~/.aws/credentials, or static keys. Example with a profile:
export AWS_PROFILE=REPLACE_ME
# Or static keys (not recommended):
# export AWS_ACCESS_KEY_ID=...
# export AWS_SECRET_ACCESS_KEY=...

# --- sccache S3 backend ---
export SCCACHE_BUCKET=REPLACE_ME-bucket-name
export SCCACHE_REGION=REPLACE_ME-aws-region   # e.g. us-east-1
export SCCACHE_S3_KEY_PREFIX=pup               # keeps this repo's entries scoped
# export SCCACHE_S3_SERVER_SIDE_ENCRYPTION=true  # if the bucket requires SSE

# --- Hook sccache into cargo ---
export RUSTC_WRAPPER=sccache
```

**Verify:**

```bash
sccache --start-server
cargo clean && cargo build     # first build populates the cache
cargo clean && cargo build     # second build should hit the cache
sccache --show-stats           # inspect hit/miss counters
```

**Notes:**
- The bucket must exist and the caller's IAM principal needs `s3:GetObject`,
  `s3:PutObject`, and `s3:ListBucket` on it.
- Do **not** set `RUSTC_WRAPPER` in `.cargo/config.toml` — keep this opt-in so
  contributors without S3 access are not forced through sccache.
- Proc-macro crates and build scripts are not cached by sccache; a full clean
  build will still do real work for those.

## Development Workflow

### 1. Create a branch

Keep your fork up to date, then branch from `main`:

```bash
git fetch upstream
git checkout main
git rebase upstream/main
git checkout -b <type>/<short-description>
```

**Branch prefixes:**
- `feat/` - New features
- `fix/` - Bug fixes
- `refactor/` - Code refactoring
- `docs/` - Documentation updates
- `test/` - Test additions/updates
- `chore/` - Maintenance tasks
- `perf/` - Performance improvements

**Examples:**
```bash
git checkout -b feat/oauth2-token-refresh
git checkout -b fix/metrics-query-timeout
git checkout -b docs/update-readme-oauth
```

### 2. Make Changes

Follow code style guidelines:

**Rust Style:**
- Follow standard Rust conventions
- Use `cargo fmt` to format code
- Run `cargo clippy` before committing
- Keep functions small and focused
- Use clear, descriptive names

**Error Handling:**
```rust
// Good: wrap errors with context
use anyhow::{Context, Result};

fn query_metrics(query: &str) -> Result<Response> {
    let resp = client.get(url)
        .send()
        .context("failed to query metrics")?;
    Ok(resp)
}

// Bad: lose context
fn query_metrics(query: &str) -> Result<Response> {
    let resp = client.get(url).send()?;
    Ok(resp)
}

// Bad: expose secrets
fn query_metrics(api_key: &str) -> Result<Response> {
    // Never include secrets in error messages
    anyhow::bail!("auth failed with key {}", api_key);
}
```

**Testing:**
- Write unit tests for all public functions
- Use `#[cfg(test)]` modules
- Mock external dependencies
- Maintain >80% coverage (CI enforced)

Example test:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_time_param_relative_hour() {
        let result = parse_time_param("1h").unwrap();
        // Assert result is approximately 1 hour ago
        assert!(result > chrono::Utc::now() - chrono::Duration::hours(1) - chrono::Duration::seconds(5));
    }

    #[test]
    fn test_parse_time_param_invalid() {
        let result = parse_time_param("bad");
        assert!(result.is_err());
    }
}
```

### 3. Commit Changes

**Stage specific files** (avoid `git add .`):
```bash
git add src/auth/pkce.rs src/auth/mod.rs
```

**Commit with conventional format:**
```bash
git commit -m "$(cat <<'EOF'
<type>(<scope>): <subject>

<body describing what and why>

- Key change 1
- Key change 2
- Key change 3

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>
EOF
)"
```

**Commit types:**
- `feat` - New feature
- `fix` - Bug fix
- `docs` - Documentation changes
- `style` - Code style changes (formatting)
- `refactor` - Code refactoring (no behavior change)
- `test` - Test additions or changes
- `chore` - Build process or tooling changes

**Example:**
```bash
git commit -m "$(cat <<'EOF'
feat(auth): add OAuth2 authentication with PKCE

Implement OAuth2 authentication flow including:
- Dynamic Client Registration (DCR)
- PKCE code challenge generation
- Secure token storage via OS keychain
- Automatic token refresh

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>
EOF
)"
```

### 4. Push and open a pull request

Push to your fork, then open a PR against `DataDog/pup:main`:

```bash
git push -u origin <type>/<short-description>
```

Use `gh` CLI for efficiency:

```bash
gh pr create \
  --repo DataDog/pup \
  --head <your-username>:<branch-name> \
  --title "<type>(<scope>): <clear, concise title>" \
  --body "$(cat <<'EOF'
## Summary
Brief overview of what this PR does (1-2 sentences).

## Changes
- Specific change 1 with file reference (src/auth/pkce.rs:123)
- Specific change 2 with file reference
- Specific change 3 with file reference

## Testing
- Test scenarios covered
- How to verify the changes
- Coverage percentage

## Related Issues
Closes #<issue-number>
Fixes #<issue-number>

---
Generated with [Claude Code](https://claude.com/claude-code)
EOF
)" \
  --label "<labels>"
```

**PR title guidelines:**
- Keep under 70 characters
- Use imperative mood ("add" not "added")
- Be specific about what changed

**PR body guidelines:**
- **Summary**: What and why in 1-2 sentences
- **Changes**: Bulleted list with file references
- **Testing**: How changes were tested
- **Related Issues**: Use `Closes #N` or `Fixes #N`
- **Breaking Changes**: Clearly marked if any
- **Screenshots**: For CLI output changes

**Example PR:**
```bash
gh pr create \
  --repo DataDog/pup \
  --head your-username:feat/oauth2-token-refresh \
  --title "feat(auth): implement OAuth2 token refresh with PKCE" \
  --body "$(cat <<'EOF'
## Summary
Implements automatic OAuth2 token refresh using PKCE flow to maintain authentication without user intervention.

## Changes
- Added token refresher in src/auth/refresh.rs:45
- Implemented background refresh scheduler
- Added unit tests in src/auth/refresh.rs (tests module)
- Updated OAuth client to use refresh tokens in src/auth/pkce.rs:123

## Testing
- Unit tests verify refresh token exchange (98% coverage)
- Integration tests validate automatic refresh before expiration
- Manual test: verified token auto-refreshes after 50 minutes
- All existing tests pass

## Related Issues
Closes #42

---
Generated with [Claude Code](https://claude.com/claude-code)
EOF
)" \
  --label "enhancement,auth"
```

## Code Review Process

1. **Automated Checks**: CI runs tests, linting, coverage checks
2. **Human Review**: Maintainer reviews code quality and design
3. **Address Feedback**: Push follow-up commits to your branch — avoid
   force-pushing after review has started, as it makes follow-up reviews
   harder (GitHub loses track of review comments)
4. **Approval**: Once approved, PR can be merged
5. **Merge**: Squash and merge to keep history clean

## Testing Requirements

All PRs must:
- Pass all existing tests
- Add tests for new functionality
- Maintain >=80% code coverage
- Pass `cargo clippy` checks
- Build successfully with `cargo build`

See [TESTING.md](TESTING.md) for detailed testing guidelines.

## Security Guidelines

**Never commit:**
- API keys or secrets
- OAuth tokens or client secrets
- Environment variables with credentials
- Test data with real user information

**Always:**
- Validate user inputs to prevent injection
- Use parameterized queries for any data storage
- Wrap errors without exposing sensitive data
- Use HTTPS for all external requests

**OAuth2 Security:**
- Use PKCE S256 for code challenge
- Validate state parameter to prevent CSRF
- Never log or print access/refresh tokens
- Use OS keychain for primary token storage
- Restrict fallback file-storage permissions to `0600`

## Documentation

When adding features:
- Update relevant documentation files
- Add usage examples to EXAMPLES.md
- Update COMMANDS.md if adding new commands
- Include inline code comments for complex logic

## License

All contributions must be compatible with Apache 2.0 license.

By contributing, you agree that your contributions will be licensed under Apache License 2.0.

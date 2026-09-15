# Testing Guide

Test strategy, coverage requirements, and CI/CD documentation for Pup.

## Coverage Requirements

**Minimum threshold: 80%** - PRs that drop coverage below 80% will fail CI.

## Running Tests Locally

```bash
# Run all tests
cargo test

# Run tests with output shown
cargo test -- --nocapture

# Run a specific test
cargo test test_parse_time_param

# Run tests in a specific module
cargo test commands::metrics

# Run tests with cargo-llvm-cov for coverage (install first: cargo install cargo-llvm-cov)
cargo llvm-cov --html
open target/llvm-cov/html/index.html  # macOS
xdg-open target/llvm-cov/html/index.html  # Linux
```

## Test Organization

### Module Tests (src/)

Unit tests co-located with source code using `#[cfg(test)]` modules:

```
src/auth/        # Authentication logic
src/client.rs    # Datadog API client wrapper
src/config.rs    # Configuration management
src/output.rs    # Output rendering and printing
src/util.rs      # Utilities (time, validation)
src/commands/    # Command implementations
```

### Integration Tests (tests/)

End-to-end and comparison tests:

```
tests/compare/         # Output comparison tests
tests/compare_outputs.sh
tests/run_exhaustive.sh
```

## Test Patterns

### Unit Tests (co-located)

Preferred pattern for Rust:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_time_param_relative_hour() {
        let result = parse_time_param("1h").unwrap();
        let expected = chrono::Utc::now() - chrono::Duration::hours(1);
        let diff = (result - expected).num_seconds().abs();
        assert!(diff < 5, "time difference too large: {}s", diff);
    }

    #[test]
    fn test_parse_time_param_relative_minutes() {
        let result = parse_time_param("30m").unwrap();
        let expected = chrono::Utc::now() - chrono::Duration::minutes(30);
        let diff = (result - expected).num_seconds().abs();
        assert!(diff < 5, "time difference too large: {}s", diff);
    }

    #[test]
    fn test_parse_time_param_invalid() {
        let result = parse_time_param("invalid");
        assert!(result.is_err());
    }
}
```

### Async Tests

For testing async functions (API calls, etc.):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_query() {
        // Setup mock or test configuration
        let config = Config::default();
        // Test async function
        let result = query_metrics(&config, "avg:system.cpu.user{*}").await;
        assert!(result.is_ok());
    }
}
```

### Command Structure Tests

Test command registration and flags:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use clap::Command;

    #[test]
    fn test_metrics_subcommands() {
        let cmd = build_metrics_command();
        let subcmds: Vec<&str> = cmd.get_subcommands()
            .map(|s| s.get_name())
            .collect();
        assert!(subcmds.contains(&"query"));
        assert!(subcmds.contains(&"list"));
        assert!(subcmds.contains(&"get"));
        assert!(subcmds.contains(&"search"));
    }
}
```

## CI/CD Pipeline

GitHub Actions workflow runs on all branches:

### 1. Test and Coverage

```yaml
- name: Run tests
  run: cargo test --all

- name: Check coverage threshold
  run: |
    cargo install cargo-llvm-cov
    coverage=$(cargo llvm-cov --text 2>&1 | grep TOTAL | awk '{print $NF}' | sed 's/%//')
    if (( $(echo "$coverage < 80" | bc -l) )); then
      echo "Coverage $coverage% is below 80% threshold"
      exit 1
    fi
```

**On Pull Requests:**
- Runs all tests
- Generates coverage reports
- Checks coverage meets 80% threshold (fails if below)

**On Main Branch:**
- All PR checks plus:
- Updates coverage badge in README.md

### 2. Lint

```yaml
- name: Run clippy
  run: cargo clippy -- -D warnings

- name: Check formatting
  run: cargo fmt -- --check
```

Enforces Rust style and best practices.

### 3. Build

```yaml
- name: Build
  run: cargo build --release

- name: Verify binary
  run: ./target/release/pup --version
```

Verifies project builds and binary executes.

## Best Practices

**Do:**
- Write tests before fixing bugs (TDD for bug fixes)
- Use `#[cfg(test)]` modules co-located with source
- Mock external dependencies
- Test error paths, not just happy paths
- Use meaningful test names (`test_parse_time_param_invalid_input`)
- Assert specific error types when possible

**Don't:**
- Skip tests or use `#[ignore]` without good reason
- Test implementation details (test behavior, not internals)
- Make tests depend on each other
- Use sleeps for timing (use mock time or async channels)
- Commit failing or commented-out tests

## Test Coverage Goals

**Current Status:**
- Overall target: 80%

**Future Goals:**
- Integration test suite
- E2E tests with mock API server
- Performance benchmarks
- Fuzz testing for parsers

## Troubleshooting Tests

**Test fails intermittently:**
- Check for race conditions in async tests
- Look for time-dependent assertions
- Ensure proper cleanup

**Coverage report inaccurate:**
- Use `cargo llvm-cov` for accurate coverage
- Ensure test modules are not excluded

**Tests slow:**
- Profile tests: check for unnecessary network calls
- Mock external dependencies
- Use `cargo test -- --test-threads=N` to control parallelism

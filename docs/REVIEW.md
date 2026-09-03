# Code Review Guidelines

Review guidelines for PRs to Pup, whether authored by humans or AI agents. These rules are enforced during automated and manual review. Inspired by [Vibe Maintainer](https://steve-yegge.medium.com/vibe-maintainer-a2273a841040) principles — we welcome AI-assisted contributions but hold a high quality bar.

## Audience

This project is built **agent-first, human-second**. Code should be easy for AI coding agents to read, navigate, and modify. It should also be clear to humans reviewing it. When the two goals conflict, favor explicitness and simplicity.

## Review Checklist

Every PR is evaluated against the following criteria. A failure in any **MUST** item blocks merge.

### 1. Code Reuse (MUST)

- **Reuse existing utilities.** Before writing a new helper, search `src/` for an existing function that does the same thing. Common locations:
  - `src/util.rs` — time parsing, validation helpers
  - `src/formatter.rs` — output formatting (JSON, YAML, table, agent envelope)
  - `src/client.rs` — API client construction and request helpers
  - `src/config.rs` — configuration loading and precedence
- **No duplicate code.** If two or more call sites need the same logic, extract it into a shared function in the appropriate module. Three identical lines is the threshold — extract on the third occurrence.
- **No reimplementation of standard library or dependency features.** Use `anyhow`, `serde`, `clap`, and other existing dependencies rather than hand-rolling equivalents.

### 2. Test Coverage (MUST)

See [TESTING.md](TESTING.md) for full test strategy, coverage thresholds, and conventions. Review-specific expectations:

- **Positive tests required.** Every new public function or subcommand must have at least one test demonstrating correct behavior.
- **Negative tests required.** Every error path, validation check, or fallible operation must have a test proving the failure mode works as intended (returns the right error, rejects bad input, etc.).
- **No test regressions.** `cargo test` must pass with zero failures. Existing tests must not be deleted or weakened without justification.

### 3. Security (MUST)

See [CONTRIBUTING.md](CONTRIBUTING.md) security guidelines for full details. Review-specific expectations:

- **No malicious code.** PRs containing obfuscated code, backdoors, exfiltration attempts, unauthorized network calls, or any code that does not serve the stated purpose of the PR will be rejected and the contributor banned.
- **No credential exposure.** Never log, print, or include in error messages: API keys, tokens, secrets, or passwords. The sole exception is the explicit `pup auth token` credential-export command, which may print only the access token to stdout while keeping diagnostics on stderr. Grep your diff for `DD_API_KEY`, `DD_APP_KEY`, access tokens, and similar patterns.
- **Input validation at boundaries.** All user-supplied input (CLI args, environment variables, config file values) must be validated before use. Prevent command injection, path traversal, and other OWASP Top 10 vulnerabilities.

### 4. Dependency Hygiene (MUST)

- **Latest stable versions only.** New dependencies must use the latest stable release. Existing dependencies must not be pinned to versions with known CVEs.
- **No unnecessary dependencies.** Do not add a crate for something the standard library or an existing dependency already handles. Justify every new entry in `Cargo.toml`.
- **Run `cargo audit`** (or equivalent) before submitting. PRs that introduce dependencies with known vulnerabilities will be rejected.
- **Minimal version range.** Use precise version constraints (e.g. `"1.2"` not `"*"`) to avoid accidental breaking upgrades.

### 5. Complexity (MUST)

- **Keep functions small and focused.** A function should do one thing. If it takes more than ~40 lines, consider splitting it.
- **Avoid deeply nested logic.** More than 3 levels of indentation is a code smell. Use early returns, `match` arms, or extract helper functions.
- **No premature abstraction.** Don't introduce traits, generics, or builder patterns unless there are already 2+ concrete implementations that justify the abstraction.
- **No dead code.** Remove unused functions, imports, and commented-out blocks. `cargo clippy` catches most of these.
- **Prefer explicit over clever.** A few extra lines of clear code is better than a one-liner that requires a comment to explain.

### 6. PR Hygiene (SHOULD)

- **One concern per PR.** A bug fix, a new feature, and a refactor should be separate PRs. Mixed-concern PRs may be cherry-picked or split-merged.
- **Minimal diff.** Change only what's needed. Don't reformat untouched code, rename unrelated variables, or add unrelated improvements.
- **Rebase before submitting.** PRs based on stale forks create unnecessary merge conflicts.
- **No draft PRs.** Submit only when the work is ready for review.
- **Descriptive title and body.** Follow the conventional commit format: `<type>(<scope>): <subject>`. Include what changed and why.

### 7. Rust-Specific Standards (MUST)

See [CONTRIBUTING.md](CONTRIBUTING.md) code style and error handling sections for full conventions. Review-enforced minimums:

- **`cargo fmt`** — code must be formatted. CI enforces this.
- **`cargo clippy -- -D warnings`** — no clippy warnings. CI enforces this.
- **Use `anyhow` for errors.** Wrap errors with context: `.map_err(|e| anyhow::anyhow!("context: {e:?}"))`.
- **Follow Rust idioms.** Use `Option`/`Result` instead of sentinel values. Use iterators over manual loops where appropriate. Use `match` over chains of `if let`.

## Review Outcomes

Adapted from the [Vibe Maintainer](https://steve-yegge.medium.com/vibe-maintainer-a2273a841040) workflow. Listed in order of preference — we optimize for throughput and getting contributions merged:

| Priority | Outcome | Description |
|----------|---------|-------------|
| 1 | **Merge** | PR meets all standards — merge as-is. |
| 2 | **Merge-fix** | PR is mergeable but has minor issues — merge as-is, then push a follow-up fix to main. |
| 3 | **Fix-merge** | PR has value but is broken (fails CI, has bugs) — pull locally, fix, and push with contributor attribution. |
| 4 | **Cherry-pick** | PR bundles multiple changes and we only want some — cherry-pick the good parts locally, fix as needed, commit with attribution, and close the PR. |
| 5 | **Split-merge** | PR mixes separate concerns that should be independent — pull locally, split into separate commits with attribution, and push each. |
| 6 | **Reimplement** | PR solves a real problem but the design is wrong — reimplement the solution with a better approach, close the PR with thanks and an explanation. |
| 7 | **Retire** | PR is obsolete — superseded by another PR, already fixed, or no longer relevant. Close with a thank-you. |
| 8 | **Reject** | PR adds tech debt, is too niche for core, contains malicious code, or violates security rules. Close with a polite explanation. |
| 9 | **Request changes** | Last resort. Sending a PR back for rework delays landing and risks contributor attrition. Only use when the contributor is best positioned to make the fix. |

## For AI Agent Authors

If you are an AI agent generating a PR for this project:

1. **Read this file and [CLAUDE.md](../CLAUDE.md)** before generating code.
2. **Search the codebase first** for existing utilities, patterns, and conventions.
3. **Run `cargo test`, `cargo clippy`, and `cargo fmt --check`** before submitting.
4. **Include both positive and negative tests** for any new functionality.
5. **Do not introduce code that you cannot explain.** Every line should serve a clear purpose.
6. **Do not add unnecessary files, comments, or documentation** beyond what the change requires.

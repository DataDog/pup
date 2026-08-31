# Integration Test Harness — Philosophy & Design

This document explains the design principles of `scripts/test_harness.py` for
engineers and AI agents who need to extend, debug, or reason about it.

---

## What It Tests (and What It Doesn't)

The harness is an **end-to-end integration test** of the `pup` binary. It runs
the real compiled binary against the real Datadog API, captures actual output,
and checks it for defects and structural regressions.

It deliberately **excludes**:

- **Write commands** (`create`, `update`, `delete`, `cancel`, `trigger`, …).
  Running these in an automated loop against a live org would cause unintended
  side effects. They are catalogued in the report as "Write ops" but never
  executed.
- **Unit tests**. Those live in `src/` under `#[cfg(test)]` and are run by
  `cargo test`. The harness is complementary, not a replacement.
- **Value-level output comparison**. API responses contain rotating IDs,
  timestamps, and counts. Comparing raw values would produce constant false
  positives. See *Snapshot Strategy* below.

---

## Two Execution Modes

Every catalogued command runs **twice** per harness invocation:

| Mode   | Environment                                    | Purpose                            |
|--------|------------------------------------------------|------------------------------------|
| Human  | DD_* vars only, no `FORCE_AGENT_MODE`          | Simulates a developer at a shell   |
| Agent  | Same DD_* vars + `FORCE_AGENT_MODE=1`          | Simulates an AI agent (Claude, etc.) calling pup |

The two modes can produce different `--help` output (JSON schema vs text) and skip confirmation prompts in agent mode. JSON command payloads are the same shape in both modes.

### Why a Clean Environment?

`build_clean_env()` starts from scratch rather than copying `os.environ`. It
forwards only `HOME`, `PATH`, and variables starting with `DD_`. This ensures:

1. The test is **reproducible** — no implicit state leaks from the developer's
   shell (e.g. `RUST_LOG`, `FORCE_AGENT_MODE` set by a calling AI assistant, etc.)
2. **Credential values** from the outer environment never appear in test output,
   snapshots, or the HTML report. Only the variable names `DD_*` are forwarded;
   their values stay confined to subprocess stdin/stdout which is not logged.

`FORCE_AGENT_MODE` is explicitly set or explicitly deleted — it is never
inherited from the outer shell. This guarantees the human-mode run is genuinely
free of agent-mode influence.

---

## Snapshot Strategy

Snapshots live in `tests/snapshots/` (gitignored — never committed). Each file
stores the **structural schema** of a command's JSON output, not the values.

### Schema Extraction

`extract_json_schema()` converts a JSON response into a type-skeleton:

```json
// Real API response (varies every run)
{"data": [{"id": "abc-123", "attributes": {"name": "prod monitor", "status": "OK"}}]}

// Stored snapshot (stable across runs)
{"data": ["<list>", {"id": "<uuid>", "attributes": {"name": "str", "status": "str"}}]}
```

Rules applied during extraction:
- **Value normalizers** replace rotating leaf values: UUIDs → `<uuid>`,
  timestamps → `<timestamp>`, URLs → `<url>`, hex hashes → `<hash>`.
- **Dynamic-key dicts** — dicts whose keys all look like tags (`env:prod`),
  metric names (`system.cpu.user`), or numeric IDs — are collapsed to
  `{"*": child_schema}` so service maps and metric dictionaries don't churn.
- **Force-dynamic paths** (`FORCE_DYNAMIC_KEY_PATHS`) apply the same collapse
  to known-variable subtrees (`.tags`, `.attributes`, `.metrics`, etc.)
  regardless of key content.
- Schemas are merged across all list elements and all dynamic-key dict values
  (union, not first-element sample), producing a **stable superset** that
  doesn't regress when the API returns different optional fields on different
  runs.

### Additive-Only Regression Policy

`diff_schemas()` returns `(regressions, additions)`:

- **Regressions** (keys removed, types changed) → the test **fails**.
- **Additions** (new keys in current output) → the snapshot is auto-merged and
  the test **passes**. This means the baseline only ever grows; it never
  tightens unless `--update-snapshots` is passed explicitly.

The intent: a pup update that *adds* fields to an API response is not a
regression. One that *removes* fields or changes types is.

**Important:** if you change `FORCE_DYNAMIC_KEY_PATHS` or other schema
extraction rules, existing snapshots will appear to regress because the
new extractor produces a different schema shape (e.g. `{"*": ...}` instead
of concrete keys). In that case, run `--update-snapshots` to regenerate all
baselines under the new rules before committing.

### Snapshot Naming

Snapshots are keyed by `label` and `mode`:
```
tests/snapshots/monitors_list__human.json
tests/snapshots/monitors_list__agent.json
```

Human-mode and agent-mode `--help` output differs (text vs JSON schema), so
they need separate baselines. Command JSON payloads should match.

---

## Authentication

The harness manages auth in a layered fallback:

1. **`dd-auth --domain DOMAIN -o`** — injects `DD_API_KEY`, `DD_APP_KEY`,
   `DD_SITE` into the subprocess environment. These are the API-key credentials
   for commands that use that auth path.
2. **`pup auth status`** — checks whether a valid OAuth2 token is cached.
3. **`pup auth refresh`** — freshens the token if one exists.
4. **`pup auth login`** (interactive fallback) — only triggered when no valid
   token is present and refresh fails.

The auth info shown in the HTML report is deliberately vague ("dd-auth:
credentials injected", "OAuth2 token: refreshed"). Credential names and values
are never written to the report.

Auth failures in test output (`401`, `403`, `DD_API_KEY` mentions in pup's
own error text) are detected by `is_auth_failure()` and classified as
`auth_fail` status, which is distinct from a real test failure. An `auth_fail`
means the command works correctly but the current environment lacks credentials
for that API — it is not a bug in pup.

---

## Test Catalog (`READ_COMMANDS`)

Each entry in `READ_COMMANDS` in `scripts/test_harness.py` is a dict:

```python
{
    "label": "monitors list",          # human-readable name; becomes the snapshot key
    "args":  ["monitors", "list"],     # argv passed to the pup binary (no "pup" prefix)
    "category": "auth_required",       # "no_auth" | "auth_status" | "auth_required"
    "expect_json": True,               # assert stdout is valid JSON on exit 0
    "expect_exit": None,               # if set, assert this exact exit code
    "skip_regression": False,          # skip snapshot diff (use for highly volatile output)
    "timeout": 60,                     # per-command timeout in seconds (overrides --timeout)
    "note": "",                        # shown in report; use for known bugs/caveats
}
```

When adding a new command, prefer specific flag values that constrain the
response to a small, stable set (e.g. `--from=1h`, `--query=*`, a known stable
metric name). Commands that require a resource ID not known in advance belong in
the **untested** pool; document why in a comment rather than guessing an ID.

---

## Untested and Write Command Tracking

`get_untested_commands()` runs `FORCE_AGENT_MODE=1 pup --help` to retrieve the
full command schema (JSON) and crosses it against the catalog. It returns:

- **Uncovered read-only** commands — `read_only=true` but no catalog entry.
  These appear in the report as `? UNTESTED` rows with instructions for adding
  them.
- **Write commands** — `read_only=false`. Appear as `✎ WRITE` rows documenting
  their existence without executing them.

The `read_only` flag is set by `is_write()` in `src/main.rs` based on command
name patterns (`delete`, `create`, `update`, `cancel`, `trigger`, …). No source
change is needed to classify a command — the schema reflects it automatically.

---

## HTML Report

Generated at `tests/harness_report.html` (gitignored). Key features:

- **Summary cards** — clickable status counts (Pass / Fail / Auth Fail /
  Skipped / Untested / Write / Total).
- **Two filter rows** that AND together:
  - *Status*: All / Failures / Passes / Auth fail / Skipped / Untested / Write ops
  - *Mode*: All modes / Human / Agent
  Synthetic (untested/write) rows use `data-mode="all"` so they remain visible
  under any mode filter.
- **Mode badge** per row (Human in teal, Agent in green).
- **Diff column** — inline (unified diff) or side-by-side toggle showing
  human-vs-agent stdout for each test. "Identical output" when they match.
- **Deep linking** — every row has a stable slug ID (`logs-query-human`) as an
  HTML anchor. Use the "Jump to ID" input or append `#logs-query-human` to the
  URL to navigate directly.
- **Collapsible stdout/stderr** inside the Details column.

---

## Subprocess Timeout Reliability

The runner uses `subprocess.Popen` with `start_new_session=True` rather than
`subprocess.run`. On timeout it kills the entire process group via
`os.killpg(SIGKILL)` rather than just the root process.

The reason: pup uses the tokio async runtime. When the top-level pup process is
killed, tokio worker threads may still hold the stdout/stderr pipe file
descriptors open for a brief window. If Python's `communicate()` sees no EOF
it blocks indefinitely — leading to 100% CPU spin and a harness that never
completes. Killing the process group ensures all threads are reaped and the
file descriptors are released.

A secondary trigger for long hangs: when a full run takes > ~1 hour and the
OAuth2 token expires mid-run, pup may block on an interactive re-auth that can
never complete in a non-terminal subprocess. The `os.killpg` approach catches
this case too because the 30-second timeout will always fire regardless of
what the pup process is waiting for.

## Invariants to Preserve

1. **Snapshots and the HTML report are never committed.** Both paths are listed
   in `.gitignore`. `filter-branch` has been run twice to remove any historical
   accidental commits. Do not add them back.

2. **No credential values in the report.** `build_clean_env()` forward only
   `DD_*` variable names, never their values into the report's auth section.
   pup's own error messages may mention variable names as instructions
   (`"set DD_API_KEY"`); that is acceptable — they don't contain values.

3. **The catalog only grows.** Removing a catalog entry without a corresponding
   removal of the command from pup is a coverage regression. Prefer marking
   entries with `skip_regression=True` or a `note` rather than deleting them.

4. **Human mode must not have `FORCE_AGENT_MODE`.** `build_clean_env()` calls
   `env.pop("FORCE_AGENT_MODE", None)` explicitly. Never pass `agent_mode=True`
   for the human env.

5. **Snapshot baselines only tighten via `--update-snapshots`.** Auto-merge
   adds keys but never removes them. Passing `--update-snapshots` replaces the
   baseline outright — only do this deliberately after reviewing the diff.

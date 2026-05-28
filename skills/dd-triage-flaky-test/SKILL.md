---
name: dd-triage-flaky-test
description: Load when investigating a specific flaky test. Gets history, failure pattern, and category, then recommends fix, quarantine, or escalate.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,ci,cicd,flaky,flaky-tests,test-optimization
  alwaysApply: "false"
---

# Triage Flaky Test

One-line summary: Investigate a specific flaky test — get history, failure pattern, and category, then recommend fix, quarantine, or escalate.

Requires: `dd-pup` skill (pup CLI installed and authenticated).

---

## Input

| Parameter | Description |
|---|---|
| Test name | Fully qualified test name (e.g. `TestMyFunc` or `com.example.MyTest`) |
| Repository | Lowercase, no-schema URL (e.g. `github.com/org/repo`). Derive from `git remote get-url origin` if not provided. |

---

## Workflow

### STEP 0 — Parse Input

Derive repository ID from git if not provided:
```bash
git remote get-url origin
# Strip protocol and trailing .git, then lowercase the result
# e.g. https://github.com/DataDog/my-repo.git → github.com/datadog/my-repo
```

**Validation fallback:** If STEP 1 returns no results, confirm the correct repository by searching without a repo filter:
```bash
pup cicd tests search \
  --query "@test.name:\"<test-name>\"" \
  --from 30d \
  --limit 5
```
Extract `@git.repository.id_v2` from results and retry STEP 1 with the confirmed value.

### STEP 1 — Get Flaky Test Details

**Preferred — use `fingerprint_fqn` if known** (`fingerprint_fqn` is a valid CI Visibility search facet, distinct from `flaky_state`):
```bash
pup cicd flaky-tests search \
  --query "fingerprint_fqn:<fqn>" \
  --sort="-last_flaked" \
  --limit 5
```

**Fallback — use name + suite + repo:**
```bash
pup cicd flaky-tests search \
  --query "@test.name:\"<test-name>\" @test.suite:\"<suite>\" @git.repository.id_v2:\"<repo>\"" \
  --sort="-last_flaked" \
  --limit 10
```
Omit `@test.suite` if unknown; if the same test name appears in multiple suites, pick the entry whose suite matches the failing test.

Do not filter by `flaky_test_state` — return the test regardless of state.

Note: the query filter facet is `flaky_test_state`; the returned response attribute is `flaky_state` — these are different names for the same concept; do not use `flaky_state:active` as a query filter.

Extract from results:
- `fingerprint_fqn` — unique test identifier; used as the `id` in STEP 5 write call. **If absent, do not proceed to quarantine — see STEP 5.**
- `flaky_state` — current state (active / quarantined / disabled / fixed)
- `test_stats.failure_rate_pct` — percentage of runs that fail
- `flaky_category` — root cause category
- `codeowners` — owning team
- `pipeline_stats.total_lost_time_ms` — total CI time lost

### STEP 2 — Get Recent Failure History

```bash
pup cicd tests search \
  --query "@test.name:\"<test-name>\" @test.suite:\"<suite>\" @test.status:fail @git.repository.id_v2:\"<repo>\"" \
  --from 7d \
  --limit 20
```

Extract:
- Error messages and stack traces (`@error.message`, `@error.stack`)
- Failing branches (`@git.branch`) — branch-specific vs. widespread
- Frequency pattern — random timing or specific conditions
- Unique `@ci.pipeline.id` values for blast radius (STEP 3)

### STEP 3 — Check Blast Radius

Count distinct pipelines impacted using pipeline IDs from STEP 2:

```bash
pup cicd events aggregate \
  --query "@ci.status:error @ci.pipeline.id:(<id1> OR <id2> OR ...) @git.repository.id_v2:\"<repo>\"" \
  --compute count \
  --group-by "@ci.pipeline.name" \
  --from 7d
```

Use the first 10 pipeline IDs from STEP 2 (cap at 10; if more are available, run a second batch and merge results by summing counts per `@ci.pipeline.name` across batches). Report blast radius as: total number of unique pipelines impacted and whether failures are branch-specific or widespread.

Note: a pipeline failure is not necessarily caused solely by this flaky test — treat blast radius as a signal, not a definitive count.

### STEP 4 — Recommend Fix or Quarantine

Use `flaky_category` from STEP 1 and error messages from STEP 2.

**Root cause first:**
- Read the full error trace from bottom to top — chained errors hide the real cause; the innermost error is the root cause, not the first line.
- Identify the exact source of nondeterminism (race, ordering, stale state, timing).
- If the root cause is a CI infrastructure problem (runner unavailable, Docker daemon failure, network outage) → do NOT propose a code fix; classify as `infra` and recommend retry instead.
- If root cause is uncertain and cannot be confirmed from the stack trace → skip fix, go to quarantine.

**Fix at the correct layer:**
- Test issue → fix in test or test helper only.
- Production bug exposed by the test → fix in production code.
- Shared helper used by multiple tests → fix the helper AND update all call sites; never patch a single test when the root cause is in shared code.

**Forbidden — do not propose these:**
- Timing hacks: increasing timeouts, adding sleeps, widening time windows, adding retries. A fix that only reduces flake probability without eliminating the root cause is invalid.
- Masking: relaxing assertions (e.g., exact match → at least 1), dropping validations.
- Partial fixes: touching one call site when multiple share the root cause.

**Fix patterns by category:**

| Category | Approach |
|---|---|
| `timeout` | Identify the slow operation and make it synchronous or deterministic — do NOT simply raise the timeout constant |
| `concurrency` | Add deterministic synchronization (barriers, channels, locks); remove shared mutable state between tests |
| `network` | Mock or stub network calls at the boundary; if the test requires a real connection, isolate it with a test server |
| `time` | Inject a controllable clock; replace wall-clock assertions with relative or event-driven checks |
| `order_dependency` | Isolate test state with setup/teardown; eliminate dependencies on execution order or global state |
| `environment_dependency` | Mock env variables and external config; use test-local fixtures, not shared directories or singletons |
| `resource_leak` | Ensure every resource opened in a test is closed in teardown; use cleanup hooks that run even on failure |
| `randomness` | Fix the random seed for the test run; use deterministic inputs instead of random generation |
| `asynchronous_wait` | Replace fixed sleeps with condition polling or event/signal-driven waits with a hard timeout |
| `io` | Use temp files/dirs cleaned up in teardown; mock or stub filesystem interactions |
| `unknown` | Skip fix attempt → go to quarantine |

**Before proposing code changes, verify all of the following — if any fails, skip fix and recommend quarantine:**
- The root cause is the innermost error in the trace, not a surface-level symptom.
- The failure is a code problem, not a CI infrastructure problem.
- The fix eliminates the root cause (not just reduces flake probability).
- The fix is at the correct layer (test vs. production vs. shared helper).
- All call sites of any shared code are updated.
- No timing hacks or relaxed assertions introduced.

**Decision:**
- If category is `unknown` OR verification above fails → skip fix, recommend quarantine
- If category is known AND root cause is confirmed AND fix is valid → propose specific code change

### STEP 5 — Produce Triage Brief and Act

```
Flaky Test Triage Brief
=======================
Test:           <fully qualified test name>
Service:        <@test.service>
Category:       <flaky_category>
Failure Rate:   <test_stats.failure_rate_pct>%
Duration Lost:  <pipeline_stats.total_lost_time_ms>ms
Codeowners:     <codeowners>
Blast Radius:   <N> pipelines (<branch-specific | widespread>) [approximate — other failures in the same pipeline runs may not be related]

Evidence:
  <1-2 key error message lines from STEP 2>

Recommendation: <fix | quarantine | escalate>
Confidence:     <high | medium | low>
Action:         <specific next step>
```

**Decision thresholds:**
- `failure_rate_pct > 10` OR blast radius > 5 pipelines → **quarantine**
- `failure_rate_pct ≤ 10` AND known category AND clear fix → **fix**
- `failure_rate_pct ≤ 10` AND category `unknown` → **escalate** to codeowners with triage brief

**If recommending quarantine**, present and require explicit user approval before writing:

```
Proposed action: quarantine "<test-name>"
  id (fingerprint_fqn): <fingerprint_fqn from STEP 1>
  Effect: test still runs but failures are suppressed (CI will not be blocked)
  Reversible: yes — update new_state to active to restore

Approve? (yes/no)
```

**If `fingerprint_fqn` was not returned in STEP 1** (test not yet in FTM or query returned no results): do not attempt the write. Surface an error and ask the user to open the Flaky Test Management UI directly to quarantine manually.

Only after explicit approval and a confirmed `fingerprint_fqn`, write the body file and run:
```bash
cat > /tmp/flaky-update.json <<'EOF'
{
  "data": {
    "type": "UpdateFlakyTestsRequest",
    "attributes": {
      "tests": [{"id": "<fingerprint_fqn>", "new_state": "quarantined"}]
    }
  }
}
EOF
pup test-optimization flaky-tests update --file /tmp/flaky-update.json
```

To undo: repeat with `"new_state": "active"`.

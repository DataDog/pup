---
name: dd-triage-flaky-test
description: Load when investigating a specific flaky test. Gets history, failure pattern, and category, then recommends fix, quarantine, or escalate.
metadata:
  version: "1.1.0"
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
# Strip protocol and trailing .git, normalize SSH git@host:org/repo to host/org/repo, then lowercase
# e.g. https://github.com/DataDog/my-repo.git → github.com/datadog/my-repo
```

**Validation fallback:** If STEP 1 returns no results, confirm the correct repository by searching without a repo filter:
```bash
pup cicd tests search \
  --query "test_level:test @test.name:\"<test-name>\"" \
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
Omit `@test.suite` if unknown. If multiple suites or modules match, resolve the test identity from the failing event before continuing; add `@test.module` when needed to distinguish them.

Do not filter by `flaky_test_state` — return the test regardless of state.

Note: the query filter facet is `flaky_test_state`; the returned response attribute is `flaky_state` — these are different names for the same concept; do not use `flaky_state:active` as a query filter.

Extract from results:
- `fingerprint_fqn` — unique test identifier; used as the `id` in STEP 5 write call. **If absent, do not proceed to quarantine — see STEP 5.**
- `flaky_state` — current state (active / quarantined / disabled / fixed)
- `test_stats.failure_rate_pct` — percentage of runs that fail
- `flaky_category` — root cause category
- `codeowners` — owning team
- `pipeline_stats.total_lost_time_ms` — total CI time lost in milliseconds
- `attempt_to_fix_id` — non-empty when a Bits Code fix has been generated
- `web_url` — datacenter-correct FTM link, when supplied by the backend

Pup returns raw JSON:API records: read fields from `data[].attributes` and use the matching record’s `data[].id` as `fingerprint_fqn` if it is not repeated in attributes. The MCP enriches responses with `fingerprint_fqn` and `web_url`; pup may not return `web_url`. Keep the ID tied to the confirmed test’s module, suite, name, and repository. If no link is supplied, use the FTM UI on the configured Datadog site; do not assume a US1 hostname.

### STEP 2 — Get Recent Failure History

```bash
pup cicd tests search \
  --query "test_level:test @test.name:\"<test-name>\" @test.suite:\"<suite>\" @test.status:fail @git.repository.id_v2:\"<repo>\"" \
  --from 7d \
  --limit 20
```

Extract:
- Error messages and stack traces (`@error.message`, `@error.stack`)
- Failing branches (`@git.branch`) — branch-specific vs. widespread
- Frequency pattern — random timing or specific conditions
- Unique `@ci.pipeline.id` values for blast radius (STEP 3)

**Reading stack traces:** Read error chains from bottom to top. Chained errors hide the real cause — the innermost error at the bottom is the root cause, not the first line. If the identified root cause would cause near-100% failure but the actual flake rate is much lower, the root cause identification is wrong — look deeper.

### STEP 2.5 — Determine Attribution

Decide whether this flake was **introduced by the current working change** (self-introduced) or is a **pre-existing, inherited** flake. This drives the fix-vs-quarantine steer in STEP 4/5 — you should not suppress a flake you just introduced.

**Preserve the confirmed test identity.** If STEP 1 identified a module, add `@test.module:"<module>"` to the STEP 2 query and rerun it before continuing. Use only history matching the confirmed module, suite, name, and repository for attribution, error analysis, and blast radius. If history cannot be scoped to the confirmed test, treat it as inconclusive.

**Failure history takes precedence.** If STEP 2 shows the test failing on the default branch, or failing across multiple unrelated branches (not just the current one), the flake pre-dates the current change — classify it `pre-existing` even if the current diff touches the test or nearby code, and even if the user claims ownership. Diff overlap or an author editing nearby code does not make an already-widespread flake self-introduced. Apply the signals below only when this pre-existing failure history is absent.

A flake is **self-introduced** when *any* signal holds (and the failure history above has not already classified it `pre-existing`):

1. **Diff overlap** (primary) — the current branch's changes touch the failing test's file, or a changed non-test file that plausibly feeds the failing code path. **First confirm the local checkout is the repository under investigation:** normalize `git remote get-url origin` (lowercase, strip scheme and trailing `.git`, and convert SSH remotes such as `git@github.com:org/repo.git` to `github.com/org/repo`) and check it equals the `<repo>` from STEP 0. If it does not match (or there is no git remote / no shell access), **skip this signal entirely** — diffing an unrelated local checkout would attribute the wrong repo's changes. Also confirm the checked-out branch/commit is the change under investigation; otherwise skip the local diff signal. Only when both match, derive the comparison base from the repository's default branch:
   ```
   git diff --name-only $(git merge-base origin/<default-branch> HEAD)...HEAD
   ```
   Derive `<default-branch>` from `git symbolic-ref refs/remotes/origin/HEAD` (strip the `refs/remotes/origin/` prefix); fall back to `main` if unset. Then judge overlap using the stack trace and error evidence already gathered in STEP 2 — the test file appearing in the diff is a direct match; a changed source file named in the stack trace is an indirect match.
2. **Branch-locality** (secondary) — the failing branches from STEP 2 (`@git.branch`) are limited to the current branch and the test is not failing on the default branch. A truncated result page is not evidence of branch-locality: check the default branch and other branches for this test before relying on this signal; if that history is unavailable, treat branch-locality as inconclusive.
3. **Stated ownership** — the user says they just added, wrote, touched, or introduced the test (or the code it exercises) in their current change / PR / branch / commit. Take this at face value as a self-introduced signal on its own; it does not require corroboration from signals 1–2, and it applies even when the local diff cannot be inspected (no shell / git access) or branch history is inconclusive. Only discount it if the gathered data flatly contradicts it (e.g. the test has been failing on the default branch for weeks).

Record attribution as one of:
- `pre-existing` — the test fails on the default branch or across multiple unrelated branches (this takes precedence over the signals above).
- `self-introduced` — no pre-existing failure history, and any signal above holds.
- `undetermined` — no pre-existing failure history and none of the signals is available: the diff cannot be inspected, branch-locality is inconclusive, and the user has not stated ownership. Fall back to the default fix/quarantine behavior in STEP 4/5.

### STEP 3 — Check Blast Radius

**Primary method:** Extract unique `@ci.pipeline.id` values from STEP 2 results and count distinct pipelines impacted.

**Fallback** (if > 10 unique pipeline IDs): Use the pipeline IDs already collected from STEP 2 and aggregate by name for supporting impact context — keeping the query scoped to pipelines that actually ran this test:

```bash
pup cicd events aggregate \
  --query "ci_level:pipeline @ci.status:error @ci.pipeline.id:(<id1> OR <id2> OR ...) @git.repository.id_v2:\"<repo>\"" \
  --compute count \
  --group-by "@ci.pipeline.name" \
  --from 7d
```

Use at most the first 20 unique IDs in the fallback query. Keep the distinct-ID count from STEP 2 as the primary count; grouped event counts are supporting evidence and must not be inflated by duplicate events. If STEP 2 was truncated, label the count as a lower bound.

Report blast radius as: number of unique pipelines impacted and whether failures are branch-specific or widespread.

**Note:** Blast radius is an approximation. A pipeline failure is not necessarily caused by this specific flaky test — the test may have failed in the same pipeline run that already had other unrelated failures. Treat blast radius as a signal, not a definitive count of pipelines blocked by this test.

### STEP 4 — Attempt Fix

**Infrastructure guard — check this before either branch below, including when `attempt_to_fix_id` is present.** Using the evidence gathered in STEP 2, determine whether the root cause is a fault in the CI/build environment or tooling itself (an unavailable daemon, a testcontainer that never starts, a missing/mis-architected binary, a network dependency down in CI). This is narrower than "not the test's own code" — a genuine bug in production code or a shared test helper is a code fault, not infrastructure, even though it also lives outside the test itself; guard 4 already routes that to a fix at the correct layer, and this guard must not override it. An existing Bits Code fix does not clear this guard: its presence means a fix was *generated*, not that the observed failure is a code issue.

If the root cause is infrastructure, jump to **STEP 5.B** when producing the recommendation — never `fix`, regardless of category or failure rate. Otherwise, use the normal path, **STEP 5.A**.

For an infrastructure root cause, you may still surface the Bits Code link or a suggested infra remediation, but only via the `AI Fix` / `Infra Fix` lines in the STEP 5 brief — never as the `Recommendation`.

**If `attempt_to_fix_id` is present (Bits Code fix available):**

An AI-generated fix already exists for this test. Surface it using `web_url` from STEP 1. If that link is unavailable, tell the user to open Flaky Test Management on their configured Datadog site and locate the confirmed test; do not invent a URL or discard the available fix. Use the link below only when it was returned:

```
AI Fix Available: [View fix in Flaky Test Management](<web_url>)

A Bits Code fix has been generated for this test. Open the link above to view and apply
the proposed diff. If you'd prefer an inline agent-native fix attempt instead, say so.

After applying the fix, include the test key in your commit message so Datadog validates the fix:
  git commit -m "fix: <description>

  DD_<attempt_to_fix_id>"
```

Substitute `<attempt_to_fix_id>` with the value extracted in STEP 1.

If the infrastructure guard above did not trigger, skip the agent-native fix attempt and proceed to **STEP 5.A** with `recommendation: fix` and `confidence: medium`. If the guard did trigger, still show the "AI Fix Available" block above as informational context, but use **STEP 5.B** instead — never `fix`, regardless of the Bits Code fix's presence.

**If `attempt_to_fix_id` is absent (V1 agent-native):**

Use `flaky_category` from STEP 1 and the stack trace and error message from STEP 2 to propose a targeted fix.

Load [references/fix-patterns.md](references/fix-patterns.md) for the full category-to-fix mapping.

**Before proposing any code change, verify all of the following. If ANY fails → emit NO code changes and apply the attribution-aware fallback in the Decision block below (do NOT assume `quarantine` — for a `self-introduced` flake the fallback is `escalate`):**
1. The failure is a code problem, not a CI infrastructure problem (Docker daemon down, testcontainer startup failure, network unavailable in CI — these cannot be fixed in test code).
2. The root cause is the innermost error in the chain, not a surface symptom. If the identified root cause would cause near-100% failures but the actual flake rate is much lower, the root cause identification is wrong.
3. The fix eliminates the root cause, not just reduces probability. Blind timing hacks (sleeps, or widening timeouts when the root cause is not a too-tight deadline) and relaxed assertions (exact → at_least) are invalid even if they lower the flake rate.
4. The fix targets the correct layer: test issues in test/helper code, production bugs in production code. Never patch one test callsite when the root cause is in shared code — update all callsites.

**Guard 1 is the infrastructure guard from the top of STEP 4, not a "fix the infra instead" branch.** When it fails, do not propose the infra remediation (rebuilding a binary, retagging a runner, restarting a service) as `recommendation: fix` — that remediation is not a pre-flight check failure to route around, it IS the guard-1 failure. Use **STEP 5.B**, not STEP 5.A — STEP 5.A's low-failure-rate/known-category rule can still select `fix`, which is specifically excluded once guard 1 has failed.

**Decision:**
- If category is known AND pre-flight passes AND error message confirms the root cause → apply the fix pattern from `references/fix-patterns.md` and propose specific code changes
- If category is `unknown` → the root cause cannot be confirmed from available signal; emit NO code changes and go to **STEP 5.A**
- If any pre-flight check fails → emit NO code changes and go to **STEP 5.A** (guards 2-4) or **STEP 5.B** (guard 1 — an infrastructure root cause)

**Attribution overrides the no-fix fallback (from STEP 2.5):**
- When attribution is `self-introduced`, `fix` is the strongly preferred outcome — you should not suppress a flake introduced by the current change. Still respect the pre-flight guards above: a self-introduced flake only gets a code change if the guards pass. If a safe fix cannot be made (category `unknown` or a guard fails), go to **STEP 5** with `recommendation: escalate` to the author (the current change owns it; this applies whether or not guard 1 was the guard that failed). Do not default to quarantine for a self-introduced flake; quarantine is only a last resort if the user explicitly insists (see the warning in STEP 5).
- When attribution is `pre-existing` or `undetermined`, use **STEP 5.A**'s thresholds: unknown categories may escalate, while failed code-fix guards may require quarantine — except when guard 1 (the infrastructure guard) failed, in which case use **STEP 5.B** instead. If `undetermined`, the `Attribution:` line in the STEP 5 brief must still read `undetermined` so the user can see the self-introduced check was inconclusive — do not present the quarantine as if attribution were confidently `pre-existing`.

After proposing the code change, provide commit message guidance:

```
To link the fix to Datadog's validation system, include the test key in your commit message:
  git commit -m "fix: <description>

  DD_<attempt_to_fix_id or fingerprint_fqn>"
```

Substitute the test key with `attempt_to_fix_id` from STEP 1 when non-empty, otherwise fall back to the confirmed `fingerprint_fqn` from STEP 1. If neither is available, omit the validation tag rather than inventing one.

### STEP 5 — Recommend Action and Produce Triage Brief

Produce a **triage brief** in this exact format:

```
Flaky Test Triage Brief
=======================
Test:           <fully qualified test name>
Service:        <@test.service>
Category:       <flaky_category>
Failure Rate:   <test_stats.failure_rate_pct>%
Duration Lost:  <pipeline_stats.total_lost_time_ms ms>
Codeowners:     <codeowners>
Blast Radius:   <N> pipelines (<branch-specific | widespread>; approximate)
Attribution:    <self-introduced | pre-existing | undetermined>

Evidence:
  <1-2 key error message lines from STEP 2 results>
Infra Fix:      <non-blocking infra/build remediation if guard 1 failed on a CI-infra root cause, e.g. "rebuild the test binary for the runner architecture" | N/A>
AI Fix:         <web_url or FTM lookup instructions if attempt_to_fix_id present | N/A>

Recommendation: <fix | quarantine | escalate>
Confidence:     <high | medium | low>
Action:         <specific next step>
```

**STEP 5.A — Decision thresholds (default path).** Use this unless the STEP 4 infrastructure guard failed — in that case use STEP 5.B instead.
- **Attribution `self-introduced` takes priority over every rule below and in STEP 5.B** — follow the STEP 4 routing: prefer fix; if no safe fix can be made, escalate to the author. Do not recommend quarantine for a self-introduced flake; fall back to quarantine only if the user explicitly insists after seeing the warning below.
- If a Bits Code fix is available and the infrastructure guard passed, use the STEP 4 handoff: **fix**, confidence **medium**, without claiming the proposed diff is validated.
- `test_stats.failure_rate_pct > 10` OR blast radius > 5 pipelines → **quarantine** (suppress failures while fix is prepared)
- `test_stats.failure_rate_pct ≤ 10` AND category is known AND fix is clear → **fix** (propose code change from STEP 4)
- `test_stats.failure_rate_pct ≤ 10` AND category is `unknown` → **escalate** to codeowners with triage brief context
- If the category is known but guards 2–4 prevent a safe fix, follow STEP 4’s no-fix fallback: **quarantine** for `pre-existing` / `undetermined`. Missing statistics are not zero; report the missing evidence and escalate when no preceding rule applies.

**STEP 5.B — Infrastructure guard routing.** Use this instead of STEP 5.A when the STEP 4 infrastructure guard failed. `fix` is never a valid outcome here, regardless of category or failure rate.
- `self-introduced` attribution → **escalate** to the author (per the priority rule in STEP 5.A).
- Otherwise → **quarantine** if `test_stats.failure_rate_pct > 10` OR blast radius > 5 pipelines, else **escalate** to codeowners.

**If recommending quarantine**, present and require explicit user approval before writing:

**If the user explicitly insists on quarantining a `self-introduced` flake** (against the fix/escalate recommendation above), prepend this warning before the approval block:

```
⚠️ This flake appears to be introduced by your current change (<attribution reason — e.g. your branch adds/edits <file>, it fails only on <branch>, or you stated you just added it>).
Quarantining suppresses a failure you just introduced and lets a known-bad test merge.
Recommended: fix it instead.
```

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

---

## References

- [Fix patterns](references/fix-patterns.md): category-specific fixes and forbidden workarounds; apply the attribution and infrastructure routing in STEP 4/5.

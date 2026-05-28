---
name: dd-unblock-pr
description: Load when investigating a failing PR CI pipeline or checking PR health. Attributes each CI failure as flaky, infra, or regression, proposes a targeted action, and reports code coverage.
metadata:
  version: "1.1.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,ci,cicd,flaky,flaky-tests,pipeline
  alwaysApply: "false"
---

# Unblock PR

One-line summary: Investigate a failing PR CI pipeline — attribute each failure as flaky, infra, or regression and propose a targeted action.

Requires: `dd-pup` skill (pup CLI installed and authenticated), `dd-triage-flaky-test` skill (for flaky failure deep investigation).

---

## Input

| Parameter | Description |
|---|---|
| PR branch | The branch under investigation (e.g. `my-feature-branch`) |
| Repository | Lowercase, no-schema URL (e.g. `github.com/org/repo`). Derive from `git remote get-url origin` if not provided. |

---

## Workflow

### STEP 0 — Parse Input

Derive repository ID and default branch from git if not provided:

```bash
# Repository ID: fully lowercase, no-schema URL (the API rejects mixed-case)
git remote get-url origin
# Strip protocol and trailing .git, then lowercase the result
# e.g. https://github.com/DataDog/my-repo.git → github.com/datadog/my-repo

# Default branch
git symbolic-ref refs/remotes/origin/HEAD
# Strip refs/remotes/origin/ prefix — fall back to main if unset
```

### STEP 1 — Get PR CI Summary (run both in parallel)

**Pipeline failures (job level):**
```bash
pup cicd events search \
  --query "@ci.status:error @git.branch:<branch> @git.repository.id_v2:\"<repo>\"" \
  --level job \
  --from 24h \
  --limit 50
```

**Test failures:**
```bash
pup cicd tests search \
  --query "@test.status:fail @git.branch:<branch> @git.repository.id_v2:\"<repo>\"" \
  --from 24h \
  --limit 50
```

Run both queries in parallel. Collect all distinct `@test.service` values from test event results. If more than one distinct service is found, note each separately in the triage brief — do not collapse them into a single service filter. If pipeline results contain only infrastructure job types (build, lint, deploy) with no test-runner output, discard test search results and skip to STEP 3.

### STEP 1.5 — Fetch Code Coverage (run in parallel with STEP 1)

This step runs unconditionally — coverage context is valuable whether CI is red or green.

The `--repo` value must be fully lowercase (the API rejects mixed-case). Normalize before calling:
```bash
repo_lower=$(echo "<repo>" | tr '[:upper:]' '[:lower:]')
pup code-coverage branch-summary \
  --repo "$repo_lower" \
  --branch "<branch>"
```

If the command returns no data or exits with an error, report "No data available" for Coverage in the PR Health section.

Note: Code quality and security violation counts are not available in pup — those lines always show "No data available".

### STEP 2 — Blame Guard per Failing Job

First check whether `@error_classification.domain` / `@error_classification.type` are present on job events from STEP 1 — if populated, use them as primary classification signals.

For each failing job where classification is still needed, run both checks in parallel:

**Default branch check** — was this job already failing before this PR?
```bash
pup cicd events aggregate \
  --query "@ci.status:error @ci.job.name:\"<job>\" @git.branch:<default-branch> @git.repository.id_v2:\"<repo>\"" \
  --compute count \
  --from 24h
```

**Blast radius check** — is this job failing on other branches too?
```bash
pup cicd events aggregate \
  --query "@ci.status:error @ci.job.name:\"<job>\" @git.repository.id_v2:\"<repo>\"" \
  --compute count \
  --group-by "@git.branch" \
  --from 24h
```

Performance fallback: if the blast radius query is slow or times out, skip it and rely on the default branch check alone.

### STEP 3 — Classify Each Failure

**Priority order:**
1. If `@error_classification.domain` / `@error_classification.type` present → use as primary signal
2. If test failure AND test appears in flaky tests with `flaky_test_state:active`:
   ```bash
   pup cicd flaky-tests search \
     --query "flaky_test_state:active @test.name:\"<test-name>\" @git.repository.id_v2:\"<repo>\""
   ```
   → **flaky**
3. Use blame guard results:

| Failing on default branch? | Failing on ≥3 other branches? | Classification |
|---|---|---|
| Yes | Yes | **infra** (pre-existing, widespread) |
| Yes | No | **infra** (pre-existing on default branch) |
| No | No | **regression** (introduced by this PR) |
| No | Yes | **flaky** (intermittent, cross-branch) |
| Insufficient data | — | **unknown** |

### STEP 4 — Produce Triage Brief

One entry per failing job:

```
PR CI Triage Brief
==================
Branch:   <branch>
Repo:     <repo>

Job: <job-name>
  Classification:  <flaky | infra | regression | unknown>
  Evidence:        <1 key data point — error message, pipeline count, or test result>
  Confidence:      <high | medium | low>
  Recommended:     <action>

[repeat for each failing job]

Overall: <N> failures — <e.g. "1 regression, 1 flaky, 1 infra">

PR Health
=========
Coverage:   <X>% on <branch> | No data available
Quality:    No data available
Security:   No data available
```

All three lines always appear.

### STEP 5 — Propose Actions

**regression** → Prompt user to investigate their code changes. No write action available.

**flaky** → Load `dd-triage-flaky-test` skill for deep investigation. That skill will:
- Attempt an agent-native fix using `flaky_category` + stack trace
- Propose quarantine via `pup test-optimization flaky-tests update` if a quick fix isn't possible

**infra** → Before proposing a retry, assess whether the failure is transient:
- Check `@error_classification.type` and error message for signals like `timeout`, `runner unavailable`, `network error`, `quota exceeded` — these indicate transient failures where a retry is likely to help
- If the error is deterministic (build misconfiguration, missing secret, explicit test assertion failure), a retry is unlikely to help — note this and suggest investigating the root cause
- If the failure is pre-existing on the default branch, inform the user — a retry will likely fail again; await the upstream fix instead

If transient and GitHub Actions: extract the run ID from `@ci.pipeline.url` (e.g. `https://github.com/org/repo/actions/runs/<run_id>`):
```bash
gh run rerun <run_id> --failed
```
For other providers, share `@ci.pipeline.url` and direct to the provider UI for retry.

**unknown** → Suggest checking raw job logs via the CI provider UI or `@ci.pipeline.url` from the pipeline event.

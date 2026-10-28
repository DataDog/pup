## PR CI Failure Classification

Use this reference during STEP 3 of the `dd-unblock-pr` workflow.

**Threshold note:** The STEP 3 table in the main skill is authoritative. This reference uses the same thresholds: ≥3 branches for **flaky** (cross-branch intermittent pattern), ≥3 for **infra** (widespread pre-existing failures). The term "widespread" in the infra heuristics below refers to the same ≥3-branch criterion.

---

## Primary Signal: @error_classification Fields

Check job-level pipeline events for these attributes before running blame guard queries. When present, they provide the most direct classification signal:

| Attribute | Description |
|---|---|
| `@error_classification.domain` | High-level failure domain (e.g. `test_failure`, `infrastructure`, `runner`) |
| `@error_classification.type` | Specific failure type within the domain |
| `@error.domain` | Error category (may be present even without full classification) |
| `@error.subdomain` | More specific error category |

If `@error_classification.domain` indicates an infrastructure or runner issue → classify as **infra** without further queries.
If `@error_classification.domain` indicates a test failure → proceed to flaky check and blame guard.

---

## Classification Heuristics

### flaky

**Signals:**
- Test failure AND test appears in the flaky-test registry with `flaky_test_state:active`
- Job failing on ≥3 branches AND NOT failing on the default branch (intermittent, cross-branch pattern)
- `@error_classification.type` indicates a non-deterministic failure category

**Confirm with:**
```bash
pup cicd flaky-tests search \
  --query "@test.name:\"<test-name>\" @test.suite:\"<suite>\" flaky_test_state:active @git.repository.id_v2:\"<repo>\""
```

**Action:** Load `dd-triage-flaky-test` for agent-native fix attempt and quarantine option.

**Attribution note:** Failure history takes precedence — a test failing on the default branch or across multiple branches is **pre-existing** (quarantine is appropriate), even if this PR touches its file; only a genuinely branch-local flake is author-owned and steered to *fix*. See **STEP 5 → flaky** in the main skill for the exact hint rule, and `dd-triage-flaky-test`'s STEP 2.5 for the authoritative attribution definition.

---

### infra

**Signals:**
- `@error_classification.domain` indicates runner, infrastructure, or environment failure
- Job is a non-test step (build, lint, deploy, docker push, etc.)
- Same job has been failing on the default branch with count > 0 in the 24h window
- High blast radius (≥3 branches) with no correlation to test failures

**Confirm with:**
```bash
pup cicd events aggregate \
  --query "ci_level:job @ci.status:error @ci.job.name:\"<job>\" @git.branch:<default-branch> @git.repository.id_v2:\"<repo>\"" \
  --compute count \
  --from 24h
```

**Action:** Follow STEP 5 in the main skill: retry only transient failures, use the available provider/backend retry path, explain the all-failed-jobs scope for GitHub, and verify the async result. Pre-existing deterministic failures need an upstream fix.

---

### regression

**Signals:**
- Job failing ONLY on this PR's branch (blast radius = 1)
- Job was NOT failing on the default branch in the same time window
- `@error_classification` absent or indicates test failure on this branch specifically
- Failure correlates with files changed in this PR

**Confirm with:**
- Default branch check returns count = 0
- Blast radius check shows only this PR's branch

**Action:** Prompt user to investigate their code changes. Suggest narrowing to changed files.

---

### unknown

**Signals:**
- Single data point (only 1 pipeline run available)
- Missing test events for a failing test-runner job
- Contradictory evidence (e.g. failing on default branch AND only on this branch)
- `@error_classification` absent AND blame guard returned insufficient data (< 3 pipeline events)

**Action:** Suggest checking raw job logs via Datadog CI pipeline view or the CI provider's UI.

---

## Edge Cases

**No test events for a failing test job:**
The test job may have failed before producing test events (e.g. compilation error, setup failure). Classify as **unknown** and recommend checking raw logs rather than guessing.

**Default branch derivation fails:**
If `git symbolic-ref refs/remotes/origin/HEAD` is unavailable, use `main` as fallback. Mention this assumption in the triage brief evidence field.

**Very new repository (< 24h of CI data):**
Insufficient blame guard data. Classify as **unknown** and note the limited history window.

**Multiple jobs failing with different classifications:**
Report each job separately in the triage brief. The overall summary should list counts per classification.

# Datadog Rust SDK Upgrade Plan

> **Audience:** Sonnet 4.6 sessions executing the upgrade, one PR at a time.
> **Goal:** Move `datadog-api-client` from the current pin (v0.30.0,
> `aa0c8416e3af27038cf6a17e74ff1bf11d6bc1a6`) to the latest `master` commit, then
> surface the net-new API surface as pup commands. Each PR below is a
> self-contained unit of work.

---

## 0. Targets and ground truth

| | Value |
|---|---|
| **Current rev** | `aa0c8416e3af27038cf6a17e74ff1bf11d6bc1a6` (release tag **v0.30.0**, 2026-04-24) |
| **Target rev** | `d4954b117c5451c0a8932dd0ad0db450fcb0989c` (`master` HEAD, 2026-05-29) |
| **Tagged alt.** | **v0.31.0** (2026-05-15) — see note below |
| **Pin location** | `Cargo.toml:132` (`datadog-api-client = { git = ..., rev = "..." }`) |
| **Lock entry** | `Cargo.lock` `[[package]] name = "datadog-api-client"` |

> **v0.31.0 vs `master` HEAD:** The user asked for "the latest commit", so the
> target is `master` HEAD. v0.31.0 is the last *tagged* release; HEAD adds ~25
> unreleased commits on top of it (the bulk of the net-new endpoints below come
> from this unreleased range). If review prefers a tagged release, fall back to
> v0.31.0 — the PR breakdown is unaffected, just the rev string changes. Confirm
> the exact HEAD SHA at execution time with
> `git ls-remote https://github.com/DataDog/datadog-api-client-rust HEAD`
> (network in this environment may need cert config; the SHA above was captured
> 2026-05-29).

---

## 1. How pup consumes the SDK (read before touching anything)

- **Typed calls.** Commands call `crate::make_api!(SomeAPI, cfg)` (or
  `make_api_no_auth!`) and use generated types from
  `datadog_api_client::datadogV1::api_*` / `datadogV2::api_*` and `::model::*`.
  See `src/commands/error_tracking.rs` for the canonical shape.
- **Raw calls.** Endpoints not in the typed client go through
  `client::raw_get/raw_post/raw_put/raw_delete/raw_request` in `src/client.rs`.
- **Two registries in `src/client.rs` that gate behavior and have count tests:**
  - `UNSTABLE_OPS` (currently **167** entries) — every unstable v2 op pup uses
    must be listed or the SDK rejects the call. Test: `test_unstable_ops_count`
    (`src/client.rs:1282`).
  - `OAUTH_EXCLUDED_ENDPOINTS` (currently **52** entries) — endpoints that must
    use API-key auth. Test: `test_oauth_excluded_count` (`src/client.rs:1287`).
  - **Any PR that adds/removes ops or excluded endpoints must update the
    corresponding count assertion.**
- **Routing.** `src/main.rs` holds the clap enums and the giant `match` that
  dispatches to `commands::*`. New subcommands touch: the domain enum, the
  dispatch arm, and (usually) doc comments.
- **Docs.** User-facing command reference lives in `docs/COMMANDS.md`.
- **Tests.** Each command module has a `#[cfg(test)] mod tests` using `mockito`
  via `crate::test_support::*`. Every PR must add positive **and** negative
  tests (see `docs/REVIEW.md`, `docs/TESTING.md`).

### Per-PR validation gate (run for every PR)

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release            # native
# spot-check WASM still builds if the change touches shared code:
cargo build --no-default-features --features wasi --target wasm32-wasip1
```

`cargo audit` per CLAUDE.md before submitting.

---

## 2. Sequencing rules

```
PR-1 (Foundation: rev bump + mandatory compile fixes)  ← MUST merge first
        │
        ├── PR-2 … PR-N  (net-new + enhancement PRs, branched off main
        │                 AFTER PR-1 merges; otherwise independent of each other)
```

- **The rev is global.** You cannot bump it in N independent PRs. PR-1 owns the
  bump and the *minimum* edits required to make `cargo build && cargo test`
  green on the new SDK. Everything else builds on top.
- **Breaking changes that stop compilation MUST live in PR-1.** They can't be
  isolated into "removed command" PRs because the tree won't build without
  them. Where a removal is large/independent enough to review on its own, it is
  still called out as its own section but noted as a PR-1 dependency.
- **Additive work is parallelizable.** After PR-1 merges, PR-2…PR-N each branch
  off `main`, touch one domain (or one subcommand group), and merge
  independently.
- Branch naming per CLAUDE.md: `<type>/<description>` (e.g.
  `feat/cost-oci-configs`). Commit format per `docs/CONTRIBUTING.md`.
- **Net-new is opt-in, not automatic.** Not every SDK endpoint needs a pup
  command. For each enhancement PR, first confirm the capability fits pup's
  surface; if it doesn't, skip it and note why in the PR description rather than
  adding a thin wrapper nobody asked for.

---

## 3. PR-1 — Foundation: rev bump + mandatory compile fixes

**Branch:** `chore/upgrade-dd-sdk-to-master`
**Type:** `chore(deps)` · **Must merge before all others.**

### Steps
1. Update `Cargo.toml:132` `rev` to the target SHA. Update the comment on
   `Cargo.toml:130` ("pinned to 0.30.0 release tag") to reflect the new pin.
2. Refresh the lock: `cargo update -p datadog-api-client --precise <SHA>` (or
   `cargo build` to let it re-resolve), then verify `Cargo.lock` shows the new
   `rev` and `version`.
3. `cargo build` and collect **every** compile error. Fix only what's needed to
   compile — no new features here. Expected breakage (confirm at execution
   time, the SDK may have shifted more):

   **(a) Incident teams removed** (SDK PR #1572 removed the deprecated
   `api_incident_teams` module). This breaks:
   - `src/commands/incidents.rs` — imports + `make_teams_api`, `teams_list/get/
     create/update/delete` (lines ~5–283) and their tests (~543+).
   - `src/main.rs` — `IncidentTeamActions` enum, the `Teams { action }` variant
     of `IncidentActions` (~`src/main.rs:3100`), and the dispatch arm
     (`src/main.rs:10993`).
   - `src/client.rs` — the 5 "Incident Teams" entries in `UNSTABLE_OPS`
     (`v2.create_incident_team` … `v2.update_incident_team`, lines ~247–252) →
     drop them and change `test_unstable_ops_count` from **167 → 162**.
   - `docs/COMMANDS.md` — remove the `incidents teams` reference.
   - **This is effectively the "remove incidents teams command" work; it lives
     in PR-1 because the build requires it.**

   **(b) Any changed signatures** that break compilation, e.g. the
   `security-monitoring histsignals/search` method change GET→POST (#1652) if
   pup binds it typed; Org Groups pagination shape change (#1547) in
   `src/commands/organizations.rs`. Fix to compile; defer behavioral
   enhancements to their feature PRs (§5).

4. `cargo test` — fix any count-assertion and snapshot failures.
5. Run the full validation gate (§1). Confirm WASM builds.

### Definition of done
- Tree builds and tests pass on the new rev with the **smallest possible** diff.
- No new commands, no new flags, no behavior beyond "keep it working".
- PR body lists each breaking change handled and links the upstream SDK PR.

---

## 4. Removed / deprecated surface (PR-1 dependents)

| Item | SDK PR | pup impact | Where it lands |
|---|---|---|---|
| Incident teams endpoints removed | #1572 | Removes `incidents teams` command | **PR-1** (compile-mandatory) |
| Feature-flags deprecated allocation key fields | #1509 | `feature_flags.rs` uses `CreateAllocationsRequest`/`OverwriteAllocationsRequest` via file-deserialized bodies (no direct field refs) → likely compiles unchanged. **Verify**; only act if it breaks. | PR-1 if breaking, else no-op |
| Cloud Cost Search Recommendations `scope` param removed | #1649 | pup has no `recommendations` command today (`grep` finds none). No action unless a typed param ref breaks. | PR-1 if breaking |
| Status Pages: creating *published* pages deprecated | #1522 | Behavioral/doc deprecation. Update help text + docs. | §5 status-pages PR |

---

## 5. Net-new & enhancement PRs (after PR-1)

Each row is one PR. Grouped by command/domain; large domains (security) are
split by subcommand. For each: confirm the domain module exists under
`src/commands/`, add the typed binding (registering new unstable ops in
`UNSTABLE_OPS` + bumping the count test where needed), wire `src/main.rs`, add
mockito tests (happy + error paths), and update `docs/COMMANDS.md`.

### Security (split — `security.rs` is ~42 KB, too large for one PR)
| PR | Scope | SDK PRs |
|---|---|---|
| `feat/security-mute-findings` | `pup security findings mute` (now stable) | #1519, #1660 |
| `feat/security-rules-bulk-convert` | `pup security rules bulk-convert` | #1675 |
| `feat/security-historical-signals` | histsignals search (GET→POST) + SIEM historical detections reconcile | #1652, #1656 |
| `feat/security-datasets` | secmon-public-api datasets endpoints | #1653 |
| `feat/security-compliance-findings` | Compliance Findings rule-based view + 30-day learningDuration/forgetAfter | #1595, #1492 |

### Cost / CCM (`cost.rs`, `cost_ccm.rs`)
| PR | Scope | SDK PRs |
|---|---|---|
| `feat/cost-oci-configs` | ListCostOCIConfigs | #1540 |
| `feat/cost-anomalies` | Cloud cost anomalies endpoints | #1588 |
| `feat/cost-tag-metadata-months` | CCM tag_metadata months endpoint | #1645 |

### Observability / LLM
| PR | Scope | SDK PRs |
|---|---|---|
| `feat/llmobs-dataset-export` | dataset export / clone / restore / records upload (SSE) | #1655 |
| `feat/llmobs-session-interactions` | session interaction types | #1558 |
| `feat/obs-pipelines-destinations` | databricks_zerobus + splunk HEC metrics destinations | #1534, #1644 |

### RUM / Error Tracking
| PR | Scope | SDK PRs |
|---|---|---|
| `feat/rum-retention-filters` | permanent retention filters endpoints | #1650 |
| `feat/rum-source-maps` | RUM Source Map Intake API | #1532 |
| `feat/error-tracking-filters` | `--state`, `--assignee`, `--team` filters + surface regression fields in `error_tracking.rs` | #1590, #1568, #1480 |

### Dashboards / widgets (`widgets.rs`, `dashboards.rs`)
| PR | Scope | SDK PRs |
|---|---|---|
| `feat/widgets-new-types` | Point Plot widget; TreeMap/Sunburst style+sort; Host Map infra request type; QueryValue comparison | #1542, #1546, #1672, #1643 |

### Logs / Synthetics / Scanning
| PR | Scope | SDK PRs |
|---|---|---|
| `feat/logs-archives-compression` | compression_method on Log Archives | #1545 |
| `feat/synthetics-downtime` | Synthetics downtime endpoints | #1518 |
| `feat/agentless-compliance-host` | compliance_host on Agentless Scanning | #1525 |

### APM / Traces
| PR | Scope | SDK PRs |
|---|---|---|
| `feat/apm-spans-public-api` | spans-public-api trace endpoints + apm metrics data source for distribution histogram | #1667, #1661 |

### Usage / metering (`usage.rs`)
| PR | Scope | SDK PRs |
|---|---|---|
| `feat/usage-new-types` | feature_flags_config_requests, serverless_apps_dsm_fargate_tasks, siem 6mo/12mo retention, GKE Autopilot types | #1486, #1646, #1647, #1559 |
| `feat/usage-query-params` | window[seconds] lookback + cross_org_uuids on v2 query endpoints | #1593, #1564 |

### New domains (no existing pup module)
| PR | Scope | SDK PRs | Note |
|---|---|---|---|
| `feat/annotations` | Annotations API v2 — new `pup annotations` command | #1631 | new module + main.rs domain enum |
| `feat/oauth2-clients` | OAuth2 client endpoints | #1664 | likely fits under `idp.rs` or new `oauth_clients.rs` |
| `feat/tag-descriptions` | path-param tag_description endpoints | #1624 | fits under `tags.rs` |
| `feat/storage-delete-sync-config` | DeleteSyncConfig for Storage Management | #1665 | confirm a pup home exists; skip if out of scope |

### Status pages / org / keys (changed shape or small adds)
| PR | Scope | SDK PRs |
|---|---|---|
| `feat/status-pages-backfilled` | surface `is_backfilled`; reflect published-page deprecation in help/docs | #1657, #1522 |
| `feat/org-groups-pagination` | adopt new `meta.page + links` pagination shape (if not already forced into PR-1 to compile) | #1547 |
| `feat/pat-sat-keys` | PAT/SAT api spec updates in `api_keys.rs`/`app_keys.rs` | #1668 |

---

## 6. Per-PR execution template

```
1. git checkout main && git pull origin main      # PR-1 must already be merged
2. git checkout -b <type>/<description>
3. Inspect upstream: read the generated src/datadogVN/api_<domain>.rs and
   model/* in the SDK for the new method/types. Match pup's existing pattern in
   the target command module.
4. Implement:
   - command fn in src/commands/<domain>.rs
   - if a new unstable v2 op: add to UNSTABLE_OPS in src/client.rs and bump
     test_unstable_ops_count
   - if API-key-only: add to OAUTH_EXCLUDED_ENDPOINTS and bump
     test_oauth_excluded_count
   - clap enum + dispatch arm in src/main.rs (with doc comments)
5. Tests: happy path + error/edge path via mockito (mirror existing tests in the
   module). Reuse src/test_support helpers; don't add new helpers if one exists.
6. docs/COMMANDS.md: document the new command/flags.
7. Validation gate (§1) + cargo audit.
8. Commit (conventional format) and open a focused, single-concern PR.
```

---

## 7. Risk notes

- **Pinning to `master` HEAD** means an unreleased, moving target. If CI/repo
  policy prefers tagged releases, pin v0.31.0 instead — PR breakdown is
  identical, only the unreleased-range items (most §5 rows) may not yet exist in
  the typed client and would need the raw-HTTP path or deferral.
- **Generated-type churn:** enum variant renames/field reshapes between v0.30.0
  and HEAD can break commands beyond the ones listed. PR-1's `cargo build` is
  the source of truth — handle whatever it surfaces.
- **WASM targets:** the SDK feature flags differ per target (`native`, `wasi`,
  `browser`). Keep new code behind existing `#[cfg(not(target_arch = "wasm32"))]`
  guards where it uses native-only paths, and build the `wasi` target before
  merging shared-code changes.
- **`cargo audit`:** the new rev may pull updated transitive deps; resolve any
  new advisories in PR-1.
```

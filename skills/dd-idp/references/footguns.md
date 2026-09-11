# UEG Query Footguns

Use this reference when a query fails, returns zero unexpectedly, or supports a high-stakes conclusion. Retry only after changing a concrete query assumption.

## Fast recovery checklist

1. Confirm one top-level `kind:<kind>` or `ref:"ref:<kind>:<id>"` is present.
2. Re-run `pup --read-only idp kinds describe <kind>` and compare every field and relation name.
3. Remove guessed relations, relation-absence checks, timestamp comparisons, CIDR syntax, broad negation, and unnecessary includes.
4. Start with a small positive query and minimal fields, then add one filter or relation at a time.
5. Check `warnings`, nulls, pagination, and relationship truncation before interpreting an empty or partial response.

## Syntax that silently misleads

### Quoted kind

```text
# wrong
kind:"service" AND owner:payments

# right
kind:service AND owner:payments
```

### Kind beneath top-level OR

```text
# wrong
kind:service OR owner:payments

# right
kind:service AND (owner:payments OR team:payments)
```

### Guessed relation

```text
# wrong
kind:service AND team.name:idp

# right only when describe declares owner_teams
kind:service AND owner_teams.name:idp
```

### Field passed to `--include`

`owner`, `contacts`, `links`, and `additional_owners` are commonly service attributes. Request them with `--field`. Use `--include` only for declared relations such as `owner_teams` or `systems`.

### Free-text pseudo-field

`free_text` is not a field. `--free-text-match partial|fuzzy` selects a mode but does not supply search text. Put text in a real filter such as `name:*catalog*`.

### Unsupported absence and advanced operators

Do not rely on `_missing_:owner_teams`, `-owner_teams.ref:*`, `NOT *`, CIDR expressions, or timestamp ranges such as `created_at:>...` and `last_seen:>now-30d`. Use positive declared fields and relations. Timestamp-typed fields do not support numeric/relative range comparisons.

## Result interpretation

### Relation samples are not inventories

An include may represent thousands of entities but return only a sample. Respect its `count` and `truncated` fields. Query the related kind directly rather than repeatedly raising `--relation-limit` for high-cardinality relations.

For team portfolios, query services directly with `kind:service AND owner:<team>` instead of expanding every `owned_services`, `users`, parent, and child relation together.

A service kind can expose dozens of relation types. For a fast one-step context query, request identity and aggregate health fields plus a small, relevant relation family. Add runtime dependencies, data stores, queues, deployments, operational objects, or security findings only when the question needs them.

### Null is unknown

Count and boolean fields may be null. Null does not mean zero, false, healthy, compliant, authenticated, or rate-limited. Distinguish “no matching rows,” “explicitly zero,” and “no data returned.”

### Time windows change meaning

Use `--timeseries-interval 168h`, not `7d`. Mention the window for incidents, monitor/SLO state, health, or other calculated fields.

## Known domain hazards

### Team contacts

Team entities primarily model membership and hierarchy. Service `contacts`, `links`, `owner`, `team`, and `additional_owners` often contain the useful channel/contact metadata. Query services owned by the team when team contact fields are absent.

### Kubernetes deployments

Never start with an unscoped `kind:integration.k8s.deployment`. Scope by `team` or `service`, request a small page, and paginate only for a defined inventory question.

### API posture

Boolean filters such as `endpoint_authenticated:false` can over-return or coexist with null data. Start from a scoped public-endpoint inventory, request the returned values, and post-filter before making a security claim. Live API endpoint data may encode schema-declared booleans as strings (`"true"` or `"false"`); accept only an explicit boolean/string value after normalization. Null or absent remains unknown.

### Pull requests

Scope every PR query with `repository.full_name` when the repository is known. An unscoped number can collide across repositories, while a broad author or reviewer query can exceed backend project-fanout limits:

```text
kind:integration.github.pull_request AND repository.full_name:"<org>/<repository>" AND number:<number>
```

Prefer `integration.github.repository` and `integration.github.pull_request`. Native `github.repository`, `github.pull_request`, and `github.commit` kinds may return backend errors. PR entities do not establish service impact unless the live schema exposes a real relation.

### Jira

For a human key such as `PAY-123`, use the `key` field rather than assuming `jira_issue_key` has the same value. Direct issue attributes are more reliable than expanding `assignee`, `reporter`, or `project`, which may fail server-side. Jira issues do not establish service impact without a declared relation.

### Scorecards and recommendations

Do not query deprecated `scorecard_outcome` or expand `scorecard_outcomes`; use service aggregate scorecard fields for high-level posture. `recommended_system` describes grouping recommendations and is not equivalent to entity-level `catalog_recommendation`, which may fail.

### Security and code findings

- Prefer repository-scoped finding queries. Monorepos make repository-to-service attribution incomplete.
- `associated_service` can behave like noisy free text; request it and verify returned rows rather than treating an exact match as authoritative.
- `library_vulnerability_secfinding.services` may be schema-present but empty. Zero matches on that field do not prove a service has no vulnerabilities.
- `source_code_vulnerability_secfinding.service_name` establishes only findings that were actually attributed; it does not prove full monorepo coverage.
- Map-typed fields such as some `tags` fields do not support wildcard/exists matching like strings or lists.

## Evidence labels

- **Authoritative:** directly returned attribute or declared relationship.
- **Inferred:** name/free-text/repository association that is plausible but not a declared edge; explain the basis.
- **Unavailable:** the live schema lacks the requested field or relation; say so rather than fabricating a join.

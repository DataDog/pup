---
name: dd-idp
description: Explore Datadog software, ownership, health, and work across services, teams, systems, repositories, pull requests, Jira, incidents, SLOs, monitors, on-call, APIs, scorecards, vulnerabilities, and security findings using Pup's read-only IDP entity graph.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/DataDog/pup
  tags: datadog,idp,entity-graph,service-catalog,ownership
---

# Datadog IDP Entity Graph

Use Pup's read-only Unified Entity Graph (UEG) to discover software, ownership, work, and operational context. Its main payoff is connected context in one bounded request: a service plus its owners, systems, code, dependencies, and health signals through declared relationships.

## Choose the right surface

- Default to `pup idp kinds` and `pup idp entities query` when the answer needs connected context across services, teams, systems, repositories, dependencies, work, operations, or security kinds.
- Use `pup idp assist` when the user values a fast, curated single-service summary, metadata gaps, and suggested next actions over graph fidelity; use `owner` for convenient owner/on-call resolution.
- Treat `find` as a simple legacy service-name lookup and `deps` as a production-only service-to-service dependency snapshot. Use UEG for explicit schema, relation families, counts, pagination, and traversal.
- Use product commands such as `pup incidents`, `pup slos`, `pup monitors`, `pup logs`, `pup traces`, or `pup security` when the user needs deeper or current telemetry.
- Use `pup service-catalog` for the legacy typed service registry and `pup software-catalog` for Catalog entity/kind reads and writes. Do not use graph queries for mutations.

## Highest-value workflows

- Build a service 360 in one request: owner/on-call, system and code placement, selected dependencies, and aggregate health.
- Estimate blast radius by following declared or runtime service, datastore, queue, external-provider, and inferred-service relations relevant to the question.
- Triage a team's portfolio from service health aggregates, then pivot to product APIs for detail.
- Add deployment or infrastructure relations to ownership and dependencies for change context.
- Inspect repository-linked APIs, code quality, secrets, and vulnerabilities without inventing service attribution.
- Discover integration and custom kinds schema-first for tenant-specific work rather than relying on a fixed kind catalog.

## Schema-first workflow

1. Discover candidate kinds when the request is broad or the entity vocabulary is unfamiliar:

   ```bash
   pup --read-only idp kinds list
   pup --read-only idp kinds list --all --include-custom
   ```

2. Describe each result kind before using unfamiliar fields or relations. Treat the live schema as authoritative:

   ```bash
   pup --read-only idp kinds describe service
   pup --read-only idp kinds describe integration.github.pull_request
   ```

3. Query exactly one top-level result kind. Select only needed attributes with `--field`; expand only the relation family needed for the question with `--include`:

   ```bash
   pup --read-only idp entities query 'kind:service AND name:"<service-name>"' \
     --field name,display_name,owner,service_health_status,active_incidents_count,alert_monitors_count,breached_slos_count \
     --include owner_teams,systems,code_locations,upstream_services,downstream_services \
     --relation-limit 3 \
     --timeseries-interval 24h \
     --limit 1
   ```

   This is the high-value default: identity, ownership, health, and declared service dependencies in one request. Add runtime, datastore, queue, deployment, or operational relations only when the question needs them; do not expand every service relation.

4. Inspect `warnings`, `page.truncated`, `page.next_cursor`, and every relationship's `count` and `truncated` state. Continue pages explicitly when completeness matters:

   ```bash
   pup --read-only idp entities query 'kind:service AND owner:"<team-handle>"' \
     --field name,owner \
     --cursor '<next_cursor>'
   ```

5. Follow returned refs instead of inventing joins:

   ```bash
   pup --read-only idp entities query 'ref:"ref:system:checkout"' \
     --field name,display_name,owner \
     --include services
   ```

6. Synthesize only what the returned fields and declared relations establish. Label noisy matches as inferred and missing edges as unavailable.

## Correctness rules

- Scope every query with one unquoted `kind:<kind>` or a concrete `ref:"ref:<kind>:<id>"`.
- Keep the kind/ref outside alternatives: `kind:service AND (owner:payments OR team:payments)`. A top-level `OR` is invalid.
- Never write `kind:"service"`; the quoted kind silently returns no results upstream and Pup rejects it.
- `--field` selects attributes. `--include` expands relations. Discover both with `kinds describe` rather than guessing.
- `--free-text-match` only chooses `partial` or `fuzzy` matching; search text still belongs in a real field filter such as `name:*catalog*`.
- Use Go-style lookbacks such as `1h`, `24h`, or `168h`; do not use `7d` for `--timeseries-interval`.
- Treat expanded relations as bounded samples. Increase `--relation-limit` or query the related kind directly when the full set matters.
- Treat `null` or absent counts/booleans as unknown, never as zero or false.
- Preserve source boundaries: UEG establishes graph facts; product APIs establish detailed operational facts.

## References

- Read [UEG DSL](references/ueg-dsl.md) when constructing or paginating a query, choosing flags, or following relations.
- Read [UEG footguns](references/footguns.md) when a query fails, unexpectedly returns zero, or touches timestamps, negation, high-cardinality relations, APIs, GitHub, Jira, scorecards, or security findings.
- Read [IDP recipes](references/recipes.md) for one-step service context, dependency and change impact, team health, repository, PR, Jira, incident, SLO, API, and security workflows.

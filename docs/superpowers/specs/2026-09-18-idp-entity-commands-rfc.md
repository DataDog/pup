# RFC: Kind-scoped IDP entity list and search commands

**Status:** Implemented  
**Date:** 2026-09-18  
**Author:** Andre Rodrigues  
**Owners:** `@DataDog/web-frameworks`

## TL;DR

In this document we are going to get a decision on adding two convenience commands to Pup's existing Unified Entity Graph (UEG) surface: `pup idp entities list --filter-kind=<kind>` and `pup idp entities search --filter-kind=<kind> --query=<expression>`. The commands preserve the existing full-power `pup idp entities query` interface while making the common single-kind workflow shorter and safer for users and agents.

## Context

Pup already includes the richer IDP entity-graph interface:

- `pup idp kinds list` and `pup idp kinds describe <kind>` for schema discovery.
- `pup idp entities query <expression>` for kind-scoped UEG queries, sparse fields, relation expansion, ordering, cursor pagination, total counts, and raw JSON:API output.

That interface requires callers to write a complete entity-graph DSL expression. In the common case, the caller already knows the desired kind and only wants to list it or apply a filter expression. Requiring the caller to repeat `kind:<kind>` is verbose, and incorrectly grouping Boolean expressions can let an `OR` escape the intended kind scope.

## Problem

- Listing one known kind requires writing `pup idp entities query 'kind:service'` instead of using Pup's familiar `list` shape.
- Searching one kind requires callers to remember to write `kind:<kind> AND (<filter>)` themselves.
- A caller who writes `kind:service AND owner:a OR owner:b` can accidentally change the query's effective scope instead of grouping the alternatives beneath `kind:service`.
- Agent callers benefit from explicit flags because they are easier to generate and validate reliably than free-form DSL strings.

## Goals

- Add `pup idp entities list --filter-kind=<kind>`.
- Add `pup idp entities search --filter-kind=<kind> --query=<expression>`.
- Reuse the existing entity-query execution path and all of its request options.
- Build `kind:<kind>` for list and `kind:<kind> AND (<query>)` for search.
- Validate the explicit kind before interpolating it into a query.
- Preserve `pup idp entities query`, `idp kinds`, `idp find`, and Software Catalog behavior.

## Non-goals

- Replacing the full entity-graph DSL command.
- Adding a new API endpoint or changing UEG response handling.
- Adding a separate count command; `--include-total-count` already exists.
- Automatic multi-page fetching.
- Facet exploration or aggregation in this change.

## Proposed interface

### List one kind

```bash
pup idp entities list --filter-kind=service

pup idp entities list \
  --filter-kind=integration.github.pull_request \
  --field=title,state,author \
  --order-by=updated_at:desc \
  --limit=25
```

This builds:

```text
kind:<filter-kind>
```

### Search one kind

```bash
pup idp entities search \
  --filter-kind=service \
  --query='owner:idp OR team:idp'
```

This builds:

```text
kind:<filter-kind> AND (<query>)
```

The parentheses are part of the contract: Boolean alternatives supplied through `--query` stay scoped to the selected kind.

## Shared options

Both convenience commands reuse the existing `idp entities query` options:

- `--field`
- `--include`
- `--order-by`
- `--limit`
- `--cursor`
- `--free-text-match`
- `--include-total-count`
- `--timeseries-interval`
- `--relation-limit`
- `--raw`

`pup idp entities query` remains the escape hatch for concrete `ref:` queries, complete DSL control, and any future query capabilities.

## Validation and safety

`--filter-kind` is an extensible identifier rather than a client-side enum. It accepts ASCII letters, numbers, `_`, `-`, and `.`, covering kinds such as `integration.github.pull_request` and `integration.k8s.deployment` while rejecting query-operator injection such as `service) OR (kind:repository`.

Blank search expressions are rejected locally. The resulting generated query is then passed through the existing `query_entities` normalization and validation, so the convenience commands share the same limits, top-level `OR` protection, relation validation, and API error behavior as `idp entities query`.

## Alternatives considered

### Keep only `idp entities query`

This avoids command growth but leaves the common one-kind workflow unnecessarily verbose and makes safe Boolean grouping the caller's responsibility. Rejected.

### Rename `query` to `search`

`query` is already established and supports more than the scoped convenience workflow. Renaming it would be a breaking change with no functional gain. Rejected.

### Add server kind counts in this PR

Live kind discovery already exists through `pup idp kinds list --all`, and richer schema inspection exists through `pup idp kinds describe`. Count-oriented aggregation can be considered separately if needed. Deferred.

## Testing

Add coverage for:

- Clap parsing and required flags for `list` and `search`.
- Generated agent-schema read-only metadata and `--filter-kind` exposure.
- Read-only command classification.
- List query generation.
- Search query generation with parenthesized Boolean alternatives.
- Dotted kind names.
- Blank kinds and filters.
- Kind strings containing query operators.
- Existing `idp entities query` parsing and behavior.

## Decision

Implement the two-command delta:

- `pup idp entities list`
- `pup idp entities search`

Both commands wrap the existing entity-query execution path and use `--filter-kind` for explicit kind selection.

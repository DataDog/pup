---
name: dd-idp
description: Answer questions about service and endpoint ownership, source code, dependency impact, team health and cost, vulnerability advisories, and Terraform drift using Pup's read-only Datadog entity graph.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/DataDog/pup
  tags: datadog,idp,entity-graph,service-catalog,ownership
---

# Datadog IDP Entity Graph

Use Pup's read-only Unified Entity Graph (UEG) to discover software, ownership, work, and operational context. Its main payoff is connected context in one bounded request: a service plus its owners, systems, code, dependencies, and health signals through declared relationships.

## Start with the user's question

Choose the closest recipe for the task.
Start from the identifier the user has and follow returned refs to connect it
to the answer.

| Question | Often useful to | Recipe |
| --- | --- | --- |
| Which service handles this endpoint, and who owns it? | On-call engineer | [Endpoint to service](references/recipes.md#which-service-handles-this-endpoint) |
| Where is this service's code, and whom should I contact? | Engineer joining a team | [Service context](references/recipes.md#where-is-this-services-code-and-who-owns-it) |
| Who is on call for this service now? | Incident lead | [On-call contact](references/recipes.md#who-is-on-call-for-this-service) |
| Which services depend on this service, database, queue, or provider? | SRE or service owner | [Caller impact](references/recipes.md#which-callers-could-be-affected-by-a-change) |
| What does our team own, and what needs attention? | Engineering lead | [Portfolio](references/recipes.md#which-services-does-our-team-own) and [health](references/recipes.md#which-services-need-reliability-attention) |
| Which services increased in cost, and where are savings suggested? | Engineering lead or FinOps partner | [Team cost](references/recipes.md#which-services-increased-in-cost) |
| Which services need ownership or standards cleanup? | Platform engineer | [Ownership gaps](references/recipes.md#which-services-are-missing-a-primary-owner) and [scorecard levels](references/recipes.md#which-services-are-below-our-scorecard-target) |
| Which public endpoints lack protections, and who owns them? | Security engineer | [API posture](references/recipes.md#which-public-endpoints-need-security-attention) |
| Which services and owners need to act on this vulnerability advisory? | Security engineer | [Advisory to owners](references/recipes.md#which-services-and-owners-are-affected-by-this-advisory) |
| Which Terraform workspaces have drift, and where is their configuration? | Platform engineer | [Terraform drift](references/recipes.md#which-terraform-workspaces-have-drift) |

For deployment, infrastructure, repository, PR, and Jira questions, use the
other [recipes](references/recipes.md). Discover unfamiliar integration or custom
kinds from the live schema. Connect resources only through returned relationships;
sharing a repository or a similar name does not establish service impact.

## Choose the right surface

- Default to `pup idp kinds` and `pup idp entities query` when the answer needs connected context across services, teams, systems, repositories, dependencies, work, operations, or security kinds.
- Use `pup idp assist` when the user values a fast, curated single-service summary, metadata gaps, and suggested next actions over graph fidelity; use `owner` for convenient owner/on-call resolution.
- Treat `find` as a simple paginated literal service-name lookup. Explicit `kind:` and `ref:` queries remain compatibility paths; use `entities query` for non-service kinds, another lookback, broader relation families, selected fields, counts, pagination, and traversal. Use `deps` as a convenient one-hour UEG runtime service-to-service dependency summary.
- Use product commands such as `pup incidents`, `pup slos`, `pup monitors`, `pup logs`, `pup traces`, or `pup security` when the user needs deeper or current telemetry.
- Use `idp entities facets` for observed field values and `idp entities aggregate` for grouped or filtered counts. Prefer these over paging an inventory just to count it.
- Use `pup service-catalog` for the legacy typed service registry and `pup software-catalog` for Catalog entity/kind reads and writes. Do not use graph queries for mutations.

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

3. Choose a query from the relevant recipe. Select only the attributes and
   relation family needed to answer the question. Use `--fields <kind>=...` to
   select related attributes. Follow returned refs when the next question needs
   another hop; avoid expanding every service relation into one large response.

4. Inspect warnings and result/relationship truncation. For inventories, use the
   recipe's explicit `--max-results` budget and check `page.stop_reason`. Continue
   with `next_request.args` when completeness is required; see
   [pagination](references/ueg-dsl.md#pagination-and-completeness).

5. Synthesize only what the returned fields and declared relations establish. Label noisy matches as inferred and missing edges as unavailable.

## Correctness rules

- Scope every query with one unquoted `kind:<kind>` or a concrete `ref:"ref:<kind>:<id>"`.
- Keep the kind/ref outside alternatives: `kind:service AND (owner:payments OR team:payments)`. A top-level `OR` is invalid.
- Never write `kind:"service"`; the quoted kind silently returns no results upstream and Pup rejects it.
- `--field` selects attributes. `--include` expands relations. Discover both with `kinds describe` rather than guessing.
- Use bare terms such as `kind:service AND catalog` with `--free-text-match partial`. The mode does not change field filters such as `name:*catalog*`.
- Use lookbacks such as `1h`, `24h`, or `7d` for `--timeseries-interval`. See the DSL reference for absolute windows and property scopes.
- Treat expanded relations as bounded samples. Increase `--relation-limit` or query the related kind directly when the full set matters.
- Treat `null` or absent counts/booleans as unknown, never as zero or false.
- Preserve source boundaries: UEG establishes graph facts; product APIs establish detailed operational facts.

## References

- Read [UEG DSL](references/ueg-dsl.md) when constructing or paginating a query, choosing flags, or following relations.
- Read [UEG footguns](references/footguns.md) when a query fails, unexpectedly returns zero, or touches timestamps, negation, high-cardinality relations, APIs, GitHub, Jira, scorecards, or security findings.
- Read [IDP recipes](references/recipes.md) for runnable queries answering the questions above and other integration workflows.

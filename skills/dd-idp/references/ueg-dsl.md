# UEG DSL and Pup Query Reference

Use this reference to construct, narrow, and paginate `pup idp entities query` calls. Inspect the result kind with `pup idp kinds describe <kind>` first; live fields, relations, and operators override examples here.

## Required result scope

Every query selects exactly one top-level kind or one concrete entity ref:

```bash
pup --read-only idp entities query 'kind:service'
pup --read-only idp entities query 'ref:"ref:service:checkout-api"'
```

Do not quote a kind value:

```text
# wrong
kind:"service"

# right
kind:service
```

## Attribute filters

Filter with fields declared by the live kind schema:

```text
kind:service AND owner:payments
kind:incident AND state:active
kind:api_endpoint AND endpoint_is_public:true
kind:service AND active_incidents_count:>0
kind:service AND name:*catalog*
kind:service AND _missing_:owner
```

Only use comparison, wildcard, missing, or negation syntax when the described field advertises a compatible operator and type.

## Relation filters and traversal

Relation names are schema-defined, not inferred from English. Describe the kind, then use the exact relation name:

```text
kind:service AND owner_teams.name:payments
kind:service AND code_locations.repository_id:"github.com/example/checkout-api"
```

Use `--include <relation>` to return a bounded relationship sample. Each normalized relationship includes `count`, `truncated`, and `sample`. Follow an entity from `sample[].ref` with a concrete-ref query, or query the target kind directly for complete enumeration.

## Boolean logic

Operators are uppercase. Parenthesize alternatives beneath a shared result scope:

```text
# valid
kind:service AND (owner:payments OR team:payments)

# invalid: ambiguous result scope
kind:service OR owner:payments
```

Use `NOT` or the compact negative form only when the live schema and a positive probe show the filter behaves as intended. Relation absence is not a reliable negative filter.

## Quoting and shell safety

Wrap the whole query in shell single quotes. Quote DSL values that contain spaces, punctuation, or refs with double quotes inside it:

```bash
pup --read-only idp entities query \
  'kind:integration.jira.issue AND key:"PAY-123"'
```

Simple identifiers can remain bare: `owner:payments`.

## Query controls

| Flag | Meaning |
| --- | --- |
| `--field name,owner` | Returned attributes; repeatable or comma-delimited |
| `--fields team=name,handle` | Override fields for an included kind; repeatable; applies to every entity of that kind |
| `--edge-fields runtime_downstream_services=requests_count,error_rate` | Select attributes of an included relationship, returned in `sample[].edge_fields` |
| `--include owner_teams` | Expanded declared relations; repeatable or comma-delimited |
| `--order-by field:asc` | Sort; repeatable or comma-delimited |
| `--limit 25` | Result page size, 1–100; default 25 |
| `--max-results 500` | Opt in to successive pages up to 1–10000 entities; conflicts with `--raw` |
| `--cursor <cursor>` | Continue the same query from `page.next_cursor` |
| `--relation-limit 25` | Client-side output sample cap, 1–100; default 25; does not limit backend expansion |
| `--timeseries-interval 24h` | Lookback for calculated time-window fields; default 1h |
| `--from <time> --to <time>` | Explicit measurement window; conflicts with relative lookback; accepts RFC3339, Unix timestamps, or Pup relative times |
| `--scope env=prod` | Scope supported properties of the result kind; repeatable; discover names in field scopes |
| `--include-total-count` | Request a total when the backend can provide one |
| `--free-text-match partial` | Mode for bare terms such as `kind:service AND catalog`; does not change field filters |
| `--raw` | Original JSON:API response; prefer normalized output for reasoning |

Request only the fields and relations needed to answer the question. Start with `--limit 25`; widen only after the query is proven useful.

For substring search across a kind's searchable text fields, use
`'kind:service AND owner:payments AND catalog' --free-text-match partial`.
For one named field, use `name:*catalog*`; the matching-mode flag does not turn
`name:catalog` into a substring filter. Fuzzy search can miss known entities when
combined with other filters on some deployments. Verify empty fuzzy results
with partial matching or an exact field query.

`--fields service=...` can also select the result kind; use it or `--field`, not
both for that kind. Projections are per kind, so a service-to-service include
shares the service field selection. Edge selections require the matching
`--include`. The current HTTP kind schema does not list edge fields; use verified
fields for the deployed API and check that the requested values were returned.

Pup validates explicit entity fields when live schema is available; typos fail
before running the entity query. Schema lookup failures remain recoverable and
produce a warning. `kinds describe` includes descriptions, display properties,
field scopes, and calculation details when supplied by UEG.

## Pagination and completeness

Normalized output contains:

```text
page.limit
page.truncated
page.next_cursor
next_request.args
relationships.<name>.count
relationships.<name>.truncated
relationships.<name>.sample
warnings
server_warnings
```

When `page.truncated` is true and the user needs complete coverage, repeat the identical query and flags with the returned cursor. Do not claim completeness until no next cursor remains. A complete result page does not make a truncated relationship sample complete; query that related kind directly when necessary.

For entity queries, `next_request.args` is a complete argument array after the
`pup` executable. Run it as argv without shell interpolation. It preserves the
effective projections, ordering, matching mode, measurement window, scopes,
org profile, and cursor. Keep the same authentication, `DD_SITE`, and config environment;
credentials are never embedded in the arguments. The query echo records absolute
times as Unix milliseconds. Pagination
does not provide an atomic snapshot of a changing graph.

Entity query output also reports `page.pages_fetched` and `page.stop_reason`:
`end_of_results`, `page_limit` (manual mode), or `result_limit` (automatic mode).
`--max-results` bounds entities across requests; `--limit` remains the maximum
size of each request. Pup reduces the last page size to fit the remaining budget
without discarding rows. A final empty page is valid. Repeated cursors, empty
nonterminal pages, and later request failures exit unsuccessfully without
presenting a successful partial inventory. Subsequent pages' server warnings
appear in `additional_page_warnings` with their page numbers; `server_warnings`
retains the first page's warnings. Neither paging mode provides snapshot isolation.

`count` on a relationship counts references returned by the server; `truncated`
reports whether Pup shortened that returned sample. It does not prove upstream
completeness. Explicit empty relations retain a zero count. A requested relation
that was omitted or could not be decoded produces a warning; do not interpret it
as no relationships. Preserve any `server_warnings` when explaining limitations.

Returned attributes remain in `fields`, including distinct `name` and
`display_name` values. Relationship samples retain per-edge measurements in
`edge_fields` when supplied by UEG; those belong to the connection, not the entity.

## Time windows and nulls

Some fields are calculated over `--timeseries-interval`. Use durations such as
`1h`, `24h`, `168h`, or `7d`; Pup converts day/week lookbacks to a compatible API
duration. State the window when it affects meaning. `--from`/`--to` affect
supported measurements, not every entity attribute or historical entity state.

Use `--scope` only for schema-declared scopes on the result kind's properties.
An environment property scope does not isolate current runtime dependency edge
aggregates, which are org-wide. Do not present those edges as verified evidence
for the requested environment.

Treat `null` or a missing field as unknown/not returned. Only report zero, false, healthy, or no findings when the API explicitly returned evidence for that claim.

## Output modes

Normalized JSON is the reasoning contract. Agent mode may wrap it in Pup's `{status,data,metadata}` envelope. Global `--jq` expressions target the command payload before envelope formatting. Use `--raw` only to debug the server contract or access fields the normalized representation intentionally omits.

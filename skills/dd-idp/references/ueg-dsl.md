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
| `--include owner_teams` | Expanded declared relations; repeatable or comma-delimited |
| `--order-by field:asc` | Sort; repeatable or comma-delimited |
| `--limit 25` | Result page size, 1–100; default 25 |
| `--cursor <cursor>` | Continue the same query from `page.next_cursor` |
| `--relation-limit 25` | Per-relation sample cap, 1–100; default 25 |
| `--timeseries-interval 24h` | Lookback for calculated time-window fields; default 1h |
| `--include-total-count` | Request a total when the backend can provide one |
| `--free-text-match partial` | Matching mode only; search text remains in a real field filter |
| `--raw` | Original JSON:API response; prefer normalized output for reasoning |

Request only the fields and relations needed to answer the question. Start with `--limit 25`; widen only after the query is proven useful.

## Pagination and completeness

Normalized output contains:

```text
page.limit
page.truncated
page.next_cursor
relationships.<name>.count
relationships.<name>.truncated
relationships.<name>.sample
warnings
```

When `page.truncated` is true and the user needs complete coverage, repeat the identical query and flags with the returned cursor. Do not claim completeness until no next cursor remains. A complete result page does not make a truncated relationship sample complete; query that related kind directly when necessary.

## Time windows and nulls

Some aggregate fields are calculated over `--timeseries-interval`. Use Go durations such as `1h`, `24h`, and `168h`, and state the window in the answer when it affects meaning.

Treat `null` or a missing field as unknown/not returned. Only report zero, false, healthy, or no findings when the API explicitly returned evidence for that claim.

## Output modes

Normalized JSON is the reasoning contract. Agent mode may wrap it in Pup's `{status,data,metadata}` envelope. Global `--jq` expressions target the command payload before envelope formatting. Use `--raw` only to debug the server contract or access fields the normalized representation intentionally omits.

# Trends and values

Choose from user intent:

- line chart or trend: `timeseries` with line display
- area chart: `timeseries` with area display
- bars over time: `timeseries` with bar display
- categorical bars: `bar_chart`
- single headline value: `query_value`
- ranked groups: `toplist`
- detailed grouped values: `query_table`
- period-over-period difference: `change`

After choosing one type, read its exact reference from
[the widget type index](../widget-types.md).

First read [handbook.md](handbook.md). Follow only the selected
type reference's request model, required fields, response formats, and enums. Do not
infer that all query widgets share the same shape.

## Preserve each query family's contract

| Type | Required intent |
| --- | --- |
| `timeseries` | use a timeseries response; preserve one formula and display style for each requested series |
| `bar_chart` | use scalar category values; put the requested category on the x-axis |
| `query_value` | use a scalar response and one displayed formula; preserve comparisons and conditional formats |
| `query_table` | use scalar rows; preserve group order, cell display modes, sorting, and row count |
| `toplist` | use scalar ranked groups; preserve count, direction, stacked display, and scaling |
| `change` | preserve current and shifted formulas; set whether an increase is good or bad |

Do not collapse two requested signals into one query. Do not invent a second
series when the user asked for one value.

For mixed timeseries displays, keep the requested line, area, or bar style on
the matching formula. Set `on_right_yaxis` only for the requested right-axis
series. Preserve `include_zero`, unit metadata, and a cell-level time override
only when the user requests them.

## Rank toplists with request sorting

For a metric toplist, use the modern scalar request. Keep the raw metric query
free of `top()`. Set the result count and direction in `sort`:

```json
{
  "type": "toplist",
  "requests": [
    {
      "queries": [
        {
          "data_source": "metrics",
          "name": "query1",
          "query": "sum:trace.http.request.errors{*} by {service}.as_count()",
          "aggregator": "sum"
        }
      ],
      "formulas": [{"formula": "query1"}],
      "response_format": "scalar",
      "sort": {
        "order_by": [{"type": "formula", "index": 0, "order": "desc"}],
        "count": 10
      }
    }
  ]
}
```

Change the metric, grouping, count, and order to match the request. Do not put
the ranking operation inside `queries[].query`. A formula-level
`"limit": {"count": 10, "order": "desc"}` is equally valid; use one or the other,
not both.

## Bound each event group separately

Request `sort` limits the number of returned rows. It does not bound an individual
facet. When the user asks for a bounded breakdown across several event facets, give
each facet its own `limit` and use the list form of `group_by`:

```json
"group_by": [
  {"facet": "service", "limit": 10, "sort": {"aggregation": "count", "order": "desc"}},
  {"facet": "env", "limit": 10, "sort": {"aggregation": "count", "order": "desc"}},
  {"facet": "host", "limit": 10, "sort": {"aggregation": "count", "order": "desc"}}
]
```

The object form, `"group_by": {"fields": ["service", "env", "host"]}`, carries no
per-facet limit. Prefer the list form whenever a limit is requested.

## Use template variables directly

Inside metric scopes and event filters, use a template variable as a complete
token. For an `env` variable, write `$env`. Do not add another tag key before
it. Both `env:$env` and `environment:$env` are wrong.

Example:

```text
avg:trace.http.request{service:checkout,$env}
```

For a direct metric or event query, use the widget's own request schema. Use a
notebook `local_dataset` request only when the user needs derived notebook data
and the chosen type appears in [local-datasets.md](local-datasets.md).

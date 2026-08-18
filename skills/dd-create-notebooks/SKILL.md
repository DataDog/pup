---
name: dd-create-notebooks
description: Create and modify Datadog notebooks with Pup. Use for notebook prose, Mermaid, code, analysis, diagrams, visualizations, widget selection, notebook envelopes, and safe create, update, or edit operations.
---

# Create Datadog notebooks

Choose the smallest notebook structure that satisfies the user's request. Keep
notebook-only cells separate from reusable widgets.

## Choose a component

Use a notebook-only cell for authored or computational content:

- prose, headings, notes, code blocks, or Mermaid: `markdown`
- raw data for later SQL, transformation, calculation, or reuse: `analysis_data_source`
- SQL over earlier notebook data: `analysis_sql`
- structured transformation over earlier notebook data: `analysis_transformation`
- general code: `python`
- Excalidraw canvas: `diagram`
- uploaded document: `html`
- alternate query text: `ddql`

Read [notebook-cells.md](references/notebook-cells.md) only when the request
needs one of these notebook-only types.

Every requested description, explanation, summary, note, instruction, or other
prose block must be stored in a `markdown` cell. Notebook attributes do not
support a top-level `description` field. Never replace requested prose with an
empty `cells` array.

Use a reusable widget when the user wants telemetry displayed directly. Pick
the user's intent, then read only that widget family:

- recent logs, spans, RUM, security, CI, or other events: [streams.md](references/widgets/streams.md)
- line charts, trends, headline values, rankings, tables, or change: [queries.md](references/widgets/queries.md)
- histograms, heatmaps, maps, correlation, or part-to-whole views: [analytical.md](references/widgets/analytical.md)
- monitors, checks, SLOs, services, or topology: [status.md](references/widgets/status.md)
- images, web content, or workflow actions: [content.md](references/widgets/content.md)

The complete [widget type index](references/widget-types.md) links directly to
every exact type reference. Use it when the family is already clear or a family
page selects a type.

Read [surface-matrix.md](references/surface-matrix.md) only when surface support
is unclear or the requested type is absent from these buckets.

For a top-N or ranked-group request, choose `toplist`, not `query_table`.
Preserve the requested metric, scope, and grouping. Put the result count and
direction in the toplist request's `sort`, or in the formula's `limit`; both are
valid and the handbook shows each. Keep ranking out of the raw query string: use
`top()` in a formula if you need it, never inside `queries[].query`.

## Verify every identifier before you write it

Metric names, tag keys, log facets, and monitor IDs are the largest single source
of wrong widgets. A plausible name is often a real metric that is not the one the
user meant, so existence alone is not enough. Look it up:

```bash
pup metrics list --filter "aws.s3.*"                        # find the real metric name
pup metrics metadata get aws.s3.all_requests                # description, to disambiguate
pup metrics tags list aws.s3.all_requests --keys-only       # exact tag keys, e.g. bucketname
pup monitors list --name "[team] Deployment is stale"       # monitor name to id
```

Never invent a metric, tag, facet, monitor, SLO, or workflow identifier. If a
lookup returns nothing, say so rather than substituting a near miss.

Those are the exact invocations. Two of them matter:

- **Always pass `--keys-only` to `pup metrics tags list`.** Without it the command
  returns every tag key paired with every value it has ever seen — 11 MB for
  `trace.http.request.hits`, which will not fit in context. You need the key names,
  not the value space.
- **Make `--name` as specific as you can**, including any bracketed prefix. A loose
  monitor name matches dozens and returns kilobytes of them.

`pup metrics tags list METRIC` needs the `list` subcommand. For any other domain,
check `pup <domain> --help` rather than guessing a flag.

For log and span attributes, structured attributes take an `@` prefix and tags do
not: `@http.status_code`, not `http.status_code`. The handbook covers this.

## Inspect the selected widget

After selecting exactly one widget type, read
[handbook.md](references/widgets/handbook.md) before writing any telemetry
request. It is generated from the same widget reference Ask Widget Expert uses and
carries the request decomposition, query, formula, sort, timeframe, unit, palette,
and title rules that every type shares.

Then read the selected type through the
[widget type index](references/widget-types.md). Its reference carries the
supported data sources, rules, warnings, and examples, and tells you how to fetch
the schema. Read one exact type reference rather than inspecting nearby types.

Fetch the schema from Pup rather than expecting it in the reference. Choose the
data source first, then ask for the layer you need:

```bash
pup widgets schema TYPE --surface notebooks --data-source SOURCE --section request
```

That is the definition, its request model, and the query and formula types. Add
`--section presentation` when the request asks for styling, palettes, units, axes,
markers, conditional formats, cell display, or a widget-level timeframe. Add
`--section local-dataset` for a visualization over an earlier analysis cell.

Omitting `--data-source` returns every source's query types, and omitting
`--section` returns every styling type; together that is about eight times larger
than you need. The output ends by listing the layers it left out and the flag that
returns each one, so if a field you need is absent, fetch that layer rather than
guessing at the shape.

Use only fields present in the schema you fetched. Do not copy `requests`, query
shapes, response formats, or local-dataset behavior from another widget. Wrap the
definition in a notebook cell. Do not add dashboard `layout` or widget identity
fields.

If any tool result ends with a truncation notice, the rest of the content is
missing. Read the remainder before writing the definition.

## Wrap definitions for notebooks

Wrap every notebook-only definition or widget definition in one cell envelope:

```json
{
  "type": "notebook_cells",
  "attributes": {
    "definition": {
      "type": "WIDGET_OR_CELL_TYPE"
    }
  }
}
```

Do not nest a `notebook_cells` envelope inside `attributes.definition`.

## Use analysis only when required

Do not create analysis cells merely to display existing telemetry. Use an
analysis chain only when the user needs SQL, transformation, calculated or
derived fields, joins, reusable intermediate data, or a visualization over
derived notebook data.

Only the workbook widgets listed in [local-datasets.md](references/widgets/local-datasets.md)
accept notebook `local_dataset` requests. Read that page only after choosing an
analysis workflow. Stream widgets do not consume local datasets.

## Build the notebook

A create request needs `name`, `time`, and `cells`:

```json
{
  "data": {
    "type": "notebooks",
    "attributes": {
      "name": "Investigation",
      "time": {"live_span": "4h"},
      "cells": []
    }
  }
}
```

Preserve explicit notebook intent:

- investigation: `"metadata": {"type": "investigation"}`
- postmortem: `"metadata": {"type": "postmortem"}`
- runbook: `"metadata": {"type": "runbook"}`

When the user calls the notebook an investigation, postmortem, or runbook,
`metadata.type` is required. Do not omit it because the rest of the notebook is
simple.

Use exactly one notebook-level time form:

```json
{"time": {"live_span": "1h"}}
```

```json
{"time": {"start": "2026-03-15T14:00:00Z", "end": "2026-03-15T16:00:00Z"}}
```

Widget-level `time` is a separate decision, and the trigger is what the request
names, not how strongly it is worded:

- The request names a timeframe — "for the last 15 minutes", "over the past hour",
  "yesterday", "between 2pm and 4pm". Set `time` at the widget definition root, as
  a sibling of `type` and `requests`.
- The request names no timeframe — "over time", "request rate", "show me errors".
  Omit widget-level `time` so the widget inherits the notebook's timeframe.

Never put `time` on an individual request. For the accepted widget-level shapes,
including calendar-aligned spans for "yesterday" or "last month", see the
timeframe section of [handbook.md](references/widgets/handbook.md).

Create with:

```bash
pup notebooks create --file notebook.json
```

## Modify safely

Inspect the notebook before modifying it:

```bash
pup notebooks get NOTEBOOK_ID
```

To add content, write an array containing only new cell envelopes:

```bash
pup notebooks edit NOTEBOOK_ID --file cells.json
```

Do not include existing cells. Pup preserves them before appending the new
entries.

For a template variable, prefer the singular write shape:

```json
"template_variables": [
  {
    "name": "env",
    "default": "prod"
  }
]
```

A fetched notebook may instead contain a sequence under `defaults`:

```json
"template_variables": [
  {
    "name": "env",
    "defaults": ["prod"]
  }
]
```

Preserve either accepted shape when it already exists. Never write an object
under `defaults`; `"defaults": {"value": "prod"}` is invalid.

When a widget query uses a template variable, insert it as a complete token.
For an `env` variable, use `$env`. Never prefix it with another tag key. Both
`env:$env` and `environment:$env` are wrong.

`pup notebooks update` is a full replacement. Fetch first and preserve every
unspecified field and cell.

Never delete or recreate a target to recover from a create, update, or edit
error. Apply the smallest correction supported by the backend error.

## Verify

Parse the JSON before mutation. After mutation, inspect stored `name`,
`metadata`, `time`, tags, template variables, cell count, cell order, and
definitions. A zero exit code alone does not prove the requested result.

Preserve the requested content and component count. Do not add explanatory,
analysis, or visualization cells the user did not need.

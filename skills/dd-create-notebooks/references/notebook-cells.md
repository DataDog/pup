# Notebook-only cell schemas

These examples show writable API definitions, not stored editor state. Put each definition inside a `notebook_cells` envelope.

## Notebook creation envelope

The current Pup SDK requires `name`, `time`, and `cells`:

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

Optional modeled attributes are `metadata`, `status`, and `template_variables`.
If present, `status` is currently `published`. Do not add undocumented
top-level attributes merely because deserialization preserves unknown fields.
There is no writable notebook-level `description` attribute. Store all user
prose in one or more `markdown` cells.

Every displayed or written JSON document must parse. Encode multiline Markdown,
Mermaid, SQL, and Python strings with `\n` escapes, or construct the document
with a JSON serializer. Never place raw line breaks inside a quoted JSON string.

## Shared rules

- Generated cell IDs, when required, are exactly eight lowercase letters or digits: `^[a-z0-9]{8}$`.
- Analysis names are unique SQL identifiers matching `^[a-z_][a-z0-9_]*$`.
- Place dataset producers before cells that reference them.
- Validate the complete ordered definition array. The backend resolves cross-cell references and runtime-checks SQL and transformations as a batch.
- Optional `collapsed` and `editor_collapsed` fields are booleans on analysis definitions.

## Markdown and Mermaid

Exact writable definition:

```json
{
  "type": "markdown",
  "text": "# Heading\n\nNarrative text"
}
```

`type` and `text` are required. Markdown text is limited to the backend maximum characters per cell. Optional comment anchors are closed objects with `anchor_offset`, `anchor_length`, and a UUID `comment_uuid`.

Mermaid is a fenced block in the Markdown text:

````json
{
  "type": "markdown",
  "text": "```mermaid\ngraph TD\n  A --> B\n```"
}
````

The API converts ordinary Markdown to rich-text storage but deliberately preserves Mermaid blocks as Markdown. Never emit `type: "mermaid"`.

## Analysis data source

Common event-platform definition:

```json
{
  "type": "analysis_data_source",
  "query": {
    "data_source": "logs",
    "name": "error_logs",
    "columns": [
      {"column": "timestamp", "type": "timestamp"},
      {"column": "service", "type": "string"},
      {"column": "message", "type": "string"}
    ],
    "search": {"query": "service:PLACEHOLDER status:error"},
    "indexes": []
  }
}
```

Required query fields are `data_source`, `name`, and `columns`. Each column requires `column` and `type`; `alias` is optional. Column types are `string`, `int64`, `float64`, `bool`, `timestamp`, or arrays of those types.

Event-platform `data_source` values:

`audit`, `ci_pipelines`, `ci_tests`, `database_queries`, `errors`, `events`, `llm_observability`, `logs`, `monitors`, `monitor_groups`, `network`, `network_device_flows`, `product_analytics`, `rum`, `security_signals`, `spans`, `synthetics_test_runs`.

Optional fields include `search: {query}`, `indexes`, `storage`, and `time_window: {from,to}` using integer timestamps. Omit `search` for match-all rather than sending `"*"`.

Other supported dataset shapes include:

- Reference data: `data_source: "reference_table"|"managed_resource"`, with `name` and `table_name`.
- Formula data: `data_source: "formula"`, with `name`, `response_format`, non-empty `queries`, and non-empty `formulas`.
- Published analysis: `data_source: "published_analysis"`, with `name` and `dataset_id`.
- Snowflake: `data_source: "snowflake_query"`, with `name` and `sql_query`.

Do not use deprecated `metric_scalar` for new work. Prefer a direct metric widget or formula dataset.

## Analysis SQL

Exact definition shape:

```json
{
  "type": "analysis_sql",
  "query": {
    "data_source": "analysis_dataset",
    "name": "errors_by_service",
    "query": {
      "type": "sql_analysis",
      "sql_query": "SELECT service, COUNT(*) AS error_count FROM error_logs GROUP BY service ORDER BY error_count DESC"
    }
  }
}
```

All shown fields are required. The SQL references earlier analysis names as tables. Use DDSQL, not an assumed database dialect. Keep the definition title, when supplied in a surrounding tool workflow, aligned with `query.name`.

## Analysis transformation

Exact outer shape:

```json
{
  "type": "analysis_transformation",
  "query": {
    "data_source": "analysis_dataset",
    "name": "parsed_logs",
    "query": {
      "type": "structured_analysis",
      "source_dataset": "error_logs",
      "transformations": [
        {
          "type": "grok",
          "source_column": "message",
          "grok_expression": "PLACEHOLDER",
          "keep_nulls": true
        }
      ]
    }
  }
}
```

`structured_analysis` requires `source_dataset` and `transformations`. Supported transformation discriminants are:

- `filter`: optional string `filter`.
- `aggregation`: optional `group_by`, `compute`, `sort`, `limit`, and `time_aggregation`.
- `join`: requires `join_type`; may specify `target_dataset`, `target_columns`, and `join_conditions`.
- `limit`: requires integer `limit`.
- `calculated_field`: supports `name` and `expression`.
- `cast_column_type`: requires `source_column`, `target_type`, and `target_column`; target is `string`, `int64`, `float64`, or `timestamp`.
- `grok`: supports `source_column`, `grok_expression`, and `keep_nulls`.
- `projection`: requires a non-empty `columns` array of `{column, alias?}`.
- `sort`: requires `column` and `order: "asc"|"desc"`.

Inspect representative upstream rows before inventing a GROK expression.

## Python

Exact schema:

```json
{
  "type": "python",
  "code": "print('hello')",
  "runtime_version": "PLACEHOLDER",
  "datasource_query_limit": 1000
}
```

`type`, `code`, and `runtime_version` are required. `datasource_query_limit` is optional and ranges from 1 to 100000. Do not guess a runtime version; preserve a known-good version or obtain it from current notebook state/product guidance.

## Excalidraw diagram

Cell schema:

```json
{
  "type": "diagram",
  "content_url": "/api/v2/files/FILE_UUID"
}
```

`type` is the only schema-required field, but a useful diagram needs `content_url`. The URL points to uploaded Excalidraw scene JSON. Creation requires the feature-gated file lifecycle: request a signed JSON upload, upload the scene, mark upload status, then create the cell. Pup does not currently expose that workflow.

## Uploaded HTML

```json
{
  "type": "html",
  "content_url": "/api/v2/files/FILE_UUID"
}
```

`content_url`, when present, must be a relative path beginning with one `/`. This cell is feature-gated and likewise requires an upload lifecycle not currently exposed by Pup.

## DDQL

```json
{
  "type": "ddql",
  "text": "PLACEHOLDER",
  "format": "table"
}
```

`type` and `text` are required. Optional `format` is `auto`, `timeseries`, or `table`. Treat the type as conditional: it is feature-gated and absent from the primary notebook allowlist despite being present in the notebook validator map.

# Stream widgets

Use stream widgets to show recent raw events directly.

## Choose a type

- matching logs or a live log flow: `log_stream`
- recent logs, spans, RUM, security, CI, or other event records: `list_stream`

## Log stream

Read [`log_stream`](../widget-types/log_stream.md).

Use direct fields such as `query`, `indexes`, `columns`, and `time` exactly as
the schema permits.

`log_stream` does not accept `requests` and does not consume a `local_dataset`.
Do not add `analysis_data_source` merely to show matching logs.

## List stream

Read [`list_stream`](../widget-types/list_stream.md).

Choose the stream source from the event type:

| Event records | Data source |
| --- | --- |
| logs | `logs_stream` |
| spans | `trace_stream` |
| RUM | `rum_stream` |

`list_stream` uses exactly one event request. Put the filter in
`query.query_string` and use `response_format: "event_list"`. For a log list,
use ordered column objects with `field` and `width`. Put `status_line` first,
then `timestamp`, followed by useful fields such as `host` and `content`.

Preserve source-specific filters. RUM errors need `rum_stream` and an error
filter such as `@type:error`. Do not add logs-only fields to a RUM request.
Do not copy the direct `log_stream` shape or use a notebook `local_dataset`
request.

# Widget construction handbook

Read this before writing any telemetry request. It is generated from the Datadog
graphing widget reference, so it is the same guidance Ask Widget Expert receives.

Two notebook adjustments apply throughout, and they override the general text
below wherever they disagree.

**Envelopes.** Wrap every definition in a `notebook_cells` envelope. Never add
dashboard `layout`, widget identity fields, or `group` widgets.

**Timeframes.** "Setting the Widget Timeframe" below opens with "leave `time`
undefined so the widget inherits the page's global timeframe selector". That is
the default only when the request names no timeframe. Decide from what the
request actually names:

- Names a timeframe — "for the last 15 minutes", "over the past hour",
  "yesterday", "between 2pm and 4pm". Set `time` at the widget definition root.
  Omitting it is wrong.
- Names none — "over time", "request rate", "show me errors". Omit widget-level
  `time` and let the notebook's own `time` apply.

Both the modern `{type, value, unit}` and the legacy `{live_span}` shapes are
accepted. Never put `time` on an individual request.


## Widget Schema Structure

A Datadog widget definition is a JSON spec containing data requests and visual configuration, containing:
- `type`: The widget type
- `title`: Display title for the widget
- `requests`: Array of request objects defining datadog queries (commonly formula requests)
  - Each formula request contains `queries` (data source definitions) and `formulas` (expressions referencing queries by name).
- Config fields: widget-specific visual/style options (style, legend, yaxis, markers, etc.)

Note: Some widget types do NOT use the formula request pattern:
- Config-dependent widgets (alert_graph, alert_value, slo, check_status, etc.) require specific IDs/config instead of queries.
- Static widgets (free_text, note, image, iframe) have no requests at all — they are purely configuration-based.
- Specialized widgets (hostmap, scatterplot, servicemap, etc.) have their own request structures — refer to the schema.

Refer to the schema for full type structure.

## Building Widgets

### Request Decomposition

Determine whether the request needs a single query or multiple queries with formulas, and verify the widget_type supports it. If not, seek clarification.

Guidelines:
* Single query or single formula: (most common) requests using grouping/filtering or single functions
  - "error logs" → 1 query; "CPU usage by host" → 1 query with group_by
  - "Compare memory across prod and staging" → single query with same dimension (env) groupby, NOT multiple queries
  - Boolean filters (service:A OR/AND service:B) for union/intersection — this won't preserve per-tag proportions, use multiple queries or groupings if that's the intent.

* Multiple queries with formulas (only if widget_type supports it): Decompose when multiple queries or formulas better serve the user's intent.
  - Arithmetic: "% of logs that are errors" → 2 queries, formulas: ["query1 / query2 * 100"], "latency on service X and browser Y" → 2 queries with different filters
  - Separate series: "latency and request count together" → formulas: ["query1", "query2"] (requires timeseries/query_table)
  - Prefer for same-signal comparison across conditions (e.g. filterA vs filterB as stacked bars/areas, i.e. can't use boolean ops)

* Time-based comparisons (month-over-month, WoW, etc): ALWAYS use calendar_shift/hour_before functions, not multiple queries.

### Query Building

Build queries following the widget schema. Key patterns by data_source:
* metrics: `query` field with metrics syntax (e.g., "avg:system.cpu.user{*} by {host}")
* events (logs/rum/etc): `search.query` for filtering, `compute` for aggregation, `group_by` for dimensions
* apm_metrics: preferred for APM hits/errors/latency queries. Supports `resource_name`, `service`, `operation_name`, `query_filter` (tag-based filtering), `group_by`, and a richer stat set including `apdex`, `hits_per_second`, `p999`, `total_time`.

### Setting the Widget Timeframe

1. When creating widgets for a dashboard or notebook, leave `time` undefined so the widget inherits the page's global timeframe selector.

2. If the user's prompt requires a timeframe, use one of:

- LiveSpan: Rolling window (e.g. "past 5 minutes", "last 2 weeks")
  → Preferred modern shape: `{"type": "live", "value": N, "unit": "minute|hour|day|week|month"}`
  → Legacy dashboard shape also works when encountered or editing existing widgets: `{"live_span": "15m"}`

- CalendarAlignedSpan: (e.g. "yesterday", "2 months ago", "last month", "month to date")
  → `{"type": "daily|weekly|monthly", "offset": N}` (0=current, 1=previous)

- FixedSpan: Explicit start AND end timestamps (e.g. "from Jan 1 to Jan 2 2024")
  → `{"type": "fixed", "from": 1712080128000, "to": 1712083128000}` (milliseconds since epoch, NOT RFC3339 strings)

When the user explicitly asks for a timeframe, set `time` at the widget root as a sibling of `type`, `title`, and `requests`. Do NOT put time on individual requests, and do NOT omit root `time` in that case.

"last N" ambiguity: N=1 → CalendarAlignedSpan, N>1 → LiveSpan

3. Retain existing widget `time` if user has no explicit timeframe, when editing.

4. Outside dashboards/notebooks, if the active page URL has timeframe params (from_ts/to_ts), use those.

5. If no timeframe is available from user, page, or widget, default to last 1 day.

### Formula Syntax

* A formula is a string expression referencing queries by name, with optional functions applied
* Query references: "query1", "query2", etc. (1-indexed, matches order in queries array)
* Only reference queries that exist - e.g. query3 requires queries[2] to exist
* Operators: +, -, *, /
* Formula types:
  - Simple reference: "query1" (just display the query as-is)
  - Arithmetic: "query1 / query2 * 100" or "2 * query1" or "query1 - query2"
  - With functions: "cumsum(query1)" or "abs(query1 - query2)"
* Multiple formulas: Return array of formula strings when user wants multiple formulas
  - e.g. ["0.1 * query1", "query2"] displays two separate queries as formulas
  - If any query is included in a formula, all queries must be in a formula so they are not hidden
* Empty formulas: Return [] when no formulas needed (single query cases)
* Functions MUST be applied in the `formula` field and not directly to raw query strings, the ONLY exceptions are
  simple `.rollup()` and metric type modifiers: .as_count(), .weighted() and .as_rate(), which are appended to the end of the metric query string if needed.
  e.g. "avg:system.cpu.user{*} by {env}.rollup(avg)"

Available Functions (applied in the formula field to query_name references like "query1"):

[Detect and highlight unusual metric values]
anomalies(query_name: str, algorithm: "basic" | "agile" | "robust", bounds: int | float), outliers(query_name: str, algorithm: "DBSCAN" | "MAD" | "scaledDBSCAN" | "scaledMAD", tolerance: int | float, pct: int | float), forecast(query_name: str, algorithm: "linear" | "seasonal", deviations: int | float)
e.g. anomalies(query1, "robust", 2), outliers(query1, "DBSCAN", 3.0, 10), forecast(query1, "linear", 1)

[Apply mathematical functions]
abs(query_name: str), log2(query_name: str), log10(query_name: str), cumsum(query_name: str), integral(query_name: str), pow(base: a query_name or constant, exponent: a query_name or constant)
e.g. pow(query1, 2), pow(1.1, query1), pow(query1, query2)

[Count the number of non-null or non-zero values]
e.g. count_nonzero(query1), count_not_null(query2)

[Exclude null or zero values]
exclude_null(query_name: str), cutoff_max(query_name: str, threshold: int | float), cutoff_min(query_name: str, threshold: int | float), clamp_max(query_name: str, threshold: int | float), clamp_min(query_name: str, threshold: int | float)
e.g. cutoff_max(query1, 100), clamp_min(query1, 0)

[Add a default value to sparse metrics]
e.g. default_zero(query1)

[Graph the top or bottom N objects for this metric]
top(query_name: str, limit: int | float, by: "mean" | "min" | "max" | "area" | "l2norm" | "last" | "sum", dir: "desc" | "asc")
e.g. top(query1, 10, "mean", "asc"), top(query1, 5, "max", "desc")

[Graph the rate at which the metric is changing]
per_second(query_name: str), per_minute(query_name: str), per_hour(query_name: str), dt(query_name: str), diff(query_name: str), monotonic_diff(query_name: str), derivative(query_name: str), throughput(query_name: str)

[Fit a trend line for the metric data. Use when user asks to see 'trend', 'direction', 'pattern', or 'line of best fit']
robust_trend(query_name: str), trend_line(query_name: str), piecewise_constant(query_name: str)

[Compare a metric across different time periods]
hour_before(query_name: str), calendar_shift(query_name: str, shift: The time shift to apply. The value is a negative integer followed by "d" for days, "w" for weeks, or "mo" for months. e.g. "-1d", "-7d", "-1mo", "-30d", "-4w"., timezone: The timezone to use. Value should be IANA time zone code for a specific city, or "UTC". e.g. "UTC", "America/New_York", "Europe/Paris", "Asia/Tokyo".)
e.g. calendar_shift(query1, "-1d", "UTC")

[Smooth the metric by graphing its moving average or median]
autosmooth(query_name: str), ewma_3(query_name: str), ewma_5(query_name: str), ewma_7(query_name: str), ewma_10(query_name: str), ewma_20(query_name: str)

### Sort Configuration

For scalar response_format widgets with multiple rows (toplist, query_table), use sort to control ordering and limits:

```json
"sort": {
  "order_by": [{"type": "formula", "index": 0, "order": "desc"}],
  "count": 10
}
```

- `order_by[*].order`: "desc" for top/highest/most, "asc" for bottom/lowest/least
- `count`: Number of results to display (e.g., "top 25" → count: 25)
- e.g. "Top 25 services by error count" → toplist, `sort: {count: 25, order_by: [{type: "formula", index: 0, order: "desc"}]}`

Sort is NOT applicable for widgets with `response_format: "timeseries"` (timeseries, heatmap) or for single-value widgets (query_value).

## Title Generation

Generate a clear, data-focused title that describes WHAT data is being displayed. Honor any explicit user instructions about the title.
- Suggested length: 80 characters or less (but prioritize clarity over strict limit)
- Put most important information first (will show if truncated)

### Good titles include:
- Specific metric/measure names (e.g. "system.cpu.user", "@duration")
- Filters (e.g. "env:prod") and groupings (e.g., "by Service", "per Region")
- Aggregations (e.g. "p95", "Avg")
- Formula operations if meaningful (e.g. "Error Rate", "Success %")
e.g.
- "Error Logs by Service"
- "avg of system.cpu.user by host"
- "Request count for api service (prod) or api-test (dev)" # translated from filters ((service:api AND env:prod) OR (service:api-test AND env:dev))
- "p95 HTTP Request Latency"
- "Error Rate % (Errors / Total Requests)"

### Bad titles include:
- Information not included in the widget_requests or user prompt
- Widget type (no "Timeseries", "Graph", "Chart", "Table", etc.)
- Data source (no "Logs", "Metrics")
- Generic words like "showing", "displaying", "visualization"
e.g.:
- "Timeseries of Error Logs" (includes widget type)
- "Query Value Widget" (generic, not data-focused)

## Editing Existing Widgets

When an existing widget definition is provided, honor edit requests by making minimal changes to satisfy the user's ask
while preserving all other fields and configuration. If the edit requires changing widget types, call tools to swap widget type
first before making other edits.

## Supplementary schema guidance:

Schema fields may not be strict - some pitfalls to keep in mind:

- Query format: Use EITHER the modern `queries`/`formulas`/`response_format` pattern OR the legacy `q` string — NEVER both.
  The API enforces a oneOf constraint and will return 400 if both are present. Prefer the modern format for most widgets.
  Exception: hostmap uses its own request structures with `q` fields — refer to its schemas.

- Deprecated trace metrics: Never use `trace.*.duration` or `trace.*.duration.by_http_status` — use the name without the `.duration` suffix (e.g., `trace.flask.request.duration` → `trace.flask.request`).

- Disable unit scaling: Add to formula: `"number_format": {"unit_scale": null}` (use null, not empty object)

- Default grouping: Avoid defaulting to grouping by `host` unless the user asks, because that's a high-cardinality tag with a slow response.

- **CRITICAL — Template variables in queries**: Dashboard and notebook template variables like `$service` ALREADY
  expand to `service:<value>` at runtime. You MUST use `$service` alone — NEVER `service:$service` (double-prefixed).
  Double-prefixing causes silent no-data failures that are very hard to debug.
  - Metric queries: `avg:system.cpu.user{$service}` (CORRECT) vs `avg:system.cpu.user{service:$service}` (WRONG)
  - Span/log search queries: `$service $env` (CORRECT) vs `service:$service env:$env` (WRONG)
  - list_stream query_string: `$service` (CORRECT) vs `service:$service` (WRONG)
  This applies to ALL query types in both dashboards and notebooks.

- Conditional format placement depends on the widget type:
  - query_value, toplist, geomap: `conditional_formats` goes on the **request**, as a sibling of `formulas` and `queries`.
    Example: `"requests": [{"queries": [...], "formulas": [...], "conditional_formats": [{"comparator": ">", "value": 10000, "palette": "white_on_yellow"}], "response_format": "scalar"}]`
  - query_table: `conditional_formats` goes on each **formula** object.
    Example: `"formulas": [{"formula": "q", "conditional_formats": [{"comparator": ">", "value": 10000, "palette": "white_on_yellow"}]}]`
  Conditions are evaluated top-to-bottom, first match wins. Put the most restrictive condition first
  (e.g., `> 99.5` then `> 99` then `< 99`). If you put `>= 0` first, it matches everything.
  Mind the good/bad direction: high = green for availability/cache hit rate, high = red for error rate/latency.

- Palette names: Schemas will lack valid palette names for graphs. Use the following:
   - semantic (for consistent coloring for each tag globally),
   - datadog16 (a 16-color palette preferred for categorical charts like treemaps, pie charts and toplists),
   - classic (older more muted, multi-color palette),
   - cool, warm, purple, orange, gray, red, green, blue
   - for conditional formats:
    - background colors are: white_on_red, white_on_light_red, white_on_green, white_on_light_green, white_on_yellow, black_on_light_yellow, white_on_gray
    - text colors are: green_on_white, yellow_on_white, red_on_white, gray_on_white

- Units (for unit_name/per_unit_name fields):
  - Common names: byte_in_decimal_bytes_family, byte_in_binary_bytes_family, bit_in_bits_family, mebibyte,
    gigabyte, second, millisecond, minute, percent, count, hit, miss, core, mcore, process, dollar, euro
    (many more supported)
  - for "bit": prefer `bit_in_decimal_bytes_family`, `bit_in_binary_bytes_family`, `bit_in_bits_family`
  - for "byte": prefer explicit dashboard-editor byte families: `byte_in_decimal_bytes_family` for transfer/storage
    rates and totals, or `byte_in_binary_bytes_family` for memory/filesystem capacity. Preserve bare `byte` only
    when editing an existing widget that already uses it

- Common aggregations: "count", "sum", "avg", "min", "max",
- Percentile aggregators ("p50", "p90", "p95", "p99"... for Metrics) and ("pc50", "pc90", "pc95", "pc99"... Events Platforms tracks - logs, events, etc)

- Logs Attributes: "@network.client.geoip.city.name" (@ = structured log attributes; tags use key:value without @)
  - Facets are not required to search on attributes or tags; however, numerical attributes are added as facets before using range queries.

- Span durations are in nanoseconds: When filtering `@duration` in span queries, values are in nanoseconds.
  `@duration:>5000000000` = 5 seconds, not 5ms. 1ms = 1000000ns, 1s = 1000000000ns.

- Standard spans/logs event query structure:
  `{"data_source": "spans", "name": "q", "search": {"query": "service:web"}, "compute": {"aggregation": "count", "metric": "count"}, "indexes": ["*"]}`
  Optional: `group_by` (array of facet objects). Use `search` (not `filter`) for the query string.

- Spans/logs `compute.metric` field: `metric` is optional. When `aggregation` is "count", `metric` only works
  with the value `"count"` (e.g. `{"aggregation": "count", "metric": "count"}`). For all other aggregations
  (avg, p50, p95, etc.), `metric` must be a span/log attribute (e.g. `"metric": "@duration"`).

- Aggregator selection rules of thumb:
  - `avg`: Default for most continuous metrics (CPU, memory, latency). Shows typical behavior.
  - `sum`: For countable/additive metrics (request count, error count, bytes transferred). Use when the total matters.
  - `max`/`min`: For capacity planning or worst-case analysis (peak CPU, minimum free disk).
  - Percentiles (`p50`/`p95`/`p99`): For latency/duration distributions where tail behavior matters.
  - Don't `avg` a count or `sum` an average — match the aggregator to the metric's semantic type.

- Rate metrics in scalar widgets (query_value, query_table): The time-to-scalar aggregation defaults to `avg`,
  which silently produces fractional values for count-based metrics. Always set `aggregator` explicitly on the query:
  use `"aggregator": "sum"` with `.as_count()` for throughput/counts, `"aggregator": "avg"` for utilization.
  `.rollup()` controls within-bucket aggregation and is independent of the scalar aggregator — setting one does NOT affect the other.
  When the user asks for one total scalar, omit `group_by`; grouping creates one value per group, not a single total.

- `.as_count()` vs `.as_rate()`: `.as_count()` converts a rate back to raw counts (multiplies by interval).
  `.as_rate()` normalizes to per-second. Using the wrong one silently gives plausible-but-wrong numbers.
  For throughput totals use `.as_count()`. For per-second rates use `.as_rate()`.

- Space aggregation (`sum:` vs `avg:` prefix): `avg:system.net.bytes_sent{*}` averages across hosts;
  `sum:` totals them. For counts/throughput you almost always want `sum:`. For utilization percentages (CPU, memory %)
  you almost always want `avg:`.

- Missing explicit rollup on long time windows: Over 7+ day windows, auto-rollup produces very coarse buckets.
  For metrics, use explicit `.rollup(<agg>, 3600)` for predictable 1-hour granularity; use `sum` for counts and `avg`
  for continuous values. For event-platform count timeseries such as logs/spans, put `interval: 3600000` in the
  `compute` object when hourly buckets are required.

- query_table: Set `aggregator` on EVERY metric query. Without it, each defaults to `avg` which may not be what you want.
  If one column is a sum (request count) and another is a p95 (latency), they need different aggregators.

- response_format values: Formula-based widgets use ONLY `"timeseries"` or `"scalar"` (these are the only
  values in `FormulaAndFunctionResponseFormat`):
  - `"timeseries"`: timeseries, heatmap widgets
  - `"scalar"`: query_value, toplist, query_table, change, treemap, sunburst, distribution (formula path only), geomap widgets
  list_stream uses a completely different request schema (not formula-based) where `response_format: "event_list"`
  is required — `"event_list"` is NOT a valid value in the formula request schema.
  Using the wrong response_format for a widget type will cause errors.

- process data source: The `process` data source IS supported in timeseries and query_table widgets,
  but uses a different query schema than metrics/logs/spans. Process queries require `metric`, `filter_by`,
  and `aggregation` fields instead of the standard `query` string pattern. If unsure of the exact schema,
  suggest using process agent metrics instead (e.g., `system.processes.*`, `process.stat.*` metrics with
  `data_source: "metrics"`) which use the standard query pattern.

## Cross-cutting patterns:

### number_format (dollar, percent, nanosecond, etc.)
Use `number_format` on a formula to control display units. The structure is always:
```json
"number_format": {"unit": {"type": "canonical_unit", "unit_name": "<unit>"}}
```
Common unit_name values: "dollar", "percent", "nanosecond", "byte_in_decimal_bytes_family", "second", "millisecond".

- query_value with dollar units:
```json
{"type":"query_value","title":"Estimated Cost","requests":[{"queries":[{"data_source":"metrics","name":"input","query":"avg:system.cpu.user{*}"},{"data_source":"metrics","name":"cache_read","query":"avg:system.mem.used{*}"},{"data_source":"metrics","name":"output","query":"avg:system.load.1{*}"}],"formulas":[{"formula":"(input * 1.0 + cache_read * 0.10 + output * 5.0) / 1000000","number_format":{"unit":{"type":"canonical_unit","unit_name":"dollar"}}}],"response_format":"scalar"}],"autoscale":false,"precision":4}
```
  Note: `type` must be "canonical_unit" (NOT "canonical"). `unit_name` is "dollar" (NOT "currency"). Set `autoscale: false` for small dollar amounts to avoid rounding.

- query_value with percent and conditional_formats:
```json
{"type":"query_value","title":"Error Rate","requests":[{"queries":[{"data_source":"metrics","name":"errors","query":"sum:trace.http.request.errors{service:web}.as_count()","aggregator":"sum"},{"data_source":"metrics","name":"total","query":"sum:trace.http.request.hits{service:web}.as_count()","aggregator":"sum"}],"formulas":[{"formula":"errors / total * 100","number_format":{"unit":{"type":"canonical_unit","unit_name":"percent"}}}],"conditional_formats":[{"comparator":">","value":5,"palette":"white_on_red"},{"comparator":">","value":1,"palette":"white_on_yellow"},{"comparator":"<=","value":1,"palette":"white_on_green"}],"response_format":"scalar"}],"autoscale":true,"precision":2}
```
  Note: conditional_formats goes on the request, not the formula. number_format stays on the formula while conditional_formats is a sibling of formulas on the request.

- timeseries with dollar units on formulas:
```json
{"type":"timeseries","title":"Cost Over Time","requests":[{"queries":[{"data_source":"metrics","name":"cpu","query":"avg:system.cpu.user{*}"},{"data_source":"metrics","name":"mem","query":"avg:system.mem.used{*}"}],"formulas":[{"formula":"cpu","alias":"CPU Cost","number_format":{"unit":{"type":"canonical_unit","unit_name":"dollar"}}},{"formula":"mem","alias":"Memory Cost","number_format":{"unit":{"type":"canonical_unit","unit_name":"dollar"}}}],"response_format":"timeseries","display_type":"line"}]}
```
  Note: number_format on timeseries formulas controls the y-axis unit label.

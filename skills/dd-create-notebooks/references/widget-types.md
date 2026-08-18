# Widget type index

Choose one exact type, then read only its linked reference. Each reference carries the
supported data sources, rules, warnings, examples, and the command that fetches the
exact schema for your data source.

Read [the construction handbook](widgets/handbook.md) first. It carries the query,
formula, sort, unit, palette, and timeframe rules that every type shares.

## Streams

- [`log_stream`](widget-types/log_stream.md): Use for a direct stream of matching logs.
- [`list_stream`](widget-types/list_stream.md): Use for recent log, span, RUM, security, CI, or other event records.

## Queries

- [`timeseries`](widget-types/timeseries.md): Use for values over time. Line, area, bars, and overlay are display forms of this type.
- [`bar_chart`](widget-types/bar_chart.md): Use for scalar values grouped into categorical bars.
- [`query_value`](widget-types/query_value.md): Use for one headline scalar value.
- [`toplist`](widget-types/toplist.md): Use for a ranked set of groups.
- [`query_table`](widget-types/query_table.md): Use for detailed grouped scalar rows and multiple value columns.
- [`change`](widget-types/change.md): Use for current-versus-reference change.

## Analysis

- [`distribution`](widget-types/distribution.md): Use for a histogram or value distribution.
- [`heatmap`](widget-types/heatmap.md): Use for density over time or two axes.
- [`geomap`](widget-types/geomap.md): Use for values grouped by geographic facets.
- [`hostmap`](widget-types/hostmap.md): Use for hosts grouped visually with separate fill and size signals.
- [`scatterplot`](widget-types/scatterplot.md): Use to correlate two or three measures.
- [`sunburst`](widget-types/sunburst.md): Use for hierarchical part-to-whole data in radial form.
- [`treemap`](widget-types/treemap.md): Use for hierarchical part-to-whole data as nested rectangles.
- [`sankey`](widget-types/sankey.md): Use for flows or journeys between ordered steps.
- [`funnel`](widget-types/funnel.md): Use for conversion through ordered journey steps.
- [`cohort`](widget-types/cohort.md): Use for cohort-based product analysis.
- [`retention_curve`](widget-types/retention_curve.md): Use for retention over time by cohort.
- [`wildcard`](widget-types/wildcard.md): Use only for a known-good custom Vega definition.

## Status

- [`alert_graph`](widget-types/alert_graph.md): Use for monitor history as a timeseries or toplist.
- [`alert_value`](widget-types/alert_value.md): Use for the current value or status of one monitor.
- [`check_status`](widget-types/check_status.md): Use for service-check health.
- [`manage_status`](widget-types/manage_status.md): Use for a monitor summary.
- [`slo`](widget-types/slo.md): Use for one SLO and its time windows.
- [`slo_list`](widget-types/slo_list.md): Use for a list of SLOs.
- [`topology_map`](widget-types/topology_map.md): Use for service or resource topology.
- [`trace_service`](widget-types/trace_service.md): Use for a service summary.

## Content

- [`image`](widget-types/image.md): Use to display an image URL.
- [`iframe`](widget-types/iframe.md): Use to embed an external web page.
- [`run_workflow`](widget-types/run_workflow.md): Use to run a configured workflow.

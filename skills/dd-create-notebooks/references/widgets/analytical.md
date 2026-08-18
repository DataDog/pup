# Distributions and relationships

Choose from user intent:

- histogram or value distribution: `distribution`
- density over time or axes: `heatmap`
- geographic values: `geomap`
- hosts grouped visually: `hostmap`
- correlation between two measures: `scatterplot`
- hierarchical part-to-whole: `sunburst` or `treemap`
- flow or journey between ordered steps: `sankey`
- conversion through ordered steps: `funnel`
- cohort comparison: `cohort`
- retention over time: `retention_curve`
- custom Vega visualization: `wildcard`, only with a known-good definition

After choosing one type, read its exact reference from
[the widget type index](../widget-types.md).

Read [handbook.md](handbook.md) for the query, then use only the
selected type reference. Do not transfer fields from another analytical widget.

## Preserve analytical intent

| Type | Required intent |
| --- | --- |
| `distribution` | preserve histogram request type, bucket count, percentile display, and requested markers |
| `geomap` | use scalar values grouped by the requested geographic facet; preserve focus and palette direction |
| `heatmap` | use a timeseries response and preserve the metric, scope, and grouping on the requested axis |
| `hostmap` | preserve the fill and size signals as separate requests |
| `scatterplot` | use table responses and separate `x`, `y`, and optional `radius` dimensions; preserve the grouping facet |
| `sunburst` | preserve hierarchy order, aggregation, and table legend settings |
| `treemap` | preserve hierarchy order, aggregation, and palette |
| `sankey` | preserve source mode, journey source, entries per step, and bounded step count |

Scatterplots need one query for each requested dimension. Do not replace their
table requests with legacy `q` fields. For sunbursts and treemaps, do not merge
or reorder hierarchical groups.

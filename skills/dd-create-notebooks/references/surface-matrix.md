# Cell and widget surface matrix

Use this matrix as the exposure boundary. "Schema-valid" alone is insufficient evidence of product support.

## Current Pup dashboard registry

### Supported by dashboards and notebooks

These 27 current Pup types appear in the backend notebook renderer allowlist:

`alert_graph`, `alert_value`, `change`, `check_status`, `distribution`, `funnel`, `geomap`, `heatmap`, `hostmap`, `iframe`, `image`, `list_stream`, `log_stream`, `manage_status`, `query_table`, `query_value`, `run_workflow`, `scatterplot`, `slo`, `slo_list`, `sunburst`, `timeseries`, `toplist`, `topology_map`, `trace_service`, `treemap`, `wildcard`.

Treat their definitions as reusable. Validate them in the target surface context.

### Dashboard-only

| Type | Reason |
| --- | --- |
| `event_stream` | Free-layout dashboard widget; omitted from notebook allowlist. |
| `event_timeline` | Free-layout dashboard widget; omitted from notebook allowlist. |
| `free_text` | Dashboard text widget; use notebook `markdown`. |
| `note` | Dashboard formatted note; use notebook `markdown`. |
| `group` | Dashboard structural container with child widget layouts. |
| `powerpack` | Dashboard reusable composition primitive. |
| `servicemap` | Dashboard-only legacy type; prefer shared `topology_map`. |

Keep their definitions and all placement operations dashboard-owned.

## Additional notebook-allowlisted shared widgets

The backend notebook allowlist also contains types not presently exposed by Pup's dashboard schema registry:

`bar_chart`, `cloud_cost_summary`, `cloudcraft`, `cohort`, `embedded_app`, `experimental`, `flame_graph`, `journey_map`, `pivot_table`, `point_plot`, `retention_curve`, `sankey`, `slo_summary`.

Do not advertise these solely from this list:

- `experimental` is internal/feature-gated.
- Several types are absent from the current widget-reference registry.
- Availability may depend on product entitlements or feature flags.

For initial agent-authored notebooks, prefer the well-covered shared types unless the user explicitly requests one of these and supplies a known-good definition.

## Notebook-only writable cells

| Group | Types | Availability |
| --- | --- | --- |
| Narrative | `markdown` | Core. Mermaid is a Markdown capability. |
| Analysis | `analysis_data_source`, `analysis_sql`, `analysis_transformation` | Core computational chain. |
| Code | `python` | Core schema; runtime availability can still vary. |
| Canvas/document | `diagram`, `html` | Feature-gated and upload-backed. |
| Alternate query | `ddql` | Feature-gated and inconsistent across registries. |

## Internal or quarantined types

Never generate these as public notebook creation primitives:

- `rich_text`, `rich_text_experimental`: editor/storage representations; author `markdown`.
- `ai_draft`: editor preview state.
- `experimental`: internal/feature-gated visualization.
- `wildcard`: public type string but custom Vega behavior requires a known-good definition and notebook-context validation.
- `cluster`: legacy alias for dashboard `group`.
- `process`, `uptime`: legacy or unsupported.
- `custom`, `dora_summary`, `form`, `heatgrid`, `image_map`: omitted from notebook allowlist and not sufficiently confirmed for general creation.
- `split_group`: dashboard structural container.

## Context-specific shared schemas

These public widget type strings use notebook workbook validators when placed in notebooks:

`bar_chart`, `point_plot`, `query_table`, `scatterplot`, `sunburst`, `timeseries`, `toplist`, `treemap`, `wildcard`.

The notebook variants can consume named analysis datasets with `request_type: "local_dataset"`. Do not fork them into notebook-only type names.

## Ownership boundary

- Shared definition selection, query/formula guidance, schema lookup, and definition validation belong to the future `pup widgets` surface.
- Dashboard list/get/add/update/remove, widget identity, `{definition,layout,id}` envelopes, free/ordered layout rules, groups, and powerpacks remain under `pup dashboards widgets`.
- Notebook envelopes, cell ordering, analysis dependencies, Markdown, code, uploads, and whole-chain validation belong to the future `pup notebooks cells` surface.

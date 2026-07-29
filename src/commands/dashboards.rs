use anyhow::Result;
use datadog_api_client::datadogV1::api_dashboards::{DashboardsAPI, ListDashboardsOptionalParams};
use datadog_api_client::datadogV1::model::{Dashboard, Widget, WidgetDefinition};
use std::io::Read;
use url::Url;

use crate::config::Config;
use crate::formatter::{self, Metadata};
use crate::util;

pub async fn list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(DashboardsAPI, cfg);
    let resp = api
        .list_dashboards(ListDashboardsOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list dashboards: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get(cfg: &Config, id: &str) -> Result<()> {
    let api = crate::make_api!(DashboardsAPI, cfg);
    let resp = api
        .get_dashboard(id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get dashboard: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let body: Dashboard = util::read_json_file(file)?;
    let api = crate::make_api!(DashboardsAPI, cfg);
    let resp = api
        .create_dashboard(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create dashboard: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, id: &str, file: &str) -> Result<()> {
    let body: Dashboard = util::read_json_file(file)?;
    let api = crate::make_api!(DashboardsAPI, cfg);
    let resp = api
        .update_dashboard(id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update dashboard: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn delete(cfg: &Config, id: &str) -> Result<()> {
    let api = crate::make_api!(DashboardsAPI, cfg);
    let resp = api
        .delete_dashboard(id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete dashboard: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn url(cfg: &Config, id: &str, from: &str, to: &str, live: bool) -> Result<()> {
    let api = crate::make_api!(DashboardsAPI, cfg);
    let dashboard = api
        .get_dashboard(id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get dashboard: {e:?}"))?;
    let base_url = dashboard
        .url
        .ok_or_else(|| anyhow::anyhow!("dashboard response did not include url"))?;
    println!("{}", dashboard_url_with_time(&base_url, from, to, live)?);
    Ok(())
}

// ---- Dashboard widget helpers ----

/// Read a widget JSON payload from a file path or stdin (`"-"`), then validate it.
fn read_widget_input(file: &str) -> Result<Widget> {
    let bytes: Vec<u8> = if file == "-" {
        let mut buf = Vec::new();
        std::io::stdin()
            .read_to_end(&mut buf)
            .map_err(|e| anyhow::anyhow!("failed to read widget JSON from stdin: {e}"))?;
        buf
    } else {
        std::fs::read(file).map_err(|e| anyhow::anyhow!("failed to read --file {file:?}: {e}"))?
    };
    let widget: Widget = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("failed to parse widget JSON: {e}"))?;
    validate_widget(&widget)?;
    Ok(widget)
}

/// Verify the widget has a known definition type.
///
/// `serde_json::from_slice::<Widget>` never fails on an unknown `type` — instead
/// the SDK falls through to `WidgetDefinition::UnparsedObject`.  We must explicitly
/// check for that variant; the internal `_unparsed` flag is `pub(crate)` and
/// unavailable here, but the public variant is sufficient.
fn validate_widget(widget: &Widget) -> Result<()> {
    if matches!(widget.definition, WidgetDefinition::UnparsedObject(_)) {
        return Err(anyhow::anyhow!(
            "widget definition has an unknown or invalid `type`; \
             run `pup dashboards widgets types` to see supported types"
        ));
    }
    Ok(())
}

/// Resolve a `--widget-id` or `--index` selector to an array position.
///
/// Exactly one of the two `Option` arguments must be `Some` (enforced at the
/// clap layer before we get here; the fallback errors are defensive).
fn locate_widget_index(
    widgets: &[Widget],
    widget_id: Option<i64>,
    index: Option<usize>,
) -> Result<usize> {
    if let Some(idx) = index {
        if idx >= widgets.len() {
            return Err(anyhow::anyhow!(
                "index {idx} out of range; dashboard has {} widget(s)",
                widgets.len()
            ));
        }
        return Ok(idx);
    }
    let wid = widget_id.ok_or_else(|| anyhow::anyhow!("--widget-id or --index is required"))?;
    let hits: Vec<usize> = widgets
        .iter()
        .enumerate()
        .filter(|(_, w)| w.id == Some(wid))
        .map(|(i, _)| i)
        .collect();
    match hits.as_slice() {
        [] => Err(anyhow::anyhow!(
            "no widget with id {wid} found in dashboard"
        )),
        [i] => Ok(*i),
        _ => Err(anyhow::anyhow!(
            "widget id {wid} is ambiguous ({} matches); use --index instead",
            hits.len()
        )),
    }
}

// ---- Dashboard widget commands ----

pub async fn widget_list(cfg: &Config, dash_id: &str) -> Result<()> {
    let api = crate::make_api!(DashboardsAPI, cfg);
    let dashboard = api
        .get_dashboard(dash_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get dashboard: {e:?}"))?;
    let rows: Vec<serde_json::Value> = dashboard
        .widgets
        .iter()
        .enumerate()
        .map(|(idx, w)| {
            let def_val = serde_json::to_value(&w.definition).unwrap_or(serde_json::Value::Null);
            let widget_type = def_val
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let title = def_val
                .get("title")
                .and_then(|v| v.as_str())
                .map(|s| serde_json::Value::String(s.to_string()))
                .unwrap_or(serde_json::Value::Null);
            serde_json::json!({
                "index": idx,
                "id": w.id,
                "type": widget_type,
                "title": title,
                "layout": w.layout,
            })
        })
        .collect();
    let count = rows.len();
    formatter::format_and_print(
        &rows,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&Metadata {
            count: Some(count),
            truncated: false,
            command: Some("dashboards widgets list".into()),
            next_action: None,
        }),
        cfg.jq.as_deref(),
    )
}

pub async fn widget_get(
    cfg: &Config,
    dash_id: &str,
    widget_id: Option<i64>,
    index: Option<usize>,
) -> Result<()> {
    let api = crate::make_api!(DashboardsAPI, cfg);
    let dashboard = api
        .get_dashboard(dash_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get dashboard: {e:?}"))?;
    let idx = locate_widget_index(&dashboard.widgets, widget_id, index)?;
    formatter::output(cfg, &dashboard.widgets[idx])
}

pub async fn widget_add(cfg: &Config, dash_id: &str, file: &str) -> Result<()> {
    let mut widget = read_widget_input(file)?;
    let api = crate::make_api!(DashboardsAPI, cfg);
    let mut dashboard = api
        .get_dashboard(dash_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get dashboard: {e:?}"))?;
    // Detect free vs ordered layout via JSON serialization (avoids needing to
    // match on SDK enum variant names that may change across SDK revisions).
    let is_free = serde_json::to_value(&dashboard.layout_type)
        .ok()
        .and_then(|v| v.as_str().map(|s| s == "free"))
        .unwrap_or(false);
    if is_free {
        if widget.layout.is_none() {
            return Err(anyhow::anyhow!(
                "this is a free-layout dashboard; the widget JSON must include a \
                 `layout` object with `x`, `y`, `width`, and `height` fields"
            ));
        }
    } else {
        // Ordered dashboards must not carry per-widget layout coordinates.
        widget.layout = None;
    }
    widget.id = None; // let the API assign an id on create
    dashboard.widgets.push(widget);
    let resp = api
        .update_dashboard(dash_id.to_string(), dashboard)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update dashboard: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn widget_update(
    cfg: &Config,
    dash_id: &str,
    widget_id: Option<i64>,
    index: Option<usize>,
    file: &str,
) -> Result<()> {
    let mut widget = read_widget_input(file)?;
    let api = crate::make_api!(DashboardsAPI, cfg);
    let mut dashboard = api
        .get_dashboard(dash_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get dashboard: {e:?}"))?;
    let idx = locate_widget_index(&dashboard.widgets, widget_id, index)?;
    // Preserve the existing widget's id so it keeps identity in the dashboard.
    widget.id = dashboard.widgets[idx].id;
    dashboard.widgets[idx] = widget;
    let resp = api
        .update_dashboard(dash_id.to_string(), dashboard)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update dashboard: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn widget_remove(
    cfg: &Config,
    dash_id: &str,
    widget_id: Option<i64>,
    index: Option<usize>,
) -> Result<()> {
    let api = crate::make_api!(DashboardsAPI, cfg);
    let mut dashboard = api
        .get_dashboard(dash_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get dashboard: {e:?}"))?;
    let idx = locate_widget_index(&dashboard.widgets, widget_id, index)?;
    dashboard.widgets.remove(idx);
    let resp = api
        .update_dashboard(dash_id.to_string(), dashboard)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update dashboard: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub fn widget_types(cfg: &Config) -> Result<()> {
    let types: Vec<serde_json::Value> = WIDGET_TYPES
        .iter()
        .map(|(t, d)| serde_json::json!({"type": t, "description": d}))
        .collect();
    formatter::format_and_print(
        &types,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&Metadata {
            count: Some(WIDGET_TYPES.len()),
            truncated: false,
            command: Some("dashboards widgets types".into()),
            next_action: None,
        }),
        cfg.jq.as_deref(),
    )
}

pub fn widget_schema(cfg: &Config, type_str: &str) -> Result<()> {
    let description = WIDGET_TYPES
        .iter()
        .find(|(t, _)| *t == type_str)
        .map(|(_, d)| *d)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "unknown widget type {type_str:?}; \
                 run `pup dashboards widgets types` to see supported types"
            )
        })?;
    let schema = widget_json_schema(type_str).expect("type is in WIDGET_TYPES so schema exists");
    let template = widget_template(type_str).expect("type is in WIDGET_TYPES so template exists");
    formatter::output(
        cfg,
        &serde_json::json!({
            "type": type_str,
            "description": description,
            "schema": schema,
            "template": template,
        }),
    )
}

// ---- Widget type registry and skeleton templates ----

/// All widget `type` strings recognised by the Datadog Dashboard API (V1),
/// paired with their descriptions from the OpenAPI spec.
///
/// Kept in sync with the `WidgetDefinition` enum in the
/// `datadog-api-client` SDK.  Unknown types round-trip via
/// `WidgetDefinition::UnparsedObject` and are rejected by `validate_widget`.
const WIDGET_TYPES: &[(&str, &str)] = &[
    ("alert_graph", "Alert graphs are timeseries graphs showing the current status of any monitor defined on your system."),
    ("alert_value", "Alert values are query values showing the current value of the metric in any monitor defined on your system."),
    ("change", "The Change graph shows you the change in a value over the time period chosen."),
    ("check_status", "Check status shows the current status or number of results for any check performed."),
    ("distribution", "The Distribution visualization is another way of showing metrics aggregated across one or several tags, such as hosts."),
    ("event_stream", "The event stream is a widget version of the stream of events on the Event Stream view. Only available on FREE layout dashboards."),
    ("event_timeline", "The event timeline is a widget version of the timeline that appears at the top of the Event Stream view. Only available on FREE layout dashboards."),
    ("free_text", "Free text is a widget that allows you to add headings to your dashboard. Commonly used to state the overall purpose of the dashboard."),
    ("funnel", "The funnel visualization displays a funnel of user sessions that maps a sequence of view navigation and user interaction in your application."),
    ("geomap", "This visualization displays a series of values by country on a world map."),
    ("group", "The group widget allows you to keep similar graphs together on your dashboard. Each group has a custom header, can hold one to many graphs, and is collapsible."),
    ("heatmap", "The heat map visualization shows metrics aggregated across many tags, such as hosts. The more hosts that have a particular value, the darker that square is."),
    ("hostmap", "The host map widget graphs any metric across your hosts using the same visualization available from the main Host Map page."),
    ("iframe", "The iframe widget allows you to embed a portion of any other web page on your dashboard."),
    ("image", "The image widget allows you to embed an image on your dashboard. An image can be a PNG, JPG, or animated GIF."),
    ("list_stream", "The list stream visualization displays a table of recent events in your application that match a search criteria using user-defined columns."),
    ("log_stream", "The Log Stream displays a log flow matching the defined query."),
    ("manage_status", "The monitor summary widget displays a summary view of all your Datadog monitors, or a subset based on a query."),
    ("note", "The notes and links widget is similar to free text widget, but allows for more formatting options."),
    ("powerpack", "The powerpack widget allows you to keep similar graphs together on your dashboard. Each group has a custom header, can hold one to many graphs, and is collapsible."),
    ("query_value", "Query values display the current value of a given metric, APM, or log query."),
    ("query_table", "The table visualization is available on dashboards. It displays columns of metrics grouped by tag key."),
    ("run_workflow", "The run workflow widget allows you to run a workflow from a dashboard."),
    ("scatterplot", "The scatter plot visualization allows you to graph a chosen scope over two different metrics with their respective aggregation."),
    ("servicemap", "This widget displays a map of a service to all of the services that call it, and all of the services that it calls."),
    ("slo", "Use the SLO and uptime widget to track your SLOs (Service Level Objectives) and uptime on dashboards."),
    ("slo_list", "Use the SLO List widget to track your SLOs (Service Level Objectives) on dashboards."),
    ("sunburst", "Sunbursts are spot on to highlight how groups contribute to the total of a query."),
    ("timeseries", "The timeseries visualization allows you to display the evolution of one or more metrics, log events, or Indexed Spans over time."),
    ("toplist", "The top list visualization enables you to display a list of Tag value like hostname or service with the most or least of any metric value, such as highest consumers of CPU, hosts with the least disk space, etc."),
    ("topology_map", "This widget displays a topology of nodes and edges for different data sources. It replaces the service map widget."),
    ("trace_service", "The service summary displays the graphs of a chosen service in your dashboard."),
    ("treemap", "The treemap visualization enables you to display hierarchical and nested data. It is well suited for queries that describe part-whole relationships, such as resource usage by availability zone, data center, or team."),
    ("wildcard", "Custom visualization widget using Vega or Vega-Lite specifications. Combines standard Datadog data requests with a Vega or Vega-Lite JSON specification for flexible, custom visualizations."),
];

/// Return a ready-to-edit skeleton JSON for the given widget type.
///
/// High-traffic types (timeseries, query_value, query_table, toplist, note,
/// free_text, heatmap, slo) get a real skeleton with the required fields.
/// Every other known type gets a minimal `{"definition":{"type":"…"}}` fallback
/// the user can flesh out.  Unknown types return `None`.
fn widget_template(t: &str) -> Option<serde_json::Value> {
    let v = match t {
        "timeseries" => serde_json::json!({
            "definition": {
                "type": "timeseries",
                "title": "",
                "requests": [{"q": "", "display_type": "line"}]
            }
        }),
        "query_value" => serde_json::json!({
            "definition": {
                "type": "query_value",
                "title": "",
                "requests": [{"q": "", "aggregator": "avg"}]
            }
        }),
        "query_table" => serde_json::json!({
            "definition": {
                "type": "query_table",
                "title": "",
                "requests": [{"q": "", "aggregator": "avg"}]
            }
        }),
        "toplist" => serde_json::json!({
            "definition": {
                "type": "toplist",
                "title": "",
                "requests": [{"q": ""}]
            }
        }),
        "note" => serde_json::json!({
            "definition": {
                "type": "note",
                "content": ""
            }
        }),
        "free_text" => serde_json::json!({
            "definition": {
                "type": "free_text",
                "text": ""
            }
        }),
        "heatmap" => serde_json::json!({
            "definition": {
                "type": "heatmap",
                "title": "",
                "requests": [{"q": ""}]
            }
        }),
        "slo" => serde_json::json!({
            "definition": {
                "type": "slo",
                "slo_id": "",
                "view_type": "detail",
                "time_windows": ["7d"]
            }
        }),
        other if WIDGET_TYPES.iter().any(|(t, _)| *t == other) => serde_json::json!({
            "definition": {"type": other}
        }),
        _ => return None,
    };
    Some(v)
}

/// Return a JSON Schema describing the full widget object for the given type.
///
/// Each widget type gets its own detailed schema derived from the Datadog
/// OpenAPI spec (v1/dashboard.yaml).  The top-level object mirrors the SDK
/// `Widget` struct: `id` (assigned by API), `layout` (required for free
/// dashboards), and `definition` (the type-specific payload).
fn widget_json_schema(t: &str) -> Option<serde_json::Value> {
    let definition = match t {
        "alert_graph" => serde_json::json!({
            "type": "object",
            "required": ["type", "alert_id", "viz_type"],
            "properties": {
                "type": {"type": "string", "enum": ["alert_graph"], "description": "Type of the alert graph widget."},
                "alert_id": {"type": "string", "description": "ID of the alert to use in the widget."},
                "viz_type": {"type": "string", "enum": ["timeseries", "toplist"], "description": "Visualization type for the widget."},
                "title": {"type": "string", "description": "The title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "alert_value" => serde_json::json!({
            "type": "object",
            "required": ["type", "alert_id"],
            "properties": {
                "type": {"type": "string", "enum": ["alert_value"], "description": "Type of the alert value widget."},
                "alert_id": {"type": "string", "description": "ID of the alert to use in the widget."},
                "precision": {"type": "integer", "description": "Number of decimal places to show. If not defined, uses the raw value."},
                "unit": {"type": "string", "description": "Unit to display with the value."},
                "text_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the value."},
                "title_size": {"type": "string", "description": "Size of the value in the widget."},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."}
            }
        }),
        "change" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests"],
            "properties": {
                "type": {"type": "string", "enum": ["change"], "description": "Type of the change widget."},
                "requests": {"type": "array", "description": "Array of one request object to display in the widget.", "items": {"type": "object", "properties": {"q": {"type": "string", "description": "Metric query."}}}},
                "custom_links": {"type": "array", "description": "List of custom links.", "items": {"type": "object"}},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "check_status" => serde_json::json!({
            "type": "object",
            "required": ["type", "check", "grouping"],
            "properties": {
                "type": {"type": "string", "enum": ["check_status"], "description": "Type of the check status widget."},
                "check": {"type": "string", "description": "Name of the check to use in the widget."},
                "grouping": {"type": "string", "enum": ["check", "cluster"], "description": "Grouping method: \"check\" shows a single check, \"cluster\" shows a group."},
                "group": {"type": "string", "description": "Group reporting a single check."},
                "tags": {"type": "array", "description": "List of tags used to filter the groups reporting a cluster check.", "items": {"type": "string"}},
                "group_by": {"type": "array", "description": "List of tag prefixes to group by for a cluster check.", "items": {"type": "string"}},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "distribution" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests"],
            "properties": {
                "type": {"type": "string", "enum": ["distribution"], "description": "Type of the distribution widget."},
                "requests": {"type": "array", "description": "Array of one request object to display in the widget.", "items": {"type": "object", "properties": {"q": {"type": "string", "description": "Metric query."}}}},
                "markers": {"type": "array", "description": "List of markers to display on the graph.", "items": {"type": "object"}},
                "xaxis": {"type": "object", "description": "X-axis configuration."},
                "yaxis": {"type": "object", "description": "Y-axis configuration."},
                "custom_links": {"type": "array", "description": "List of custom links.", "items": {"type": "object"}},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "event_stream" => serde_json::json!({
            "type": "object",
            "required": ["type", "query"],
            "properties": {
                "type": {"type": "string", "enum": ["event_stream"], "description": "Type of the event stream widget."},
                "query": {"type": "string", "description": "Query to filter the event stream with."},
                "event_size": {"type": "string", "enum": ["s", "l"], "description": "Size of each event: \"s\" (small) or \"l\" (large)."},
                "tags_execution": {"type": "string", "description": "Execution method for multi-value filters: \"and\" or \"or\"."},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "event_timeline" => serde_json::json!({
            "type": "object",
            "required": ["type", "query"],
            "properties": {
                "type": {"type": "string", "enum": ["event_timeline"], "description": "Type of the event timeline widget."},
                "query": {"type": "string", "description": "Query to filter the event timeline with."},
                "tags_execution": {"type": "string", "description": "Execution method for multi-value filters: \"and\" or \"or\"."},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "free_text" => serde_json::json!({
            "type": "object",
            "required": ["type", "text"],
            "properties": {
                "type": {"type": "string", "enum": ["free_text"], "description": "Type of the free text widget."},
                "text": {"type": "string", "description": "Text to display."},
                "color": {"type": "string", "description": "Color of the text."},
                "font_size": {"type": "string", "description": "Size of the text."},
                "text_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the text."},
                "background_color": {"type": "string", "description": "Background color of the widget."}
            }
        }),
        "funnel" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests"],
            "properties": {
                "type": {"type": "string", "enum": ["funnel"], "description": "Type of the funnel widget."},
                "requests": {
                    "type": "array",
                    "description": "Request payload. Exactly one item.",
                    "minItems": 1,
                    "maxItems": 1,
                    "items": {"type": "object", "properties": {
                        "request_type": {"type": "string", "enum": ["funnel"], "description": "Request type."},
                        "query": {"type": "object", "description": "Funnel query.", "properties": {
                            "data_source": {"type": "string", "enum": ["rum"], "description": "Data source."},
                            "query_string": {"type": "string", "description": "Query filter string."},
                            "steps": {"type": "array", "description": "Funnel steps.", "items": {"type": "object", "properties": {"facet": {"type": "string"}, "value": {"type": "string"}}}}
                        }}
                    }}
                },
                "title": {"type": "string", "description": "The title of the widget."},
                "title_size": {"type": "string", "description": "The size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "geomap" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests", "style", "view"],
            "properties": {
                "type": {"type": "string", "enum": ["geomap"], "description": "Type of the geomap widget."},
                "requests": {"type": "array", "description": "Array of request objects. May include a region layer and/or points layer request.", "items": {"type": "object"}},
                "style": {"type": "object", "description": "Widget style.", "properties": {
                    "palette": {"type": "string", "description": "Color palette to apply to the widget."},
                    "palette_flip": {"type": "boolean", "description": "Boolean indicating whether to flip the palette tones."}
                }},
                "view": {"type": "object", "description": "Widget view.", "required": ["focus"], "properties": {
                    "focus": {"type": "string", "description": "Two-letter ISO 3166 country code or \"WORLD\"."}
                }},
                "custom_links": {"type": "array", "description": "List of custom links.", "items": {"type": "object"}},
                "title": {"type": "string", "description": "The title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "group" => serde_json::json!({
            "type": "object",
            "required": ["type", "layout_type", "widgets"],
            "properties": {
                "type": {"type": "string", "enum": ["group"], "description": "Type of the group widget."},
                "layout_type": {"type": "string", "enum": ["ordered", "free"], "description": "Layout type of the group widget."},
                "widgets": {"type": "array", "description": "List of widgets in the group.", "items": {"type": "object"}},
                "title": {"type": "string", "description": "Title of the group widget."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "background_color": {"type": "string", "description": "Background color of the group widget header."},
                "banner_img": {"type": "string", "description": "URL of image to display as a banner for the group."},
                "show_title": {"type": "boolean", "default": true, "description": "Whether to show the title or not."}
            }
        }),
        "heatmap" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests"],
            "properties": {
                "type": {"type": "string", "enum": ["heatmap"], "description": "Type of the heat map widget."},
                "requests": {"type": "array", "description": "List of heat map widget requests.", "items": {"type": "object", "properties": {"q": {"type": "string", "description": "Metric query."}}}},
                "markers": {"type": "array", "description": "List of markers to display on the graph.", "items": {"type": "object"}},
                "xaxis": {"type": "object", "description": "X-axis configuration."},
                "yaxis": {"type": "object", "description": "Y-axis configuration."},
                "custom_links": {"type": "array", "description": "List of custom links.", "items": {"type": "object"}},
                "show_legend": {"type": "boolean", "description": "Whether or not to display the legend on this widget."},
                "legend_size": {"type": "string", "enum": ["0", "2", "4", "8", "16", "auto"], "description": "Legend size."},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "hostmap" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests"],
            "properties": {
                "type": {"type": "string", "enum": ["hostmap"], "description": "Type of the host map widget."},
                "requests": {"type": "object", "description": "Host map request configuration.", "properties": {
                    "fill": {"type": "object", "description": "Request for the fill colour of each node.", "properties": {"q": {"type": "string", "description": "Metric query for the fill."}}},
                    "size": {"type": "object", "description": "Request for the size of each node.", "properties": {"q": {"type": "string", "description": "Metric query for the size."}}}
                }},
                "node_type": {"type": "string", "enum": ["host", "container"], "description": "Type of nodes to display: \"host\" or \"container\"."},
                "no_metric_hosts": {"type": "boolean", "description": "Whether to show hosts with no metrics."},
                "no_group_hosts": {"type": "boolean", "description": "Whether to show hosts that don't fit in a group."},
                "group": {"type": "array", "description": "List of tag prefixes to group by.", "items": {"type": "string"}},
                "scope": {"type": "array", "description": "List of tags used to filter the map.", "items": {"type": "string"}},
                "style": {"type": "object", "description": "Widget style.", "properties": {
                    "palette": {"type": "string", "description": "Color palette."},
                    "palette_flip": {"type": "boolean", "description": "Whether to flip the palette."},
                    "fill_min": {"type": "string", "description": "Minimum value for the fill colour scale."},
                    "fill_max": {"type": "string", "description": "Maximum value for the fill colour scale."}
                }},
                "custom_links": {"type": "array", "description": "List of custom links.", "items": {"type": "object"}},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "notes": {"type": "string", "description": "Notes displayed on the title."}
            }
        }),
        "iframe" => serde_json::json!({
            "type": "object",
            "required": ["type", "url"],
            "properties": {
                "type": {"type": "string", "enum": ["iframe"], "description": "Type of the iframe widget."},
                "url": {"type": "string", "description": "URL of the iframe."}
            }
        }),
        "image" => serde_json::json!({
            "type": "object",
            "required": ["type", "url"],
            "properties": {
                "type": {"type": "string", "enum": ["image"], "description": "Type of the image widget."},
                "url": {"type": "string", "description": "URL of the image."},
                "url_dark_theme": {"type": "string", "description": "URL of the image to display in dark mode."},
                "sizing": {"type": "string", "enum": ["fill", "contain", "cover", "none", "scale-down", "zoom"], "description": "How the image is sized within the widget."},
                "margin": {"type": "string", "enum": ["sm", "md", "lg"], "description": "Margin around the image."},
                "has_background": {"type": "boolean", "default": true, "description": "Whether to display a background or not."},
                "has_border": {"type": "boolean", "default": true, "description": "Whether to display a border or not."},
                "horizontal_align": {"type": "string", "enum": ["center", "left", "right"], "description": "Horizontal alignment of the image."},
                "vertical_align": {"type": "string", "enum": ["top", "center", "bottom"], "description": "Vertical alignment of the image."}
            }
        }),
        "list_stream" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests"],
            "properties": {
                "type": {"type": "string", "enum": ["list_stream"], "description": "Type of the list stream widget."},
                "requests": {
                    "type": "array",
                    "description": "Request payload. Exactly one item.",
                    "minItems": 1,
                    "maxItems": 1,
                    "items": {"type": "object", "properties": {
                        "response_format": {"type": "string", "enum": ["event_list"], "description": "Response format."},
                        "columns": {"type": "array", "description": "Columns to display.", "items": {"type": "object", "properties": {
                            "field": {"type": "string", "description": "Column field name."},
                            "width": {"type": "string", "enum": ["auto", "compact", "full"], "description": "Column width."}
                        }}},
                        "query": {"type": "object", "description": "Query definition.", "properties": {
                            "data_source": {"type": "string", "description": "Data source, e.g. \"apm_issue_stream\", \"logs_stream\"."},
                            "query_string": {"type": "string", "description": "Query filter string."}
                        }}
                    }}
                },
                "title": {"type": "string", "description": "Title of the widget."},
                "show_legend": {"type": "boolean", "description": "Whether or not to display the legend on this widget."},
                "legend_size": {"type": "string", "enum": ["0", "2", "4", "8", "16", "auto"], "description": "Legend size."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "log_stream" => serde_json::json!({
            "type": "object",
            "required": ["type"],
            "properties": {
                "type": {"type": "string", "enum": ["log_stream"], "description": "Type of the log stream widget."},
                "indexes": {"type": "array", "description": "Array of index names to query. Use [] to query all indexes at once.", "items": {"type": "string"}},
                "query": {"type": "string", "description": "Query to filter the log stream with."},
                "columns": {"type": "array", "description": "Which columns to display on the widget.", "items": {"type": "string"}},
                "show_date_column": {"type": "boolean", "description": "Whether to show the date column or not."},
                "show_message_column": {"type": "boolean", "description": "Whether to show the message column or not."},
                "message_display": {"type": "string", "enum": ["inline", "expanded-md", "expanded-lg"], "description": "Amount of log lines to display."},
                "sort": {"type": "object", "description": "Sort configuration.", "properties": {
                    "column": {"type": "string", "description": "Column to sort by."},
                    "order": {"type": "string", "enum": ["asc", "desc"], "description": "Sort order."}
                }},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "manage_status" => serde_json::json!({
            "type": "object",
            "required": ["type", "query"],
            "properties": {
                "type": {"type": "string", "enum": ["manage_status"], "description": "Type of the monitor summary widget."},
                "query": {"type": "string", "description": "Query to filter the monitors with."},
                "sort": {"type": "string", "description": "Sort order for the monitors.", "enum": ["name", "group", "status", "tags", "triggered", "group,asc", "name,asc", "status,asc", "tags,asc", "triggered,asc", "group,desc", "name,desc", "status,desc", "tags,desc", "triggered,desc"]},
                "summary_type": {"type": "string", "enum": ["monitors", "groups", "combined"], "description": "Summary type."},
                "display_format": {"type": "string", "enum": ["counts", "countsAndList", "list"], "description": "Display format."},
                "color_preference": {"type": "string", "enum": ["background", "text"], "description": "Whether to colorize text or background."},
                "hide_zero_counts": {"type": "boolean", "description": "Whether to show counts of 0 or not."},
                "show_last_triggered": {"type": "boolean", "description": "Whether to show the elapsed time since the monitor/group triggered."},
                "show_priority": {"type": "boolean", "default": false, "description": "Whether to show the priorities column."},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."}
            }
        }),
        "note" => serde_json::json!({
            "type": "object",
            "required": ["type", "content"],
            "properties": {
                "type": {"type": "string", "enum": ["note"], "description": "Type of the note widget."},
                "content": {"type": "string", "description": "Content of the note."},
                "background_color": {"type": "string", "description": "Background color of the note."},
                "font_size": {"type": "string", "description": "Size of the text."},
                "text_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the text."},
                "vertical_align": {"type": "string", "enum": ["top", "center", "bottom"], "description": "Vertical alignment of the text."},
                "has_padding": {"type": "boolean", "default": true, "description": "Whether to add padding or not."},
                "show_tick": {"type": "boolean", "description": "Whether to show a tick or not."},
                "tick_pos": {"type": "string", "description": "Where to position the tick on an edge, as a percentage (e.g. \"50%\")."},
                "tick_edge": {"type": "string", "enum": ["bottom", "left", "right", "top"], "description": "Edge where the tick should be placed."}
            }
        }),
        "powerpack" => serde_json::json!({
            "type": "object",
            "required": ["type", "powerpack_id"],
            "properties": {
                "type": {"type": "string", "enum": ["powerpack"], "description": "Type of the powerpack widget."},
                "powerpack_id": {"type": "string", "description": "UUID of the associated powerpack."},
                "title": {"type": "string", "description": "Title of the widget."},
                "background_color": {"type": "string", "description": "Background color of the powerpack title."},
                "banner_img": {"type": "string", "description": "URL of image to display as a banner for the powerpack."},
                "template_variables": {"type": "object", "description": "Template variable values to apply to the powerpack."},
                "show_title": {"type": "boolean", "default": true, "description": "Whether to show the title or not."}
            }
        }),
        "query_value" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests"],
            "properties": {
                "type": {"type": "string", "enum": ["query_value"], "description": "Type of the query value widget."},
                "requests": {"type": "array", "description": "Widget requests.", "items": {"type": "object", "properties": {
                    "q": {"type": "string", "description": "Metric query."},
                    "aggregator": {"type": "string", "enum": ["avg", "last", "max", "min", "sum", "percentile"], "description": "Aggregation method."}
                }}},
                "autoscale": {"type": "boolean", "description": "Whether to use auto-scaling or not."},
                "custom_unit": {"type": "string", "description": "Display a unit of your choice on the widget."},
                "precision": {"type": "integer", "description": "Number of decimals to show. If not defined, the widget uses the raw value."},
                "text_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the value."},
                "custom_links": {"type": "array", "description": "List of custom links.", "items": {"type": "object"}},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "query_table" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests"],
            "properties": {
                "type": {"type": "string", "enum": ["query_table"], "description": "Type of the table widget."},
                "requests": {"type": "array", "description": "Widget requests.", "items": {"type": "object", "properties": {
                    "q": {"type": "string", "description": "Metric query."},
                    "aggregator": {"type": "string", "enum": ["avg", "last", "max", "min", "sum", "percentile"], "description": "Aggregation method."},
                    "alias": {"type": "string", "description": "Column alias to display."}
                }}},
                "has_search_bar": {"type": "string", "enum": ["always", "never", "auto"], "description": "Controls the display of the search bar."},
                "custom_links": {"type": "array", "description": "List of custom links.", "items": {"type": "object"}},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "run_workflow" => serde_json::json!({
            "type": "object",
            "required": ["type", "workflow_id"],
            "properties": {
                "type": {"type": "string", "enum": ["run_workflow"], "description": "Type of the run workflow widget."},
                "workflow_id": {"type": "string", "description": "Workflow ID to execute."},
                "inputs": {"type": "array", "description": "Array of workflow inputs to map to dashboard template variables.", "items": {"type": "object", "properties": {
                    "name": {"type": "string", "description": "Name of the workflow input parameter."},
                    "value": {"type": "string", "description": "Dashboard template variable name to use as the input value."}
                }}},
                "custom_links": {"type": "array", "description": "List of custom links.", "items": {"type": "object"}},
                "title": {"type": "string", "description": "Title of your widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "scatterplot" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests"],
            "properties": {
                "type": {"type": "string", "enum": ["scatterplot"], "description": "Type of the scatter plot widget."},
                "requests": {"type": "object", "description": "Scatter plot request configuration.", "properties": {
                    "x": {"type": "object", "description": "Request for the X-axis metric.", "properties": {
                        "q": {"type": "string", "description": "Metric query."},
                        "aggregator": {"type": "string", "enum": ["avg", "last", "max", "min", "sum"], "description": "Aggregation method."}
                    }},
                    "y": {"type": "object", "description": "Request for the Y-axis metric.", "properties": {
                        "q": {"type": "string", "description": "Metric query."},
                        "aggregator": {"type": "string", "enum": ["avg", "last", "max", "min", "sum"], "description": "Aggregation method."}
                    }}
                }},
                "xaxis": {"type": "object", "description": "X-axis configuration."},
                "yaxis": {"type": "object", "description": "Y-axis configuration."},
                "color_by_groups": {"type": "array", "description": "List of groups used for colors.", "items": {"type": "string"}},
                "custom_links": {"type": "array", "description": "List of custom links.", "items": {"type": "object"}},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "servicemap" => serde_json::json!({
            "type": "object",
            "required": ["type", "filters", "service"],
            "properties": {
                "type": {"type": "string", "enum": ["servicemap"], "description": "Type of the service map widget."},
                "filters": {"type": "array", "description": "Your environment and primary tag (or * if enabled for your account).", "items": {"type": "string"}},
                "service": {"type": "string", "description": "The ID of the service you want to map."},
                "custom_links": {"type": "array", "description": "List of custom links.", "items": {"type": "object"}},
                "title": {"type": "string", "description": "The title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."}
            }
        }),
        "slo" => serde_json::json!({
            "type": "object",
            "required": ["type", "view_type"],
            "properties": {
                "type": {"type": "string", "enum": ["slo"], "description": "Type of the SLO widget."},
                "view_type": {"type": "string", "default": "detail", "description": "Type of view displayed by the widget. Use \"detail\"."},
                "slo_id": {"type": "string", "description": "ID of the SLO to display."},
                "show_error_budget": {"type": "boolean", "description": "Whether to show the error budget."},
                "view_mode": {"type": "string", "enum": ["overall", "component", "both"], "description": "View mode."},
                "global_time_target": {"type": "string", "description": "Global time target."},
                "time_windows": {"type": "array", "description": "Time windows to monitor.", "items": {"type": "string", "enum": ["7d", "30d", "90d", "week_to_date", "previous_week", "month_to_date", "previous_month", "global_time"]}},
                "additional_query_filters": {"type": "string", "description": "Additional filters applied to the SLO query."},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."}
            }
        }),
        "slo_list" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests"],
            "properties": {
                "type": {"type": "string", "enum": ["slo_list"], "description": "Type of the SLO list widget."},
                "requests": {"type": "array", "description": "Array of one request object to display in the widget.", "items": {"type": "object", "properties": {
                    "request_type": {"type": "string", "enum": ["slo_list"], "description": "Request type."},
                    "query": {"type": "object", "description": "SLO list query.", "properties": {
                        "q": {"type": "string", "description": "Query string to filter SLOs."},
                        "limit": {"type": "integer", "description": "Maximum number of results to display."}
                    }}
                }}},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."}
            }
        }),
        "sunburst" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests"],
            "properties": {
                "type": {"type": "string", "enum": ["sunburst"], "description": "Type of the sunburst widget."},
                "requests": {"type": "array", "description": "List of sunburst widget requests.", "items": {"type": "object", "properties": {"q": {"type": "string", "description": "Metric query."}}}},
                "hide_total": {"type": "boolean", "description": "Whether to hide the total value in this widget."},
                "legend": {"type": "object", "description": "Legend configuration.", "properties": {
                    "type": {"type": "string", "enum": ["inline", "table", "none"], "description": "Legend type."}
                }},
                "custom_links": {"type": "array", "description": "List of custom links.", "items": {"type": "object"}},
                "title": {"type": "string", "description": "Title of your widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "timeseries" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests"],
            "properties": {
                "type": {"type": "string", "enum": ["timeseries"], "description": "Type of the timeseries widget."},
                "requests": {"type": "array", "description": "List of timeseries widget requests.", "items": {"type": "object", "properties": {
                    "q": {"type": "string", "description": "Metric query string."},
                    "display_type": {"type": "string", "enum": ["area", "bars", "line", "overlay"], "description": "Type of display used for the request."}
                }}},
                "yaxis": {"type": "object", "description": "Y-axis configuration."},
                "right_yaxis": {"type": "object", "description": "Right Y-axis configuration."},
                "markers": {"type": "array", "description": "List of markers to display on the graph.", "items": {"type": "object"}},
                "custom_links": {"type": "array", "description": "List of custom links.", "items": {"type": "object"}},
                "show_legend": {"type": "boolean", "description": "(screenboard only) Show the legend for this widget."},
                "legend_size": {"type": "string", "enum": ["0", "2", "4", "8", "16", "auto"], "description": "Legend size."},
                "legend_layout": {"type": "string", "enum": ["auto", "horizontal", "vertical"], "description": "Layout of the legend."},
                "legend_columns": {"type": "array", "description": "Columns to display in the legend.", "items": {"type": "string", "enum": ["value", "avg", "sum", "min", "max"]}},
                "title": {"type": "string", "description": "Title of your widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "toplist" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests"],
            "properties": {
                "type": {"type": "string", "enum": ["toplist"], "description": "Type of the top list widget."},
                "requests": {"type": "array", "description": "List of top list widget requests.", "items": {"type": "object", "properties": {"q": {"type": "string", "description": "Metric query."}}}},
                "style": {"type": "object", "description": "Style configuration.", "properties": {
                    "palette": {"type": "string", "description": "Color palette."},
                    "palette_flip": {"type": "boolean", "description": "Whether to flip the palette."}
                }},
                "custom_links": {"type": "array", "description": "List of custom links.", "items": {"type": "object"}},
                "title": {"type": "string", "description": "Title of your widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "topology_map" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests"],
            "properties": {
                "type": {"type": "string", "enum": ["topology_map"], "description": "Type of the topology map widget."},
                "requests": {"type": "array", "description": "One or more topology requests.", "items": {"type": "object", "properties": {
                    "request_type": {"type": "string", "enum": ["topology"], "description": "Request type."},
                    "query": {"type": "object", "description": "Topology query.", "properties": {
                        "service": {"type": "string", "description": "Service name."},
                        "env": {"type": "string", "description": "Environment."},
                        "data_source": {"type": "string", "enum": ["service_map", "data_streams"], "description": "Data source."}
                    }}
                }}},
                "title": {"type": "string", "description": "Title of your widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "custom_links": {"type": "array", "description": "List of custom links.", "items": {"type": "object"}}
            }
        }),
        "trace_service" => serde_json::json!({
            "type": "object",
            "required": ["type", "env", "service", "span_name"],
            "properties": {
                "type": {"type": "string", "enum": ["trace_service"], "description": "Type of the service summary widget."},
                "env": {"type": "string", "description": "APM environment."},
                "service": {"type": "string", "description": "APM service."},
                "span_name": {"type": "string", "description": "APM span name."},
                "show_hits": {"type": "boolean", "description": "Whether to show the hits metrics or not."},
                "show_errors": {"type": "boolean", "description": "Whether to show the error metrics or not."},
                "show_latency": {"type": "boolean", "description": "Whether to show the latency metrics or not."},
                "show_breakdown": {"type": "boolean", "description": "Whether to show the latency breakdown or not."},
                "show_distribution": {"type": "boolean", "description": "Whether to show the latency distribution or not."},
                "show_resource_list": {"type": "boolean", "description": "Whether to show the resource list or not."},
                "size_format": {"type": "string", "enum": ["small", "medium", "large"], "description": "Widget size format."},
                "display_format": {"type": "string", "enum": ["one_column", "two_column", "three_column"], "description": "Number of columns to display."},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "treemap" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests"],
            "properties": {
                "type": {"type": "string", "enum": ["treemap"], "description": "Type of the treemap widget."},
                "requests": {"type": "array", "description": "List of treemap widget requests.", "items": {"type": "object", "properties": {"q": {"type": "string", "description": "Metric query."}}}},
                "size_by": {"type": "string", "enum": ["pct_of_parent", "pct_of_total"], "description": "What the size of each group represents."},
                "color_by": {"type": "string", "enum": ["user", "facet"], "description": "What the color of each group represents."},
                "group_by": {"type": "array", "description": "Group by configuration.", "items": {"type": "object"}},
                "custom_links": {"type": "array", "description": "List of custom links.", "items": {"type": "object"}},
                "title": {"type": "string", "description": "Title of your widget."},
                "description": {"type": "string", "description": "Description of the widget."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        "wildcard" => serde_json::json!({
            "type": "object",
            "required": ["type", "requests", "specification"],
            "properties": {
                "type": {"type": "string", "enum": ["wildcard"], "description": "Type of the wildcard widget."},
                "requests": {"type": "array", "description": "List of data requests for the wildcard widget.", "items": {"type": "object", "properties": {
                    "formulas": {"type": "array", "description": "List of formulas.", "items": {"type": "object"}},
                    "queries": {"type": "array", "description": "List of queries.", "items": {"type": "object", "properties": {
                        "data_source": {"type": "string", "description": "Data source, e.g. \"metrics\"."},
                        "name": {"type": "string", "description": "Query alias name used in formulas."},
                        "query": {"type": "string", "description": "Metric query string."},
                        "aggregator": {"type": "string", "description": "Aggregation method."}
                    }}},
                    "response_format": {"type": "string", "enum": ["timeseries", "scalar", "event_list"], "description": "Response format."}
                }}},
                "specification": {"type": "object", "description": "Vega or Vega-Lite specification for the custom visualization."},
                "custom_links": {"type": "array", "description": "List of custom links.", "items": {"type": "object"}},
                "title": {"type": "string", "description": "Title of the widget."},
                "title_size": {"type": "string", "description": "Size of the title."},
                "title_align": {"type": "string", "enum": ["left", "center", "right"], "description": "Horizontal alignment of the title."},
                "time": {"type": "object", "description": "Time setting for the widget.", "properties": {"live_span": {"type": "string", "description": "Timeframe, e.g. \"1h\", \"4h\", \"1d\"."}}}
            }
        }),
        _ => return None,
    };

    Some(serde_json::json!({
        "type": "object",
        "required": ["definition"],
        "properties": {
            "id": {
                "type": "integer",
                "description": "ID of the widget. Assigned by the API; omit when creating."
            },
            "layout": {
                "type": "object",
                "description": "Position and size. Required for free-layout dashboards; omit for ordered-layout.",
                "required": ["x", "y", "width", "height"],
                "properties": {
                    "x": {"type": "integer", "description": "Position on the x (horizontal) axis. Should be a non-negative integer."},
                    "y": {"type": "integer", "description": "Position on the y (vertical) axis. Should be a non-negative integer."},
                    "width": {"type": "integer", "description": "Width of the widget. Should be a non-negative integer."},
                    "height": {"type": "integer", "description": "Height of the widget. Should be a non-negative integer."}
                }
            },
            "definition": definition
        }
    }))
}

fn dashboard_url_with_time(base_url: &str, from: &str, to: &str, live: bool) -> Result<String> {
    let mut url = Url::parse(base_url).map_err(|e| {
        anyhow::anyhow!("dashboard response included invalid url {base_url:?}: {e}")
    })?;
    let mut query_pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| key != "from_ts" && key != "to_ts" && key != "live")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    query_pairs.push(("from_ts".to_string(), from.to_string()));
    query_pairs.push(("to_ts".to_string(), to.to_string()));
    query_pairs.push(("live".to_string(), live.to_string()));
    url.query_pairs_mut().clear().extend_pairs(query_pairs);
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_dashboards_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"dashboards": []}"#).await;

        let result = super::list(&cfg).await;
        assert!(result.is_ok(), "dashboards list failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_dashboards_get() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(
            &mut server,
            "GET",
            r#"{"id": "abc-123", "title": "Test Dashboard", "layout_type": "ordered", "widgets": []}"#,
        )
        .await;

        let result = super::get(&cfg, "abc-123").await;
        assert!(result.is_ok(), "dashboards get failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_dashboards_delete() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(
            &mut server,
            "DELETE",
            r#"{"deleted_dashboard_id": "abc-123"}"#,
        )
        .await;

        let result = super::delete(&cfg, "abc-123").await;
        assert!(
            result.is_ok(),
            "dashboards delete failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[test]
    fn test_dashboard_url_with_time_adds_live_window() {
        let url = super::dashboard_url_with_time(
            "https://app.datadoghq.com/dashboard/abc-123/test-dashboard?tpl_var_env=prod",
            "now-1w",
            "now",
            true,
        )
        .expect("dashboard URL should be valid");

        assert_eq!(
            url,
            "https://app.datadoghq.com/dashboard/abc-123/test-dashboard?tpl_var_env=prod&from_ts=now-1w&to_ts=now&live=true"
        );
    }

    #[test]
    fn test_dashboard_url_with_time_replaces_existing_time_params() {
        let url = super::dashboard_url_with_time(
            "https://app.datadoghq.com/dashboard/abc-123/test-dashboard?from_ts=old&to_ts=old&live=false",
            "now-1w",
            "now",
            true,
        )
        .expect("dashboard URL should be valid");

        assert_eq!(
            url,
            "https://app.datadoghq.com/dashboard/abc-123/test-dashboard?from_ts=now-1w&to_ts=now&live=true"
        );
    }

    // ---- widget_types / widget_schema (unit, no server) ----

    #[test]
    fn test_widget_types_list_not_empty() {
        assert!(
            super::WIDGET_TYPES.len() >= 20,
            "expected at least 20 widget types, got {}",
            super::WIDGET_TYPES.len()
        );
        assert!(super::WIDGET_TYPES.iter().any(|(t, _)| *t == "timeseries"));
        assert!(super::WIDGET_TYPES.iter().any(|(t, _)| *t == "query_value"));
        assert!(super::WIDGET_TYPES.iter().any(|(t, _)| *t == "note"));
    }

    #[test]
    fn test_widget_template_returns_known_skeleton() {
        let tmpl = super::widget_template("timeseries").expect("timeseries should have a template");
        let ty = tmpl
            .get("definition")
            .and_then(|d| d.get("type"))
            .and_then(|t| t.as_str())
            .expect("template must have definition.type");
        assert_eq!(ty, "timeseries");
        assert!(
            tmpl["definition"].get("requests").is_some(),
            "timeseries template must include requests"
        );
    }

    #[test]
    fn test_widget_template_generic_fallback() {
        // "geomap" is a known type but has no custom skeleton
        let tmpl = super::widget_template("geomap").expect("geomap should have a fallback");
        assert_eq!(tmpl["definition"]["type"], "geomap");
    }

    #[test]
    fn test_widget_template_unknown_type_returns_none() {
        assert!(
            super::widget_template("not_a_real_widget_type").is_none(),
            "unknown type must return None"
        );
    }

    #[test]
    fn test_validate_widget_rejects_unparsed_object() {
        // An unknown type deserialized through the SDK falls through to
        // WidgetDefinition::UnparsedObject and must be rejected.
        let json = r#"{"definition":{"type":"not_a_real_type"}}"#;
        let widget: datadog_api_client::datadogV1::model::Widget =
            serde_json::from_str(json).expect("from_str must not fail even for unknown types");
        let result = super::validate_widget(&widget);
        assert!(
            result.is_err(),
            "validate_widget must reject UnparsedObject definitions"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("unknown or invalid"),
            "error message should describe the problem, got: {msg}"
        );
    }

    // ---- locate_widget_index (unit, no server) ----

    fn make_widget(id: Option<i64>) -> datadog_api_client::datadogV1::model::Widget {
        let json = format!(
            r#"{{"definition":{{"type":"timeseries","requests":[{{"q":""}}]}},"id":{}}}"#,
            match id {
                Some(n) => n.to_string(),
                None => "null".to_string(),
            }
        );
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn test_locate_by_index_ok() {
        let widgets = vec![make_widget(None), make_widget(None)];
        assert_eq!(
            super::locate_widget_index(&widgets, None, Some(1)).unwrap(),
            1
        );
    }

    #[test]
    fn test_locate_by_index_out_of_range() {
        let widgets = vec![make_widget(None)];
        assert!(super::locate_widget_index(&widgets, None, Some(5)).is_err());
    }

    #[test]
    fn test_locate_by_id_found() {
        let widgets = vec![make_widget(Some(1)), make_widget(Some(2))];
        assert_eq!(
            super::locate_widget_index(&widgets, Some(2), None).unwrap(),
            1
        );
    }

    #[test]
    fn test_locate_by_id_not_found() {
        let widgets = vec![make_widget(Some(1))];
        assert!(super::locate_widget_index(&widgets, Some(99), None).is_err());
    }

    #[test]
    fn test_locate_by_id_ambiguous() {
        let widgets = vec![make_widget(Some(7)), make_widget(Some(7))];
        let err = super::locate_widget_index(&widgets, Some(7), None).unwrap_err();
        assert!(
            err.to_string().contains("ambiguous"),
            "expected ambiguous error, got: {err}"
        );
    }

    // ---- widget_list (mockito) ----

    const DASHBOARD_WITH_WIDGETS: &str = r#"{
        "id": "abc-123",
        "title": "Test",
        "layout_type": "ordered",
        "widgets": [
            {"id": 1, "definition": {"type": "timeseries", "title": "My Chart", "requests": [{"q": "avg:system.cpu.user{*}"}]}},
            {"id": 2, "definition": {"type": "note", "content": "Hello"}}
        ]
    }"#;

    #[tokio::test]
    async fn test_widget_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", DASHBOARD_WITH_WIDGETS).await;

        let result = super::widget_list(&cfg, "abc-123").await;
        assert!(result.is_ok(), "widget_list failed: {:?}", result.err());
        cleanup_env();
    }

    // ---- widget_get (mockito) ----

    #[tokio::test]
    async fn test_widget_get_by_index() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", DASHBOARD_WITH_WIDGETS).await;

        let result = super::widget_get(&cfg, "abc-123", None, Some(0)).await;
        assert!(
            result.is_ok(),
            "widget_get by index failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_widget_get_by_id() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", DASHBOARD_WITH_WIDGETS).await;

        let result = super::widget_get(&cfg, "abc-123", Some(2), None).await;
        assert!(
            result.is_ok(),
            "widget_get by id failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    // ---- widget_add (mockito) ----

    #[tokio::test]
    async fn test_widget_add() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock_get = mock_any(&mut server, "GET", DASHBOARD_WITH_WIDGETS).await;
        let _mock_put = mock_any(&mut server, "PUT", DASHBOARD_WITH_WIDGETS).await;

        let widget_json =
            r#"{"definition":{"type":"timeseries","requests":[{"q":"avg:system.cpu.user{*}"}]}}"#;
        let path = write_temp_json("test_widget_add.json", widget_json);
        let result = super::widget_add(&cfg, "abc-123", path.to_str().unwrap()).await;
        assert!(result.is_ok(), "widget_add failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_widget_add_invalid_json_fails() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let path = write_temp_json("test_widget_add_bad.json", "not json at all");
        let result = super::widget_add(&cfg, "abc-123", path.to_str().unwrap()).await;
        assert!(result.is_err(), "widget_add should fail on bad JSON");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_widget_add_unknown_type_fails() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let widget_json = r#"{"definition":{"type":"totally_unknown_type"}}"#;
        let path = write_temp_json("test_widget_add_unknown.json", widget_json);
        let result = super::widget_add(&cfg, "abc-123", path.to_str().unwrap()).await;
        assert!(
            result.is_err(),
            "widget_add should reject an unknown widget type"
        );
        cleanup_env();
    }

    // ---- widget_update (mockito) ----

    #[tokio::test]
    async fn test_widget_update_by_index() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock_get = mock_any(&mut server, "GET", DASHBOARD_WITH_WIDGETS).await;
        let _mock_put = mock_any(&mut server, "PUT", DASHBOARD_WITH_WIDGETS).await;

        let widget_json = r#"{"definition":{"type":"note","content":"Updated"}}"#;
        let path = write_temp_json("test_widget_update.json", widget_json);
        let result =
            super::widget_update(&cfg, "abc-123", None, Some(1), path.to_str().unwrap()).await;
        assert!(result.is_ok(), "widget_update failed: {:?}", result.err());
        cleanup_env();
    }

    // ---- widget_remove (mockito) ----

    #[tokio::test]
    async fn test_widget_remove_by_id() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock_get = mock_any(&mut server, "GET", DASHBOARD_WITH_WIDGETS).await;
        let _mock_put = mock_any(&mut server, "PUT", DASHBOARD_WITH_WIDGETS).await;

        let result = super::widget_remove(&cfg, "abc-123", Some(1), None).await;
        assert!(result.is_ok(), "widget_remove failed: {:?}", result.err());
        cleanup_env();
    }

    // ---- widget_types / widget_schema (with config, no server) ----

    #[tokio::test]
    async fn test_widget_types_command_ok() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused.local");
        let result = super::widget_types(&cfg);
        assert!(result.is_ok(), "widget_types failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_widget_schema_timeseries_ok() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused.local");
        let result = super::widget_schema(&cfg, "timeseries");
        assert!(
            result.is_ok(),
            "widget_schema(timeseries) failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_widget_schema_generic_fallback_ok() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused.local");
        let result = super::widget_schema(&cfg, "geomap");
        assert!(
            result.is_ok(),
            "widget_schema(geomap) failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_widget_schema_unknown_type_fails() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused.local");
        let result = super::widget_schema(&cfg, "totally_bogus_widget_type");
        assert!(
            result.is_err(),
            "widget_schema should fail for unknown types"
        );
        cleanup_env();
    }
}

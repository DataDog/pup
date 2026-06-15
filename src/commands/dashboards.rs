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
        std::fs::read(file)
            .map_err(|e| anyhow::anyhow!("failed to read --file {file:?}: {e}"))?
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
            let def_val =
                serde_json::to_value(&w.definition).unwrap_or(serde_json::Value::Null);
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
    let tmpl = widget_template(type_str).expect("type is in WIDGET_TYPES so template exists");
    formatter::output(
        cfg,
        &serde_json::json!({
            "type": type_str,
            "description": description,
            "template": tmpl,
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
        assert_eq!(super::locate_widget_index(&widgets, None, Some(1)).unwrap(), 1);
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
        assert!(result.is_ok(), "widget_get by index failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_widget_get_by_id() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", DASHBOARD_WITH_WIDGETS).await;

        let result = super::widget_get(&cfg, "abc-123", Some(2), None).await;
        assert!(result.is_ok(), "widget_get by id failed: {:?}", result.err());
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

        let widget_json = r#"{"definition":{"type":"timeseries","requests":[{"q":"avg:system.cpu.user{*}"}]}}"#;
        let path = write_temp_json("test_widget_add.json", widget_json);
        let result = super::widget_add(&cfg, "abc-123", path.to_str().unwrap()).await;
        assert!(result.is_ok(), "widget_add failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_widget_add_invalid_json_fails() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let path = write_temp_json("test_widget_add_bad.json", "not json at all");
        let result = super::widget_add(&cfg, "abc-123", path.to_str().unwrap()).await;
        assert!(result.is_err(), "widget_add should fail on bad JSON");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_widget_add_unknown_type_fails() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
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
        assert!(result.is_ok(), "widget_schema(timeseries) failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_widget_schema_generic_fallback_ok() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused.local");
        let result = super::widget_schema(&cfg, "geomap");
        assert!(result.is_ok(), "widget_schema(geomap) failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_widget_schema_unknown_type_fails() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused.local");
        let result = super::widget_schema(&cfg, "totally_bogus_widget_type");
        assert!(result.is_err(), "widget_schema should fail for unknown types");
        cleanup_env();
    }
}

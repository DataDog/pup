use anyhow::{bail, Result};
use datadog_api_client::datadogV2::api_rum::{ListRUMEventsOptionalParams, RUMAPI};
use datadog_api_client::datadogV2::api_rum_metrics::RumMetricsAPI;
use datadog_api_client::datadogV2::api_rum_replay_heatmaps::{
    ListReplayHeatmapSnapshotsOptionalParams, RumReplayHeatmapsAPI,
};
use datadog_api_client::datadogV2::api_rum_replay_playlists::{
    AddRumReplaySessionToPlaylistOptionalParams, ListRumReplayPlaylistSessionsOptionalParams,
    ListRumReplayPlaylistsOptionalParams, RumReplayPlaylistsAPI,
};
use datadog_api_client::datadogV2::api_rum_replay_sessions::{
    GetSegmentsOptionalParams, RumReplaySessionsAPI,
};
use datadog_api_client::datadogV2::api_rum_replay_viewership::{
    ListRumReplaySessionWatchersOptionalParams,
    ListRumReplayViewershipHistorySessionsOptionalParams, RumReplayViewershipAPI,
};
use datadog_api_client::datadogV2::api_rum_retention_filters::RumRetentionFiltersAPI;
use datadog_api_client::datadogV2::model::{
    Playlist, RUMApplicationCreate, RUMApplicationCreateAttributes, RUMApplicationCreateRequest,
    RUMApplicationCreateType, RUMApplicationUpdateRequest, RUMQueryFilter, RUMQueryPageOptions,
    RUMSearchEventsRequest, RUMSort, RumMetricCreateRequest, RumMetricUpdateRequest,
    RumRetentionFilterCreateRequest, RumRetentionFilterUpdateRequest, SessionIdArray, Watch,
    WatchData, WatchDataType,
};

use crate::config::Config;
use crate::formatter;
use crate::raw_client;
use crate::util_ext;

pub async fn apps_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(RUMAPI, cfg);
    let resp = api
        .get_rum_applications()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list RUM apps: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn apps_get(cfg: &Config, app_id: &str) -> Result<()> {
    let api = crate::make_api!(RUMAPI, cfg);
    let resp = api
        .get_rum_application(app_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get RUM app: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn apps_create(cfg: &Config, name: &str, app_type: Option<String>) -> Result<()> {
    let api = crate::make_api!(RUMAPI, cfg);
    let mut attrs = RUMApplicationCreateAttributes::new(name.to_string());
    if let Some(t) = app_type {
        attrs = attrs.type_(t);
    }
    let data = RUMApplicationCreate::new(attrs, RUMApplicationCreateType::RUM_APPLICATION_CREATE);
    let body = RUMApplicationCreateRequest::new(data);
    let resp = api
        .create_rum_application(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create RUM app: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn apps_delete(cfg: &Config, app_id: &str) -> Result<()> {
    let api = crate::make_api!(RUMAPI, cfg);
    api.delete_rum_application(app_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete RUM app: {e:?}"))?;
    println!("Successfully deleted RUM application {app_id}");
    Ok(())
}

pub async fn events_list(
    cfg: &Config,
    query: Option<String>,
    from: String,
    to: String,
    limit: i32,
) -> Result<()> {
    let api = crate::make_api!(RUMAPI, cfg);

    let from_dt = util_ext::parse_time_to_datetime(&from)?;
    let to_dt = util_ext::parse_time_to_datetime(&to)?;

    let mut params = ListRUMEventsOptionalParams::default()
        .filter_from(from_dt)
        .filter_to(to_dt)
        .page_limit(limit);
    if let Some(q) = query {
        params = params.filter_query(q);
    }

    let resp = api
        .list_rum_events(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list RUM events: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn sessions_search(
    cfg: &Config,
    query: Option<String>,
    from: String,
    to: String,
    limit: i32,
) -> Result<()> {
    let api = crate::make_api!(RUMAPI, cfg);

    let from_ms = util_ext::parse_time_to_unix_millis(&from)?;
    let to_ms = util_ext::parse_time_to_unix_millis(&to)?;
    let from_str = chrono::DateTime::from_timestamp_millis(from_ms)
        .ok_or_else(|| anyhow::anyhow!("--from value {from_ms}ms is out of representable range"))?
        .to_rfc3339();
    let to_str = chrono::DateTime::from_timestamp_millis(to_ms)
        .ok_or_else(|| anyhow::anyhow!("--to value {to_ms}ms is out of representable range"))?
        .to_rfc3339();

    let mut filter = RUMQueryFilter::new().from(from_str).to(to_str);
    if let Some(q) = query {
        filter = filter.query(q);
    }

    let body = RUMSearchEventsRequest::new()
        .filter(filter)
        .page(RUMQueryPageOptions::new().limit(limit))
        .sort(RUMSort::TIMESTAMP_DESCENDING);

    let resp = api
        .search_rum_events(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search RUM sessions: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn apps_update(cfg: &Config, app_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(RUMAPI, cfg);
    let body: RUMApplicationUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_rum_application(app_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update RUM app: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- RUM Metrics ----

pub async fn metrics_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(RumMetricsAPI, cfg);
    let resp = api
        .list_rum_metrics()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list RUM metrics: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metrics_get(cfg: &Config, metric_id: &str) -> Result<()> {
    let api = crate::make_api!(RumMetricsAPI, cfg);
    let resp = api
        .get_rum_metric(metric_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get RUM metric: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metrics_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(RumMetricsAPI, cfg);
    let body: RumMetricCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_rum_metric(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create RUM metric: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metrics_update(cfg: &Config, metric_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(RumMetricsAPI, cfg);
    let body: RumMetricUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_rum_metric(metric_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update RUM metric: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metrics_delete(cfg: &Config, metric_id: &str) -> Result<()> {
    let api = crate::make_api!(RumMetricsAPI, cfg);
    api.delete_rum_metric(metric_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete RUM metric: {e:?}"))?;
    println!("RUM metric {metric_id} deleted.");
    Ok(())
}

// ---- RUM Retention Filters ----

pub async fn retention_filters_list(cfg: &Config, app_id: &str) -> Result<()> {
    let api = crate::make_api!(RumRetentionFiltersAPI, cfg);
    let resp = api
        .list_retention_filters(app_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list RUM retention filters: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn retention_filters_get(cfg: &Config, app_id: &str, filter_id: &str) -> Result<()> {
    let api = crate::make_api!(RumRetentionFiltersAPI, cfg);
    let resp = api
        .get_retention_filter(app_id.to_string(), filter_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get RUM retention filter: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn retention_filters_create(cfg: &Config, app_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(RumRetentionFiltersAPI, cfg);
    let body: RumRetentionFilterCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_retention_filter(app_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create RUM retention filter: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn retention_filters_update(
    cfg: &Config,
    app_id: &str,
    filter_id: &str,
    file: &str,
) -> Result<()> {
    let api = crate::make_api!(RumRetentionFiltersAPI, cfg);
    let body: RumRetentionFilterUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_retention_filter(app_id.to_string(), filter_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update RUM retention filter: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn retention_filters_delete(cfg: &Config, app_id: &str, filter_id: &str) -> Result<()> {
    let api = crate::make_api!(RumRetentionFiltersAPI, cfg);
    api.delete_retention_filter(app_id.to_string(), filter_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete RUM retention filter: {e:?}"))?;
    println!("RUM retention filter {filter_id} deleted.");
    Ok(())
}

// ---- RUM Sessions ----

pub async fn sessions_list(cfg: &Config, from: String, to: String, limit: i32) -> Result<()> {
    let api = crate::make_api!(RUMAPI, cfg);

    let from_ms = util_ext::parse_time_to_unix_millis(&from)?;
    let to_ms = util_ext::parse_time_to_unix_millis(&to)?;
    let from_str = chrono::DateTime::from_timestamp_millis(from_ms)
        .ok_or_else(|| anyhow::anyhow!("--from value {from_ms}ms is out of representable range"))?
        .to_rfc3339();
    let to_str = chrono::DateTime::from_timestamp_millis(to_ms)
        .ok_or_else(|| anyhow::anyhow!("--to value {to_ms}ms is out of representable range"))?
        .to_rfc3339();

    let filter = RUMQueryFilter::new()
        .from(from_str)
        .to(to_str)
        .query("@type:session".to_string());

    let body = RUMSearchEventsRequest::new()
        .filter(filter)
        .sort(RUMSort::TIMESTAMP_DESCENDING)
        .page(datadog_api_client::datadogV2::model::RUMQueryPageOptions::new().limit(limit));

    let resp = api
        .search_rum_events(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list RUM sessions: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- RUM Playlists ----

pub async fn playlists_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(RumReplayPlaylistsAPI, cfg);
    let resp = api
        .list_rum_replay_playlists(ListRumReplayPlaylistsOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list RUM playlists: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn playlists_get(cfg: &Config, playlist_id: i32) -> Result<()> {
    let api = crate::make_api!(RumReplayPlaylistsAPI, cfg);
    let resp = api
        .get_rum_replay_playlist(playlist_id as i64)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get RUM playlist: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn playlists_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(RumReplayPlaylistsAPI, cfg);
    let body: Playlist = crate::util::read_json_file(file)?;
    let resp = api
        .create_rum_replay_playlist(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create RUM playlist: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn playlists_update(cfg: &Config, playlist_id: i32, file: &str) -> Result<()> {
    let api = crate::make_api!(RumReplayPlaylistsAPI, cfg);
    let body: Playlist = crate::util::read_json_file(file)?;
    let resp = api
        .update_rum_replay_playlist(playlist_id as i64, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update RUM playlist: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn playlists_delete(cfg: &Config, playlist_id: i32) -> Result<()> {
    let api = crate::make_api!(RumReplayPlaylistsAPI, cfg);
    api.delete_rum_replay_playlist(playlist_id as i64)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete RUM playlist: {e:?}"))?;
    println!("RUM playlist {playlist_id} deleted.");
    Ok(())
}

pub async fn playlists_sessions_list(
    cfg: &Config,
    playlist_id: i32,
    page_number: Option<i64>,
    page_size: i64,
) -> Result<()> {
    let api = crate::make_api!(RumReplayPlaylistsAPI, cfg);
    let mut params = ListRumReplayPlaylistSessionsOptionalParams::default().page_size(page_size);
    if let Some(page_number) = page_number {
        params = params.page_number(page_number);
    }
    let resp = api
        .list_rum_replay_playlist_sessions(playlist_id as i64, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list RUM playlist sessions: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn playlists_sessions_add(
    cfg: &Config,
    playlist_id: i32,
    session_id: String,
    ts: Option<i64>,
    data_source: Option<String>,
) -> Result<()> {
    let api = crate::make_api!(RumReplayPlaylistsAPI, cfg);
    let ts = ts.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    let mut params = AddRumReplaySessionToPlaylistOptionalParams::default();
    if let Some(data_source) = data_source {
        params = params.data_source(data_source);
    }
    let resp = api
        .add_rum_replay_session_to_playlist(ts, playlist_id as i64, session_id, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to add session to RUM playlist: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn playlists_sessions_remove(
    cfg: &Config,
    playlist_id: i32,
    session_id: String,
) -> Result<()> {
    let api = crate::make_api!(RumReplayPlaylistsAPI, cfg);
    api.remove_rum_replay_session_from_playlist(playlist_id as i64, session_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to remove session from RUM playlist: {e:?}"))?;
    println!("Session removed from RUM playlist {playlist_id}.");
    Ok(())
}

pub async fn playlists_sessions_bulk_remove(
    cfg: &Config,
    playlist_id: i32,
    file: &str,
) -> Result<()> {
    let api = crate::make_api!(RumReplayPlaylistsAPI, cfg);
    let body: SessionIdArray = crate::util::read_json_file(file)?;
    api.bulk_remove_rum_replay_playlist_sessions(playlist_id as i64, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to bulk-remove RUM playlist sessions: {e:?}"))?;
    println!("Sessions removed from RUM playlist {playlist_id}.");
    Ok(())
}

// ---- RUM Replay Segments ----

pub struct ReplaySegmentsGetArgs {
    pub session_id: String,
    pub view_id: String,
    pub source: Option<String>,
    pub ts: Option<i64>,
    pub max_list_size: Option<i64>,
    pub paging: Option<String>,
}

fn output_response_content(cfg: &Config, content: &str) -> Result<()> {
    if content.trim().is_empty() {
        formatter::output(cfg, &serde_json::Value::Null)?;
        return Ok(());
    }
    let value: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| anyhow::anyhow!("failed to parse API response JSON: {e}"))?;
    formatter::output(cfg, &value)
}

pub async fn replay_segments_get(cfg: &Config, args: ReplaySegmentsGetArgs) -> Result<()> {
    let api = crate::make_api!(RumReplaySessionsAPI, cfg);
    let ReplaySegmentsGetArgs {
        session_id,
        view_id,
        source,
        ts,
        max_list_size,
        paging,
    } = args;

    let mut params = GetSegmentsOptionalParams::default();
    if let Some(source) = source {
        params = params.source(source);
    }
    if let Some(ts) = ts {
        params = params.ts(ts);
    }
    if let Some(max_list_size) = max_list_size {
        params = params.max_list_size(max_list_size);
    }
    if let Some(paging) = paging {
        params = params.paging(paging);
    }

    let resp = api
        .get_segments_with_http_info(view_id, session_id, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get RUM replay segments: {e:?}"))?;
    output_response_content(cfg, &resp.content)
}

// ---- RUM Viewership ----

pub struct ViewershipHistoryListArgs {
    pub from: String,
    pub to: String,
    pub page_number: Option<i64>,
    pub page_size: i64,
    pub session_ids: Option<String>,
    pub application_id: Option<String>,
    pub created_by: Option<String>,
}

pub async fn viewership_history_list(cfg: &Config, args: ViewershipHistoryListArgs) -> Result<()> {
    let api = crate::make_api!(RumReplayViewershipAPI, cfg);
    let ViewershipHistoryListArgs {
        from,
        to,
        page_number,
        page_size,
        session_ids,
        application_id,
        created_by,
    } = args;
    let from_ms = util_ext::parse_time_to_unix_millis(&from)?;
    let to_ms = util_ext::parse_time_to_unix_millis(&to)?;

    let mut params = ListRumReplayViewershipHistorySessionsOptionalParams::default()
        .filter_watched_at_start(from_ms)
        .filter_watched_at_end(to_ms)
        .page_size(page_size);
    if let Some(page_number) = page_number {
        params = params.page_number(page_number);
    }
    if let Some(session_ids) = session_ids {
        params = params.filter_session_ids(session_ids);
    }
    if let Some(application_id) = application_id {
        params = params.filter_application_id(application_id);
    }
    if let Some(created_by) = created_by {
        params = params.filter_created_by(created_by);
    }

    let resp = api
        .list_rum_replay_viewership_history_sessions(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list RUM viewership history: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn viewership_watch_create(
    cfg: &Config,
    session_id: String,
    file: Option<String>,
) -> Result<()> {
    let api = crate::make_api!(RumReplayViewershipAPI, cfg);
    let body: Watch = if let Some(file) = file {
        crate::util::read_json_file(&file)?
    } else {
        Watch::new(WatchData::new(WatchDataType::RUM_REPLAY_WATCH))
    };
    let resp = api
        .create_rum_replay_session_watch(session_id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create RUM replay watch: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn viewership_watch_delete(cfg: &Config, session_id: String) -> Result<()> {
    let api = crate::make_api!(RumReplayViewershipAPI, cfg);
    api.delete_rum_replay_session_watch(session_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete RUM replay watch: {e:?}"))?;
    println!("RUM replay watch deleted.");
    Ok(())
}

pub async fn viewership_watchers_list(
    cfg: &Config,
    session_id: String,
    page_number: Option<i64>,
    page_size: i64,
) -> Result<()> {
    let api = crate::make_api!(RumReplayViewershipAPI, cfg);
    let mut params = ListRumReplaySessionWatchersOptionalParams::default().page_size(page_size);
    if let Some(page_number) = page_number {
        params = params.page_number(page_number);
    }
    let resp = api
        .list_rum_replay_session_watchers(session_id, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list RUM replay watchers: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- RUM Heatmaps ----

pub async fn heatmaps_query(cfg: &Config, view_name: &str) -> Result<()> {
    let api = crate::make_api!(RumReplayHeatmapsAPI, cfg);
    let resp = api
        .list_replay_heatmap_snapshots(
            view_name.to_string(),
            ListReplayHeatmapSnapshotsOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to query RUM heatmaps: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- RUM Aggregate ----

pub struct RumAggregateArgs {
    pub query: String,
    pub from: String,
    pub to: String,
    pub compute: Vec<String>,
    pub group_by: Vec<String>,
    pub limit: i32,
}

fn parse_rum_compute(input: &str) -> Result<(String, Option<String>)> {
    let input = input.trim();
    if input.is_empty() {
        bail!("--compute is required");
    }
    if input == "count" {
        return Ok(("count".into(), None));
    }
    if let Some(paren) = input.find('(') {
        let func = &input[..paren];
        let rest = input[paren + 1..].trim_end_matches(')').trim();
        if func == "percentile" {
            let parts: Vec<&str> = rest.splitn(2, ',').collect();
            if parts.len() != 2 {
                bail!("percentile requires field and value: percentile(@duration, 99)");
            }
            let metric = parts[0].trim().to_string();
            let pct: u32 = parts[1]
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid percentile value: {}", parts[1].trim()))?;
            let agg_name = match pct {
                75 => "pc75",
                90 => "pc90",
                95 => "pc95",
                98 => "pc98",
                99 => "pc99",
                _ => bail!("unsupported percentile: {pct} (supported: 75, 90, 95, 98, 99)"),
            };
            return Ok((agg_name.into(), Some(metric)));
        }
        let metric = rest.to_string();
        let agg_name = match func {
            "avg" | "sum" | "min" | "max" | "median" | "cardinality" => func.to_string(),
            "count" => bail!("count does not accept a field argument; use just 'count'"),
            _ => bail!("unknown aggregation function: {func}"),
        };
        return Ok((agg_name, Some(metric)));
    }
    bail!(
        "invalid --compute format: {input:?}\n\
         Expected: count, avg(@duration), sum(@duration), percentile(@duration, 99), etc."
    )
}

pub async fn aggregate(cfg: &Config, args: RumAggregateArgs) -> Result<()> {
    let RumAggregateArgs {
        query,
        from,
        to,
        mut compute,
        group_by,
        limit,
    } = args;
    if compute.is_empty() {
        compute.push("count".into());
    }
    let from_ms = util_ext::parse_time_to_unix_millis(&from)?;
    let to_ms = util_ext::parse_time_to_unix_millis(&to)?;

    let compute_arr: Vec<serde_json::Value> = compute
        .iter()
        .map(|c| {
            let (aggregation, metric) = parse_rum_compute(c)?;
            let mut obj = serde_json::json!({ "type": "total", "aggregation": aggregation });
            if let Some(m) = metric {
                obj["metric"] = serde_json::Value::String(m);
            }
            Ok(obj)
        })
        .collect::<Result<Vec<_>>>()?;

    let filter = serde_json::json!({
        "query": query,
        "from": from_ms.to_string(),
        "to": to_ms.to_string()
    });

    let mut body = serde_json::json!({
        "filter": filter,
        "compute": compute_arr
    });

    if !group_by.is_empty() {
        let group_by_arr: Vec<serde_json::Value> = group_by
            .iter()
            .map(|facet| {
                let mut obj = serde_json::json!({ "facet": facet });
                if limit > 0 {
                    obj["limit"] = serde_json::json!(limit);
                }
                obj
            })
            .collect();
        body["group_by"] = serde_json::json!(group_by_arr);
    }

    let data = raw_client::raw_post(cfg, "/api/v2/rum/analytics/aggregate", body).await?;
    formatter::output(cfg, &data)?;
    Ok(())
}

/// Split a comma-separated compute string, respecting parentheses.
pub fn split_rum_compute_args(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut depth = 0u32;
    for ch in input.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    result.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        result.push(trimmed);
    }
    result
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_rum_apps_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::apps_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_rum_apps_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {"id": "abc", "type": "rum_browser"}}"#).await;
        let _ = super::apps_get(&cfg, "abc").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_rum_apps_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let _ = super::apps_delete(&cfg, "abc").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_rum_metrics_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::metrics_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_rum_metrics_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::metrics_get(&cfg, "m1").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_rum_metrics_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let _ = super::metrics_delete(&cfg, "m1").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_rum_retention_filters_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::retention_filters_list(&cfg, "app1").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_rum_events_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::events_list(&cfg, None, "1h".into(), "now".into(), 10).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_rum_playlists_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::playlists_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_rum_playlists_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(
            &mut s,
            r#"{"data": {"id": "1", "type": "rum_replay_playlist"}}"#,
        )
        .await;
        let path =
            std::env::temp_dir().join(format!("pup-rum-playlist-{}.json", std::process::id()));
        std::fs::write(
            &path,
            r#"{"data":{"type":"rum_replay_playlist","attributes":{"name":"test"}}}"#,
        )
        .unwrap();
        let _ = super::playlists_create(&cfg, path.to_str().unwrap()).await;
        let _ = std::fs::remove_file(path);
        cleanup_env();
    }

    #[tokio::test]
    async fn test_rum_playlists_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let _ = super::playlists_delete(&cfg, 123).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_rum_playlists_sessions_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::playlists_sessions_list(&cfg, 123, None, 100).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_rum_replay_segments_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let result = super::replay_segments_get(
            &cfg,
            super::ReplaySegmentsGetArgs {
                session_id: "sess-1".into(),
                view_id: "view-1".into(),
                source: None,
                ts: None,
                max_list_size: None,
                paging: None,
            },
        )
        .await;
        assert!(
            result.is_ok(),
            "replay segments get failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_rum_viewership_history_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::viewership_history_list(
            &cfg,
            super::ViewershipHistoryListArgs {
                from: "1h".into(),
                to: "now".into(),
                page_number: None,
                page_size: 100,
                session_ids: None,
                application_id: None,
                created_by: None,
            },
        )
        .await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_rum_viewership_watch_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(
            &mut s,
            r#"{"data": {"id": "watch-1", "type": "rum_replay_watch"}}"#,
        )
        .await;
        let _ = super::viewership_watch_create(&cfg, "sess-1".into(), None).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_rum_viewership_watchers_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::viewership_watchers_list(&cfg, "sess-1".into(), None, 100).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_output_response_content_parses_json() {
        let _lock = lock_env().await;
        let cfg = test_config("https://example.com");
        let result = super::output_response_content(&cfg, r#"{"ok":true}"#);
        assert!(result.is_ok());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_output_response_content_rejects_invalid_json() {
        let _lock = lock_env().await;
        let cfg = test_config("https://example.com");
        let result = super::output_response_content(&cfg, "not-json");
        assert!(result.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_rum_sessions_search() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let result = super::sessions_search(&cfg, None, "1h".into(), "now".into(), 25).await;
        assert!(
            result.is_ok(),
            "rum sessions search failed: {:?}",
            result.err()
        );
        cleanup_env();
    }
}

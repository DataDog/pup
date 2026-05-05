use anyhow::Result;
use serde_json::json;

use crate::client;
use crate::config::Config;
use crate::formatter;
use crate::util;

fn parse_window(from: &str, to: &str) -> Result<(String, String)> {
    let from_ms = util::parse_time_to_unix_millis(from)
        .map_err(|e| anyhow::anyhow!("invalid --from value: {e}"))?;
    let to_ms = util::parse_time_to_unix_millis(to)
        .map_err(|e| anyhow::anyhow!("invalid --to value: {e}"))?;
    let from_iso = chrono::DateTime::from_timestamp_millis(from_ms)
        .ok_or_else(|| {
            anyhow::anyhow!("--from {from:?} resolved to {from_ms} ms which is outside the representable date range")
        })?
        .to_rfc3339();
    let to_iso = chrono::DateTime::from_timestamp_millis(to_ms)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "--to {to:?} resolved to {to_ms} ms which is outside the representable date range"
            )
        })?
        .to_rfc3339();
    Ok((from_iso, to_iso))
}

fn filter_body(query: &str, from: &str, to: &str) -> Result<serde_json::Value> {
    let (from_iso, to_iso) = parse_window(from, to)?;
    Ok(json!({
        "filter": { "from": from_iso, "to": to_iso, "query": query },
    }))
}

fn split_csv(flag: &str, value: Option<String>) -> Result<Vec<String>> {
    let Some(raw) = value else {
        return Ok(Vec::new());
    };
    let parts: Vec<String> = raw
        .split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        anyhow::bail!("{flag} was provided but contained no non-empty values: {raw:?}");
    }
    Ok(parts)
}

#[allow(clippy::too_many_arguments)]
pub async fn aggregate(
    cfg: &Config,
    query: String,
    profile_type: String,
    from: String,
    to: String,
    limit: u32,
    aggregation_function: String,
    show_from: Option<String>,
) -> Result<()> {
    let (from_iso, to_iso) = parse_window(&from, &to)?;
    // /profiling/api/v1/aggregate expects a flat body — query/from/to are siblings, not wrapped in filter{}.
    let body = json!({
        "profileType": profile_type,
        "query": query,
        "from": from_iso,
        "to": to_iso,
        "limit": limit,
        "aggregationFunction": aggregation_function,
    });
    let mut resp = client::raw_post(cfg, "/profiling/api/v1/aggregate", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to aggregate profiles: {e:?}"))?;
    if let Some(name) = show_from {
        apply_show_from(&mut resp, &name)?;
        prune_aggregate_response(&mut resp);
    }
    formatter::output(cfg, &resp)
}

// When --show-from has trimmed `flameGraph` to a small subtree, the rest of the
// response (frames, strings, metadata) is mostly dead weight. Compact `frames`
// and `strings` to just the entries the trimmed flame graph still references,
// remap IDs accordingly, and drop heavy fields the UI uses but a CLI consumer
// almost never wants alongside a single-function view.
fn prune_aggregate_response(resp: &mut serde_json::Value) {
    use std::collections::{HashMap, HashSet};

    let Some(flame) = resp.get("flameGraph").cloned() else {
        return;
    };
    let Some(frames) = resp.get("frames").and_then(|f| f.as_array()).cloned() else {
        return;
    };
    let Some(strings) = resp.get("strings").and_then(|s| s.as_array()).cloned() else {
        return;
    };

    let mut used_frames: HashSet<i64> = HashSet::new();
    collect_used_frame_ids(&flame, &mut used_frames);
    if used_frames.is_empty() {
        return;
    }

    // Indices 2..=5 of a frame are string-table refs (library, package, function, file).
    let mut used_strings: HashSet<i64> = HashSet::new();
    for &fid in &used_frames {
        let Some(f) = frames.get(fid as usize).and_then(|f| f.as_array()) else {
            continue;
        };
        for idx in 2usize..=5 {
            if let Some(sid) = f.get(idx).and_then(|v| v.as_i64()) {
                used_strings.insert(sid);
            }
        }
    }

    let mut sorted_frames: Vec<i64> = used_frames.iter().copied().collect();
    sorted_frames.sort_unstable();
    let frame_remap: HashMap<i64, i64> = sorted_frames
        .iter()
        .enumerate()
        .map(|(i, &old)| (old, i as i64))
        .collect();

    let mut sorted_strings: Vec<i64> = used_strings.iter().copied().collect();
    sorted_strings.sort_unstable();
    let string_remap: HashMap<i64, i64> = sorted_strings
        .iter()
        .enumerate()
        .map(|(i, &old)| (old, i as i64))
        .collect();

    let new_frames: Vec<serde_json::Value> = sorted_frames
        .iter()
        .filter_map(|&old_fid| {
            let mut f = frames.get(old_fid as usize).cloned()?;
            if let Some(arr) = f.as_array_mut() {
                for idx in 2usize..=5 {
                    if let Some(slot) = arr.get_mut(idx) {
                        if let Some(old_sid) = slot.as_i64() {
                            if let Some(&new_sid) = string_remap.get(&old_sid) {
                                *slot = json!(new_sid);
                            }
                        }
                    }
                }
            }
            Some(f)
        })
        .collect();

    let new_strings: Vec<serde_json::Value> = sorted_strings
        .iter()
        .filter_map(|&old_sid| strings.get(old_sid as usize).cloned())
        .collect();

    let mut new_flame = flame;
    remap_frame_ids(&mut new_flame, &frame_remap);

    resp["flameGraph"] = new_flame;
    resp["frames"] = json!(new_frames);
    resp["strings"] = json!(new_strings);

    // These are large UI-oriented fields with no relationship to the
    // post-filter subtree; strip to keep terminal output usable.
    if let Some(obj) = resp.as_object_mut() {
        for k in [
            "metadata",
            "frameSchemas",
            "endpointCounts",
            "endpointValues",
            "summaryTable",
            "summaryValues",
            "summaryDurations",
            "availableAttributes",
            "featureUpgrades",
            "languageFrameCounts",
            "profileIds",
            "emptyStateReason",
        ] {
            obj.remove(k);
        }
    }
}

fn collect_used_frame_ids(node: &serde_json::Value, out: &mut std::collections::HashSet<i64>) {
    let Some(arr) = node.as_array() else {
        return;
    };
    if let Some(fid) = arr.first().and_then(|v| v.as_i64()) {
        out.insert(fid);
    }
    if let Some(children) = arr.get(3).and_then(|c| c.as_array()) {
        for c in children {
            collect_used_frame_ids(c, out);
        }
    }
}

fn remap_frame_ids(node: &mut serde_json::Value, remap: &std::collections::HashMap<i64, i64>) {
    let Some(arr) = node.as_array_mut() else {
        return;
    };
    if let Some(slot) = arr.first_mut() {
        if let Some(old) = slot.as_i64() {
            if let Some(&new) = remap.get(&old) {
                *slot = json!(new);
            }
        }
    }
    if let Some(children) = arr.get_mut(3).and_then(|c| c.as_array_mut()) {
        for c in children {
            remap_frame_ids(c, remap);
        }
    }
}

// Client-side equivalent of the UI's `show_from(<function>)` flame-graph filter.
// Replaces `data.flameGraph` with a synthetic root whose children are the
// topmost subtrees rooted at frames with the given function name (exact match).
// Frame names are resolved via `data.frames[i][4]` (the `function` field of
// `frameSchema`) → `data.strings[id]`. `data.frames` and `data.strings` are
// left untouched so callers can still resolve frame metadata.
fn apply_show_from(resp: &mut serde_json::Value, function_name: &str) -> Result<()> {
    use std::collections::HashSet;

    let strings = resp
        .get("strings")
        .and_then(|s| s.as_array())
        .ok_or_else(|| anyhow::anyhow!("response has no 'strings' array"))?;
    let target_string_ids: HashSet<i64> = strings
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            s.as_str()
                .filter(|st| *st == function_name)
                .map(|_| i as i64)
        })
        .collect();
    if target_string_ids.is_empty() {
        anyhow::bail!(
            "--show-from: no string in 'strings' matches function name {function_name:?}"
        );
    }

    let frames_arr: Vec<serde_json::Value> = resp
        .get("frames")
        .and_then(|f| f.as_array())
        .ok_or_else(|| anyhow::anyhow!("response has no 'frames' array"))?
        .clone();
    let target_frame_ids: HashSet<i64> = frames_arr
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            let arr = f.as_array()?;
            let fname_id = arr.get(4)?.as_i64()?;
            if target_string_ids.contains(&fname_id) {
                Some(i as i64)
            } else {
                None
            }
        })
        .collect();
    if target_frame_ids.is_empty() {
        anyhow::bail!(
            "--show-from: no frame in 'frames' references function name {function_name:?}"
        );
    }

    let flame = resp
        .get("flameGraph")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("response has no 'flameGraph'"))?;
    let mut matches = Vec::new();
    collect_show_from_subtrees(&flame, &target_frame_ids, &mut matches);
    if matches.is_empty() {
        anyhow::bail!(
            "--show-from: no node in 'flameGraph' references function name {function_name:?}"
        );
    }

    resp["flameGraph"] = merge_subtrees_by_function(matches, &frames_arr);
    Ok(())
}

// Merge a set of flame-graph subtrees into a single subtree by collapsing
// children that share the same function name (frames[fid][4] -> string id).
// Datadog's flame graph emits a distinct `frame_id` per (function, file, line)
// tuple, so a single logical function can appear under many frame IDs after
// inlining/generics. The UI merges siblings by display name; we mirror that.
fn merge_subtrees_by_function(
    mut nodes: Vec<serde_json::Value>,
    frames: &[serde_json::Value],
) -> serde_json::Value {
    if nodes.len() == 1 {
        return nodes.pop().unwrap();
    }
    let representative_fid = nodes
        .iter()
        .filter_map(|n| n.as_array().and_then(|a| a.first()).and_then(|v| v.as_i64()))
        .next()
        .unwrap_or(0);
    let total_value: i64 = nodes
        .iter()
        .filter_map(|n| n.as_array().and_then(|a| a.get(1)).and_then(|v| v.as_i64()))
        .sum();
    let mut all_children: Vec<serde_json::Value> = Vec::new();
    for n in &nodes {
        if let Some(children) = n.as_array().and_then(|a| a.get(3)).and_then(|c| c.as_array()) {
            all_children.extend(children.iter().cloned());
        }
    }
    // Group by function string id; preserve insertion order so output is stable.
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<i64, Vec<serde_json::Value>> = BTreeMap::new();
    let mut keyless: Vec<serde_json::Value> = Vec::new();
    for c in all_children {
        let key = c
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.as_i64())
            .and_then(|fid| frames.get(fid as usize))
            .and_then(|f| f.as_array())
            .and_then(|fa| fa.get(4))
            .and_then(|v| v.as_i64());
        match key {
            Some(k) => groups.entry(k).or_default().push(c),
            None => keyless.push(c),
        }
    }
    let mut merged_children: Vec<serde_json::Value> = groups
        .into_values()
        .map(|grp| merge_subtrees_by_function(grp, frames))
        .collect();
    merged_children.append(&mut keyless);
    json!([representative_fid, total_value, -1.0, merged_children])
}

fn collect_show_from_subtrees(
    node: &serde_json::Value,
    targets: &std::collections::HashSet<i64>,
    out: &mut Vec<serde_json::Value>,
) {
    let Some(arr) = node.as_array() else {
        return;
    };
    if let Some(fid) = arr.first().and_then(|v| v.as_i64()) {
        if targets.contains(&fid) {
            out.push(node.clone());
            return; // topmost match only — don't descend into nested re-entries
        }
    }
    if let Some(children) = arr.get(3).and_then(|c| c.as_array()) {
        for c in children {
            collect_show_from_subtrees(c, targets, out);
        }
    }
}

pub async fn analysis(cfg: &Config, profile_id: &str, event_id: Option<String>) -> Result<()> {
    let path = format!("/profiling/api/v1/profiles/{profile_id}/analysis");
    let query: Vec<(&str, &str)> = match event_id.as_deref() {
        Some(eid) => vec![("eventId", eid)],
        None => vec![],
    };
    let resp = client::raw_get(cfg, &path, &query)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get profile analysis: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[allow(clippy::too_many_arguments)]
pub async fn analytics(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    group_by: Option<String>,
    compute: Option<String>,
    limit: u32,
) -> Result<()> {
    let mut body = filter_body(&query, &from, &to)?;
    body["limit"] = json!(limit);
    let groups = split_csv("--group-by", group_by)?;
    if !groups.is_empty() {
        body["groupBy"] = json!(groups);
    }
    let computes = split_csv("--compute", compute)?;
    if !computes.is_empty() {
        body["compute"] = json!(computes);
    }
    let resp = client::raw_post(cfg, "/api/unstable/profiles/analytics", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to run profiling analytics: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn breakdown(
    cfg: &Config,
    profile_id: &str,
    query: Option<String>,
    from: Option<String>,
    to: Option<String>,
) -> Result<()> {
    let mut body = json!({ "profileIds": [profile_id] });
    match (query.as_ref(), from.as_ref(), to.as_ref()) {
        (Some(q), Some(f), Some(t)) => {
            let (from_iso, to_iso) = parse_window(f, t)?;
            body["filter"] = json!({ "from": from_iso, "to": to_iso, "query": q });
        }
        (None, None, None) => {}
        _ => {
            anyhow::bail!("--query, --from, and --to must all be provided together, or all omitted")
        }
    }
    let path = format!("/profiling/api/v1/profiles/{profile_id}/breakdown");
    let resp = client::raw_post(cfg, &path, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to compute profile breakdown: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn callgraph(
    cfg: &Config,
    query: String,
    profile_type: String,
    from: String,
    to: String,
    limit: u32,
) -> Result<()> {
    let mut body = filter_body(&query, &from, &to)?;
    body["profileType"] = json!(profile_type);
    body["limit"] = json!(limit);
    let resp = client::raw_post(cfg, "/api/unstable/profiles/callgraph", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to load call graph: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn download(cfg: &Config, event_id: &str, output: Option<String>) -> Result<()> {
    use std::io::Write;
    // The path segment is named "profiles/<id>", but the ID is the profile event ID
    // (the `id` field on a `pup profiling list` result), not `attributes.profile-id`.
    let url_path = format!("/api/ui/profiling/profiles/{event_id}/download");
    let resp = client::raw_request(
        cfg,
        "GET",
        &url_path,
        &[],
        None,
        None,
        "application/octet-stream",
        &[],
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to download profile: {e:?}"))?;

    match output {
        Some(out_path) => {
            let mut f = std::fs::File::create(&out_path)
                .map_err(|e| anyhow::anyhow!("failed to create {out_path}: {e}"))?;
            f.write_all(&resp.bytes)
                .map_err(|e| anyhow::anyhow!("failed to write {out_path}: {e}"))?;
            f.sync_all()
                .map_err(|e| anyhow::anyhow!("failed to flush {out_path} to disk: {e}"))?;
            eprintln!("Wrote {} bytes to {}", resp.bytes.len(), out_path);
        }
        None => {
            let mut out = std::io::stdout().lock();
            out.write_all(&resp.bytes)
                .map_err(|e| anyhow::anyhow!("failed to write to stdout: {e}"))?;
            out.flush()
                .map_err(|e| anyhow::anyhow!("failed to flush stdout: {e}"))?;
        }
    }
    Ok(())
}

pub async fn fields(
    cfg: &Config,
    field: String,
    query: String,
    from: String,
    to: String,
    limit: u32,
) -> Result<()> {
    let mut body = filter_body(&query, &from, &to)?;
    body["fieldName"] = json!(field);
    body["limit"] = json!(limit);
    let resp = client::raw_post(
        cfg,
        "/api/unstable/profiles/interactive-analytics/field",
        body,
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to list field values: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn info(cfg: &Config, profile_id: &str, event_id: Option<String>) -> Result<()> {
    let path = format!("/profiling/api/v1/profiles/{profile_id}/info");
    let query: Vec<(&str, &str)> = match event_id.as_deref() {
        Some(eid) => vec![("eventId", eid)],
        None => vec![],
    };
    let resp = client::raw_get(cfg, &path, &query)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get profile info: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn list(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    sort_field: Option<String>,
    sort_order: String,
    limit: u32,
) -> Result<()> {
    let mut body = filter_body(&query, &from, &to)?;
    body["limit"] = json!(limit);
    if let Some(field) = sort_field {
        body["sort"] = json!({ "field": field, "order": sort_order });
    }
    let resp = client::raw_post(cfg, "/api/unstable/profiles/list", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list profiles: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn save_favorite(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    query_id: String,
    limit: u32,
) -> Result<()> {
    let mut body = filter_body(&query, &from, &to)?;
    body["queryId"] = json!(query_id);
    body["limit"] = json!(limit);
    let resp = client::raw_post(cfg, "/api/unstable/profiles/save-favorite", body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to save favorite: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn timeline(cfg: &Config, profile_id: &str, event_id: &str) -> Result<()> {
    // TimelineRequest DTO uses kebab-case JSON keys and requires both profile-ids and event-ids.
    let body = json!({
        "profile-ids": [profile_id],
        "event-ids": [event_id],
        "archivalContext": "",
    });
    let path = format!("/profiling/api/v1/profiles/{profile_id}/timeline");
    let resp = client::raw_post(cfg, &path, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to load profile timeline: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_profiling_list_ok() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let mock = s
            .mock("POST", "/api/unstable/profiles/list")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        let result = super::list(
            &cfg,
            "*".into(),
            "15m".into(),
            "now".into(),
            None,
            "desc".into(),
            100,
        )
        .await;
        assert!(result.is_ok(), "list failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_profiling_list_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("POST", "/api/unstable/profiles/list")
            .with_status(500)
            .with_body(r#"{"errors":["boom"]}"#)
            .create_async()
            .await;
        let result = super::list(
            &cfg,
            "*".into(),
            "15m".into(),
            "now".into(),
            None,
            "desc".into(),
            100,
        )
        .await;
        assert!(result.is_err(), "expected error on 500");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_desc_upsert_with_cloud_success() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "PUT", r#"{}"#).await;
        let result = crate::commands::cost_ccm::tag_desc_upsert(
            &cfg,
            "team",
            "The team tag",
            Some("aws".into()),
        )
        .await;
        assert!(
            result.is_ok(),
            "tag_desc_upsert with cloud failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_profiling_info_ok() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let mock = s
            .mock("GET", "/profiling/api/v1/profiles/abc123/info")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":{"id":"abc123"}}"#)
            .create_async()
            .await;
        let result = super::info(&cfg, "abc123", None).await;
        assert!(result.is_ok(), "info failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_profiling_info_with_event_id() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let mock = s
            .mock("GET", "/profiling/api/v1/profiles/abc123/info")
            .match_query(mockito::Matcher::UrlEncoded(
                "eventId".into(),
                "evt-1".into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":{"id":"abc123"}}"#)
            .create_async()
            .await;
        let result = super::info(&cfg, "abc123", Some("evt-1".into())).await;
        assert!(result.is_ok(), "info failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_profiling_info_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .with_body(r#"{"errors":["not found"]}"#)
            .create_async()
            .await;
        let result = super::info(&cfg, "missing", None).await;
        assert!(result.is_err(), "expected error on 404");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_profiling_analysis_ok() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let mock = s
            .mock("GET", "/profiling/api/v1/profiles/abc/analysis")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":{"insights":[]}}"#)
            .create_async()
            .await;
        let result = super::analysis(&cfg, "abc", None).await;
        assert!(result.is_ok(), "analysis failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_profiling_analysis_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .with_status(500)
            .create_async()
            .await;
        let result = super::analysis(&cfg, "abc", None).await;
        assert!(result.is_err(), "expected error on 500");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_profiling_analytics_ok() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let mock = s
            .mock("POST", "/api/unstable/profiles/analytics")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        let result = super::analytics(
            &cfg,
            "service:web".into(),
            "15m".into(),
            "now".into(),
            Some("service,env".into()),
            Some("count".into()),
            100,
        )
        .await;
        assert!(result.is_ok(), "analytics failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_desc_delete_success() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "DELETE", r#"{}"#).await;
        let result = crate::commands::cost_ccm::tag_desc_delete(&cfg, "team", None).await;
        assert!(result.is_ok(), "tag_desc_delete failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_desc_delete_with_cloud_success() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "DELETE", r#"{}"#).await;
        let result =
            crate::commands::cost_ccm::tag_desc_delete(&cfg, "team", Some("azure".into())).await;
        assert!(
            result.is_ok(),
            "tag_desc_delete with cloud failed: {:?}",
            result.err()
        );
        cleanup_env();
    }
}

//! Governance Console commands. Preserve JSON:API responses, including nulls and new fields.
use std::collections::HashSet;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};

use crate::config::Config;
use crate::formatter::{self, Metadata};
use crate::raw_client::{self, AuthType, HttpError};
use crate::util_ext::{confirm_destructive, parse_uuid, percent_encode};

const ROOT: &str = "/api/v2/governance";
const DOCS: &str = "https://docs.datadoghq.com/account_management/governance_console/";
const PREVIEW_ACTION: &str =
    "No changes were made. Get the user's approval, then rerun without --dry-run.";

#[derive(Clone, Copy)]
enum Permission<'a> {
    Console,
    Insights,
    Mitigation {
        control: &'a Value,
        mitigation: &'a Value,
    },
}

fn permission_scopes(need: Permission<'_>) -> Vec<String> {
    let mut scopes = vec!["governance_console_read".to_string()];
    match need {
        Permission::Insights => {
            scopes.extend(["metrics_read", "events_read", "audit_logs_read"].map(String::from))
        }
        Permission::Mitigation {
            control,
            mitigation,
        } => {
            if let Some(permissions) = mitigation["permissions"].as_array() {
                scopes.extend(
                    permissions
                        .iter()
                        .filter_map(Value::as_str)
                        .map(String::from),
                );
            }
            if mitigation["id"] == "delete_dashboard"
                && control["data"]["attributes"]["detection_parameters"]
                    ["include_rbac_dashboards_in_scope"]
                    == true
            {
                scopes.push("user_access_manage".into());
            }
        }
        Permission::Console => {}
    }
    let mut seen = HashSet::new();
    scopes.retain(|s| seen.insert(s.clone()));
    scopes
}

fn permission_hint(cfg: &Config, need: Permission<'_>) -> String {
    let scopes = permission_scopes(need);
    let grants = scopes.join(", ");
    let mut text = match need {
        Permission::Insights => format!("access denied: listing insights needs any of these permissions: {grants}."),
        Permission::Mitigation { mitigation, .. } => format!(
            "access denied applying mitigation {}. The server does not report which grant is missing. Required permissions: {grants}. Some controls also need read access to detected resources (for example metrics_read for metrics controls).",
            mitigation["id"].as_str().unwrap_or("(unknown)")
        ),
        Permission::Console => format!("access denied: this operation needs the {grants} permission."),
    };
    if raw_client::get_auth_type(cfg) == AuthType::OAuth {
        text.push_str(&format!(
            " Add the scopes with: pup auth login --extra-scopes {}.",
            scopes.join(",")
        ));
    } else {
        text.push_str(" Ask an administrator for a role that has the required permissions.");
    }
    if matches!(need, Permission::Mitigation { .. }) {
        text.push_str(" If you already have these, check your token's scopes with `pup auth status`, try API and application keys of a user whose role has them, or ask an administrator; the denied check is recorded in Audit Trail for each detection.");
    }
    format!("{text} See {DOCS}")
}

fn governance_error(
    cfg: &Config,
    op: &str,
    err: anyhow::Error,
    need: Permission<'_>,
) -> anyhow::Error {
    let hint = match err.downcast_ref::<HttpError>() {
        Some(e) if e.status == 403 && e.body.contains("Governance Console is not enabled") => {
            format!("Governance Console is not enabled for this organization. An administrator can enable it; see {DOCS}")
        }
        Some(e) if e.status == 403 => format!("{op}: {}", permission_hint(cfg, need)),
        Some(e) => crate::output::format_api_error(op, Some(e.status), None),
        None => format!("failed to {op}"),
    };
    err.context(hint)
}

async fn get(
    cfg: &Config,
    suffix: &str,
    query: &[(&str, &str)],
    need: Permission<'_>,
) -> Result<Value> {
    raw_client::raw_get(cfg, &format!("{ROOT}/{suffix}"), query)
        .await
        .map_err(|e| governance_error(cfg, &format!("read governance {suffix}"), e, need))
}

pub async fn insights_list(cfg: &Config, products: &[String], no_values: bool) -> Result<()> {
    let mut query = vec![("withValues", if no_values { "false" } else { "true" })];
    query.extend(products.iter().map(|p| ("filter[product]", p.as_str())));
    let response = get(cfg, "insights", &query, Permission::Insights).await?;
    formatter::output(cfg, &response)
}

pub async fn controls_list(cfg: &Config) -> Result<()> {
    let response = get(cfg, "control", &[], Permission::Console).await?;
    formatter::output(cfg, &response)
}

pub async fn controls_get(cfg: &Config, detection_type: &str) -> Result<()> {
    let response = get(
        cfg,
        &format!("control/{}", percent_encode(detection_type)),
        &[],
        Permission::Console,
    )
    .await?;
    formatter::output(cfg, &response)
}

fn detection_page_meta(response: &Value, page: u64, page_size: i64) -> Metadata {
    Metadata {
        next_page: response["data"]
            .as_array()
            .filter(|rows| rows.len() == page_size as usize)
            .and_then(|_| page.checked_add(1)),
        ..Default::default()
    }
}

pub async fn detections_list(
    cfg: &Config,
    detection_type: &str,
    states: &[String],
    query: Option<&str>,
    page: u64,
    page_size: i64,
) -> Result<()> {
    let states = states.join(",");
    let page_number = page.to_string();
    let size = page_size.to_string();
    let mut filters = vec![
        ("page[number]", page_number.as_str()),
        ("page[size]", size.as_str()),
    ];
    if !states.is_empty() {
        filters.push(("filter[state]", &states));
    }
    if let Some(query) = query {
        filters.push(("filter[query]", query));
    }
    let response = get(
        cfg,
        &format!("control/{}/detections", percent_encode(detection_type)),
        &filters,
        Permission::Console,
    )
    .await?;
    formatter::output_with_meta(
        cfg,
        &response,
        &detection_page_meta(&response, page, page_size),
    )
}

async fn fetch_detection(cfg: &Config, id: &str) -> Result<Value> {
    get(
        cfg,
        &format!("detections/{}", percent_encode(id)),
        &[],
        Permission::Console,
    )
    .await
}

pub async fn detections_get(cfg: &Config, id: &str) -> Result<()> {
    formatter::output(cfg, &fetch_detection(cfg, id).await?)
}

pub async fn limits_list(cfg: &Config, no_resource_limits: bool) -> Result<()> {
    let mut limits = get(cfg, "limits", &[], Permission::Console).await?;
    let rows = limits["data"]
        .as_array_mut()
        .context("governance limits response must contain a data array")?;
    if !no_resource_limits {
        let resources = get(cfg, "resource_limits", &[], Permission::Console)
            .await
            .context("failed to read resource limits; use --no-resource-limits to skip them")?;
        rows.extend(
            resources["data"]
                .as_array()
                .context("governance resource limits response must contain a data array; use --no-resource-limits to skip them")?
                .iter()
                .cloned(),
        );
    }
    formatter::output(cfg, &limits)
}

pub async fn best_practices_list(
    cfg: &Config,
    category: Option<&str>,
    query: Option<&str>,
) -> Result<()> {
    let mut filters = vec![];
    if let Some(category) = category {
        filters.push(("filter[category]", category));
    }
    if let Some(query) = query {
        filters.push(("filter[query]", query));
    }
    formatter::output(
        cfg,
        &get(cfg, "best_practices", &filters, Permission::Console).await?,
    )
}

pub fn parse_detection_ids(ids: &[String]) -> Result<Vec<String>> {
    if ids.is_empty() || ids.len() > 25 {
        bail!("--ids requires between 1 and 25 detection ids");
    }
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for id in ids {
        let id = id.trim();
        if id.is_empty() {
            bail!("--ids must not contain empty detection ids");
        }
        if !seen.insert(id) {
            bail!("--ids must not contain duplicate detection ids: {id}");
        }
        result.push(id.to_string());
    }
    Ok(result)
}

pub fn parse_parameters(parameters: Option<&str>) -> Result<Option<Map<String, Value>>> {
    parameters
        .map(|text| {
            let value: Value =
                serde_json::from_str(text).context("--parameters must be a valid JSON object")?;
            value
                .as_object()
                .cloned()
                .context("--parameters must be a JSON object")
        })
        .transpose()
}

fn validate_deadline(deadline: &str) -> Result<()> {
    let instant = DateTime::parse_from_rfc3339(deadline)
        .context("--mitigate-after must be RFC 3339 (for example 2099-11-30T00:00:00Z)")?;
    if instant <= Utc::now() {
        bail!("--mitigate-after must be in the future");
    }
    Ok(())
}

pub fn parse_update_state(state: &str) -> std::result::Result<String, String> {
    if !matches!(state, "active" | "exception") {
        return Err("--state must be active or exception; use `pup governance detections mitigate` to mitigate detections".into());
    }
    Ok(state.to_string())
}

pub fn build_update_attributes(
    state: Option<&str>,
    mitigate_after: Option<&str>,
    assigned_to: Option<&str>,
    assigned_team: Option<&str>,
) -> Result<Map<String, Value>> {
    let mut attrs = Map::new();
    if let Some(state) = state {
        attrs.insert(
            "state".into(),
            json!(parse_update_state(state).map_err(anyhow::Error::msg)?),
        );
    }
    if let Some(deadline) = mitigate_after {
        validate_deadline(deadline)?;
        if state == Some("exception") {
            bail!("--mitigate-after cannot be used with --state exception; use --state active");
        }
        attrs.insert("mitigate_after".into(), json!(deadline));
    }
    for (flag, field, value) in [
        ("--assigned-to", "assigned_to", assigned_to),
        ("--assigned-team", "assigned_team", assigned_team),
    ] {
        if let Some(value) = value {
            if !value.is_empty() {
                parse_uuid(value, flag)?;
            }
            attrs.insert(field.into(), json!(value));
        }
    }
    if attrs.is_empty() {
        bail!("set at least one of --state, --mitigate-after, --assigned-to or --assigned-team");
    }
    Ok(attrs)
}

fn previous_fields(current: &Value) -> Value {
    let attrs = &current["data"]["attributes"];
    json!({"state": attrs["state"], "mitigate_after": attrs["mitigate_after"],
        "assigned_to": attrs["assigned_to"], "assigned_team": attrs["assigned_team"]})
}

pub fn plan_update_effects(current: &Value, attrs: &Map<String, Value>) -> Result<Vec<String>> {
    let previous = previous_fields(current);
    let previous_state = previous["state"]
        .as_str()
        .context("detection response is missing its current state; no update was sent")?;
    let state = attrs
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or(previous_state);
    if attrs.contains_key("mitigate_after") && state != "active" {
        bail!("--mitigate-after requires an active detection; add --state active");
    }
    let mut effects = Vec::new();
    for (field, value) in attrs {
        effects.push(format!("{field}: {} -> {value}", previous[field]));
    }
    if state != "active" && !previous["mitigate_after"].is_null() {
        effects.push(format!("mitigate_after: {} -> cleared (server clears the deadline when the state is not active)", previous["mitigate_after"]));
    }
    if state == "active" && previous["state"] != "active" && !attrs.contains_key("mitigate_after") {
        if previous["mitigate_after"].is_null() {
            effects.push(format!(
                "state: {previous_state} -> active; no mitigation deadline is set"
            ));
        } else {
            effects.push(format!(
                "mitigate_after: {} will be retained on activation",
                previous["mitigate_after"]
            ));
        }
    }
    Ok(effects)
}

pub fn verify_update(requested: &Map<String, Value>, response: &Value) -> Result<()> {
    let returned = &response["data"]["attributes"];
    if let Some(state) = requested.get("state") {
        if returned["state"] != *state {
            bail!(
                "server did not apply state: requested {state}, returned {}",
                returned["state"]
            );
        }
    }
    if let Some(deadline) = requested.get("mitigate_after") {
        let parse = |v: &Value| {
            v.as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        };
        if parse(deadline).is_none() || parse(deadline) != parse(&returned["mitigate_after"]) {
            bail!(
                "server did not apply mitigate_after: requested {deadline}, returned {}",
                returned["mitigate_after"]
            );
        }
    }
    if requested.get("state").is_some_and(|s| s == "exception")
        && !returned["mitigate_after"].is_null()
    {
        bail!(
            "server did not clear mitigate_after for state exception; returned {}",
            returned["mitigate_after"]
        );
    }
    Ok(())
}

pub fn find_manual_mitigation<'a>(control: &'a Value, id: &str) -> Result<&'a Value> {
    let manual: Vec<&Value> = control["data"]["attributes"]["mitigations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| {
            m["execution_modes"]
                .as_array()
                .is_some_and(|modes| modes.iter().any(|mode| mode == "manual"))
        })
        .collect();
    let valid_ids = manual
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect::<Vec<_>>()
        .join(", ");
    manual.iter().find(|m| m["id"] == id).copied().with_context(|| {
        format!("--mitigation-type {id:?} must support manual execution. Valid manual mitigation ids: {valid_ids}")
    })
}

fn update_body(attrs: &Map<String, Value>) -> Value {
    json!({"data": {"type": "governance_control_detection", "attributes": attrs}})
}

fn preview(cfg: &Config, data: Value) -> Result<()> {
    formatter::output_with_meta(
        cfg,
        &json!({"data": data}),
        &Metadata {
            next_action: Some(PREVIEW_ACTION.into()),
            ..Default::default()
        },
    )
}

// Reqwest's built-in protocol-NACK replay is intentional: those requests were not processed.
fn write_failure(
    cfg: &Config,
    op: &str,
    err: anyhow::Error,
    need: Permission<'_>,
    inspection: &str,
) -> anyhow::Error {
    if let Some(http) = err.downcast_ref::<HttpError>() {
        if (400..500).contains(&http.status) {
            return governance_error(cfg, op, err, need).context("rejected; nothing was applied");
        }
    }
    if err
        .downcast_ref::<reqwest::Error>()
        .is_some_and(reqwest::Error::is_connect)
    {
        return err.context(format!(
            "{op}: not sent; the connection failed before the request went out"
        ));
    }
    err.context(format!("{op}: Outcome unknown: the request may have been applied (to some or all detections for a mitigation). Do not resubmit. Check `{inspection}` and Audit Trail events for Governance Detection assets before deciding. Any resubmission needs new approval."))
}

pub async fn detections_update(
    cfg: &Config,
    id: &str,
    attrs: Map<String, Value>,
    dry_run: bool,
) -> Result<()> {
    let current = fetch_detection(cfg, id).await?;
    let effects = plan_update_effects(&current, &attrs)?;
    let previous = previous_fields(&current);
    let path = format!("{ROOT}/detections/{}", percent_encode(id));
    let body = update_body(&attrs);
    if dry_run {
        return preview(
            cfg,
            json!({"dry_run": true, "request": {"method": "PATCH", "path": path, "body": body},
            "current": current["data"], "effects": effects}),
        );
    }
    if !confirm_destructive(
        cfg,
        &format!(
            "Update governance detection {id}? Effects: {}.",
            effects.join("; ")
        ),
    )? {
        return Ok(());
    }
    // A user may have spent time at the prompt. Never send an expired deadline.
    if let Some(deadline) = attrs.get("mitigate_after").and_then(Value::as_str) {
        validate_deadline(deadline)?;
    }
    let inspection = format!("pup governance detections get {id}");
    let mut response = raw_client::raw_patch(cfg, &path, body)
        .await
        .map_err(|e| write_failure(cfg, "update detection", e, Permission::Console, &inspection))
        .with_context(|| format!("Detection preimage (previous): {previous}"))?;
    verify_update(&attrs, &response)
        .with_context(|| format!("Detection preimage (previous): {previous}"))?;
    let obj = response.as_object_mut().with_context(|| {
        format!("update response must be an object; Detection preimage (previous): {previous}")
    })?;
    obj.insert("previous".into(), previous);
    obj.insert("effects".into(), json!(effects));
    formatter::output(cfg, &response)
}

async fn submit_mitigation(
    cfg: &Config,
    path: &str,
    body: &Value,
    permission: Permission<'_>,
    inspection: &str,
) -> Result<Value> {
    let response = raw_client::raw_request(
        cfg,
        "POST",
        path,
        &[],
        Some(serde_json::to_vec(body)?),
        Some("application/json"),
        "*/*",
        &[],
    )
    .await
    .map_err(|e| write_failure(cfg, "mitigate detections", e, permission, inspection))?;
    Ok(if response.bytes.iter().all(u8::is_ascii_whitespace) {
        Value::Null
    } else {
        serde_json::from_slice(&response.bytes)
            .unwrap_or_else(|_| json!(String::from_utf8_lossy(&response.bytes)))
    })
}

pub async fn detections_mitigate(
    cfg: &Config,
    detection_type: &str,
    ids: &[String],
    mitigation_type: &str,
    parameters: Option<&str>,
    dry_run: bool,
) -> Result<()> {
    let ids = parse_detection_ids(ids)?;
    let parameters = parse_parameters(parameters)?;
    let control = get(
        cfg,
        &format!("control/{}", percent_encode(detection_type)),
        &[],
        Permission::Console,
    )
    .await?;
    let mitigation = find_manual_mitigation(&control, mitigation_type)?;
    let mut attrs = json!({"detection_type": detection_type, "detection_ids": ids, "mitigation_type": mitigation_type});
    if let Some(parameters) = parameters {
        attrs["mitigation_parameters"] = json!(parameters);
    }
    let body = update_body(attrs.as_object().expect("attributes are an object"));
    let path = format!("{ROOT}/detections/mitigate");
    if dry_run {
        return preview(
            cfg,
            json!({"dry_run": true, "request": {"method": "POST", "path": path, "body": body}, "mitigation": mitigation}),
        );
    }
    if !confirm_destructive(
        cfg,
        &format!(
            "Apply mitigation {mitigation_type} to {} governance detections?",
            ids.len()
        ),
    )? {
        return Ok(());
    }
    let inspection = format!(
        "pup governance detections list {detection_type} --state mitigation_in_progress,mitigated"
    );
    let response = submit_mitigation(
        cfg,
        &path,
        &body,
        Permission::Mitigation {
            control: &control,
            mitigation,
        },
        &inspection,
    )
    .await?;
    formatter::output_with_meta(
        cfg,
        &json!({"data": {"accepted": true, "detection_type": detection_type,
        "mitigation_type": mitigation_type, "detection_ids": ids, "response": response}}),
        &Metadata {
            next_action: Some(format!(
                "Mitigation runs asynchronously; check progress with `{inspection}`"
            )),
            ..Default::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{cleanup_env, lock_env, test_config};
    use mockito::{Matcher, Server};

    const FUTURE: &str = "2999-11-30T00:00:00Z";
    const USER: &str = "00000000-0000-4000-8000-000000000001";

    fn current(state: &str, deadline: Value) -> Value {
        json!({"data":{"id":"d1","type":"governance_control_detection","attributes":{
            "state":state,"mitigate_after":deadline,"assigned_to":USER,"assigned_team":null
        }}})
    }

    fn control() -> Value {
        json!({"data":{"attributes":{"mitigations":[
            {"id":"delete_dashboard","title":"Delete dashboard","description":"Deletes the dashboard",
             "execution_modes":["manual"],"permissions":["dashboards_write","org_app_keys_write"]},
            {"id":"automatic","execution_modes":["automatic"]}
        ],"detection_parameters":{"include_rbac_dashboards_in_scope":true}}}})
    }

    fn http(status: u16, body: &str) -> anyhow::Error {
        raw_client::http_error(
            status,
            "POST",
            "http://localhost/test",
            body,
            &Default::default(),
        )
        .into()
    }

    #[tokio::test]
    async fn insights_values_products_and_permissions() {
        let _lock = lock_env().await;
        let mut server = Server::new_async().await;
        let cfg = test_config(&server.url());
        for no_values in [false, true] {
            // Mockito's UrlEncoded matcher collapses repeated keys into a map.
            // Match the complete query too, to prove both product values are sent.
            let mock = server
                .mock("GET", "/api/v2/governance/insights")
                .match_query(Matcher::AllOf(vec![
                    Matcher::UrlEncoded("withValues".into(), (!no_values).to_string()),
                    Matcher::Exact(format!(
                        "withValues={}&filter%5Bproduct%5D=metrics&filter%5Bproduct%5D=logs",
                        !no_values
                    )),
                ]))
                .with_body(r#"{"data":[{"attributes":{"value":null,"new_field":true}}]}"#)
                .create_async()
                .await;
            insights_list(&cfg, &["metrics".into(), "logs".into()], no_values)
                .await
                .unwrap();
            mock.assert_async().await;
            mock.remove_async().await;
        }
        let mock = server
            .mock("GET", "/api/v2/governance/insights")
            .match_query(Matcher::Any)
            .with_status(403)
            .create_async()
            .await;
        let error = format!("{:#}", insights_list(&cfg, &[], false).await.unwrap_err());
        for value in [
            "governance_console_read",
            "metrics_read",
            "events_read",
            "audit_logs_read",
            DOCS,
        ] {
            assert!(error.contains(value), "{error}");
        }
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn controls_paths_and_not_found() {
        let _lock = lock_env().await;
        let mut server = Server::new_async().await;
        let cfg = test_config(&server.url());
        let list = server
            .mock("GET", "/api/v2/governance/control")
            .match_header("dd-api-key", "test-api-key")
            .match_header("dd-application-key", "test-app-key")
            .with_body(r#"{"data":[{"attributes":{"mitigations":null}}]}"#)
            .create_async()
            .await;
        controls_list(&cfg).await.unwrap();
        list.assert_async().await;
        let one = server
            .mock("GET", "/api/v2/governance/control/a%2Fb%20c")
            .with_body(control().to_string())
            .create_async()
            .await;
        controls_get(&cfg, "a/b c").await.unwrap();
        one.assert_async().await;
        let bad = server
            .mock("GET", "/api/v2/governance/control/missing")
            .with_status(404)
            .with_body("unknown control")
            .create_async()
            .await;
        let error = format!("{:#}", controls_get(&cfg, "missing").await.unwrap_err());
        assert!(
            error.contains("verify the ID") && error.contains("unknown control"),
            "{error}"
        );
        bad.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn detections_list_queries_and_pagination() {
        let _lock = lock_env().await;
        let mut server = Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/v2/governance/control/a%2Fb/detections")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("filter[state]".into(), "active,exception".into()),
                Matcher::UrlEncoded("filter[query]".into(), "name:a & b".into()),
                Matcher::UrlEncoded("page[number]".into(), "2".into()),
                Matcher::UrlEncoded("page[size]".into(), "1".into()),
            ]))
            .with_body(r#"{"data":[{"id":"d1"}]}"#)
            .create_async()
            .await;
        detections_list(
            &cfg,
            "a/b",
            &["active".into(), "exception".into()],
            Some("name:a & b"),
            2,
            1,
        )
        .await
        .unwrap();
        mock.assert_async().await;
        assert_eq!(
            detection_page_meta(&json!({"data":[{}]}), 2, 1).next_page,
            Some(3)
        );
        assert_eq!(
            detection_page_meta(&json!({"data":[]}), 2, 1).next_page,
            None
        );
        mock.remove_async().await;
        let bad = server
            .mock("GET", "/api/v2/governance/control/a/detections")
            .match_query(Matcher::Any)
            .with_status(403)
            .create_async()
            .await;
        assert!(format!(
            "{:#}",
            detections_list(&cfg, "a", &[], None, 0, 100)
                .await
                .unwrap_err()
        )
        .contains("governance_console_read"));
        bad.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn detections_get_encoded_id_and_error() {
        let _lock = lock_env().await;
        let mut server = Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/v2/governance/detections/d%2F1")
            .with_body(current("active", Value::Null).to_string())
            .create_async()
            .await;
        detections_get(&cfg, "d/1").await.unwrap();
        mock.assert_async().await;
        let bad = server
            .mock("GET", "/api/v2/governance/detections/missing")
            .with_status(404)
            .create_async()
            .await;
        assert!(
            format!("{:#}", detections_get(&cfg, "missing").await.unwrap_err())
                .contains("verify the ID")
        );
        bad.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn limits_merge_skip_and_resource_failure() {
        let _lock = lock_env().await;
        let mut server = Server::new_async().await;
        let cfg = test_config(&server.url());
        let limits = server
            .mock("GET", "/api/v2/governance/limits")
            .expect(4)
            .with_body(r#"{"data":[{"id":"rate","type":"limit"}]}"#)
            .create_async()
            .await;
        let resource = server
            .mock("GET", "/api/v2/governance/resource_limits")
            .with_body(r#"{"data":[{"id":"resource","type":"resource-limit"}]}"#)
            .create_async()
            .await;
        limits_list(&cfg, false).await.unwrap();
        resource.assert_async().await;
        resource.remove_async().await;
        let skipped = server
            .mock("GET", "/api/v2/governance/resource_limits")
            .expect(0)
            .create_async()
            .await;
        limits_list(&cfg, true).await.unwrap();
        skipped.assert_async().await;
        skipped.remove_async().await;
        let failed = server
            .mock("GET", "/api/v2/governance/resource_limits")
            .with_status(500)
            .create_async()
            .await;
        let error = format!("{:#}", limits_list(&cfg, false).await.unwrap_err());
        assert!(
            error.contains("resource limits") && error.contains("--no-resource-limits"),
            "{error}"
        );
        failed.assert_async().await;
        failed.remove_async().await;
        let malformed = server
            .mock("GET", "/api/v2/governance/resource_limits")
            .with_body(r#"{"data":{}}"#)
            .create_async()
            .await;
        let error = format!("{:#}", limits_list(&cfg, false).await.unwrap_err());
        assert!(
            error.contains("resource limits") && error.contains("--no-resource-limits"),
            "{error}"
        );
        malformed.assert_async().await;
        malformed.remove_async().await;
        limits.assert_async().await;
        limits.remove_async().await;
        let bad_limits = server
            .mock("GET", "/api/v2/governance/limits")
            .with_body("{}")
            .create_async()
            .await;
        let no_resources = server
            .mock("GET", "/api/v2/governance/resource_limits")
            .expect(0)
            .create_async()
            .await;
        assert!(limits_list(&cfg, false)
            .await
            .unwrap_err()
            .to_string()
            .contains("data array"));
        bad_limits.assert_async().await;
        no_resources.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn best_practices_filters_and_error() {
        let _lock = lock_env().await;
        let mut server = Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/v2/governance/best_practices")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("filter[category]".into(), "security".into()),
                Matcher::UrlEncoded("filter[query]".into(), "api keys".into()),
            ]))
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        best_practices_list(&cfg, Some("security"), Some("api keys"))
            .await
            .unwrap();
        mock.assert_async().await;
        mock.remove_async().await;
        let bad = server
            .mock("GET", "/api/v2/governance/best_practices")
            .with_status(403)
            .create_async()
            .await;
        assert!(format!(
            "{:#}",
            best_practices_list(&cfg, None, None).await.unwrap_err()
        )
        .contains("governance_console_read"));
        bad.assert_async().await;
        cleanup_env();
    }

    #[test]
    fn update_attributes_validate_and_only_include_given_fields() {
        assert_eq!(
            build_update_attributes(None, None, Some(""), Some(USER)).unwrap(),
            json!({"assigned_to":"","assigned_team":USER})
                .as_object()
                .unwrap()
                .clone()
        );
        let cases = [
            (None, None, None, None, "at least one"),
            (Some("mitigated"), None, None, None, "detections mitigate"),
            (None, Some("tomorrow"), None, None, "RFC 3339"),
            (None, Some("2000-01-01T00:00:00Z"), None, None, "future"),
            (
                Some("exception"),
                Some(FUTURE),
                None,
                None,
                "--state exception",
            ),
            (None, None, Some("bad"), None, "--assigned-to"),
            (None, None, None, Some("bad"), "--assigned-team"),
        ];
        for (state, deadline, user, team, expected) in cases {
            let err = format!(
                "{:#}",
                build_update_attributes(state, deadline, user, team).unwrap_err()
            );
            assert!(err.contains(expected), "{err}");
        }
    }

    #[test]
    fn update_effects_state_dependent_deadline_and_previous() {
        let before = current("active", json!(FUTURE));
        let attrs = build_update_attributes(Some("exception"), None, None, None).unwrap();
        let effects = plan_update_effects(&before, &attrs).unwrap().join("\n");
        assert!(effects.contains("mitigate_after") && effects.contains("cleared"));
        assert_eq!(
            previous_fields(&before),
            json!({"state":"active","mitigate_after":FUTURE,"assigned_to":USER,"assigned_team":null})
        );
        let attrs = build_update_attributes(None, Some(FUTURE), None, None).unwrap();
        assert!(
            plan_update_effects(&current("exception", Value::Null), &attrs)
                .unwrap_err()
                .to_string()
                .contains("--state active")
        );
        plan_update_effects(&before, &attrs).unwrap();
        let attrs = build_update_attributes(Some("active"), None, Some(""), None).unwrap();
        assert!(
            plan_update_effects(&current("exception", Value::Null), &attrs)
                .unwrap()
                .join("\n")
                .contains("no mitigation deadline")
        );
        let retained = plan_update_effects(&current("exception", json!(FUTURE)), &attrs)
            .unwrap()
            .join("\n");
        assert!(retained.contains("retained") && !retained.contains("no mitigation deadline"));
        assert!(plan_update_effects(&json!({}), &Map::new()).is_err());
        assert!(plan_update_effects(&json!({}), &attrs).is_err());
    }

    #[test]
    fn update_verification_rejects_mismatches_and_compares_instants() {
        let attrs = build_update_attributes(Some("active"), Some(FUTURE), None, None).unwrap();
        verify_update(
            &attrs,
            &current("active", json!("2999-11-30T01:00:00+01:00")),
        )
        .unwrap();
        assert!(verify_update(&attrs, &current("exception", json!(FUTURE)))
            .unwrap_err()
            .to_string()
            .contains("state"));
        for returned in [Value::Null, json!("bad"), json!("2999-11-29T00:00:00Z")] {
            assert!(verify_update(&attrs, &current("active", returned))
                .unwrap_err()
                .to_string()
                .contains("mitigate_after"));
        }
        let exception = build_update_attributes(Some("exception"), None, None, None).unwrap();
        assert!(verify_update(&exception, &current("exception", json!(FUTURE))).is_err());
    }

    #[tokio::test]
    async fn update_preflight_patch_and_dry_run() {
        let _lock = lock_env().await;
        let mut server = Server::new_async().await;
        let mut cfg = test_config(&server.url());
        cfg.auto_approve = true;
        for initial in ["active", "exception"] {
            let attrs = build_update_attributes(
                if initial == "exception" {
                    Some("active")
                } else {
                    None
                },
                Some(FUTURE),
                Some(""),
                None,
            )
            .unwrap();
            let get = server
                .mock("GET", "/api/v2/governance/detections/d1")
                .expect(2)
                .with_body(current(initial, Value::Null).to_string())
                .create_async()
                .await;
            let no_patch = server
                .mock("PATCH", Matcher::Any)
                .expect(0)
                .create_async()
                .await;
            detections_update(&cfg, "d1", attrs.clone(), true)
                .await
                .unwrap();
            no_patch.assert_async().await;
            no_patch.remove_async().await;
            let patch = server
                .mock("PATCH", "/api/v2/governance/detections/d1")
                .match_body(Matcher::Json(update_body(&attrs)))
                .expect(1)
                .with_body(current("active", json!(FUTURE)).to_string())
                .create_async()
                .await;
            detections_update(&cfg, "d1", attrs, false).await.unwrap();
            get.assert_async().await;
            patch.assert_async().await;
            get.remove_async().await;
            patch.remove_async().await;
        }
        let get = server
            .mock("GET", "/api/v2/governance/detections/d1")
            .with_body(current("exception", Value::Null).to_string())
            .create_async()
            .await;
        let no_patch = server
            .mock("PATCH", Matcher::Any)
            .expect(0)
            .create_async()
            .await;
        let attrs = build_update_attributes(None, Some(FUTURE), None, None).unwrap();
        assert!(detections_update(&cfg, "d1", attrs, false)
            .await
            .unwrap_err()
            .to_string()
            .contains("--state active"));
        get.assert_async().await;
        no_patch.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn update_server_mismatch_keeps_preimage_in_error() {
        let _lock = lock_env().await;
        let mut server = Server::new_async().await;
        let mut cfg = test_config(&server.url());
        cfg.auto_approve = true;
        let get = server
            .mock("GET", "/api/v2/governance/detections/d1")
            .expect(3)
            .with_body(current("exception", Value::Null).to_string())
            .create_async()
            .await;
        let requested = build_update_attributes(Some("active"), Some(FUTURE), None, None).unwrap();
        for (returned, attrs, expected) in [
            (
                current("exception", json!(FUTURE)),
                requested.clone(),
                "server did not apply",
            ),
            (
                current("active", Value::Null),
                requested,
                "server did not apply",
            ),
            (
                json!([]),
                build_update_attributes(None, None, Some(""), None).unwrap(),
                "response must be an object",
            ),
        ] {
            let patch = server
                .mock("PATCH", "/api/v2/governance/detections/d1")
                .with_body(returned.to_string())
                .create_async()
                .await;
            let error = format!(
                "{:#}",
                detections_update(&cfg, "d1", attrs, false)
                    .await
                    .unwrap_err()
            );
            assert!(
                error.contains("previous") && error.contains(expected),
                "{error}"
            );
            patch.assert_async().await;
            patch.remove_async().await;
        }
        get.assert_async().await;
        cleanup_env();
    }

    #[test]
    fn mitigate_validation_ids_parameters_and_manual_definitions() {
        assert_eq!(
            parse_detection_ids(&[" a ".into(), "b".into()]).unwrap(),
            ["a", "b"]
        );
        for ids in [
            vec![],
            vec!["".into()],
            vec!["a".into(), " a ".into()],
            (0..26).map(|i| i.to_string()).collect(),
        ] {
            assert!(parse_detection_ids(&ids).is_err());
        }
        assert_eq!(parse_parameters(None).unwrap(), None);
        assert_eq!(parse_parameters(Some("{}")).unwrap(), Some(Map::new()));
        for params in ["[1]", "not json", "null", "1", "\"text\""] {
            assert!(parse_parameters(Some(params)).is_err());
        }
        let control = control();
        assert_eq!(
            find_manual_mitigation(&control, "delete_dashboard").unwrap()["title"],
            "Delete dashboard"
        );
        for id in ["missing", "automatic"] {
            assert!(find_manual_mitigation(&control, id)
                .unwrap_err()
                .to_string()
                .contains("delete_dashboard"));
        }
        assert!(find_manual_mitigation(
            &json!({"data":{"attributes":{"mitigations":null}}}),
            "missing"
        )
        .is_err());
    }

    #[tokio::test]
    async fn mitigate_local_failures_send_nothing() {
        let _lock = lock_env().await;
        let mut server = Server::new_async().await;
        let cfg = test_config(&server.url());
        let get = server
            .mock("GET", Matcher::Any)
            .expect(0)
            .create_async()
            .await;
        let post = server
            .mock("POST", Matcher::Any)
            .expect(0)
            .create_async()
            .await;
        for ids in [
            vec![],
            vec!["".into()],
            vec!["a".into(), " a ".into()],
            (0..26).map(|i| i.to_string()).collect(),
        ] {
            assert!(
                detections_mitigate(&cfg, "test", &ids, "delete_dashboard", None, false)
                    .await
                    .is_err()
            );
        }
        for params in ["[1]", "bad json"] {
            assert!(detections_mitigate(
                &cfg,
                "test",
                &["d1".into()],
                "delete_dashboard",
                Some(params),
                false
            )
            .await
            .is_err());
        }
        get.assert_async().await;
        post.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn mitigate_control_validation_dry_run_and_exact_post() {
        let _lock = lock_env().await;
        let mut server = Server::new_async().await;
        let mut cfg = test_config(&server.url());
        cfg.auto_approve = true;
        let get = server
            .mock("GET", "/api/v2/governance/control/a%2Fb")
            .expect(6)
            .with_body(control().to_string())
            .create_async()
            .await;
        let no_post = server
            .mock("POST", Matcher::Any)
            .expect(0)
            .create_async()
            .await;
        for id in ["missing", "automatic"] {
            assert!(
                detections_mitigate(&cfg, "a/b", &["d1".into()], id, None, false)
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("delete_dashboard")
            );
        }
        detections_mitigate(&cfg, "a/b", &["d1".into()], "delete_dashboard", None, true)
            .await
            .unwrap();
        no_post.assert_async().await;
        no_post.remove_async().await;
        for response in ["", "Queued", "{\"queued\":true}"] {
            let post = server.mock("POST", "/api/v2/governance/detections/mitigate").expect(1)
                .match_header("accept", "*/*")
                .match_body(Matcher::Json(json!({"data":{"type":"governance_control_detection","attributes":{
                    "detection_type":"a/b","detection_ids":["d1","d2"],"mitigation_type":"delete_dashboard","mitigation_parameters":{"key":"value"}
                }}}))).with_status(202).with_body(response).create_async().await;
            detections_mitigate(
                &cfg,
                "a/b",
                &[" d1 ".into(), "d2".into()],
                "delete_dashboard",
                Some(r#"{"key":"value"}"#),
                false,
            )
            .await
            .unwrap();
            post.assert_async().await;
            post.remove_async().await;
        }
        get.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn errors_permissions_gate_rbac_and_no_secrets() {
        let _lock = lock_env().await;
        let mut cfg = test_config("http://localhost");
        for oauth in [false, true] {
            cfg.access_token = oauth.then(|| "fixture-secret-token".into());
            let error = format!(
                "{:#}",
                governance_error(
                    &cfg,
                    "list controls",
                    http(403, "denied"),
                    Permission::Console
                )
            );
            assert!(
                error.contains(if oauth {
                    "pup auth login --extra-scopes governance_console_read"
                } else {
                    "administrator for a role"
                }),
                "{error}"
            );
            assert!(error.contains(DOCS));
            for rbac in [false, true] {
                let mut control = control();
                control["data"]["attributes"]["detection_parameters"]
                    ["include_rbac_dashboards_in_scope"] = json!(rbac);
                let mitigation = find_manual_mitigation(&control, "delete_dashboard").unwrap();
                let error = format!(
                    "{:#}",
                    governance_error(
                        &cfg,
                        "mitigate",
                        http(403, "user does not have required permissions"),
                        Permission::Mitigation {
                            control: &control,
                            mitigation
                        }
                    )
                );
                for expected in [
                    "governance_console_read",
                    "dashboards_write",
                    "org_app_keys_write",
                    "metrics_read",
                    "server does not report which grant",
                    "pup auth status",
                    "administrator",
                    "Audit Trail",
                ] {
                    assert!(error.contains(expected), "{error}");
                }
                assert_eq!(error.contains("user_access_manage"), rbac, "{error}");
                if oauth && rbac {
                    assert!(error.contains("--extra-scopes governance_console_read,dashboards_write,org_app_keys_write,user_access_manage"));
                }
                for secret in ["test-api-key", "test-app-key", "fixture-secret-token"] {
                    assert!(!error.contains(secret));
                }
            }
            let gate = governance_error(
                &cfg,
                "list controls",
                http(
                    403,
                    "Governance Console is not enabled for this organization",
                ),
                Permission::Console,
            )
            .to_string();
            assert!(gate.contains("Governance Console is not enabled") && gate.contains(DOCS));
        }
        cleanup_env();
    }

    #[tokio::test]
    async fn write_failure_classification_and_no_retry_hints() {
        let _lock = lock_env().await;
        let cfg = test_config("http://localhost");
        for op in ["update detection", "mitigate detections"] {
            for code in [400, 403, 409, 429, 500, 503] {
                let error = format!(
                    "{:#}",
                    write_failure(
                        &cfg,
                        op,
                        http(code, "error"),
                        Permission::Console,
                        "pup governance detections get d1"
                    )
                );
                if code < 500 {
                    assert!(error.contains("rejected; nothing was applied"), "{error}");
                } else {
                    assert!(
                        error.contains("Outcome unknown")
                            && error.contains("Do not resubmit")
                            && error.contains("Audit Trail"),
                        "{error}"
                    );
                    assert!(
                        !error.contains("try again") && !error.contains("retry"),
                        "{error}"
                    );
                }
            }
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let err = reqwest::get(format!("http://{addr}")).await.unwrap_err();
        assert!(
            write_failure(&cfg, "update", err.into(), Permission::Console, "inspect")
                .to_string()
                .contains("not sent")
        );
        cleanup_env();
    }
}

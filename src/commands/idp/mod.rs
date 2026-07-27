use anyhow::Result;
use serde::Serialize;

use crate::config::Config;
use crate::formatter;
use crate::raw_client;
use crate::util_ext;

mod migrate;
pub use migrate::migrate_schema;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AssistResponse {
    entity: EntitySummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<OwnerInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    on_call: Option<OnCallInfo>,
    health: HealthSummary,
    dependencies: DependencySummary,
    metadata_gaps: Vec<String>,
    links: Vec<LinkEntry>,
    suggested_next_actions: Vec<String>,
}

#[derive(Serialize)]
struct EntitySummary {
    #[serde(rename = "ref")]
    entity_ref: String,
    name: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lifecycle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    definition_github_url: Option<String>,
}

#[derive(Serialize, Clone)]
struct LinkEntry {
    name: String,
    #[serde(rename = "type")]
    link_type: String,
    url: String,
}

#[derive(Serialize)]
struct OwnerInfo {
    team_handle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    team_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    member_count: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    contacts: Vec<ContactEntry>,
}

#[derive(Serialize)]
struct ContactEntry {
    name: String,
    #[serde(rename = "type")]
    contact_type: String,
    contact: String,
}

#[derive(Serialize)]
struct OnCallInfo {
    responders: Vec<OnCallResponder>,
}

#[derive(Serialize)]
struct OnCallResponder {
    name: String,
    email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    level: Option<String>,
}

#[derive(Serialize)]
struct HealthSummary {
    status: String,
    monitors: MonitorCounts,
    incidents: IncidentCounts,
    slos: SloCounts,
}

#[derive(Serialize)]
struct MonitorCounts {
    ok: i64,
    alert: i64,
    warn: i64,
    no_data: i64,
}

#[derive(Serialize)]
struct IncidentCounts {
    active: i64,
    stable: i64,
}

#[derive(Serialize)]
struct SloCounts {
    ok: i64,
    breached: i64,
    warning: i64,
    no_data: i64,
}

#[derive(Serialize)]
struct DependencySummary {
    upstream: Vec<String>,
    downstream: Vec<String>,
}

// ---------------------------------------------------------------------------
// Helpers to extract fields from the UEG JSON:API response
// ---------------------------------------------------------------------------

fn str_attr(attrs: &serde_json::Value, key: &str) -> Option<String> {
    attrs.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn i64_attr(attrs: &serde_json::Value, key: &str) -> i64 {
    attrs.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
}

fn extract_entity_summary(entity: &serde_json::Value) -> EntitySummary {
    let attrs = &entity["attributes"];
    let kind = entity
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("service");
    let name = str_attr(attrs, "name")
        .or_else(|| str_attr(attrs, "display_name"))
        .unwrap_or_default();

    EntitySummary {
        entity_ref: format!("{kind}:{name}"),
        name,
        kind: kind.to_string(),
        description: str_attr(attrs, "description"),
        lifecycle: str_attr(attrs, "lifecycle"),
        tier: str_attr(attrs, "tier"),
        owner: str_attr(attrs, "owner"),
        definition_github_url: str_attr(attrs, "definition_github_url"),
    }
}

fn extract_contacts(attrs: &serde_json::Value) -> Vec<ContactEntry> {
    attrs
        .get("contacts")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    Some(ContactEntry {
                        name: str_attr(c, "name")?,
                        contact_type: str_attr(c, "type")?,
                        contact: str_attr(c, "contact")?,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn extract_owner(
    included: &serde_json::Value,
    entity_contacts: Vec<ContactEntry>,
) -> Option<OwnerInfo> {
    let arr = included.as_array()?;
    let team = arr
        .iter()
        .find(|inc| inc.get("type").and_then(|t| t.as_str()) == Some("team"))?;
    let attrs = &team["attributes"];
    Some(OwnerInfo {
        team_handle: str_attr(attrs, "handle").unwrap_or_default(),
        team_name: str_attr(attrs, "name"),
        description: str_attr(attrs, "summary").or_else(|| str_attr(attrs, "description")),
        member_count: i64_attr(attrs, "user_count"),
        contacts: entity_contacts,
    })
}

/// Extract team ID from the entity graph `included` array.
fn extract_team_id(included: &serde_json::Value) -> Option<String> {
    let arr = included.as_array()?;
    let team = arr
        .iter()
        .find(|inc| inc.get("type").and_then(|t| t.as_str()) == Some("team"))?;
    str_attr(&team["attributes"], "id")
}

/// Fetch on-call responders from the on-call API for a given team ID.
async fn fetch_on_call(cfg: &Config, team_id: &str) -> Option<OnCallInfo> {
    let path = format!(
        "/api/v2/on-call/teams/{team_id}/on-call?include=responders,escalations.responders"
    );
    let data = raw_client::raw_get(cfg, &path, &[]).await.ok()?;
    let included = data.get("included")?.as_array()?;

    // Primary responders come from data.relationships.responders
    let primary_ids: Vec<String> = data
        .get("data")
        .and_then(|d| d.get("relationships"))
        .and_then(|r| r.get("responders"))
        .and_then(|r| r.get("data"))
        .and_then(|d| d.as_array())
        .map(|arr| arr.iter().filter_map(|r| str_attr(r, "id")).collect())
        .unwrap_or_default();

    // Escalation responders (secondary, tertiary, etc.)
    let escalation_ids: Vec<String> = data
        .get("data")
        .and_then(|d| d.get("relationships"))
        .and_then(|r| r.get("escalations"))
        .and_then(|r| r.get("data"))
        .and_then(|d| d.as_array())
        .into_iter()
        .flatten()
        .filter_map(|step| {
            let step_id = str_attr(step, "id")?;
            // Find this step in included to get its responders
            included.iter().find_map(|inc| {
                if str_attr(inc, "id")? == step_id {
                    inc.get("relationships")?
                        .get("responders")?
                        .get("data")?
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|r| str_attr(r, "id"))
                                .collect::<Vec<_>>()
                        })
                } else {
                    None
                }
            })
        })
        .flatten()
        .collect();

    // Resolve user IDs to names/emails from included users
    let users: Vec<&serde_json::Value> = included
        .iter()
        .filter(|inc| inc.get("type").and_then(|t| t.as_str()) == Some("users"))
        .collect();

    let mut responders = Vec::new();

    // Primary responders
    for uid in &primary_ids {
        if let Some(user) = users
            .iter()
            .find(|u| str_attr(u, "id").as_deref() == Some(uid))
        {
            let attrs = &user["attributes"];
            responders.push(OnCallResponder {
                name: str_attr(attrs, "name").unwrap_or_default(),
                email: str_attr(attrs, "email").unwrap_or_default(),
                level: Some("primary".to_string()),
            });
        }
    }

    // Escalation responders (secondary)
    for uid in &escalation_ids {
        if primary_ids.contains(uid) {
            continue; // already listed as primary
        }
        if let Some(user) = users
            .iter()
            .find(|u| str_attr(u, "id").as_deref() == Some(uid))
        {
            let attrs = &user["attributes"];
            responders.push(OnCallResponder {
                name: str_attr(attrs, "name").unwrap_or_default(),
                email: str_attr(attrs, "email").unwrap_or_default(),
                level: Some("escalation".to_string()),
            });
        }
    }

    if responders.is_empty() {
        None
    } else {
        Some(OnCallInfo { responders })
    }
}

fn extract_health(attrs: &serde_json::Value) -> HealthSummary {
    let status = str_attr(attrs, "service_health_status").unwrap_or_else(|| "unknown".to_string());
    HealthSummary {
        status,
        monitors: MonitorCounts {
            ok: i64_attr(attrs, "ok_monitors_count"),
            alert: i64_attr(attrs, "alert_monitors_count"),
            warn: i64_attr(attrs, "warning_monitors_count"),
            no_data: i64_attr(attrs, "no_data_monitors_count"),
        },
        incidents: IncidentCounts {
            active: i64_attr(attrs, "active_incidents_count"),
            stable: i64_attr(attrs, "stable_incidents_count"),
        },
        slos: SloCounts {
            ok: i64_attr(attrs, "ok_slos_count"),
            breached: i64_attr(attrs, "breached_slos_count"),
            warning: i64_attr(attrs, "warning_slos_count"),
            no_data: i64_attr(attrs, "no_data_slos_count"),
        },
    }
}

fn extract_links(attrs: &serde_json::Value) -> Vec<LinkEntry> {
    attrs
        .get("links")
        .and_then(|l| l.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|link| {
                    Some(LinkEntry {
                        name: str_attr(link, "name")?,
                        link_type: str_attr(link, "type")?,
                        url: str_attr(link, "url")?,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn has_link_type(links: &[LinkEntry], link_type: &str) -> bool {
    links.iter().any(|l| l.link_type == link_type)
}

fn compute_metadata_gaps(
    entity: &EntitySummary,
    _attrs: &serde_json::Value,
    links: &[LinkEntry],
) -> Vec<String> {
    let mut gaps = Vec::new();
    if entity.description.is_none() {
        gaps.push("description not set".into());
    }
    if entity.lifecycle.is_none() {
        gaps.push("lifecycle not set".into());
    }
    if entity.tier.is_none() {
        gaps.push("tier not set".into());
    }
    if !has_link_type(links, "runbook") {
        gaps.push("no runbook link".into());
    }
    if !has_link_type(links, "doc") && !has_link_type(links, "docs") {
        gaps.push("no documentation link".into());
    }
    gaps
}

fn compute_next_actions(entity_name: &str, health: &HealthSummary, gaps: &[String]) -> Vec<String> {
    let mut actions = Vec::new();

    if health.monitors.alert > 0 || health.incidents.active > 0 {
        actions.push(format!(
            "Investigate active alerts: `pup monitors list --tag=\"service:{entity_name}\"`"
        ));
    }
    if health.slos.breached > 0 {
        actions.push(format!(
            "Review breached SLOs: `pup slos list` and filter for {entity_name}"
        ));
    }
    if !gaps.is_empty() {
        let gap_list = gaps.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
        actions.push(format!("Fill metadata gaps: {gap_list}"));
    }
    actions.push(format!("View dependencies: `pup idp deps {entity_name}`"));
    actions.push(format!("Get owner details: `pup idp owner {entity_name}`"));

    actions
}

// ---------------------------------------------------------------------------
// Parse dependencies from /api/v1/service_dependencies response
// Format: { "service_name": { "calls": ["dep1", "dep2"] }, ... }
// ---------------------------------------------------------------------------

fn parse_dependencies(deps_data: &serde_json::Value, entity: &str) -> (Vec<String>, Vec<String>) {
    let mut upstream = Vec::new();
    let mut downstream = Vec::new();

    if let Some(deps_map) = deps_data.as_object() {
        // Downstream: services this entity calls
        if let Some(calls) = deps_map
            .get(entity)
            .and_then(|v| v.get("calls"))
            .and_then(|v| v.as_array())
        {
            downstream = calls
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }
        // Upstream: services that call this entity
        for (svc, entry) in deps_map {
            if svc == entity {
                continue;
            }
            if let Some(calls) = entry.get("calls").and_then(|v| v.as_array()) {
                if calls.iter().any(|d| d.as_str() == Some(entity)) {
                    upstream.push(svc.clone());
                }
            }
        }
    }

    (upstream, downstream)
}

// ---------------------------------------------------------------------------
// Build the UEG query URL for a service entity by name
// ---------------------------------------------------------------------------

fn entity_query_url(entity: &str, include: &str) -> String {
    let query = util_ext::percent_encode(&format!("kind:service AND name:{entity}"));
    let mut url = format!("/api/v2/idp/entity_graph/entities?query={query}&page%5Blimit%5D=1");
    if !include.is_empty() {
        url.push_str(&format!("&include={include}"));
    }
    url
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Flagship command: returns concise entity context + suggested next actions.
pub async fn assist(cfg: &Config, entity: &str) -> Result<()> {
    // Fan out: entity graph + dependencies in parallel
    let entity_path = entity_query_url(entity, "owner_teams");
    let deps_path = "/api/v1/service_dependencies?env=prod";

    let (entity_res, deps_res) = tokio::join!(
        raw_client::raw_get(cfg, &entity_path, &[]),
        raw_client::raw_get(cfg, deps_path, &[]),
    );

    let entity_data = entity_res?;

    // Parse entity from UEG response (JSON:API format: { data: [...], included: [...] })
    let entities = entity_data
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| anyhow::anyhow!("no entities found matching '{entity}'"))?;

    if entities.is_empty() {
        anyhow::bail!("no entity found matching '{entity}'");
    }

    let primary = &entities[0];
    let attrs = &primary["attributes"];
    let included = entity_data
        .get("included")
        .cloned()
        .unwrap_or(serde_json::json!([]));

    let summary = extract_entity_summary(primary);
    let contacts = extract_contacts(attrs);
    let owner = extract_owner(&included, contacts);
    let health = extract_health(attrs);

    // Fetch on-call using team ID from the entity graph response
    let on_call = match extract_team_id(&included) {
        Some(team_id) => fetch_on_call(cfg, &team_id).await,
        None => None,
    };
    let links = extract_links(attrs);
    let gaps = compute_metadata_gaps(&summary, attrs, &links);
    let next_actions = compute_next_actions(&summary.name, &health, &gaps);

    // Parse dependencies
    let (upstream, downstream) = match deps_res {
        Ok(ref deps_data) => parse_dependencies(deps_data, entity),
        Err(_) => (vec![], vec![]),
    };

    let response = AssistResponse {
        entity: summary,
        owner,
        on_call,
        health,
        dependencies: DependencySummary {
            upstream,
            downstream,
        },
        metadata_gaps: gaps,
        links,
        suggested_next_actions: next_actions,
    };

    let meta = formatter::Metadata {
        count: Some(1),
        truncated: false,
        command: Some(format!("idp assist {entity}")),
        next_action: Some(format!(
            "Use `pup idp owner {entity}` for full ownership details, or `pup idp deps {entity}` for dependency graph"
        )),
    };

    formatter::format_and_print(
        &response,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&meta),
        cfg.jq.as_deref(),
    )
}

/// Find entities matching a query.
pub async fn find(cfg: &Config, query: &str) -> Result<()> {
    // The UEG API requires kind in the query. If the user didn't specify one, default to service.
    let full_query = if query.contains("kind:") {
        query.to_string()
    } else {
        format!("kind:service AND name:*{query}*")
    };
    let encoded = util_ext::percent_encode(&full_query);
    let path = format!("/api/v2/idp/entity_graph/entities?query={encoded}&page%5Blimit%5D=10");
    let data = raw_client::raw_get(cfg, &path, &[]).await?;

    let meta = formatter::Metadata {
        count: data.get("data").and_then(|d| d.as_array()).map(|a| a.len()),
        truncated: false,
        command: Some(format!("idp find {query}")),
        next_action: Some(
            "Use `pup idp assist <entity>` for full context on a specific entity".into(),
        ),
    };

    formatter::format_and_print(
        &data,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&meta),
        cfg.jq.as_deref(),
    )
}

/// Resolve owner, team, and on-call context for an entity.
pub async fn owner(cfg: &Config, entity: &str) -> Result<()> {
    let path = entity_query_url(entity, "owner_teams");
    let data = raw_client::raw_get(cfg, &path, &[]).await?;

    let entities = data
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| anyhow::anyhow!("no entities found matching '{entity}'"))?;

    if entities.is_empty() {
        anyhow::bail!("no entity found matching '{entity}'");
    }

    let primary = &entities[0];
    let included = data
        .get("included")
        .cloned()
        .unwrap_or(serde_json::json!([]));
    let contacts = extract_contacts(&primary["attributes"]);
    let owner_info = extract_owner(&included, contacts);
    let on_call = match extract_team_id(&included) {
        Some(team_id) => fetch_on_call(cfg, &team_id).await,
        None => None,
    };

    let mut response = serde_json::json!({
        "entity": entity,
    });
    if let Some(o) = &owner_info {
        response["owner"] = serde_json::to_value(o)?;
    }
    if let Some(oc) = &on_call {
        response["on_call"] = serde_json::to_value(oc)?;
    }

    let meta = formatter::Metadata {
        count: Some(1),
        truncated: false,
        command: Some(format!("idp owner {entity}")),
        next_action: None,
    };

    formatter::format_and_print(
        &response,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&meta),
        cfg.jq.as_deref(),
    )
}

/// Show dependency and relationship context for an entity.
pub async fn deps(cfg: &Config, entity: &str) -> Result<()> {
    let deps_path = "/api/v1/service_dependencies?env=prod";
    let deps_data = raw_client::raw_get(cfg, deps_path, &[]).await?;
    let (upstream, downstream) = parse_dependencies(&deps_data, entity);

    let response = serde_json::json!({
        "entity": entity,
        "dependencies": {
            "upstream": upstream,
            "downstream": downstream,
        }
    });

    let meta = formatter::Metadata {
        count: Some(upstream.len() + downstream.len()),
        truncated: false,
        command: Some(format!("idp deps {entity}")),
        next_action: Some("Use `pup idp assist <dep_name>` to inspect any dependency".to_string()),
    };

    formatter::format_and_print(
        &response,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&meta),
        cfg.jq.as_deref(),
    )
}

/// Register a service definition from a YAML file.
pub async fn register(cfg: &Config, file: &str) -> Result<()> {
    let content =
        std::fs::read_to_string(file).map_err(|e| anyhow::anyhow!("failed to read {file}: {e}"))?;

    // Parse YAML to JSON for the API
    let yaml_value: serde_json::Value = serde_norway::from_str(&content)
        .map_err(|e| anyhow::anyhow!("failed to parse YAML in {file}: {e}"))?;

    let data = raw_client::raw_post(cfg, "/api/v2/services/definitions", yaml_value).await?;

    let service_name = content
        .lines()
        .find(|l| l.starts_with("dd-service:"))
        .and_then(|l| l.strip_prefix("dd-service:"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| file.to_string());

    let meta = formatter::Metadata {
        count: Some(1),
        truncated: false,
        command: Some(format!("idp register {file}")),
        next_action: Some(format!(
            "Use `pup idp assist {service_name}` to verify the registered service"
        )),
    };

    formatter::format_and_print(
        &data,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&meta),
        cfg.jq.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_query_url_encodes_special_chars() {
        // Colons, spaces, and other characters in entity names and the query
        // syntax must be percent-encoded so the URL is well-formed.
        let url = entity_query_url("my service", "");
        assert!(
            url.contains("kind%3Aservice"),
            "colon should be encoded: {url}"
        );
        assert!(
            url.contains("my%20service"),
            "space should be encoded: {url}"
        );
        assert!(!url.contains("include="), "empty include should be omitted");
    }

    #[test]
    fn test_entity_query_url_appends_include() {
        let url = entity_query_url("svc", "owner_teams");
        assert!(
            url.contains("&include=owner_teams"),
            "include param missing: {url}"
        );
    }
}

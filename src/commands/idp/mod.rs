use anyhow::Result;
use serde::Serialize;

use crate::config::Config;
use crate::formatter;
use crate::raw_client;
use crate::util_ext;

mod entity_kinds;
mod entity_query;
mod entity_types;
mod migrate;

pub use entity_kinds::{describe_kind, list_kinds};
pub use entity_query::{query_entities, EntityQueryOptions};
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

/// Register one or more Catalog entities from a YAML or JSON file.
pub async fn register(cfg: &Config, file: &str) -> Result<()> {
    let content =
        std::fs::read_to_string(file).map_err(|e| anyhow::anyhow!("failed to read {file}: {e}"))?;
    if content.trim().is_empty() {
        anyhow::bail!("cannot register an empty Catalog entity file: {file}");
    }
    let content_type = if std::path::Path::new(file)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        "application/json"
    } else {
        "application/yaml"
    };

    // The Catalog entity endpoint parses raw YAML (including multiple documents)
    // and JSON, and accepts legacy service-definition schemas as well as v3.
    let response = raw_client::raw_request(
        cfg,
        "POST",
        "/api/v2/catalog/entity",
        &[],
        Some(content.into_bytes()),
        Some(content_type),
        "application/json",
        &[],
    )
    .await?;
    let data = serde_json::from_slice::<serde_json::Value>(&response.bytes)
        .map_err(|e| anyhow::anyhow!("Catalog API returned invalid JSON for {file}: {e}"))?;

    let meta = registration_metadata(&data, file);

    formatter::format_and_print(
        &data,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&meta),
        cfg.jq.as_deref(),
    )
}

fn registration_metadata(data: &serde_json::Value, file: &str) -> formatter::Metadata {
    let count = data
        .pointer("/meta/count")
        .and_then(serde_json::Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .or_else(|| {
            data.get("data")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len)
        });
    let first_ref = data
        .pointer("/data/0/attributes")
        .and_then(serde_json::Value::as_object)
        .and_then(|attributes| {
            let kind = attributes.get("kind")?.as_str()?;
            let name = attributes.get("name")?.as_str()?;
            let namespace = attributes
                .get("namespace")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("default");
            Some(format!("{kind}:{namespace}/{name}"))
        });
    let entity_description = match count {
        Some(1) => "the registered entity",
        _ => "the first registered entity",
    };
    let warning_count = data
        .get("included")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|included| {
            included
                .pointer("/attributes/schema/metadata/managed/status/warnings")
                .and_then(serde_json::Value::as_array)
        })
        .map(Vec::len)
        .sum::<usize>();
    let next_action = match (warning_count, first_ref) {
        (0, Some(entity_ref)) => Some(format!(
            "Use `pup --read-only software-catalog entities list --filter-ref '{entity_ref}'` to verify {entity_description}"
        )),
        (0, None) => None,
        (count, Some(entity_ref)) => Some(format!(
            "Review {count} schema warning(s) in `included[].attributes.schema.metadata.managed.status.warnings`; then use `pup --read-only software-catalog entities list --filter-ref '{entity_ref}'` to verify {entity_description}"
        )),
        (count, None) => Some(format!(
            "Review {count} schema warning(s) in `included[].attributes.schema.metadata.managed.status.warnings`"
        )),
    };

    formatter::Metadata {
        count,
        truncated: false,
        command: Some(format!("idp register {file}")),
        next_action,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use mockito::Matcher;

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

    #[test]
    fn test_registration_metadata_uses_response_count_and_first_ref() {
        let response = serde_json::json!({
            "data": [{
                "attributes": {
                    "kind": "datastore",
                    "name": "orders",
                    "namespace": "payments"
                }
            }],
            "meta": {"count": 2}
        });

        let metadata = registration_metadata(&response, "entities.yaml");

        assert_eq!(metadata.count, Some(2));
        assert_eq!(
            metadata.next_action.as_deref(),
            Some(
                "Use `pup --read-only software-catalog entities list --filter-ref 'datastore:payments/orders'` to verify the first registered entity"
            )
        );
    }

    #[test]
    fn test_registration_metadata_surfaces_schema_warnings() {
        let response = serde_json::json!({
            "data": [{
                "attributes": {
                    "kind": "service",
                    "name": "checkout",
                    "namespace": "default"
                }
            }],
            "included": [{
                "attributes": {
                    "schema": {
                        "metadata": {
                            "managed": {
                                "status": {
                                    "warnings": [
                                        {"message": "first warning"},
                                        {"message": "second warning"}
                                    ]
                                }
                            }
                        }
                    }
                }
            }],
            "meta": {"count": 1}
        });

        let metadata = registration_metadata(&response, "service.datadog.yaml");

        assert_eq!(
            metadata.next_action.as_deref(),
            Some(
                "Review 2 schema warning(s) in `included[].attributes.schema.metadata.managed.status.warnings`; then use `pup --read-only software-catalog entities list --filter-ref 'service:default/checkout'` to verify the registered entity"
            )
        );
    }

    #[tokio::test]
    async fn test_register_posts_multi_document_yaml_to_catalog_entity() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let dir = TempDir::new("idp_register_yaml");
        let path = dir.path().join("entities.datadog.yaml");
        let body = "apiVersion: v3\nkind: service\nmetadata:\n  name: checkout\n---\nschema-version: v2.2\ndd-service: payments\n";
        std::fs::write(&path, body).unwrap();
        let response = r#"{"data":[{"attributes":{"apiVersion":"v3","kind":"service","name":"checkout","namespace":"default"}},{"attributes":{"apiVersion":"v2.2","kind":"service","name":"payments","namespace":"default"}}],"meta":{"count":2}}"#;
        let mock = server
            .mock("POST", "/api/v2/catalog/entity")
            .match_header("content-type", "application/yaml")
            .match_header("accept", "application/json")
            .match_body(Matcher::Exact(body.to_string()))
            .with_status(202)
            .with_header("content-type", "application/json")
            .with_body(response)
            .create_async()
            .await;

        let result = register(&cfg, path.to_str().unwrap()).await;

        assert!(result.is_ok(), "register failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_register_posts_json_unchanged_to_catalog_entity() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let dir = TempDir::new("idp_register_json");
        let path = dir.path().join("entity.json");
        let body = r#"{"apiVersion":"v3","kind":"datastore","metadata":{"name":"orders"}}"#;
        std::fs::write(&path, body).unwrap();
        let mock = server
            .mock("POST", "/api/v2/catalog/entity")
            .match_header("content-type", "application/json")
            .match_body(Matcher::Exact(body.to_string()))
            .with_status(202)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[],"meta":{"count":0}}"#)
            .create_async()
            .await;

        let result = register(&cfg, path.to_str().unwrap()).await;

        assert!(result.is_ok(), "register failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_register_propagates_catalog_validation_error() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let dir = TempDir::new("idp_register_error");
        let path = dir.path().join("invalid.datadog.yaml");
        std::fs::write(&path, "apiVersion: v3\nkind: service\nmetadata: {}\n").unwrap();
        let _mock = server
            .mock("POST", "/api/v2/catalog/entity")
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":[{"title":"Validation Error"}]}"#)
            .create_async()
            .await;

        let error = register(&cfg, path.to_str().unwrap()).await.unwrap_err();

        assert!(error.to_string().contains("HTTP 400"));
        assert!(error.to_string().contains("Validation Error"));
        cleanup_env();
    }

    #[tokio::test]
    async fn test_register_rejects_empty_file_without_request() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let dir = TempDir::new("idp_register_empty");
        let path = dir.path().join("empty.datadog.yaml");
        std::fs::write(&path, "  \n").unwrap();

        let error = register(&cfg, path.to_str().unwrap()).await.unwrap_err();

        assert!(error.to_string().contains("empty Catalog entity file"));
        cleanup_env();
    }
}

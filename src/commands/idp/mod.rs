use anyhow::{bail, Context, Result};
use serde::Serialize;

use crate::config::Config;
use crate::formatter;
use crate::raw_client;
use crate::util_ext;

mod entity_kinds;
mod entity_query;
mod entity_types;
mod migrate;

const RUNTIME_DEPENDENCY_RELATIONS: &str = "runtime_upstream_services,runtime_downstream_services";
const ASSIST_RELATIONS: &str = "owner_teams,runtime_upstream_services,runtime_downstream_services";
const RUNTIME_DEPENDENCY_LOOKBACK: &str = "1h";

pub use entity_kinds::{describe_kind, list_kinds};
pub use entity_query::{build_scoped_query, query_entities, EntityQueryOptions};
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

#[derive(Debug, PartialEq, Eq, Serialize)]
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

fn relationship_service_names(entity: &serde_json::Value, relation: &str) -> Vec<String> {
    let mut names = entity
        .get("relationships")
        .and_then(|relationships| relationships.get(relation))
        .and_then(|relationship| relationship.get("data"))
        .map(entity_query::parse_relationship_data)
        .unwrap_or_default()
        .into_iter()
        .filter(|identifier| identifier.kind == "service")
        .map(|identifier| {
            identifier
                .id
                .strip_prefix("ref:service:")
                .unwrap_or(&identifier.id)
                .to_string()
        })
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn extract_runtime_dependencies(entity: &serde_json::Value) -> DependencySummary {
    DependencySummary {
        upstream: relationship_service_names(entity, "runtime_upstream_services"),
        downstream: relationship_service_names(entity, "runtime_downstream_services"),
    }
}

fn first_entity<'a>(data: &'a serde_json::Value, entity: &str) -> Result<&'a serde_json::Value> {
    data.get("data")
        .and_then(serde_json::Value::as_array)
        .context("entity graph response did not contain an entity list")?
        .first()
        .ok_or_else(|| anyhow::anyhow!("no entity found matching '{entity}'"))
}

// ---------------------------------------------------------------------------
// Build the UEG query URL for a service entity by concrete ref
// ---------------------------------------------------------------------------

fn entity_query_url(entity: &str, include: &str) -> String {
    let entity_ref = serde_json::to_string(&format!("ref:service:{entity}"))
        .expect("serializing a string cannot fail");
    let query = util_ext::percent_encode(&format!("ref:{entity_ref}"));
    let mut url = format!(
        "/api/v2/idp/entity_graph/entities?query={query}&page%5Blimit%5D=1&time%5Bpast%5D={RUNTIME_DEPENDENCY_LOOKBACK}"
    );
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
    let entity_path = entity_query_url(entity, ASSIST_RELATIONS);
    let entity_data = raw_client::raw_get(cfg, &entity_path, &[]).await?;

    // Parse entity from UEG response (JSON:API format: { data: [...], included: [...] })
    let primary = first_entity(&entity_data, entity)?;
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

    let dependencies = extract_runtime_dependencies(primary);

    let response = AssistResponse {
        entity: summary,
        owner,
        on_call,
        health,
        dependencies,
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

fn find_query(query: &str) -> Result<String> {
    let query = query.trim();
    if query.is_empty() {
        bail!("search query cannot be empty");
    }
    if query.contains("kind:") || query.contains("ref:") {
        entity_query::validate_query_scope(query)?;
        return Ok(query.to_string());
    }
    Ok(format!(
        "kind:service AND name:*{}*",
        escape_ueg_glob_literal(query)
    ))
}

fn escape_ueg_glob_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_whitespace()
            || matches!(
                character,
                '\\' | '+'
                    | '-'
                    | '='
                    | '&'
                    | '|'
                    | '>'
                    | '<'
                    | '!'
                    | '('
                    | ')'
                    | '{'
                    | '}'
                    | '['
                    | ']'
                    | '^'
                    | '"'
                    | '~'
                    | '*'
                    | '?'
                    | ':'
            )
        {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// Find entities matching a query.
pub async fn find(cfg: &Config, query: &str, limit: usize, cursor: Option<&str>) -> Result<()> {
    entity_query::validate_limit("limit", limit, entity_query::MAX_PAGE_LIMIT)?;
    let full_query = find_query(query)?;
    let limit = limit.to_string();
    let mut params = vec![
        ("query", full_query.as_str()),
        ("page[limit]", limit.as_str()),
    ];
    if let Some(cursor) = cursor.filter(|cursor| !cursor.trim().is_empty()) {
        params.push(("page[cursor]", cursor));
    }
    let data = raw_client::raw_get(cfg, entity_query::ENTITIES_PATH, &params).await?;
    let (count, truncated, next_action) = entity_query::raw_response_metadata(&data);

    let meta = formatter::Metadata {
        count,
        truncated,
        command: Some(format!("idp find {query}")),
        next_action: next_action.or_else(|| {
            Some("Use `pup idp assist <entity>` for full context on a specific entity".into())
        }),
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

    let primary = first_entity(&data, entity)?;
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
    let entity_path = entity_query_url(entity, RUNTIME_DEPENDENCY_RELATIONS);
    let entity_data = raw_client::raw_get(cfg, &entity_path, &[]).await?;
    let primary = first_entity(&entity_data, entity)?;
    let dependencies = extract_runtime_dependencies(primary);
    let dependency_count = dependencies.upstream.len() + dependencies.downstream.len();

    let response = serde_json::json!({
        "entity": entity,
        "dependencies": dependencies,
    });

    let meta = formatter::Metadata {
        count: Some(dependency_count),
        truncated: false,
        command: Some(format!("idp deps {entity}")),
        next_action: Some(
            "Use `pup idp entities query` for a different lookback or broader dependency relations"
                .to_string(),
        ),
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

    fn service_with_runtime_dependencies() -> serde_json::Value {
        serde_json::json!({
            "type": "service",
            "id": "ref:service:catalog-http",
            "attributes": {
                "name": "catalog-http",
                "owner": "idp"
            },
            "relationships": {
                "runtime_upstream_services": {
                    "data": [
                        {"type": "service", "id": "ref:service:web"},
                        {"type": "service", "id": "api"},
                        {"type": "team", "id": "ref:team:idp"},
                        {"type": "service", "id": "ref:service:web"}
                    ]
                },
                "runtime_downstream_services": {
                    "data": [
                        {"type": "service", "id": "ref:service:database"},
                        {"type": "service", "id": "ref:service:cache"}
                    ]
                }
            }
        })
    }

    fn entity_graph_response() -> serde_json::Value {
        serde_json::json!({
            "data": [service_with_runtime_dependencies()],
            "included": []
        })
    }

    #[test]
    fn test_entity_query_url_encodes_special_chars() {
        // Colons, spaces, and other characters in entity names and the query
        // syntax must be percent-encoded so the URL is well-formed.
        let url = entity_query_url("my service", "");
        assert!(
            url.contains("ref%3A%22ref%3Aservice%3Amy%20service%22"),
            "concrete entity ref should be encoded: {url}"
        );
        assert!(
            url.contains("time%5Bpast%5D=1h"),
            "runtime lookback should be explicit: {url}"
        );
        assert!(!url.contains("include="), "empty include should be omitted");
    }

    #[test]
    fn test_entity_query_url_escapes_quoted_ref_values() {
        let url = entity_query_url("quoted\"service\\name", "");

        assert!(
            url.contains("quoted%5C%22service%5C%5Cname"),
            "quote and backslash should be escaped before encoding: {url}"
        );
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
    fn test_extract_runtime_dependencies_normalizes_service_refs() {
        let dependencies = extract_runtime_dependencies(&service_with_runtime_dependencies());

        assert_eq!(
            dependencies,
            DependencySummary {
                upstream: vec!["api".into(), "web".into()],
                downstream: vec!["cache".into(), "database".into()],
            }
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

    #[test]
    fn test_extract_runtime_dependencies_handles_missing_relationships() {
        let dependencies = extract_runtime_dependencies(&serde_json::json!({}));

        assert_eq!(
            dependencies,
            DependencySummary {
                upstream: Vec::new(),
                downstream: Vec::new(),
            }
        );
    }

    #[test]
    fn test_first_entity_rejects_malformed_response() {
        let error = first_entity(&serde_json::json!({"data": {}}), "catalog-http").unwrap_err();

        assert!(error
            .to_string()
            .contains("entity graph response did not contain an entity list"));
    }

    #[tokio::test]
    async fn test_deps_uses_ueg_runtime_relationships() {
        let _guard = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/api/v2/idp/entity_graph/entities")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("query".into(), "ref:\"ref:service:catalog-http\"".into()),
                Matcher::UrlEncoded("page[limit]".into(), "1".into()),
                Matcher::UrlEncoded("time[past]".into(), RUNTIME_DEPENDENCY_LOOKBACK.into()),
                Matcher::UrlEncoded("include".into(), RUNTIME_DEPENDENCY_RELATIONS.into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(entity_graph_response().to_string())
            .create_async()
            .await;
        let cfg = crate::test_support::test_config(&server.url());

        deps(&cfg, "catalog-http").await.unwrap();

        mock.assert_async().await;
        crate::test_support::cleanup_env();
    }

    #[tokio::test]
    async fn test_deps_errors_when_service_is_missing() {
        let _guard = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/api/v2/idp/entity_graph/entities")
            .match_query(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": []}"#)
            .create_async()
            .await;
        let cfg = crate::test_support::test_config(&server.url());

        let error = deps(&cfg, "missing").await.unwrap_err();

        assert!(error
            .to_string()
            .contains("no entity found matching 'missing'"));
        mock.assert_async().await;
        crate::test_support::cleanup_env();
    }

    #[tokio::test]
    async fn test_assist_fetches_runtime_dependencies_with_service_context() {
        let _guard = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/api/v2/idp/entity_graph/entities")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("include".into(), ASSIST_RELATIONS.into()),
                Matcher::UrlEncoded("page[limit]".into(), "1".into()),
                Matcher::UrlEncoded("time[past]".into(), RUNTIME_DEPENDENCY_LOOKBACK.into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(entity_graph_response().to_string())
            .create_async()
            .await;
        let cfg = crate::test_support::test_config(&server.url());

        assist(&cfg, "catalog-http").await.unwrap();

        mock.assert_async().await;
        crate::test_support::cleanup_env();
    }

    #[test]
    fn test_find_query_defaults_to_service_name_search() {
        assert_eq!(
            find_query("  catalog  ").unwrap(),
            "kind:service AND name:*catalog*"
        );
    }

    #[test]
    fn test_find_query_escapes_literal_text_from_ueg_syntax() {
        assert_eq!(
            find_query("catalog OR api").unwrap(),
            r"kind:service AND name:*catalog\ OR\ api*"
        );
        assert_eq!(
            find_query(r#"payments:(api)*\v2"#).unwrap(),
            r#"kind:service AND name:*payments\:\(api\)\*\\v2*"#
        );
    }

    #[test]
    fn test_find_query_preserves_explicit_kind_or_ref() {
        assert_eq!(
            find_query("kind:team AND name:*platform*").unwrap(),
            "kind:team AND name:*platform*"
        );
        assert_eq!(
            find_query(r#"ref:"ref:service:catalog-http""#).unwrap(),
            r#"ref:"ref:service:catalog-http""#
        );
    }

    #[test]
    fn test_find_query_rejects_empty_or_invalid_scope() {
        assert!(find_query("   ").unwrap_err().to_string().contains("empty"));
        assert!(find_query(r#"kind:"service""#)
            .unwrap_err()
            .to_string()
            .contains("query must include kind:<kind>"));
        assert!(find_query("kind:service OR kind:team")
            .unwrap_err()
            .to_string()
            .contains("top-level OR is invalid"));
        assert!(find_query("kind:service AND free_text:catalog")
            .unwrap_err()
            .to_string()
            .contains("free_text is not an entity field"));
    }

    #[tokio::test]
    async fn test_find_sends_limit_and_cursor_to_ueg() {
        let _guard = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", entity_query::ENTITIES_PATH)
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("query".into(), "kind:service AND name:*catalog*".into()),
                Matcher::UrlEncoded("page[limit]".into(), "5".into()),
                Matcher::UrlEncoded("page[cursor]".into(), "next-page".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[],"meta":{"page":{"next_cursor":"another-page"}}}"#)
            .create_async()
            .await;
        let cfg = crate::test_support::test_config(&server.url());

        find(&cfg, "catalog", 5, Some("next-page")).await.unwrap();

        mock.assert_async().await;
        crate::test_support::cleanup_env();
    }

    #[tokio::test]
    async fn test_find_escapes_literal_text_before_request() {
        let _guard = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", entity_query::ENTITIES_PATH)
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded(
                    "query".into(),
                    r"kind:service AND name:*catalog\ OR\ api*".into(),
                ),
                Matcher::UrlEncoded("page[limit]".into(), "10".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        let cfg = crate::test_support::test_config(&server.url());

        find(&cfg, "catalog OR api", 10, None).await.unwrap();

        mock.assert_async().await;
        crate::test_support::cleanup_env();
    }

    #[tokio::test]
    async fn test_find_rejects_invalid_limit_before_request() {
        let cfg = crate::test_support::test_config("http://unused.local");

        let error = find(&cfg, "catalog", 0, None).await.unwrap_err();

        assert!(error
            .to_string()
            .contains("--limit must be between 1 and 100"));
    }
}

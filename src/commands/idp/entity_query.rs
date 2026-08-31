use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use anyhow::{bail, Context, Result};
use regex::Regex;
use serde_json::Value;

use super::entity_kinds::{fetch_kind, validate_includes_against_kind};
use super::entity_types::{
    EntitiesResponse, EntityIdentity, EntityResource, NormalizedEntitiesResponse, NormalizedEntity,
    NormalizedPage, QueryEcho, RelationshipSummary, ResourceIdentifier,
};
use crate::config::Config;
use crate::formatter::{self, Metadata};
use crate::raw_client;

const ENTITIES_PATH: &str = "/api/v2/idp/entity_graph/entities";
const MAX_PAGE_LIMIT: usize = 100;
const MAX_RELATION_LIMIT: usize = 100;

#[derive(Debug, Clone)]
pub struct EntityQueryOptions {
    pub query: String,
    pub fields: Vec<String>,
    pub include: Vec<String>,
    pub order_by: Vec<String>,
    pub limit: usize,
    pub cursor: Option<String>,
    pub free_text_match: Option<String>,
    pub include_total_count: bool,
    pub timeseries_interval: String,
    pub relation_limit: usize,
    pub raw: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OrderBy {
    field: String,
    direction: String,
}

#[derive(Debug, Clone)]
struct NormalizedQueryOptions {
    query: String,
    kind: String,
    fields: Vec<String>,
    include: Vec<String>,
    order_by: Vec<OrderBy>,
    limit: usize,
    cursor: Option<String>,
    free_text_match: Option<String>,
    include_total_count: bool,
    timeseries_interval: String,
    relation_limit: usize,
    raw: bool,
}

pub async fn query_entities(cfg: &Config, options: EntityQueryOptions) -> Result<()> {
    let options = normalize_options(options)?;
    let relation_target_kinds = validate_includes(cfg, &options).await?;

    let query_pairs = entity_query_params(&options, &relation_target_kinds);
    let query_refs: Vec<(&str, &str)> = query_pairs
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let raw = raw_client::raw_get(cfg, ENTITIES_PATH, &query_refs)
        .await
        .context("failed to query Datadog entities")?;

    if options.raw {
        let (count, truncated, next_action) = raw_response_metadata(&raw);
        return formatter::format_and_print(
            &raw,
            &cfg.output_format,
            cfg.agent_mode,
            Some(&Metadata {
                count,
                truncated,
                command: Some("pup idp entities query".into()),
                next_action,
            }),
            cfg.jq.as_deref(),
        );
    }

    let response: EntitiesResponse =
        serde_json::from_value(raw).context("failed to decode Datadog entity response")?;
    let normalized = normalize_entities_response(&options, response);
    let next_action = normalized
        .page
        .next_cursor
        .as_ref()
        .map(|cursor| format!("Fetch the next page with --cursor {cursor}"));
    let metadata = Metadata {
        count: Some(normalized.count),
        truncated: normalized.page.truncated,
        command: Some("pup idp entities query".into()),
        next_action,
    };
    formatter::format_and_print(
        &normalized,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&metadata),
        cfg.jq.as_deref(),
    )
}

fn normalize_options(options: EntityQueryOptions) -> Result<NormalizedQueryOptions> {
    let query = options.query.trim().to_string();
    if query.is_empty() {
        bail!("query is required");
    }
    let kind = infer_kind(&query).ok_or_else(|| {
        anyhow::anyhow!(
            "query must include kind:<kind> or ref:\"ref:<kind>:<id>\"; quoted kind filters like kind:\"service\" are invalid"
        )
    })?;
    if has_semantic_top_level_or(&query) {
        bail!(
            "top-level OR is invalid because the entity graph cannot determine one result kind; keep kind:<kind> or ref:\"ref:<kind>:<id>\" in the shared scope, for example kind:service AND (owner:idp OR team:idp)"
        );
    }
    if free_text_pattern().is_match(&query) {
        bail!(
            "free_text is not an entity field; use a real field such as name:*text*, or set --free-text-match to partial or fuzzy"
        );
    }
    validate_limit("limit", options.limit, MAX_PAGE_LIMIT)?;
    validate_limit("relation-limit", options.relation_limit, MAX_RELATION_LIMIT)?;

    let free_text_match = normalize_free_text_match(options.free_text_match)?;
    let timeseries_interval = options.timeseries_interval.trim().to_string();
    if !is_valid_go_duration(&timeseries_interval) {
        bail!(
            "invalid timeseries interval {:?}: use a duration such as 1h, 24h, or 168h",
            options.timeseries_interval
        );
    }

    let fields = clean_strings(options.fields);
    let fields = if fields.is_empty() {
        default_fields_for_kind(&kind)
            .iter()
            .map(|field| (*field).to_string())
            .collect()
    } else {
        fields
    };

    Ok(NormalizedQueryOptions {
        query,
        kind,
        fields,
        include: clean_strings(options.include),
        order_by: normalize_order_by(options.order_by)?,
        limit: options.limit,
        cursor: options.cursor.filter(|cursor| !cursor.trim().is_empty()),
        free_text_match,
        include_total_count: options.include_total_count,
        timeseries_interval,
        relation_limit: options.relation_limit,
        raw: options.raw,
    })
}

fn validate_limit(name: &str, value: usize, maximum: usize) -> Result<()> {
    if value == 0 || value > maximum {
        bail!("--{name} must be between 1 and {maximum}, got {value}");
    }
    Ok(())
}

fn normalize_free_text_match(value: Option<String>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "partial" | "fuzzy" => Ok(Some(normalized)),
        _ => bail!(
            "invalid free-text match {:?}: use partial or fuzzy; put search text in a real field filter such as name:*text*",
            value
        ),
    }
}

fn normalize_order_by(values: Vec<String>) -> Result<Vec<OrderBy>> {
    clean_strings(values)
        .into_iter()
        .map(|value| {
            let mut parts = value.split(':');
            let field = parts.next().unwrap_or_default().trim();
            let direction = parts.next().unwrap_or("asc").trim().to_ascii_lowercase();
            if field.is_empty() || parts.next().is_some() {
                bail!("invalid --order-by {value:?}: use <field> or <field>:<asc|desc>");
            }
            if direction != "asc" && direction != "desc" {
                bail!(
                    "invalid --order-by direction {direction:?} for field {field:?}: use asc or desc"
                );
            }
            Ok(OrderBy {
                field: field.to_string(),
                direction,
            })
        })
        .collect()
}

fn clean_strings(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

async fn validate_includes(cfg: &Config, options: &NormalizedQueryOptions) -> Result<Vec<String>> {
    if options.include.is_empty() {
        return Ok(Vec::new());
    }
    let Ok(schema) = fetch_kind(cfg, &options.kind).await else {
        return Ok(Vec::new());
    };
    validate_includes_against_kind(&options.kind, &options.include, &schema)?;
    Ok(options
        .include
        .iter()
        .filter_map(|relation| schema.attributes.relations.get(relation))
        .map(|relation| relation.target_kind.trim())
        .filter(|kind| !kind.is_empty())
        .map(str::to_string)
        .collect())
}

fn entity_query_params(
    options: &NormalizedQueryOptions,
    relation_target_kinds: &[String],
) -> Vec<(String, String)> {
    let mut params = vec![
        ("query".into(), options.query.clone()),
        ("page[limit]".into(), options.limit.to_string()),
        ("time[past]".into(), options.timeseries_interval.clone()),
    ];
    if let Some(cursor) = &options.cursor {
        params.push(("page[cursor]".into(), cursor.clone()));
    }
    if !options.include.is_empty() {
        params.push(("include".into(), options.include.join(",")));
    }
    if !options.fields.is_empty() {
        params.push((
            format!("fields[{}]", options.kind),
            options.fields.join(","),
        ));
    }
    add_required_included_fields(&mut params, options, relation_target_kinds);
    if !options.order_by.is_empty() {
        params.push((
            "order_by".into(),
            options
                .order_by
                .iter()
                .map(|order| format!("{}:{}", order.field, order.direction))
                .collect::<Vec<_>>()
                .join(","),
        ));
    }
    if let Some(mode) = &options.free_text_match {
        params.push(("free_text_match".into(), mode.clone()));
    }
    if options.include_total_count {
        params.push(("meta[fields]".into(), "total_count".into()));
    }
    params
}

fn add_required_included_fields(
    params: &mut Vec<(String, String)>,
    options: &NormalizedQueryOptions,
    relation_target_kinds: &[String],
) {
    let mut kinds = relation_target_kinds.to_vec();
    kinds.extend(
        options
            .include
            .iter()
            .filter_map(|relation| required_included_field_kind(&options.kind, relation))
            .map(str::to_string),
    );
    kinds.sort_unstable();
    kinds.dedup();
    for kind in kinds {
        if !has_specific_default_fields(&kind) {
            continue;
        }
        let key = format!("fields[{kind}]");
        if params.iter().any(|(existing, _)| existing == &key) {
            continue;
        }
        params.push((key, default_fields_for_kind(&kind).join(",")));
    }
}

fn required_included_field_kind(kind: &str, relation: &str) -> Option<&'static str> {
    match kind {
        "integration.github.user"
            if matches!(
                relation,
                "assigned_pull_requests"
                    | "authored_pull_requests"
                    | "reviewed_pull_requests"
                    | "reviewing_pull_requests"
            ) =>
        {
            Some("integration.github.pull_request")
        }
        "integration.github.team" if relation == "reviewing_pull_requests" => {
            Some("integration.github.pull_request")
        }
        "integration.github.repository" if relation == "pull_requests" => {
            Some("integration.github.pull_request")
        }
        "github.repository" if relation == "pull_requests" => Some("github.pull_request"),
        _ => None,
    }
}

fn normalize_entities_response(
    options: &NormalizedQueryOptions,
    response: EntitiesResponse,
) -> NormalizedEntitiesResponse {
    let included: HashMap<String, EntityResource> = response
        .included
        .into_iter()
        .map(|entity| (entity_key(&entity.kind, &entity.id), entity))
        .collect();
    let mut warnings = Vec::new();
    let results = response
        .data
        .into_iter()
        .map(|entity| normalize_entity(entity, &included, options.relation_limit, &mut warnings))
        .collect::<Vec<_>>();
    let next_cursor =
        (!response.meta.page.next_cursor.is_empty()).then_some(response.meta.page.next_cursor);
    if next_cursor.is_some() {
        warnings.push(
            "Results are truncated. Use --cursor with the returned next_cursor to fetch the next page."
                .into(),
        );
    }

    NormalizedEntitiesResponse {
        query: QueryEcho {
            query: options.query.clone(),
            inferred_kind: options.kind.clone(),
            include: options.include.clone(),
            fields: options.fields.clone(),
            timeseries_interval: options.timeseries_interval.clone(),
            relation_limit: options.relation_limit,
        },
        count: results.len(),
        results,
        page: NormalizedPage {
            limit: options.limit,
            truncated: next_cursor.is_some(),
            next_cursor,
        },
        total_count: response.meta.total_count,
        warnings,
    }
}

fn normalize_entity(
    entity: EntityResource,
    included: &HashMap<String, EntityResource>,
    relation_limit: usize,
    warnings: &mut Vec<String>,
) -> NormalizedEntity {
    let identity = entity_identity(&entity, false);
    let fields = data_fields(&entity.attributes);
    let relationships = entity
        .relationships
        .iter()
        .filter_map(|(name, relation)| {
            let identifiers = parse_relationship_data(&relation.data);
            if identifiers.is_empty() {
                return None;
            }
            let truncated = identifiers.len() > relation_limit;
            let sample = identifiers
                .iter()
                .take(relation_limit)
                .map(|identifier| {
                    included
                        .get(&entity_key(&identifier.kind, &identifier.id))
                        .map(|related| entity_identity(related, true))
                        .unwrap_or_else(|| identifier_identity(identifier))
                })
                .collect();
            if truncated {
                warnings.push(format!(
                    "Relationship {name:?} on {} has {} entities; sample limited to {relation_limit}. Re-query that relation more narrowly if needed.",
                    identity.entity_ref,
                    identifiers.len()
                ));
            }
            Some((
                name.clone(),
                RelationshipSummary {
                    count: identifiers.len(),
                    truncated,
                    sample,
                },
            ))
        })
        .collect();

    NormalizedEntity {
        entity: identity,
        fields,
        relationships,
    }
}

fn entity_identity(entity: &EntityResource, include_fields: bool) -> EntityIdentity {
    let entity_ref = string_attribute(&entity.attributes, "ref")
        .map(str::to_string)
        .unwrap_or_else(|| {
            if entity.id.starts_with("ref:") {
                entity.id.clone()
            } else {
                format!("ref:{}:{}", entity.kind, entity.id)
            }
        });
    let display_name = ["display_name", "name", "title", "summary"]
        .iter()
        .find_map(|key| string_attribute(&entity.attributes, key).map(str::to_string));
    EntityIdentity {
        entity_ref,
        kind: entity.kind.clone(),
        id: entity.id.clone(),
        display_name,
        fields: if include_fields {
            data_fields(&entity.attributes)
        } else {
            BTreeMap::new()
        },
    }
}

fn identifier_identity(identifier: &ResourceIdentifier) -> EntityIdentity {
    EntityIdentity {
        entity_ref: if identifier.id.starts_with("ref:") {
            identifier.id.clone()
        } else {
            format!("ref:{}:{}", identifier.kind, identifier.id)
        },
        kind: identifier.kind.clone(),
        id: identifier.id.clone(),
        display_name: None,
        fields: BTreeMap::new(),
    }
}

fn data_fields(attributes: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    attributes
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "ref" | "name" | "display_name"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn string_attribute<'a>(attributes: &'a BTreeMap<String, Value>, key: &str) -> Option<&'a str> {
    attributes.get(key).and_then(Value::as_str)
}

fn parse_relationship_data(value: &Value) -> Vec<ResourceIdentifier> {
    if value.is_null() {
        return Vec::new();
    }
    serde_json::from_value::<Vec<ResourceIdentifier>>(value.clone())
        .or_else(|_| {
            serde_json::from_value::<ResourceIdentifier>(value.clone()).map(|item| vec![item])
        })
        .unwrap_or_default()
}

fn raw_response_metadata(raw: &Value) -> (Option<usize>, bool, Option<String>) {
    let count = raw.get("data").and_then(Value::as_array).map(Vec::len);
    let cursor = raw
        .pointer("/meta/page/next_cursor")
        .and_then(Value::as_str)
        .filter(|cursor| !cursor.is_empty());
    (
        count,
        cursor.is_some(),
        cursor.map(|cursor| format!("Fetch the next page with --cursor {cursor}")),
    )
}

fn entity_key(kind: &str, id: &str) -> String {
    format!("{kind}:{id}")
}

pub(super) fn infer_kind(query: &str) -> Option<String> {
    if let Some(captures) = kind_pattern().captures(query) {
        return captures.get(1).map(|value| value.as_str().to_string());
    }
    ref_kind_pattern()
        .captures(query)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_string())
}

fn kind_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?:^|[\s(])kind:([A-Za-z0-9_.-]+)").unwrap())
}

fn ref_kind_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r#"(?:^|[\s(])ref\s*:\s*"ref:([^:"]+):"#).unwrap())
}

fn free_text_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?:^|[\s(])free_text\s*:").unwrap())
}

fn has_semantic_top_level_or(query: &str) -> bool {
    let bytes = query.as_bytes();
    let mut minimum_depth = usize::MAX;
    let mut or_depths = Vec::new();
    let mut depth = 0usize;
    let mut in_quote = false;
    let mut escaped = false;

    for (index, byte) in bytes.iter().copied().enumerate() {
        if in_quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_quote = false;
            }
            continue;
        }
        match byte {
            b'"' => in_quote = true,
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b' ' | b'\t' | b'\r' | b'\n' => {}
            _ => {
                minimum_depth = minimum_depth.min(depth);
                if byte == b'O'
                    && bytes.get(index + 1) == Some(&b'R')
                    && boolean_boundary(bytes, index.checked_sub(1))
                    && boolean_boundary(bytes, Some(index + 2))
                {
                    or_depths.push(depth);
                }
            }
        }
    }
    or_depths.contains(&minimum_depth)
}

fn boolean_boundary(bytes: &[u8], index: Option<usize>) -> bool {
    let Some(byte) = index.and_then(|index| bytes.get(index)) else {
        return true;
    };
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | b'(' | b')')
}

fn is_valid_go_duration(value: &str) -> bool {
    if value.is_empty() || value.starts_with(['+', '-']) {
        return false;
    }
    let mut rest = value;
    while !rest.is_empty() {
        let number_len = duration_number_len(rest);
        if number_len == 0 {
            return false;
        }
        rest = &rest[number_len..];
        let Some(unit) = ["ns", "us", "µs", "ms", "s", "m", "h"]
            .iter()
            .find(|unit| rest.starts_with(**unit))
        else {
            return false;
        };
        rest = &rest[unit.len()..];
    }
    true
}

fn duration_number_len(value: &str) -> usize {
    let bytes = value.as_bytes();
    let integer_digits = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if integer_digits == 0 {
        return 0;
    }
    if bytes.get(integer_digits) != Some(&b'.') {
        return integer_digits;
    }
    let fraction_digits = bytes[integer_digits + 1..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if fraction_digits == 0 {
        integer_digits
    } else {
        integer_digits + 1 + fraction_digits
    }
}

pub(super) fn default_fields_for_kind(kind: &str) -> &'static [&'static str] {
    match kind {
        "service" => &[
            "name",
            "display_name",
            "owner",
            "team",
            "tier",
            "lifecycle",
            "service_health_status",
            "description",
            "contacts",
            "links",
            "additional_owners",
            "html_url",
            "ref",
        ],
        "team" => &[
            "name",
            "handle",
            "description",
            "user_count",
            "html_url",
            "ref",
        ],
        "user" => &["name", "email", "handle", "title", "status", "ref"],
        "system" => &[
            "name",
            "display_name",
            "owner",
            "description",
            "html_url",
            "ref",
        ],
        "repository" => &[
            "name",
            "display_name",
            "owner",
            "default_branch",
            "definition_github_url",
            "ref",
        ],
        "github.repository" => &[
            "name",
            "name_with_owner",
            "url",
            "visibility",
            "open_pull_requests_count",
            "ref",
        ],
        "integration.github.repository" => &[
            "name",
            "display_name",
            "full_name",
            "owner",
            "html_url",
            "ref",
        ],
        "code_location" => &[
            "name",
            "repository_id",
            "path_pattern",
            "source",
            "entity_ref",
            "ref",
        ],
        "ai_skill" => &[
            "name",
            "description",
            "owner",
            "team",
            "scope",
            "source_repo",
            "source_path",
            "source_url",
            "tags",
            "updated_at",
            "ref",
        ],
        "scorecard_outcome" => &[
            "entity_reference",
            "entity_kind",
            "entity_owner",
            "rule_name",
            "rule_id",
            "state",
            "level",
            "ref",
        ],
        "scorecard_rule" => &["name", "description", "level", "ref"],
        "incident" => &[
            "public_id",
            "title",
            "state",
            "severity",
            "service_names",
            "teams",
            "visibility",
        ],
        "monitor" => &[
            "name",
            "status",
            "monitor_id",
            "service_tags",
            "host_tags",
            "env",
            "timestamp",
            "muted",
        ],
        "slo" => &[
            "name",
            "state",
            "target_threshold",
            "sli",
            "service_names",
            "team_names",
            "error_budget_remaining",
        ],
        "current_oncall" => &[
            "current_oncall_id",
            "oncall_service_id",
            "provider",
            "user_name",
            "user_email",
            "escalation_level",
        ],
        "integration.github.pull_request" => &[
            "title",
            "state",
            "updated_at",
            "mergeable",
            "changed_files",
            "html_url",
            "ref",
        ],
        "github.pull_request" => &[
            "title",
            "state",
            "updated_at",
            "mergeable",
            "review_decision",
            "url",
            "ref",
        ],
        "integration.jira.issue" => &[
            "key",
            "summary",
            "status",
            "status_category",
            "priority",
            "assignee_name",
            "due_date",
            "html_url",
            "ref",
        ],
        "api_endpoint" => &[
            "resource_name",
            "http_method",
            "http_route",
            "http_hosts",
            "endpoint_is_public",
            "endpoint_authenticated",
            "endpoint_is_rate_limited",
            "service_name",
            "team_names",
            "last_seen_traffic_at",
            "ref",
        ],
        "library_vulnerability" => &[
            "severity",
            "repository_id",
            "services",
            "package_normalized_name",
            "package_version",
            "advisory_id",
        ],
        "source_code_vulnerability" => &[
            "severity",
            "repository_id",
            "service_name",
            "package_decl_filename",
            "code_location_filename",
            "finding_type",
        ],
        "secret" | "iac_misconfiguration" => &["severity", "repository_id"],
        "source_code_vulnerability_secfinding" => &[
            "severity",
            "finding_type",
            "repository_id",
            "service_name",
            "code_location_filename",
            "package_decl_filename",
        ],
        "integration.k8s.deployment" => &[
            "name",
            "team",
            "service",
            "cluster_name",
            "namespace",
            "available_replicas",
            "ready_replicas",
            "replicas_desired",
            "unavailable_replicas",
            "status",
            "html_url",
        ],
        "recommended_system" => &[
            "name",
            "display_name",
            "status",
            "owners",
            "components",
            "description",
            "html_url",
            "created_at",
        ],
        "code_violation" => &[
            "status",
            "k9_severity",
            "associated_service",
            "repository_id",
            "category",
            "message",
        ],
        _ => &[
            "name",
            "display_name",
            "title",
            "owner",
            "team",
            "status",
            "state",
            "html_url",
            "ref",
        ],
    }
}

fn has_specific_default_fields(kind: &str) -> bool {
    matches!(
        kind,
        "service"
            | "team"
            | "user"
            | "system"
            | "repository"
            | "github.repository"
            | "integration.github.repository"
            | "code_location"
            | "ai_skill"
            | "scorecard_outcome"
            | "scorecard_rule"
            | "incident"
            | "monitor"
            | "slo"
            | "current_oncall"
            | "integration.github.pull_request"
            | "github.pull_request"
            | "integration.jira.issue"
            | "api_endpoint"
            | "library_vulnerability"
            | "source_code_vulnerability"
            | "secret"
            | "iac_misconfiguration"
            | "source_code_vulnerability_secfinding"
            | "integration.k8s.deployment"
            | "recommended_system"
            | "code_violation"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Matcher;

    fn options(query: &str) -> EntityQueryOptions {
        EntityQueryOptions {
            query: query.into(),
            fields: Vec::new(),
            include: Vec::new(),
            order_by: Vec::new(),
            limit: 25,
            cursor: None,
            free_text_match: None,
            include_total_count: false,
            timeseries_interval: "1h".into(),
            relation_limit: 25,
            raw: false,
        }
    }

    #[test]
    fn normalizes_valid_query_and_defaults() {
        let normalized =
            normalize_options(options("kind:service AND (owner:idp OR team:idp)")).unwrap();
        assert_eq!(normalized.kind, "service");
        assert!(normalized.fields.contains(&"owner".into()));
        assert_eq!(normalized.limit, 25);
        assert_eq!(normalized.relation_limit, 25);
    }

    #[test]
    fn rejects_query_without_concrete_kind() {
        let error = normalize_options(options("name:*catalog*")).unwrap_err();
        assert!(error.to_string().contains("must include kind:<kind>"));
        let error = normalize_options(options(r#"kind:"service""#)).unwrap_err();
        assert!(error.to_string().contains("quoted kind filters"));
        let normalized = normalize_options(options(r#"ref:"ref:team:idp""#)).unwrap();
        assert_eq!(normalized.kind, "team");
    }

    #[test]
    fn rejects_semantic_top_level_or_but_allows_grouped_filters() {
        let error = normalize_options(options("kind:service OR kind:team")).unwrap_err();
        assert!(error.to_string().contains("top-level OR"));
        let error = normalize_options(options("(kind:service OR kind:team)")).unwrap_err();
        assert!(error.to_string().contains("top-level OR"));
        assert!(normalize_options(options("kind:service AND (owner:idp OR team:idp)")).is_ok());
    }

    #[test]
    fn validates_modes_limits_ordering_and_duration() {
        let mut invalid = options("kind:service");
        invalid.free_text_match = Some("contains".into());
        assert!(normalize_options(invalid).is_err());

        let mut invalid = options("kind:service");
        invalid.limit = 101;
        assert!(normalize_options(invalid).is_err());

        let mut invalid = options("kind:service");
        invalid.relation_limit = 0;
        assert!(normalize_options(invalid).is_err());

        let mut invalid = options("kind:service");
        invalid.order_by = vec!["name:sideways".into()];
        assert!(normalize_options(invalid).is_err());

        let mut invalid = options("kind:service");
        invalid.timeseries_interval = "7d".into();
        assert!(normalize_options(invalid).is_err());

        let error = normalize_options(options("kind:service AND free_text:catalog")).unwrap_err();
        assert!(error
            .to_string()
            .contains("free_text is not an entity field"));

        assert!(is_valid_go_duration("1h30m"));
        assert!(is_valid_go_duration("1.5h"));
    }

    #[test]
    fn builds_entity_query_parameters() {
        let mut input = options("kind:integration.github.repository AND name:api");
        input.include = vec!["pull_requests".into()];
        input.order_by = vec!["updated_at:desc".into()];
        input.cursor = Some("cursor value".into());
        input.include_total_count = true;
        let normalized = normalize_options(input).unwrap();
        let params: BTreeMap<_, _> = entity_query_params(&normalized, &[]).into_iter().collect();
        assert_eq!(params["page[cursor]"], "cursor value");
        assert_eq!(params["order_by"], "updated_at:desc");
        assert_eq!(params["meta[fields]"], "total_count");
        assert!(params.contains_key("fields[integration.github.pull_request]"));
    }

    #[test]
    fn normalizes_relationships_and_pagination() {
        let normalized_options = normalize_options(options("kind:service")).unwrap();
        let response: EntitiesResponse = serde_json::from_value(serde_json::json!({
            "data": [{
                "type": "service",
                "id": "checkout",
                "attributes": {"name": "checkout", "owner": "payments", "ref": "ref:service:checkout"},
                "relationships": {"owner_teams": {"data": [{"type": "team", "id": "payments"}]}}
            }],
            "included": [{
                "type": "team",
                "id": "payments",
                "attributes": {"name": "Payments", "handle": "payments"}
            }],
            "meta": {"total_count": 3, "page": {"next_cursor": "next"}}
        }))
        .unwrap();
        let normalized = normalize_entities_response(&normalized_options, response);
        assert_eq!(normalized.count, 1);
        assert_eq!(normalized.total_count, Some(3));
        assert!(normalized.page.truncated);
        assert_eq!(
            normalized.results[0].relationships["owner_teams"].sample[0]
                .display_name
                .as_deref(),
            Some("Payments")
        );
    }

    #[test]
    fn normalizes_single_null_malformed_and_truncated_relations() {
        let mut input = options("kind:service");
        input.relation_limit = 1;
        let normalized_options = normalize_options(input).unwrap();
        let response: EntitiesResponse = serde_json::from_value(serde_json::json!({
            "data": [{
                "type": "service",
                "id": "checkout",
                "attributes": {},
                "relationships": {
                    "single": {"data": {"type": "team", "id": "payments"}},
                    "many": {"data": [
                        {"type": "system", "id": "store"},
                        {"type": "system", "id": "billing"}
                    ]},
                    "empty": {"data": null},
                    "malformed": {"data": "not-an-identifier"}
                }
            }],
            "meta": {"page": {"next_cursor": ""}}
        }))
        .unwrap();

        let normalized = normalize_entities_response(&normalized_options, response);

        let entity = &normalized.results[0];
        assert_eq!(entity.entity.entity_ref, "ref:service:checkout");
        assert_eq!(entity.relationships["single"].count, 1);
        assert_eq!(entity.relationships["many"].count, 2);
        assert!(entity.relationships["many"].truncated);
        assert!(!entity.relationships.contains_key("empty"));
        assert!(!entity.relationships.contains_key("malformed"));
        assert!(normalized
            .warnings
            .iter()
            .any(|warning| warning.contains("sample limited to 1")));
    }

    #[tokio::test]
    async fn query_entities_sends_encoded_parameters() {
        let _guard = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let query = "kind:service AND owner:payments";
        let mock = server
            .mock("GET", ENTITIES_PATH)
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("query".into(), query.into()),
                Matcher::UrlEncoded("page[limit]".into(), "25".into()),
                Matcher::UrlEncoded("time[past]".into(), "1h".into()),
                Matcher::UrlEncoded(
                    "fields[service]".into(),
                    default_fields_for_kind("service").join(","),
                ),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[],"meta":{"page":{"next_cursor":""}}}"#)
            .create_async()
            .await;
        let cfg = crate::test_support::test_config(&server.url());

        query_entities(&cfg, options(query)).await.unwrap();

        mock.assert_async().await;
        crate::test_support::cleanup_env();
    }

    #[tokio::test]
    async fn query_entities_reports_api_errors() {
        let _guard = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", ENTITIES_PATH)
            .match_query(Matcher::Any)
            .with_status(500)
            .with_body("entity graph unavailable")
            .create_async()
            .await;
        let cfg = crate::test_support::test_config(&server.url());

        let error = query_entities(&cfg, options("kind:service"))
            .await
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("failed to query Datadog entities"));
        mock.assert_async().await;
        crate::test_support::cleanup_env();
    }

    #[tokio::test]
    async fn query_entities_rejects_attribute_includes_from_live_schema() {
        let _guard = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let schema = server
            .mock("GET", "/api/v2/idp/entity_graph/kinds/service")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"data":{"id":"service","attributes":{"attribute_types":{"owner":{"dataType":"string"}},"relations":{"owner_teams":{"target_kind":"team"}}}}}"#,
            )
            .create_async()
            .await;
        let cfg = crate::test_support::test_config(&server.url());
        let mut input = options("kind:service");
        input.include = vec!["owner".into()];

        let error = query_entities(&cfg, input).await.unwrap_err();

        assert!(error.to_string().contains("attribute, not a relation"));
        schema.assert_async().await;
        crate::test_support::cleanup_env();
    }

    #[tokio::test]
    async fn query_entities_projects_known_relation_target_kinds() {
        let _guard = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let schema = server
            .mock("GET", "/api/v2/idp/entity_graph/kinds/service")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"data":{"id":"service","attributes":{"relations":{"owner_teams":{"target_kind":"team"},"systems":{"target_kind":"system"}}}}}"#,
            )
            .create_async()
            .await;
        let entities = server
            .mock("GET", ENTITIES_PATH)
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded(
                    "fields[team]".into(),
                    default_fields_for_kind("team").join(","),
                ),
                Matcher::UrlEncoded(
                    "fields[system]".into(),
                    default_fields_for_kind("system").join(","),
                ),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[],"meta":{"page":{"next_cursor":""}}}"#)
            .create_async()
            .await;
        let cfg = crate::test_support::test_config(&server.url());
        let mut input = options("kind:service");
        input.include = vec!["owner_teams".into(), "systems".into()];

        query_entities(&cfg, input).await.unwrap();

        schema.assert_async().await;
        entities.assert_async().await;
        crate::test_support::cleanup_env();
    }

    #[tokio::test]
    async fn query_entities_continues_when_kind_schema_is_unavailable() {
        let _guard = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let schema = server
            .mock("GET", "/api/v2/idp/entity_graph/kinds/service")
            .with_status(500)
            .with_body("schema unavailable")
            .create_async()
            .await;
        let entities = server
            .mock("GET", ENTITIES_PATH)
            .match_query(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[],"meta":{"page":{"next_cursor":""}}}"#)
            .create_async()
            .await;
        let cfg = crate::test_support::test_config(&server.url());
        let mut input = options("kind:service");
        input.include = vec!["owner_teams".into()];

        query_entities(&cfg, input).await.unwrap();

        schema.assert_async().await;
        entities.assert_async().await;
        crate::test_support::cleanup_env();
    }

    #[tokio::test]
    async fn query_entities_rejects_malformed_json() {
        let _guard = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", ENTITIES_PATH)
            .match_query(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("not json")
            .create_async()
            .await;
        let cfg = crate::test_support::test_config(&server.url());

        let error = query_entities(&cfg, options("kind:service"))
            .await
            .unwrap_err();

        assert!(format!("{error:#}").contains("expected ident"));
        mock.assert_async().await;
        crate::test_support::cleanup_env();
    }
}

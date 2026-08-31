use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use serde::Serialize;

use super::entity_query::default_fields_for_kind;
use super::entity_types::{
    KindAttribute, KindListResponse, KindRelation, KindResource, KindResponse,
};
use crate::config::Config;
use crate::formatter::{self, Metadata};
use crate::raw_client;
use crate::util_ext;

const KINDS_PATH: &str = "/api/v2/idp/entity_graph/kinds";

#[derive(Debug, Clone, Serialize)]
struct CuratedKindsResponse {
    categories: Vec<KindCategory>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    custom_kinds: Vec<KindSummary>,
    hints: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct KindCategory {
    name: String,
    description: String,
    kinds: Vec<KindSummary>,
}

#[derive(Debug, Clone, Serialize)]
struct KindSummary {
    kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    display_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    why_use: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    top_fields: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    top_relations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AllKindsResponse {
    kinds: Vec<KindSummary>,
    count: usize,
}

#[derive(Debug, Serialize)]
struct DescribeKindResponse {
    kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    display_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    description: String,
    kind_exists: bool,
    schema_available: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    default_fields: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    attributes: Vec<AttributeSummary>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    relations: Vec<RelationSummary>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    examples: Vec<QueryExample>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    hints: Vec<String>,
    caveats: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AttributeSummary {
    name: String,
    data_type: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    operators: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    calculation: Option<String>,
}

#[derive(Debug, Serialize)]
struct RelationSummary {
    name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    target_kind: String,
}

#[derive(Debug, Serialize)]
struct QueryExample {
    question: String,
    query: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fields: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    include: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeseries_interval: Option<String>,
}

pub async fn list_kinds(
    cfg: &Config,
    all: bool,
    include_custom: bool,
    include_low_level: bool,
    exclude_experimental: bool,
) -> Result<()> {
    let include_experimental = !exclude_experimental;
    if all || include_custom {
        cfg.validate_auth()?;
        let live = fetch_kinds(cfg).await?;
        if all {
            let response = build_all_kinds_response(
                live,
                include_custom,
                include_low_level,
                include_experimental,
            );
            let metadata = kind_metadata(response.count);
            return formatter::format_and_print(
                &response,
                &cfg.output_format,
                cfg.agent_mode,
                Some(&metadata),
                cfg.jq.as_deref(),
            );
        }
        let mut response = curated_kinds(include_low_level, include_experimental);
        response.custom_kinds = custom_kind_summaries(&live);
        let metadata = kind_metadata(curated_count(&response));
        return formatter::format_and_print(
            &response,
            &cfg.output_format,
            cfg.agent_mode,
            Some(&metadata),
            cfg.jq.as_deref(),
        );
    }

    let response = curated_kinds(include_low_level, include_experimental);
    let metadata = kind_metadata(curated_count(&response));
    formatter::format_and_print(
        &response,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&metadata),
        cfg.jq.as_deref(),
    )
}

pub async fn describe_kind(cfg: &Config, kind: &str, no_examples: bool) -> Result<()> {
    let kind = kind.trim();
    validate_kind_name(kind)?;
    cfg.validate_auth()?;

    let response = match fetch_kind(cfg, kind).await {
        Ok(schema) => describe_response(schema, !no_examples),
        Err(detail_error) if http_status(&detail_error) == Some(404) => {
            match fetch_kind_from_list(cfg, kind).await {
                Ok(schema) => {
                    let mut response = describe_response(schema, !no_examples);
                    response.warnings.push(format!(
                        "The kind detail endpoint returned 404 for {kind:?}; schema was loaded from the kind list instead."
                    ));
                    response
                }
                Err(_) => {
                    if let Some(summary) = curated_kind_summary(kind) {
                        describe_fallback_response(summary, !no_examples, &detail_error)
                    } else {
                        bail!(
                            "failed to describe entity kind {kind:?}: {detail_error}{}",
                            unknown_kind_suggestion(kind)
                        );
                    }
                }
            }
        }
        Err(error) => bail!(
            "failed to describe entity kind {kind:?}: {error}{}",
            unknown_kind_suggestion(kind)
        ),
    };

    formatter::format_and_print(
        &response,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&Metadata {
            count: Some(response.attributes.len() + response.relations.len()),
            truncated: false,
            command: Some("pup idp kinds describe".into()),
            next_action: Some(format!(
                "Query this kind with: pup idp entities query 'kind:{kind}'"
            )),
        }),
        cfg.jq.as_deref(),
    )
}

fn kind_metadata(count: usize) -> Metadata {
    Metadata {
        count: Some(count),
        truncated: false,
        command: Some("pup idp kinds list".into()),
        next_action: Some("Inspect a kind with: pup idp kinds describe <kind>".into()),
    }
}

pub(super) async fn fetch_kind(cfg: &Config, kind: &str) -> Result<KindResource> {
    validate_kind_name(kind)?;
    let path = format!("{KINDS_PATH}/{}", util_ext::percent_encode(kind));
    let value = raw_client::raw_get(cfg, &path, &[]).await?;
    let response: KindResponse =
        serde_json::from_value(value).context("failed to decode entity kind schema")?;
    Ok(response.data)
}

async fn fetch_kinds(cfg: &Config) -> Result<Vec<KindResource>> {
    let value = raw_client::raw_get(cfg, KINDS_PATH, &[])
        .await
        .context("failed to list Datadog entity kinds")?;
    let response: KindListResponse =
        serde_json::from_value(value).context("failed to decode entity kind list")?;
    Ok(response.data)
}

async fn fetch_kind_from_list(cfg: &Config, kind: &str) -> Result<KindResource> {
    fetch_kinds(cfg)
        .await?
        .into_iter()
        .find(|candidate| candidate.kind() == kind)
        .ok_or_else(|| anyhow::anyhow!("kind {kind:?} was not found in the kind list"))
}

fn validate_kind_name(kind: &str) -> Result<()> {
    if kind.is_empty() {
        bail!("kind is required");
    }
    if !kind
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        bail!("invalid kind {kind:?}: use only letters, numbers, dots, underscores, and hyphens");
    }
    Ok(())
}

fn http_status(error: &anyhow::Error) -> Option<u16> {
    error
        .downcast_ref::<raw_client::HttpError>()
        .map(|error| error.status)
}

pub(super) fn validate_includes_against_kind(
    kind: &str,
    includes: &[String],
    schema: &KindResource,
) -> Result<()> {
    for include in includes {
        if schema.attributes.relations.contains_key(include) {
            continue;
        }
        if schema.attributes.attribute_types.contains_key(include) {
            bail!(
                "invalid --include {include:?} for kind {kind:?}: {include:?} is an attribute, not a relation.{}",
                include_attribute_suggestion(kind, include)
            );
        }
        bail!(
            "invalid --include {include:?} for kind {kind:?}: --include only accepts relation names.{}",
            valid_relation_suggestion(&schema.attributes.relations)
        );
    }
    Ok(())
}

fn include_attribute_suggestion(kind: &str, include: &str) -> String {
    if kind == "service" {
        return match include {
            "owner" => " Use --field owner to return the owner attribute, or --include owner_teams to expand owner team entities.".into(),
            "contacts" => " Use --field contacts to return service contacts; contacts is not expandable with --include.".into(),
            "links" => " Use --field links to return service links; links is not expandable with --include.".into(),
            "additional_owners" => " Use --field additional_owners to return additional owners; additional_owners is not expandable with --include.".into(),
            _ => format!(" Use --field {include} to return the attribute."),
        };
    }
    format!(" Use --field {include} to return the attribute.")
}

fn valid_relation_suggestion(relations: &BTreeMap<String, KindRelation>) -> String {
    if relations.is_empty() {
        return " This kind has no described relations. Run pup idp kinds describe <kind> to inspect its fields and relations.".into();
    }
    let names = relations.keys().take(6).cloned().collect::<Vec<_>>();
    format!(
        " Valid relations include: {}. Run pup idp kinds describe <kind> for the full schema.",
        names.join(", ")
    )
}

fn build_all_kinds_response(
    kinds: Vec<KindResource>,
    include_custom: bool,
    include_low_level: bool,
    include_experimental: bool,
) -> AllKindsResponse {
    let mut summaries = kinds
        .iter()
        .filter(|kind| include_custom || !kind.kind().starts_with("idp_custom_entities."))
        .filter(|kind| include_low_level || !is_low_level_kind(kind.kind()))
        .filter(|kind| include_experimental || !is_experimental_kind(kind.kind()))
        .map(live_kind_summary)
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| left.kind.cmp(&right.kind));
    AllKindsResponse {
        count: summaries.len(),
        kinds: summaries,
    }
}

fn custom_kind_summaries(kinds: &[KindResource]) -> Vec<KindSummary> {
    let mut summaries = kinds
        .iter()
        .filter(|kind| kind.kind().starts_with("idp_custom_entities."))
        .map(|kind| live_kind_summary_with_limits(kind, 8, 5))
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| left.kind.cmp(&right.kind));
    summaries
}

fn live_kind_summary(kind: &KindResource) -> KindSummary {
    live_kind_summary_with_limits(kind, 12, 8)
}

fn live_kind_summary_with_limits(
    kind: &KindResource,
    field_limit: usize,
    relation_limit: usize,
) -> KindSummary {
    KindSummary {
        kind: kind.kind().to_string(),
        display_name: kind.attributes.display_name.clone(),
        why_use: String::new(),
        top_fields: kind
            .attributes
            .attribute_types
            .keys()
            .take(field_limit)
            .cloned()
            .collect(),
        top_relations: kind
            .attributes
            .relations
            .keys()
            .take(relation_limit)
            .cloned()
            .collect(),
    }
}

fn describe_response(schema: KindResource, include_examples: bool) -> DescribeKindResponse {
    let kind = schema.kind().to_string();
    let default_fields = default_fields_for_kind(&kind)
        .iter()
        .filter(|field| schema.attributes.attribute_types.contains_key(**field))
        .map(|field| (*field).to_string())
        .collect();
    let attributes = summarize_attributes(&schema.attributes.attribute_types);
    let relations = summarize_relations(&schema.attributes.relations);
    let examples = if include_examples {
        examples_for_kind(
            &kind,
            &schema.attributes.attribute_types,
            &schema.attributes.relations,
        )
    } else {
        Vec::new()
    };
    DescribeKindResponse {
        kind: kind.clone(),
        display_name: schema.attributes.display_name,
        description: String::new(),
        kind_exists: true,
        schema_available: true,
        default_fields,
        attributes,
        relations,
        examples,
        hints: hints_for_kind(&kind),
        caveats: common_caveats(),
        warnings: Vec::new(),
    }
}

fn describe_fallback_response(
    summary: KindSummary,
    include_examples: bool,
    cause: &anyhow::Error,
) -> DescribeKindResponse {
    let examples = if include_examples {
        examples_for_kind(&summary.kind, &BTreeMap::new(), &BTreeMap::new())
    } else {
        Vec::new()
    };
    let relations = summary
        .top_relations
        .iter()
        .map(|name| RelationSummary {
            name: name.clone(),
            target_kind: String::new(),
        })
        .collect();
    let mut hints = hints_for_kind(&summary.kind);
    hints.push(
        "Live schema is unavailable for this curated kind. A query may still work with the curated fields, relations, and examples shown here."
            .into(),
    );
    DescribeKindResponse {
        kind: summary.kind.clone(),
        display_name: summary.display_name,
        description: summary.why_use,
        kind_exists: true,
        schema_available: false,
        default_fields: summary.top_fields,
        attributes: Vec::new(),
        relations,
        examples,
        hints,
        caveats: common_caveats(),
        warnings: vec![format!(
            "Live schema detail was unavailable for {:?} ({cause}). This does not mean the kind is invalid.",
            summary.kind
        )],
    }
}

fn summarize_attributes(attributes: &BTreeMap<String, KindAttribute>) -> Vec<AttributeSummary> {
    attributes
        .iter()
        .map(|(name, attribute)| AttributeSummary {
            name: name.clone(),
            data_type: attribute.data_type.clone(),
            operators: operators_for_data_type(&attribute.data_type),
            calculation: attribute
                .calculation
                .as_ref()
                .map(|calculation| calculation.calculation_type.clone())
                .filter(|calculation| !calculation.is_empty()),
        })
        .collect()
}

fn summarize_relations(relations: &BTreeMap<String, KindRelation>) -> Vec<RelationSummary> {
    relations
        .iter()
        .map(|(name, relation)| RelationSummary {
            name: name.clone(),
            target_kind: relation.target_kind.clone(),
        })
        .collect()
}

fn operators_for_data_type(data_type: &str) -> Vec<String> {
    let normalized = data_type.to_ascii_lowercase();
    let operators: &[&str] = if normalized == "map" || normalized.starts_with("map<") {
        &[]
    } else if matches!(normalized.as_str(), "string" | "uuid") {
        &["eq", "exists", "missing", "prefix", "wildcard"]
    } else if normalized.starts_with("list<string>") {
        &["eq", "exists", "missing", "wildcard"]
    } else if normalized.starts_with("list") {
        &["eq", "exists", "missing"]
    } else if matches!(normalized.as_str(), "int" | "double" | "float" | "decimal") {
        &["eq", "gt", "gte", "lt", "lte", "range", "exists", "missing"]
    } else if matches!(normalized.as_str(), "bool" | "boolean")
        || normalized.contains("timestamp")
        || matches!(normalized.as_str(), "date" | "time")
    {
        &["eq", "exists", "missing"]
    } else {
        &["exists", "missing"]
    };
    operators
        .iter()
        .map(|operator| (*operator).into())
        .collect()
}

fn common_caveats() -> Vec<String> {
    vec![
        "Use returned field and relation names exactly. Quoted kind filters like kind:\"service\" are invalid; use kind:service."
            .into(),
    ]
}

fn hints_for_kind(kind: &str) -> Vec<String> {
    let hints: &[&str] = match kind {
        "service" => &[
            "Use service for ownership metadata, contacts, links, descriptions, health, incidents, monitors, SLOs, scorecards, and related operational context.",
            "For Slack/contact/channel questions, request service fields such as contacts, links, owner, team, and additional_owners.",
        ],
        "team" => &[
            "Use team for membership, hierarchy, and ownership identity.",
            "Contact metadata for what a team owns usually lives on service entities. Query services owned by the team and request contacts, links, owner, team, and additional_owners.",
        ],
        "ai_skill" => &["Use ai_skill for agent skill inventory and source lookup. Repository/source fields connect skills back to code."],
        "scorecard_outcome" => &["scorecard_outcome is deprecated in the entity graph. Prefer service aggregate fields such as highest_completed_scorecard_level."],
        "scorecard_rule" | "scorecard_entity" => &["Do not expand the deprecated scorecard_outcomes relation. Prefer service aggregate fields for high-level scorecard reports."],
        "integration.github.pull_request" => &["When filtering by pull request number, also scope by repository.full_name to avoid repository fanout limits."],
        "source_code_vulnerability_secfinding" => &["service_name is useful when populated, but many rows may not be service-attributed. Treat repository-scoped monorepo results as broad fan-in, not exact service ownership."],
        "integration.k8s.deployment" => &["Always scope deployment queries by team or service. Unscoped fleet-wide queries can return very large result sets."],
        "recommended_system" => &["Use recommended_system for system-grouping recommendations."],
        _ => &[],
    };
    hints.iter().map(|hint| (*hint).into()).collect()
}

fn examples_for_kind(
    kind: &str,
    attributes: &BTreeMap<String, KindAttribute>,
    relations: &BTreeMap<String, KindRelation>,
) -> Vec<QueryExample> {
    match kind {
        "service" => vec![
            example(
                "Pre-change service brief",
                "kind:service AND name:catalog-http",
                &[
                    "name",
                    "display_name",
                    "owner",
                    "team",
                    "contacts",
                    "links",
                    "service_health_status",
                    "highest_completed_scorecard_level",
                ],
                &["systems", "code_locations", "current_oncalls"],
                Some("1h"),
            ),
            example(
                "Services owned by a team with active incidents",
                "kind:service AND owner:\"team-x\" AND active_incidents_count:>0",
                &["name", "display_name", "owner", "active_incidents_count"],
                &["owner_teams"],
                Some("1h"),
            ),
            example(
                "Services missing an owner",
                "kind:service AND _missing_:owner",
                &["name", "display_name", "owner"],
                &[],
                None,
            ),
        ],
        "team" => vec![example(
            "Team membership and hierarchy",
            "kind:team AND name:idp",
            &["name", "handle", "description", "user_count"],
            &["users", "parent_teams", "child_teams"],
            None,
        )],
        "system" => vec![example(
            "System member services",
            "kind:system AND name:service-catalog",
            &["name", "display_name", "owner"],
            &["services"],
            None,
        )],
        "ai_skill" => vec![example(
            "AI skills from a repository",
            "kind:ai_skill AND source_repo:dd-source",
            &[
                "name",
                "description",
                "owner",
                "team",
                "source_repo",
                "source_path",
                "source_url",
            ],
            &["repository"],
            None,
        )],
        "incident" => vec![example(
            "Active incidents on a service",
            "kind:incident AND service_names:catalog-http AND state:active",
            &[
                "public_id",
                "title",
                "state",
                "severity",
                "service_names",
                "teams",
            ],
            &[],
            Some("24h"),
        )],
        "integration.github.pull_request" => vec![example(
            "Open pull requests by author",
            "kind:integration.github.pull_request AND author.login:octocat AND state:open",
            &[
                "title",
                "state",
                "updated_at",
                "mergeable",
                "changed_files",
                "html_url",
            ],
            &["repository", "author"],
            None,
        )],
        "integration.jira.issue" => vec![example(
            "Jira issue by key",
            "kind:integration.jira.issue AND key:\"SER-2180\"",
            &[
                "key",
                "summary",
                "status",
                "status_category",
                "priority",
                "assignee_name",
                "html_url",
            ],
            &[],
            None,
        )],
        "api_endpoint" => vec![example(
            "Public API endpoints owned by a team",
            "kind:api_endpoint AND service.owner:idp AND endpoint_is_public:true",
            &[
                "resource_name",
                "http_method",
                "http_route",
                "endpoint_is_public",
                "endpoint_authenticated",
                "endpoint_is_rate_limited",
                "service_name",
                "team_names",
            ],
            &["service", "teams"],
            None,
        )],
        "secret" | "iac_misconfiguration" => vec![example(
            "Findings by repository",
            &format!("kind:{kind} AND repository_id:\"github.com/datadog/dd-source\""),
            &["severity", "repository_id"],
            &[],
            None,
        )],
        "integration.k8s.deployment" => vec![example(
            "Kubernetes deployments by team",
            "kind:integration.k8s.deployment AND team:idp",
            &[
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
            &[],
            None,
        )],
        _ => generic_examples(kind, attributes, relations),
    }
}

fn generic_examples(
    kind: &str,
    attributes: &BTreeMap<String, KindAttribute>,
    relations: &BTreeMap<String, KindRelation>,
) -> Vec<QueryExample> {
    let fields = default_fields_for_kind(kind)
        .iter()
        .filter(|field| attributes.is_empty() || attributes.contains_key(**field))
        .copied()
        .collect::<Vec<_>>();
    let mut examples = vec![example(
        "List this kind",
        &format!("kind:{kind}"),
        &fields,
        &[],
        None,
    )];
    if attributes.contains_key("name") {
        examples.push(example(
            "Find by name substring",
            &format!("kind:{kind} AND name:*example*"),
            &["name"],
            &[],
            None,
        ));
    }
    if let Some(relation) = relations.keys().next() {
        examples.push(example(
            "Filter through a relation",
            &format!("kind:{kind} AND {relation}.name:*example*"),
            &[],
            &[],
            None,
        ));
    }
    examples
}

fn example(
    question: &str,
    query: &str,
    fields: &[&str],
    include: &[&str],
    timeseries_interval: Option<&str>,
) -> QueryExample {
    QueryExample {
        question: question.into(),
        query: query.into(),
        fields: strings(fields),
        include: strings(include),
        timeseries_interval: timeseries_interval.map(str::to_string),
    }
}

fn curated_kinds(include_low_level: bool, include_experimental: bool) -> CuratedKindsResponse {
    let mut categories = base_categories();
    if include_experimental {
        categories.extend(experimental_categories());
    }
    if include_low_level {
        categories.push(category(
            "infrastructure_low_level",
            "Runtime infrastructure kinds. Useful for blast radius but noisy for default discovery.",
            vec![
                summary("deployment", "Deployment", "", &["name", "cluster_name", "namespace"], &["namespaces"]),
                summary("host", "Host", "", &["hostname", "service", "team"], &["services"]),
                summary("pod", "Pod", "", &["name", "service", "namespace"], &["services", "namespace"]),
                summary("container", "Container", "", &["name", "service"], &["services"]),
                summary("namespace", "Namespace", "", &["name"], &["services", "pods"]),
                summary("cluster", "Cluster", "", &["cluster_name", "cluster_id", "node_count"], &[]),
                summary("environment", "Environment", "", &["name"], &[]),
            ],
        ));
    }
    CuratedKindsResponse {
        categories,
        custom_kinds: Vec::new(),
        hints: vec![
            "For contact or channel questions about a team, query the team's owned services and request service fields such as contacts, links, owner, team, and additional_owners.".into(),
            "The integration.github.* and github.* families overlap; prefer integration.github.* for repository and work-mirror data. Use native github.* when check/status graph details are specifically needed.".into(),
        ],
    }
}

fn base_categories() -> Vec<KindCategory> {
    vec![
        category(
            "core",
            "Primary entity graph anchors agents should usually start with.",
            vec![
                summary("service", "Service", "Core software/service entity with ownership, contacts, links, health, scorecard, dependency, vulnerability, and operational aggregates.", &["name", "display_name", "owner", "team", "tier", "lifecycle", "contacts", "links", "service_health_status"], &["owner_teams", "current_oncalls", "systems", "incidents", "slos", "monitors"]),
                summary("team", "Team", "Ownership and accountability anchor for services, systems, users, and work.", &["name", "handle", "description", "user_count"], &["owned_services", "users", "parent_teams", "child_teams"]),
                summary("user", "User", "People graph node for ownership, review, assignment, and identity workflows.", &["name", "email", "handle", "title"], &["teams"]),
                summary("system", "System", "Groups services and code locations into a product or system boundary.", &["name", "display_name", "owner"], &["services", "code_locations", "current_oncalls"]),
                summary("repository", "Repository", "Connects source ownership and catalog entities to code.", &["name", "display_name", "owner", "definition_github_url"], &["systems", "current_oncalls"]),
                summary("code_location", "Code Location", "Bridge between services or systems and concrete source locations.", &["name", "repository_id", "path_pattern", "source"], &["repository", "services", "systems"]),
            ],
        ),
        category(
            "operations",
            "Health, incident, SLO, monitor, and on-call context.",
            vec![
                summary("incident", "Incident", "Incident state connected to services and teams.", &["public_id", "title", "state", "severity", "service_names", "teams"], &["services"]),
                summary("monitor", "Monitor", "Alerting state connected to service tags and monitor metadata.", &["name", "status", "monitor_id", "service_tags"], &[]),
                summary("slo", "SLO", "Reliability objectives connected to services and teams.", &["name", "state", "target_threshold", "sli"], &["services", "teams"]),
                summary("current_oncall", "Current On-Call", "Current on-call records keyed by provider service id.", &["current_oncall_id", "oncall_service_id", "provider", "user_name", "user_email"], &[]),
                summary("pagerduty_incident", "PagerDuty Incident", "PagerDuty incident state routed through service ownership.", &["title", "status", "pagerduty_incident_id"], &["services"]),
                summary("opsgenie_incident", "Opsgenie Incident", "Opsgenie incident state routed through service ownership.", &["title", "status"], &["services"]),
            ],
        ),
        category(
            "scorecards_governance",
            "Software Catalog opinion and governance layer.",
            vec![
                summary("scorecard_outcome", "Scorecard Outcome", "Deprecated entity kind. Prefer service scorecard aggregate fields such as highest_completed_scorecard_level.", &["entity_reference", "entity_kind", "entity_owner", "rule_name", "state", "level"], &["scorecard_rule", "scorecard_entity"]),
                summary("scorecard_rule", "Scorecard Rule", "Rule definitions. Do not expand the deprecated scorecard_outcomes relation.", &["name", "description", "level"], &["scorecard_outcomes"]),
                summary("scorecard_entity", "Scorecard Entity", "Scorecard-scoped entity records. Do not expand the deprecated scorecard_outcomes relation.", &["reference", "entity_kind", "owner"], &["scorecard_outcomes"]),
                summary("recommended_system", "Recommended System", "System-grouping recommendations for catalog hygiene and ownership cleanup.", &["name", "display_name", "status", "owners", "components", "html_url"], &[]),
                summary("campaign", "Campaign", "Governance campaigns connected to remediation work.", &["title", "status", "status_name"], &[]),
                summary("work", "Work", "Governance and remediation work items.", &["title", "status", "status_name", "assignee_id"], &[]),
            ],
        ),
        category(
            "security_code_quality",
            "Security and code-quality findings connected to ownership.",
            vec![
                summary("library_vulnerability", "Library Vulnerability", "Dependency vulnerabilities with repository id and affected services fields.", &["severity", "repository_id", "services"], &[]),
                summary("source_code_vulnerability", "Source Code Vulnerability", "Code vulnerabilities with repository id and service name fields.", &["severity", "repository_id", "service_name"], &[]),
                summary("code_violation", "Code Violation", "Code quality findings with associated service and repository id fields.", &["status", "k9_severity", "associated_service", "repository_id"], &[]),
                summary("secret", "Secret", "Secret-scanning findings by repository.", &["severity", "repository_id"], &[]),
                summary("iac_misconfiguration", "IaC Misconfiguration", "Infrastructure-as-code findings by repository.", &["severity", "repository_id"], &[]),
                summary("source_code_vulnerability_secfinding", "Source Code Vulnerability SecFinding", "Security finding projection for source-code vulnerabilities by repository.", &["severity", "finding_type", "repository_id", "service_name", "code_location_filename"], &[]),
            ],
        ),
        category(
            "runtime_api_surface",
            "Runtime, API, and product-surface entities.",
            vec![
                summary("api_endpoint", "API Endpoint", "API exposure, authentication, rate-limit, service, and team ownership questions.", &["resource_name", "http_method", "http_route", "endpoint_is_public", "endpoint_authenticated", "endpoint_is_rate_limited", "service_name", "team_names"], &["service", "teams", "apis"]),
                summary("api", "API", "API inventory connected to source and system metadata.", &["name", "display_name"], &["code_locations", "current_oncalls", "systems"]),
                summary("frontend", "Frontend", "Frontend product surface connected to source, monitors, and systems.", &["name", "display_name", "owner"], &["code_locations", "current_oncalls", "monitors"]),
                summary("datastore", "Datastore", "Dependency and blast-radius entity for persistence.", &["name", "display_name"], &["systems", "incidents", "slos", "monitors"]),
                summary("queue", "Queue", "Dependency and blast-radius entity for asynchronous systems.", &["name", "display_name"], &["systems", "current_oncalls", "code_locations"]),
            ],
        ),
    ]
}

fn experimental_categories() -> Vec<KindCategory> {
    vec![
        category(
            "agent_native",
            "Agent-facing skills and automation assets connected back to source.",
            vec![summary("ai_skill", "AI Skill", "Agent skill inventory with owner, team, scope, source repository, path, and source URL.", &["name", "description", "owner", "team", "scope", "source_repo", "source_path", "source_url", "tags", "updated_at"], &["repository"])],
        ),
        category(
            "work_integrations",
            "Third-party work mirrors for pull request, review, Jira, and repository questions.",
            vec![
                summary("integration.github.pull_request", "GitHub Pull Request", "Rich GitHub pull request mirror for work-in-flight reports.", &["title", "state", "updated_at", "mergeable", "changed_files"], &["repository", "author", "reviewer_users", "reviewer_teams", "reviews", "review_threads"]),
                summary("integration.github.repository", "GitHub Repository", "GitHub integration repository mirror.", &["name", "full_name", "owner"], &["pull_requests"]),
                summary("integration.github.user", "GitHub User", "GitHub user identity for authors and reviewers.", &["login", "name"], &["authored_pull_requests", "reviewed_pull_requests"]),
                summary("integration.github.team", "GitHub Team", "GitHub team identity for review routing.", &["name", "slug"], &["reviewing_pull_requests"]),
                summary("integration.jira.issue", "Jira Issue", "Jira work items for daily and team reports.", &["key", "summary", "status", "status_category", "priority", "assignee_name"], &["project", "assignee", "reporter"]),
                summary("integration.jira.project", "Jira Project", "Jira project grouping for issues.", &["key", "name"], &["issues"]),
                summary("integration.jira.user", "Jira User", "Jira identity for assignment and reporting.", &["display_name", "email_address"], &["assigned_issues", "reported_issues"]),
            ],
        ),
        category(
            "infrastructure_integrations",
            "Integration-backed infrastructure entities. Scope these queries narrowly by team, service, repository, or owner.",
            vec![summary("integration.k8s.deployment", "Kubernetes Deployment", "Kubernetes deployment inventory and replica health by team or service. Avoid unscoped queries.", &["name", "team", "service", "cluster_name", "namespace", "available_replicas", "ready_replicas", "replicas_desired", "unavailable_replicas"], &["services"])],
        ),
        category(
            "github_native_secondary",
            "Overlapping GitHub graph family for status and check details. Prefer integration.github.* for repository and work-mirror data.",
            vec![
                summary("github.repository", "GitHub Repository", "Native GitHub repository graph for check and status details.", &["name", "name_with_owner", "url"], &["pull_requests", "owner", "repository_topics"]),
                summary("github.pull_request", "GitHub Pull Request", "Native GitHub pull request graph for check and status details.", &["title", "state", "updated_at"], &["repository", "author", "reviews", "status_check_rollup"]),
                summary("github.commit", "GitHub Commit", "Native GitHub commit graph for check and status details when available.", &["sha", "ref"], &["check_suites", "status", "status_check_rollup"]),
                summary("github.status_check_rollup", "GitHub Status Check Rollup", "", &["id", "state", "ref"], &[]),
            ],
        ),
    ]
}

fn category(name: &str, description: &str, kinds: Vec<KindSummary>) -> KindCategory {
    KindCategory {
        name: name.into(),
        description: description.into(),
        kinds,
    }
}

fn summary(
    kind: &str,
    display_name: &str,
    why_use: &str,
    top_fields: &[&str],
    top_relations: &[&str],
) -> KindSummary {
    KindSummary {
        kind: kind.into(),
        display_name: display_name.into(),
        why_use: why_use.into(),
        top_fields: strings(top_fields),
        top_relations: strings(top_relations),
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

fn curated_count(response: &CuratedKindsResponse) -> usize {
    response
        .categories
        .iter()
        .map(|category| category.kinds.len())
        .sum::<usize>()
        + response.custom_kinds.len()
}

fn is_low_level_kind(kind: &str) -> bool {
    matches!(
        kind,
        "deployment" | "host" | "pod" | "container" | "namespace" | "cluster" | "environment"
    )
}

fn is_experimental_kind(kind: &str) -> bool {
    kind == "ai_skill" || kind.starts_with("integration.") || kind.starts_with("github.")
}

fn curated_kind_summary(kind: &str) -> Option<KindSummary> {
    curated_kinds(true, true)
        .categories
        .into_iter()
        .flat_map(|category| category.kinds)
        .find(|summary| summary.kind == kind)
}

fn unknown_kind_suggestion(kind: &str) -> String {
    let suggestions: &[&str] = match kind.trim().to_ascii_lowercase().as_str() {
        "github_pr"
        | "github_prs"
        | "github.pullrequest"
        | "github_pull_request"
        | "github_pull_requests" => &["integration.github.pull_request", "github.pull_request"],
        "scorecard" | "scorecards" => &["scorecard_outcome", "scorecard_rule", "scorecard_entity"],
        _ => &[],
    };
    if suggestions.is_empty() {
        String::new()
    } else {
        format!(
            ". Did you mean {}?",
            suggestions
                .iter()
                .map(|suggestion| format!("{suggestion:?}"))
                .collect::<Vec<_>>()
                .join(" or ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> KindResource {
        serde_json::from_value(serde_json::json!({
            "id": "service",
            "attributes": {
                "display_name": "Service",
                "attribute_types": {
                    "name": {"dataType": "string"},
                    "owner": {"dataType": "string"},
                    "active_incidents_count": {"dataType": "int", "calculation": {"type": "timeseries"}}
                },
                "relations": {"owner_teams": {"target_kind": "team"}}
            }
        }))
        .unwrap()
    }

    #[test]
    fn curated_list_filters_optional_categories() {
        let default = curated_kinds(false, true);
        assert!(curated_kind_summary("service").is_some());
        assert!(default
            .categories
            .iter()
            .any(|category| category.name == "agent_native"));
        assert!(!default
            .categories
            .iter()
            .any(|category| category.name == "infrastructure_low_level"));

        let filtered = curated_kinds(true, false);
        assert!(filtered
            .categories
            .iter()
            .any(|category| category.name == "infrastructure_low_level"));
        assert!(!filtered
            .categories
            .iter()
            .any(|category| category.name == "agent_native"));
    }

    #[test]
    fn describe_summarizes_schema_and_operators() {
        let response = describe_response(schema(), true);
        assert!(response.schema_available);
        assert_eq!(response.relations[0].name, "owner_teams");
        let numeric = response
            .attributes
            .iter()
            .find(|attribute| attribute.name == "active_incidents_count")
            .unwrap();
        assert!(numeric.operators.contains(&"gte".into()));
        assert_eq!(numeric.calculation.as_deref(), Some("timeseries"));
        assert!(!response.examples.is_empty());
    }

    #[test]
    fn include_validation_distinguishes_attributes_and_relations() {
        assert!(
            validate_includes_against_kind("service", &["owner_teams".into()], &schema()).is_ok()
        );
        let error =
            validate_includes_against_kind("service", &["owner".into()], &schema()).unwrap_err();
        assert!(error.to_string().contains("attribute, not a relation"));
        let error =
            validate_includes_against_kind("service", &["unknown".into()], &schema()).unwrap_err();
        assert!(error
            .to_string()
            .contains("Valid relations include: owner_teams"));
    }

    #[test]
    fn validates_kind_names() {
        assert!(validate_kind_name("integration.github.pull_request").is_ok());
        assert!(validate_kind_name("").is_err());
        assert!(validate_kind_name("service/../../secret").is_err());
    }

    #[test]
    fn all_kinds_filters_custom_low_level_and_experimental() {
        let kinds: KindListResponse = serde_json::from_value(serde_json::json!({
            "data": [
                {"id": "service", "attributes": {}},
                {"id": "pod", "attributes": {}},
                {"id": "integration.github.pull_request", "attributes": {}},
                {"id": "idp_custom_entities.widget", "attributes": {}}
            ]
        }))
        .unwrap();
        let response = build_all_kinds_response(kinds.data, false, false, false);
        assert_eq!(response.count, 1);
        assert_eq!(response.kinds[0].kind, "service");
    }

    #[tokio::test]
    async fn fetch_kind_uses_detail_endpoint() {
        let _guard = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/api/v2/idp/entity_graph/kinds/service")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"data":{"id":"service","attributes":{"display_name":"Service","attribute_types":{},"relations":{}}}}"#,
            )
            .create_async()
            .await;
        let cfg = crate::test_support::test_config(&server.url());

        let kind = fetch_kind(&cfg, "service").await.unwrap();

        assert_eq!(kind.kind(), "service");
        assert_eq!(kind.attributes.display_name, "Service");
        mock.assert_async().await;
        crate::test_support::cleanup_env();
    }

    #[tokio::test]
    async fn fetch_kind_rejects_malformed_responses() {
        let _guard = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/api/v2/idp/entity_graph/kinds/service")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"unexpected":true}"#)
            .create_async()
            .await;
        let cfg = crate::test_support::test_config(&server.url());

        let error = fetch_kind(&cfg, "service").await.unwrap_err();

        assert!(error
            .to_string()
            .contains("failed to decode entity kind schema"));
        mock.assert_async().await;
        crate::test_support::cleanup_env();
    }

    #[tokio::test]
    async fn list_kinds_fetches_live_inventory_for_all() {
        let _guard = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", KINDS_PATH)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"data":[{"id":"service","attributes":{"display_name":"Service","attribute_types":{"name":{"dataType":"string"}},"relations":{}}}]}"#,
            )
            .create_async()
            .await;
        let cfg = crate::test_support::test_config(&server.url());

        list_kinds(&cfg, true, false, false, false).await.unwrap();

        mock.assert_async().await;
        crate::test_support::cleanup_env();
    }

    #[tokio::test]
    async fn describe_kind_falls_back_to_live_kind_list_after_404() {
        let _guard = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let detail = server
            .mock("GET", "/api/v2/idp/entity_graph/kinds/service")
            .with_status(404)
            .with_body("not found")
            .create_async()
            .await;
        let list = server
            .mock("GET", KINDS_PATH)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"data":[{"id":"service","attributes":{"display_name":"Service","attribute_types":{"name":{"dataType":"string"}},"relations":{}}}]}"#,
            )
            .create_async()
            .await;
        let cfg = crate::test_support::test_config(&server.url());

        describe_kind(&cfg, "service", false).await.unwrap();

        detail.assert_async().await;
        list.assert_async().await;
        crate::test_support::cleanup_env();
    }

    #[tokio::test]
    async fn describe_kind_errors_for_unknown_kind() {
        let _guard = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let detail = server
            .mock("GET", "/api/v2/idp/entity_graph/kinds/unknown")
            .with_status(404)
            .with_body("not found")
            .create_async()
            .await;
        let list = server
            .mock("GET", KINDS_PATH)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        let cfg = crate::test_support::test_config(&server.url());

        let error = describe_kind(&cfg, "unknown", false).await.unwrap_err();

        assert!(error
            .to_string()
            .contains("failed to describe entity kind \"unknown\""));
        detail.assert_async().await;
        list.assert_async().await;
        crate::test_support::cleanup_env();
    }
}

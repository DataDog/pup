use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

use anyhow::{bail, Context, Result};
use regex::Regex;
use serde_json::Value;

use super::entity_kinds::{
    default_fields_from_schema, fetch_kind, validate_fields, validate_includes_against_kind,
    validate_kind_name,
};
use super::entity_types::{
    EntitiesResponse, EntityIdentity, EntityResource, NextRequest, NormalizedEntitiesResponse,
    NormalizedEntity, NormalizedPage, QueryEcho, RelatedEntity, RelationshipSummary,
    ResourceIdentifier, ServerPageWarnings,
};
use crate::config::Config;
use crate::formatter::{self, Metadata};
use crate::raw_client;

pub(super) const ENTITIES_PATH: &str = "/api/v2/idp/entity_graph/entities";
pub(super) const MAX_PAGE_LIMIT: usize = 100;
const MAX_RELATION_LIMIT: usize = 100;
const MAX_RESULTS: usize = 10_000;

#[derive(Debug, Clone)]
pub struct EntityQueryOptions {
    pub query: String,
    pub fields: Vec<String>,
    pub fields_by_kind: Vec<String>,
    pub edge_fields: Vec<String>,
    pub include: Vec<String>,
    pub order_by: Vec<String>,
    pub limit: usize,
    pub max_results: Option<usize>,
    pub cursor: Option<String>,
    pub free_text_match: Option<String>,
    pub include_total_count: bool,
    pub timeseries_interval: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub scopes: Vec<String>,
    pub relation_limit: usize,
    pub raw: bool,
}

#[derive(Debug, Clone)]
struct NormalizedQueryOptions {
    query: String,
    kind: String,
    fields: Vec<String>,
    explicit_fields: bool,
    fields_by_kind: BTreeMap<String, Vec<String>>,
    edge_fields: BTreeMap<String, Vec<String>>,
    include: Vec<String>,
    order_by: Vec<String>,
    limit: usize,
    max_results: Option<usize>,
    cursor: Option<String>,
    free_text_match: Option<String>,
    include_total_count: bool,
    time: QueryTime,
    scopes: BTreeMap<String, String>,
    relation_limit: usize,
    raw: bool,
}

#[derive(Debug, Clone)]
struct QueryTime {
    past: Option<String>,
    start: Option<i64>,
    end: Option<i64>,
}

pub async fn query_entities(cfg: &Config, options: EntityQueryOptions) -> Result<()> {
    let mut options = normalize_options(options)?;
    let mut warnings = Vec::new();
    let relation_target_kinds = prepare_query(cfg, &mut options, &mut warnings).await?;

    let query_pairs = entity_query_params(&options, &relation_target_kinds);
    if options.raw {
        let raw = fetch_entity_page(cfg, &query_pairs).await?;
        for warning in &warnings {
            eprintln!("Warning: {warning}");
        }
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

    let mut normalized = fetch_normalized_pages(cfg, &options, &query_pairs).await?;
    normalized.warnings.extend(warnings);
    if normalized.count == 0 && options.free_text_match.as_deref() == Some("fuzzy") {
        normalized.warnings.push(
            "No fuzzy matches were returned. Verify with --free-text-match partial or an explicit name filter before concluding that the entity is absent; some providers miss fuzzy matches when combined with other filters.".into(),
        );
    }
    if normalized.page.truncated {
        normalized.warnings.push(
            "The API returned a continuation cursor. Use next_request.args to check for more results.".into(),
        );
    }
    normalized.next_request = normalized
        .page
        .next_cursor
        .as_ref()
        .map(|cursor| next_request(cfg, &options, &query_pairs, cursor));
    let next_action = normalized.next_request.as_ref().map(|request| {
        format!(
            "Continue with: pup {}",
            request
                .args
                .iter()
                .map(|arg| shell_words::quote(arg))
                .collect::<Vec<_>>()
                .join(" ")
        )
    });
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

async fn fetch_entity_page(cfg: &Config, params: &[(String, String)]) -> Result<Value> {
    let refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    raw_client::raw_get(cfg, ENTITIES_PATH, &refs)
        .await
        .context("failed to query Datadog entities")
}

async fn fetch_normalized_pages(
    cfg: &Config,
    options: &NormalizedQueryOptions,
    params: &[(String, String)],
) -> Result<NormalizedEntitiesResponse> {
    let budget = options.max_results.unwrap_or(options.limit);
    let mut combined: Option<NormalizedEntitiesResponse> = None;
    let mut cursor = options.cursor.clone();
    let mut seen_cursors = HashSet::new();
    if let Some(cursor) = &cursor {
        seen_cursors.insert(cursor.clone());
    }
    loop {
        let collected = combined.as_ref().map_or(0, |result| result.count);
        let page_limit = options.limit.min(budget - collected);
        let mut page_params = params.to_vec();
        page_params.retain(|(key, _)| key != "page[limit]" && key != "page[cursor]");
        page_params.push(("page[limit]".into(), page_limit.to_string()));
        if let Some(cursor) = &cursor {
            page_params.push(("page[cursor]".into(), cursor.clone()));
        }
        let raw = fetch_entity_page(cfg, &page_params).await?;
        let response: EntitiesResponse =
            serde_json::from_value(raw).context("failed to decode Datadog entity response")?;
        cursor = (!response.meta.page.next_cursor.is_empty())
            .then(|| response.meta.page.next_cursor.clone());
        if options.max_results.is_some() {
            if response.data.len() > page_limit {
                bail!("UEG returned more entities than the requested page limit; cannot safely continue the bounded query");
            }
            if let Some(cursor) = &cursor {
                if !seen_cursors.insert(cursor.clone()) {
                    bail!("UEG repeated a pagination cursor; refusing to return an incomplete inventory");
                }
                if response.data.is_empty() {
                    bail!("UEG returned an empty page with a continuation cursor; cannot safely continue the bounded query");
                }
            }
        }
        let page = normalize_entities_response(options, response);
        if let Some(result) = &mut combined {
            result.results.extend(page.results);
            result.count = result.results.len();
            result.page.pages_fetched += 1;
            result.page.next_cursor = page.page.next_cursor;
            result.page.truncated = page.page.truncated;
            result.warnings.extend(page.warnings);
            if let Some(warnings) = page.server_warnings {
                result.additional_page_warnings.push(ServerPageWarnings {
                    page: result.page.pages_fetched,
                    warnings,
                });
            }
            if result.total_count != page.total_count && page.total_count.is_some() {
                result.warnings.push("The API's total count changed between pages; this inventory is not a consistent snapshot.".into());
            }
        } else {
            combined = Some(page);
        }
        let result = combined.as_mut().expect("first page initialized");
        result.page.stop_reason = if cursor.is_none() {
            "end_of_results"
        } else if options.max_results.is_none() {
            "page_limit"
        } else if result.count >= budget {
            "result_limit"
        } else {
            continue;
        };
        return Ok(combined.expect("first page initialized"));
    }
}

fn normalize_options(options: EntityQueryOptions) -> Result<NormalizedQueryOptions> {
    let query = options.query.trim().to_string();
    if query.is_empty() {
        bail!("query is required");
    }
    let kind = validate_query_scope(&query)?;
    validate_limit("limit", options.limit, MAX_PAGE_LIMIT)?;
    if let Some(max_results) = options.max_results {
        validate_limit("max-results", max_results, MAX_RESULTS)?;
        if options.raw {
            bail!("--max-results cannot be combined with --raw; raw mode returns one API response");
        }
    }
    validate_limit("relation-limit", options.relation_limit, MAX_RELATION_LIMIT)?;

    let time = normalize_time(&options)?;
    let scopes = normalize_scopes(options.scopes, &kind)?;
    let free_text_match = normalize_free_text_match(options.free_text_match)?;

    let mut fields_by_kind = parse_field_selections(options.fields_by_kind, "fields")?;
    let edge_fields = parse_field_selections(options.edge_fields, "edge-fields")?;
    let mut fields = clean_strings(options.fields);
    if let Some(kind_fields) = fields_by_kind.remove(&kind) {
        if !fields.is_empty() {
            bail!("use either --field or --fields {kind}=... for the result kind, not both");
        }
        fields = kind_fields;
    }
    let explicit_fields = !fields.is_empty();
    let fields = if fields.is_empty() {
        default_fields_for_kind(&kind)
            .iter()
            .map(|field| (*field).to_string())
            .collect()
    } else {
        fields
    };

    let include = clean_strings(options.include);
    for relation in edge_fields.keys() {
        if !include.contains(relation) {
            bail!("--edge-fields {relation}=... requires --include {relation}");
        }
    }
    if !fields_by_kind.is_empty() && include.is_empty() {
        bail!("--fields for related kinds requires --include to expand their relations");
    }
    Ok(NormalizedQueryOptions {
        query,
        kind,
        fields,
        explicit_fields,
        fields_by_kind,
        edge_fields,
        include,
        order_by: normalize_order_by(options.order_by)?,
        limit: options.limit,
        max_results: options.max_results,
        cursor: options.cursor.filter(|cursor| !cursor.trim().is_empty()),
        free_text_match,
        include_total_count: options.include_total_count,
        time,
        scopes,
        relation_limit: options.relation_limit,
        raw: options.raw,
    })
}

fn normalize_time(options: &EntityQueryOptions) -> Result<QueryTime> {
    match (&options.from, &options.to) {
        (Some(from), Some(to)) => {
            if options.timeseries_interval.is_some() {
                bail!("--from/--to cannot be combined with --timeseries-interval");
            }
            let start = crate::util_ext::parse_time_to_datetime(from)?.timestamp_millis();
            let end = crate::util_ext::parse_time_to_datetime(to)?.timestamp_millis();
            if start >= end {
                bail!("--from must be earlier than --to");
            }
            Ok(QueryTime {
                past: None,
                start: Some(start),
                end: Some(end),
            })
        }
        (None, None) => {
            let interval = options
                .timeseries_interval
                .as_deref()
                .unwrap_or("1h")
                .trim();
            if interval.starts_with(['+', '-']) {
                bail!("--timeseries-interval must be positive");
            }
            // Accept familiar day/week lookbacks while sending a Go duration even
            // to deployments predating UEG's day/week parser support.
            let interval = if interval.ends_with(['d', 'w']) {
                let millis = crate::util_ext::parse_duration_to_millis(interval)?;
                if millis <= 0 {
                    bail!("--timeseries-interval must be positive");
                }
                format!("{millis}ms")
            } else {
                interval.to_string()
            };
            if !is_valid_go_duration(&interval)
                || !interval.chars().any(|ch| matches!(ch, '1'..='9'))
            {
                bail!("invalid timeseries interval {interval:?}: use a positive duration such as 1h, 24h, or 7d");
            }
            Ok(QueryTime {
                past: Some(interval),
                start: None,
                end: None,
            })
        }
        _ => bail!("--from and --to must be supplied together"),
    }
}

fn normalize_scopes(values: Vec<String>, kind: &str) -> Result<BTreeMap<String, String>> {
    if !values.is_empty() && kind.contains('.') {
        bail!("the UEG HTTP API does not support property scope parameters for dotted kind names");
    }
    let mut scopes = BTreeMap::new();
    for value in values {
        let (name, value) = value
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid --scope: use <name>=<value>"))?;
        let (name, value) = (name.trim(), value.trim());
        if name.is_empty()
            || value.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".:_-".contains(&byte))
        {
            bail!("invalid --scope {name}={value}: use nonempty names and values supported by the UEG scope API");
        }
        if scopes.insert(name.to_string(), value.to_string()).is_some() {
            bail!("duplicate --scope {name:?}");
        }
    }
    Ok(scopes)
}

fn parse_field_selections(
    values: Vec<String>,
    flag: &str,
) -> Result<BTreeMap<String, Vec<String>>> {
    let mut selections: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for value in values {
        let (key, fields) = value.split_once('=').ok_or_else(|| {
            anyhow::anyhow!("invalid --{flag} {value:?}: use <kind-or-relation>=<field>,<field>")
        })?;
        let key = key.trim();
        validate_kind_name(key)?;
        let entry = selections.entry(key.to_string()).or_default();
        for field in fields.split(',').map(str::trim) {
            if field.is_empty() || field.contains('=') {
                bail!("invalid --{flag} {value:?}: field names cannot be empty or contain '='");
            }
            if !entry.iter().any(|existing| existing == field) {
                entry.push(field.to_string());
            }
        }
    }
    Ok(selections)
}

pub(super) fn validate_query_scope(query: &str) -> Result<String> {
    let kind = infer_kind(query).ok_or_else(|| {
        anyhow::anyhow!(
            "query must include kind:<kind> or ref:\"ref:<kind>:<id>\"; quoted kind filters like kind:\"service\" are invalid"
        )
    })?;
    if has_semantic_top_level_or(query) {
        bail!(
            "top-level OR is invalid because the entity graph cannot determine one result kind; keep kind:<kind> or ref:\"ref:<kind>:<id>\" in the shared scope, for example kind:service AND (owner:idp OR team:idp)"
        );
    }
    if free_text_pattern().is_match(query) {
        bail!(
            "free_text is not an entity field; use name:*text* for a field filter, or a bare term such as 'kind:service AND catalog' with --free-text-match partial"
        );
    }
    Ok(kind)
}

pub(super) fn validate_limit(name: &str, value: usize, maximum: usize) -> Result<()> {
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
            "invalid free-text match {:?}: use partial or fuzzy with a bare search term such as 'kind:service AND catalog'",
            value
        ),
    }
}

pub(super) fn normalize_order_by(values: Vec<String>) -> Result<Vec<String>> {
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
            Ok(format!("{field}:{direction}"))
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

async fn prepare_query(
    cfg: &Config,
    options: &mut NormalizedQueryOptions,
    warnings: &mut Vec<String>,
) -> Result<Vec<String>> {
    if options.scopes.is_empty()
        && options.include.is_empty()
        && !options.explicit_fields
        && has_specific_default_fields(&options.kind)
    {
        return Ok(Vec::new());
    }
    let Ok(schema) = fetch_kind(cfg, &options.kind).await else {
        warnings.push(format!(
            "Live schema for {:?} was unavailable; fields, relations, and scopes could not be validated.",
            options.kind
        ));
        return Ok(Vec::new());
    };
    validate_includes_against_kind(&options.kind, &options.include, &schema)?;
    for scope in options.scopes.keys() {
        if !schema.attributes.attribute_types.values().any(|field| {
            field.scopes.iter().any(|declared| {
                &declared.name == scope
                    || (scope == "default_primary_tag" && declared.is_primary_tag)
            })
        }) {
            bail!("scope {scope:?} is not declared for kind {:?}; inspect `pup idp kinds describe {}`", options.kind, options.kind);
        }
    }
    if !options.scopes.is_empty()
        && options
            .include
            .iter()
            .any(|relation| relation.starts_with("runtime_"))
    {
        warnings.push("Property scopes do not establish environment isolation of runtime dependency edges; current runtime edge aggregates are org-wide.".into());
    }
    if options.explicit_fields {
        validate_fields(&options.kind, &options.fields, &schema)?;
    } else if !schema.attributes.attribute_types.is_empty() {
        options.fields = default_fields_from_schema(&schema);
    }
    let target_kinds: Vec<String> = options
        .include
        .iter()
        .filter_map(|relation| schema.attributes.relations.get(relation))
        .map(|relation| relation.target_kind.trim())
        .filter(|kind| !kind.is_empty())
        .map(str::to_string)
        .collect();
    for (kind, fields) in &options.fields_by_kind {
        if !target_kinds.contains(kind) {
            bail!("--fields {kind}=... does not target any expanded relation; inspect `pup idp kinds describe {}`", options.kind);
        }
        match fetch_kind(cfg, kind).await {
            Ok(schema) => validate_fields(kind, fields, &schema)?,
            Err(_) => warnings.push(format!(
                "Live schema for {kind:?} was unavailable; related fields could not be validated."
            )),
        }
    }
    Ok(target_kinds)
}

fn entity_query_params(
    options: &NormalizedQueryOptions,
    relation_target_kinds: &[String],
) -> Vec<(String, String)> {
    let mut params = vec![
        ("query".into(), options.query.clone()),
        ("page[limit]".into(), options.limit.to_string()),
    ];
    if let Some(past) = &options.time.past {
        params.push(("time[past]".into(), past.clone()));
    }
    if let Some(start) = options.time.start {
        params.push(("time[start]".into(), start.to_string()));
    }
    if let Some(end) = options.time.end {
        params.push(("time[end]".into(), end.to_string()));
    }
    for (name, value) in &options.scopes {
        params.push((
            format!("properties[{}][scope][*][{name}]", options.kind),
            value.clone(),
        ));
    }
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
    for (kind, fields) in &options.fields_by_kind {
        params.push((format!("fields[{kind}]"), fields.join(",")));
    }
    for (relation, fields) in &options.edge_fields {
        params.push((
            format!("fields[{}.{relation}]", options.kind),
            fields.join(","),
        ));
    }
    add_required_included_fields(&mut params, options, relation_target_kinds);
    if !options.order_by.is_empty() {
        params.push(("order_by".into(), options.order_by.join(",")));
    }
    if let Some(mode) = &options.free_text_match {
        params.push(("free_text_match".into(), mode.clone()));
    }
    if options.include_total_count {
        params.push(("meta[fields]".into(), "total_count".into()));
    }
    params
}

fn next_request(
    cfg: &Config,
    options: &NormalizedQueryOptions,
    params: &[(String, String)],
    cursor: &str,
) -> NextRequest {
    let mut args = vec!["--read-only".into()];
    if let Some(org) = &cfg.org {
        args.extend(["--org".into(), org.clone()]);
    }
    args.extend([
        "idp".into(),
        "entities".into(),
        "query".into(),
        options.query.clone(),
    ]);
    let mut add = |flag: &str, value: String| {
        args.push(flag.into());
        args.push(value);
    };
    add("--limit", options.limit.to_string());
    if let Some(max_results) = options.max_results {
        add("--max-results", max_results.to_string());
    }
    add("--cursor", cursor.into());
    add("--relation-limit", options.relation_limit.to_string());
    if !options.include.is_empty() {
        add("--include", options.include.join(","));
    }
    for (key, value) in params {
        if let Some(kind) = key
            .strip_prefix("fields[")
            .and_then(|key| key.strip_suffix(']'))
        {
            if let Some(relation) = options
                .edge_fields
                .keys()
                .find(|relation| format!("{}.{relation}", options.kind) == kind)
            {
                add("--edge-fields", format!("{relation}={value}"));
            } else {
                add("--fields", format!("{kind}={value}"));
            }
        }
    }
    if let Some(past) = &options.time.past {
        add("--timeseries-interval", past.clone());
    }
    for (flag, timestamp) in [("--from", options.time.start), ("--to", options.time.end)] {
        if let Some(timestamp) = timestamp {
            add(
                flag,
                chrono::DateTime::from_timestamp_millis(timestamp)
                    .expect("validated timestamp")
                    .to_rfc3339(),
            );
        }
    }
    for (name, value) in &options.scopes {
        add("--scope", format!("{name}={value}"));
    }
    for order in &options.order_by {
        add("--order-by", order.clone());
    }
    if let Some(mode) = &options.free_text_match {
        add("--free-text-match", mode.clone());
    }
    if options.include_total_count {
        args.push("--include-total-count".into());
    }
    NextRequest { args }
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
        .map(|entity| {
            for relation in &options.include {
                if !entity.relationships.contains_key(relation) {
                    warnings.push(format!(
                        "Requested relationship {relation:?} on {} was not returned; absence does not establish that no related entities exist.",
                        entity.id
                    ));
                }
            }
            normalize_entity(entity, &included, options.relation_limit, &mut warnings)
        })
        .collect::<Vec<_>>();
    let next_cursor =
        (!response.meta.page.next_cursor.is_empty()).then_some(response.meta.page.next_cursor);

    NormalizedEntitiesResponse {
        query: QueryEcho {
            query: options.query.clone(),
            inferred_kind: options.kind.clone(),
            include: options.include.clone(),
            fields: options.fields.clone(),
            fields_by_kind: options.fields_by_kind.clone(),
            edge_fields: options.edge_fields.clone(),
            timeseries_interval: options.time.past.clone(),
            from: options.time.start,
            to: options.time.end,
            scopes: options.scopes.clone(),
            order_by: options.order_by.clone(),
            cursor: options.cursor.clone(),
            free_text_match: options.free_text_match.clone(),
            include_total_count: options.include_total_count,
            relation_limit: options.relation_limit,
        },
        count: results.len(),
        results,
        page: NormalizedPage {
            limit: options.limit,
            pages_fetched: 1,
            stop_reason: if next_cursor.is_some() {
                "page_limit"
            } else {
                "end_of_results"
            },
            max_results: options.max_results,
            truncated: next_cursor.is_some(),
            next_cursor,
        },
        total_count: response.meta.total_count,
        warnings,
        server_warnings: response.meta.warnings,
        additional_page_warnings: Vec::new(),
        next_request: None,
    }
}

fn normalize_entity(
    entity: EntityResource,
    included: &HashMap<String, EntityResource>,
    relation_limit: usize,
    warnings: &mut Vec<String>,
) -> NormalizedEntity {
    let identity = entity_identity(&entity, false);
    let relationships = entity
        .relationships
        .iter()
        .filter_map(|(name, relation)| {
            let identifiers = match try_parse_relationship_data(&relation.data) {
                Ok(identifiers) => identifiers,
                Err(_) => {
                    warnings.push(format!(
                        "Relationship {name:?} on {} could not be decoded; its contents are unknown.",
                        identity.entity_ref
                    ));
                    return None;
                }
            };
            let truncated = identifiers.len() > relation_limit;
            let sample = identifiers
                .iter()
                .take(relation_limit)
                .map(|identifier| RelatedEntity {
                    entity: included
                        .get(&entity_key(&identifier.kind, &identifier.id))
                        .map(|related| entity_identity(related, true))
                        .unwrap_or_else(|| identifier_identity(identifier)),
                    edge_fields: identifier.meta.clone(),
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
        fields: entity.attributes,
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
            entity.attributes.clone()
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

fn string_attribute<'a>(attributes: &'a BTreeMap<String, Value>, key: &str) -> Option<&'a str> {
    attributes.get(key).and_then(Value::as_str)
}

pub(super) fn parse_relationship_data(value: &Value) -> Vec<ResourceIdentifier> {
    try_parse_relationship_data(value).unwrap_or_default()
}

fn try_parse_relationship_data(value: &Value) -> serde_json::Result<Vec<ResourceIdentifier>> {
    if value.is_null() {
        return Ok(Vec::new());
    }
    serde_json::from_value::<Vec<ResourceIdentifier>>(value.clone()).or_else(|_| {
        serde_json::from_value::<ResourceIdentifier>(value.clone()).map(|item| vec![item])
    })
}

pub(super) fn raw_response_metadata(raw: &Value) -> (Option<usize>, bool, Option<String>) {
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

pub(super) fn has_specific_default_fields(kind: &str) -> bool {
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
            fields_by_kind: Vec::new(),
            edge_fields: Vec::new(),
            include: Vec::new(),
            order_by: Vec::new(),
            limit: 25,
            max_results: None,
            cursor: None,
            free_text_match: None,
            include_total_count: false,
            timeseries_interval: None,
            from: None,
            to: None,
            scopes: Vec::new(),
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
        invalid.timeseries_interval = Some("0h".into());
        assert!(normalize_options(invalid).is_err());

        let error = normalize_options(options("kind:service AND free_text:catalog")).unwrap_err();
        assert!(error
            .to_string()
            .contains("free_text is not an entity field"));

        assert!(is_valid_go_duration("1h30m"));
        assert!(is_valid_go_duration("1.5h"));
    }

    #[test]
    fn absolute_windows_and_day_lookbacks_map_to_api_parameters() {
        let mut input = options("kind:service");
        input.from = Some("2026-09-17T10:00:00Z".into());
        input.to = Some("2026-09-17T11:00:00Z".into());
        input.scopes = vec!["env=prod".into()];
        let normalized = normalize_options(input).unwrap();
        let params: BTreeMap<_, _> = entity_query_params(&normalized, &[]).into_iter().collect();
        assert!(!params.contains_key("time[past]"));
        assert_eq!(params["time[start]"], "1789639200000");
        assert_eq!(params["time[end]"], "1789642800000");
        assert_eq!(params["properties[service][scope][*][env]"], "prod");
        let mut input = options("kind:service");
        input.timeseries_interval = Some("7d".into());
        assert_eq!(
            normalize_options(input).unwrap().time.past.as_deref(),
            Some("604800000ms")
        );
    }

    #[test]
    fn rejects_incomplete_reversed_conflicting_windows_and_bad_scopes() {
        let mut input = options("kind:service");
        input.from = Some("2026-09-17T11:00:00Z".into());
        assert!(normalize_options(input.clone()).is_err());
        input.to = Some("2026-09-17T10:00:00Z".into());
        assert!(normalize_options(input.clone()).is_err());
        input.to = Some("2026-09-17T12:00:00Z".into());
        input.timeseries_interval = Some("1h".into());
        assert!(normalize_options(input).is_err());
        let mut input = options("kind:service");
        input.timeseries_interval = Some("-7d".into());
        assert!(normalize_options(input).is_err());
        for bad in ["env", "env=", "env=prod*", "env=prod OR test"] {
            assert!(normalize_scopes(vec![bad.into()], "service").is_err());
        }
        assert!(normalize_scopes(vec!["env=prod".into(), "env=dev".into()], "service").is_err());
        assert!(normalize_scopes(vec!["env=prod".into()], "integration.test").is_err());
    }

    #[test]
    fn builds_entity_query_parameters() {
        let mut input = options("kind:integration.github.repository AND name:api");
        input.include = vec!["pull_requests".into()];
        input.order_by = vec![" updated_at:DESC ".into(), "name".into()];
        input.cursor = Some("cursor value".into());
        input.include_total_count = true;
        let normalized = normalize_options(input).unwrap();
        let params: BTreeMap<_, _> = entity_query_params(&normalized, &[]).into_iter().collect();
        assert_eq!(params["page[cursor]"], "cursor value");
        assert_eq!(params["order_by"], "updated_at:desc,name:asc");
        assert_eq!(params["meta[fields]"], "total_count");
        assert!(params.contains_key("fields[integration.github.pull_request]"));
    }

    #[test]
    fn projects_related_kinds_and_edges_without_overwriting_root_fields() {
        let mut input = options("kind:service");
        input.fields = vec!["name".into()];
        input.include = vec!["owner_teams".into(), "runtime_downstream_services".into()];
        input.fields_by_kind = vec!["team=name,handle".into(), "team=handle,user_count".into()];
        input.edge_fields = vec!["runtime_downstream_services=requests_count,error_rate".into()];
        let normalized = normalize_options(input).unwrap();
        let params: BTreeMap<_, _> =
            entity_query_params(&normalized, &["team".into(), "service".into()])
                .into_iter()
                .collect();
        assert_eq!(params["fields[service]"], "name");
        assert_eq!(params["fields[team]"], "name,handle,user_count");
        assert_eq!(
            params["fields[service.runtime_downstream_services]"],
            "requests_count,error_rate"
        );
    }

    #[test]
    fn rejects_ambiguous_or_unused_field_selections() {
        for bad in ["team", "=name", "team=", "team=name,", "team=name=owner"] {
            assert!(
                parse_field_selections(vec![bad.into()], "fields").is_err(),
                "{bad}"
            );
        }
        let mut input = options("kind:service");
        input.fields = vec!["name".into()];
        input.fields_by_kind = vec!["service=owner".into()];
        assert!(normalize_options(input)
            .unwrap_err()
            .to_string()
            .contains("either --field"));
        let mut input = options("kind:service");
        input.edge_fields = vec!["runtime_downstream_services=error_rate".into()];
        assert!(normalize_options(input)
            .unwrap_err()
            .to_string()
            .contains("requires --include"));
        let mut input = options("kind:service");
        input.fields_by_kind = vec!["team=name".into()];
        assert!(normalize_options(input).is_err());
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
                .entity
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
        assert_eq!(entity.relationships["empty"].count, 0);
        assert!(!entity.relationships.contains_key("malformed"));
        assert!(normalized
            .warnings
            .iter()
            .any(|warning| warning.contains("could not be decoded")));
        assert!(normalized
            .warnings
            .iter()
            .any(|warning| warning.contains("sample limited to 1")));
    }

    #[test]
    fn preserves_selected_attributes_edge_values_and_server_warnings() {
        let mut input = options("kind:service");
        input.include = vec!["runtime_downstream_services".into(), "owner_teams".into()];
        let opts = normalize_options(input).unwrap();
        let response = serde_json::from_value(serde_json::json!({
            "data": [{"type": "service", "id": "ref:service:checkout", "attributes": {
                "name": "checkout", "display_name": "Checkout API", "active_incidents_count": null
            }, "relationships": {"runtime_downstream_services": {"data": [{
                "type": "service", "id": "ref:service:payments",
                "meta": {"requests_count": 42, "error_rate": null}
            }]}}}],
            "meta": {"warnings": {"omitted_relations": [{"relation": "owner_teams", "reason": "access denied"}]}}
        })).unwrap();
        let result = normalize_entities_response(&opts, response);
        assert_eq!(result.results[0].fields["name"], "checkout");
        assert_eq!(result.results[0].fields["display_name"], "Checkout API");
        assert!(result.results[0].fields["active_incidents_count"].is_null());
        let edge = &result.results[0].relationships["runtime_downstream_services"].sample[0];
        assert_eq!(edge.entity.entity_ref, "ref:service:payments");
        assert_eq!(edge.edge_fields["requests_count"], 42);
        assert!(edge.edge_fields["error_rate"].is_null());
        assert!(result.server_warnings.is_some());
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("owner_teams")));
    }

    #[test]
    fn rejects_missing_entity_list_instead_of_reporting_empty_results() {
        assert!(serde_json::from_value::<EntitiesResponse>(
            serde_json::json!({"unexpected": true})
        )
        .is_err());
        // A relationship without linkage data is unknown, not an empty relation.
        assert!(
            serde_json::from_value::<EntitiesResponse>(serde_json::json!({
                "data": [{"type": "service", "id": "checkout", "relationships": {
                    "owner_teams": {"links": {"related": "/teams"}}
                }}]
            }))
            .is_err()
        );
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

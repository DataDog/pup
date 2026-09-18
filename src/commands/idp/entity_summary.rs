use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use super::entity_query::{
    normalize_order_expressions, raw_response_metadata, validate_limit, validate_query_scope,
    MAX_PAGE_LIMIT,
};
use crate::{config::Config, formatter, raw_client};

const FACETS_PATH: &str = "/api/v2/idp/entity_graph/facets";
const AGGREGATE_PATH: &str = "/api/v2/idp/entity_graph/aggregate";

#[derive(Clone, Copy)]
enum SummaryCommand {
    Facets,
    Aggregate,
}

impl SummaryCommand {
    fn name(self) -> &'static str {
        match self {
            Self::Facets => "pup idp entities facets",
            Self::Aggregate => "pup idp entities aggregate",
        }
    }
}

pub struct EntityAggregateOptions {
    pub query: String,
    pub group_by: Vec<String>,
    pub counts: Vec<String>,
    pub order_by: Vec<String>,
    pub limit: usize,
    pub cursor: Option<String>,
    pub raw: bool,
}

pub async fn facets(
    cfg: &Config,
    query: &str,
    fields: Vec<String>,
    limit: usize,
    cursor: Option<String>,
    raw: bool,
) -> Result<()> {
    validate_query_scope(query)?;
    validate_limit("limit", limit, MAX_PAGE_LIMIT)?;
    let fields = required_names(fields, "facet")?;
    let mut params = vec![
        ("query".to_string(), query.to_string()),
        ("facets".into(), fields.join(",")),
        ("page[limit]".into(), limit.to_string()),
    ];
    if let Some(cursor) = cursor.as_ref().filter(|cursor| !cursor.trim().is_empty()) {
        params.push(("page[cursor]".into(), cursor.clone()));
    }
    let refs: Vec<_> = params
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let response = raw_client::raw_get(cfg, FACETS_PATH, &refs)
        .await
        .context("failed to facet Datadog entities")?;
    print_summary(
        cfg,
        response,
        json!({"query": query, "facets": fields, "cursor": cursor}),
        limit,
        raw,
        SummaryCommand::Facets,
    )
}

pub async fn aggregate(cfg: &Config, options: EntityAggregateOptions) -> Result<()> {
    let attributes = aggregate_attributes(&options)?;
    // UEG's HTTP aggregate handler uses the JSON:API decoder.
    let response = raw_client::raw_post(
        cfg,
        AGGREGATE_PATH,
        json!({"data": {"attributes": attributes}}),
    )
    .await
    .context("failed to aggregate Datadog entities")?;
    print_summary(
        cfg,
        response,
        attributes,
        options.limit,
        options.raw,
        SummaryCommand::Aggregate,
    )
}

fn aggregate_attributes(options: &EntityAggregateOptions) -> Result<Value> {
    validate_query_scope(&options.query)?;
    validate_limit("limit", options.limit, MAX_PAGE_LIMIT)?;
    let group_by = required_names(options.group_by.clone(), "group-by")?;
    let order_by = normalize_order_expressions(options.order_by.clone())?;
    let mut names = BTreeSet::from(["count".to_string()]);
    let mut metrics = vec![json!({"name": "count", "func": "count", "filter": "*"})];
    for count in &options.counts {
        let (name, filter) = count
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid --count {count:?}: use <name>=<filter>"))?;
        let name = name.trim();
        let filter = filter.trim();
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            || filter.is_empty()
        {
            bail!("invalid --count {count:?}: use a name containing letters, numbers, or underscores and a nonempty filter");
        }
        if !names.insert(name.to_string()) {
            bail!("duplicate count name {name:?}; 'count' is the built-in total per group");
        }
        metrics.push(json!({"name": name, "func": "count", "filter": filter}));
    }
    Ok(json!({
        "query": options.query.trim(), "group_by": group_by, "metrics": metrics,
        "order_by": order_by,
        "pagination": {"limit": options.limit, "cursor": options.cursor.as_deref().unwrap_or("")}
    }))
}

fn required_names(names: Vec<String>, flag: &str) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for name in names {
        let name = name.trim();
        if name.is_empty() {
            bail!("--{flag} cannot contain an empty field name");
        }
        if !result.iter().any(|existing| existing == name) {
            result.push(name.to_string());
        }
    }
    if result.is_empty() {
        bail!("at least one --{flag} is required");
    }
    Ok(result)
}

#[derive(Deserialize)]
struct SummaryResponse {
    data: Vec<SummaryResource>,
}

#[derive(Deserialize)]
struct SummaryResource {
    attributes: serde_json::Map<String, Value>,
}

fn print_summary(
    cfg: &Config,
    response: Value,
    query: Value,
    limit: usize,
    raw: bool,
    command: SummaryCommand,
) -> Result<()> {
    let (count, truncated, next_action) = raw_response_metadata(&response);
    let mut metadata = formatter::Metadata {
        count,
        truncated,
        command: Some(command.name().into()),
        next_action,
    };
    let output = if raw {
        response
    } else {
        let parsed: SummaryResponse = serde_json::from_value(response.clone())
            .context("failed to decode entity summary response")?;
        let mut results: Vec<_> = parsed
            .data
            .into_iter()
            .map(|resource| resource.attributes)
            .collect();
        let sampled = if matches!(command, SummaryCommand::Facets) {
            limit_facet_values(&mut results, limit)?
        } else {
            false
        };
        let mut warnings = Vec::new();
        if sampled {
            let hint = "Facet values were sampled locally because the API returned more than --limit. Use --raw for all returned values, or aggregate --group-by <field> for pageable counts; a cursor does not recover locally omitted values.";
            warnings.push(hint);
            metadata.truncated = true;
            metadata.next_action = Some(hint.into());
        }
        json!({
            "query": query,
            "results": results,
            "count": count,
            "page": {"limit": limit, "truncated": metadata.truncated, "next_cursor": response.pointer("/meta/page/next_cursor").filter(|value| value.as_str().is_some_and(|cursor| !cursor.is_empty()))},
            "warnings": warnings,
            "server_warnings": response.pointer("/meta/warnings")
        })
    };
    formatter::format_and_print(
        &output,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&metadata),
        cfg.jq.as_deref(),
    )
}

fn limit_facet_values(
    results: &mut [serde_json::Map<String, Value>],
    limit: usize,
) -> Result<bool> {
    let mut sampled = false;
    for facet in results {
        let values = facet
            .get_mut("values")
            .and_then(Value::as_array_mut)
            .context("failed to decode entity facet response: expected a values array")?;
        let returned = values.len();
        let truncated = returned > limit;
        values.truncate(limit);
        facet.insert("values_returned".into(), json!(returned));
        facet.insert("values_truncated".into(), json!(truncated));
        sampled |= truncated;
    }
    Ok(sampled)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> EntityAggregateOptions {
        EntityAggregateOptions {
            query: "kind:service".into(),
            group_by: vec!["owner".into()],
            counts: vec![],
            order_by: vec![],
            limit: 25,
            cursor: None,
            raw: false,
        }
    }

    #[test]
    fn aggregate_supports_filtered_counts_and_ordered_pages() {
        let mut input = options();
        input.counts = vec!["unowned=_missing_:owner".into()];
        input.order_by = vec!["unowned:desc".into()];
        input.cursor = Some("opaque".into());
        let body = aggregate_attributes(&input).unwrap();
        assert_eq!(body["metrics"][0]["filter"], "*");
        assert_eq!(body["metrics"][1]["filter"], "_missing_:owner");
        assert_eq!(body["pagination"]["cursor"], "opaque");
        assert_eq!(body["order_by"], json!(["unowned:desc"]));
    }

    #[test]
    fn aggregate_rejects_missing_groups_bad_metrics_and_limits() {
        let mut input = options();
        input.group_by.clear();
        assert!(aggregate_attributes(&input).is_err());
        for count in ["broken", "=state:active", "count=*", "bad-name=*", "empty="] {
            let mut input = options();
            input.counts = vec![count.into()];
            assert!(aggregate_attributes(&input).is_err(), "{count}");
        }
        let mut input = options();
        input.limit = 0;
        assert!(aggregate_attributes(&input).is_err());
        assert!(required_names(vec!["".into()], "facet").is_err());
    }
}

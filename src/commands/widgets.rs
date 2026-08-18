use anyhow::Result;
use datadog_api_client::datadogV2::api_widgets::{SearchWidgetsOptionalParams, WidgetsAPI};
use datadog_api_client::datadogV2::model::WidgetExperienceType;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::OnceLock;

use crate::config::Config;
use crate::formatter;
use crate::util;

const WIDGET_DEPS_JSON: &str = include_str!("../../assets/widget_registry/widget_deps.json");
const WIDGET_DATA_SOURCES_JSON: &str =
    include_str!("../../assets/widget_registry/widget_data_sources.json");
const WIDGET_SCHEMAS_TYPESCRIPT: &str =
    include_str!("../../assets/widget_registry/generated__widget_schemas_compressed.ts");
const WIDGET_SCHEMA_VERSION: &str = "bd59608d073aaa44e848b8d2b82b08814211eb9a";

const NOTEBOOK_UNSUPPORTED_TYPES: &[&str] = &[
    "event_stream",
    "event_timeline",
    "free_text",
    "group",
    "note",
    "powerpack",
    "servicemap",
];
const DASHBOARD_UNSUPPORTED_TYPES: &[&str] = &[
    "analysis_data_source",
    "analysis_sql",
    "analysis_transformation",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum WidgetSurface {
    #[value(alias = "dashboards")]
    Dashboard,
    #[value(alias = "notebooks")]
    Notebook,
}

impl WidgetSurface {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dashboard => "dashboard",
            Self::Notebook => "notebook",
        }
    }

    fn supports(self, widget_type: &str) -> bool {
        let unsupported = match self {
            Self::Dashboard => DASHBOARD_UNSUPPORTED_TYPES,
            Self::Notebook => NOTEBOOK_UNSUPPORTED_TYPES,
        };
        !unsupported.contains(&widget_type)
    }
}

/// Slice of a widget schema that a caller can request on its own.
///
/// `All` is an expansion alias for the four real slices; it is never used as a
/// per-type label.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, clap::ValueEnum)]
pub enum SchemaSection {
    /// The widget definition itself and its `type` discriminator.
    Root,
    /// Request, query, formula, and sort types.
    Request,
    /// Style, palette, number format, time span, and custom link types.
    Presentation,
    /// Notebook local-dataset request types.
    LocalDataset,
    /// Every section (default).
    All,
}

impl SchemaSection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Request => "request",
            Self::Presentation => "presentation",
            Self::LocalDataset => "local-dataset",
            Self::All => "all",
        }
    }
}

/// Real sections in display order. Also the order omission notices are listed in.
const SECTION_ORDER: [SchemaSection; 4] = [
    SchemaSection::Root,
    SchemaSection::Request,
    SchemaSection::Presentation,
    SchemaSection::LocalDataset,
];

/// Real sections in classification priority order: a type is owned by the first
/// section whose walk reaches it, so shared leaves land in the most essential
/// section rather than in `presentation`.
const SECTION_PRIORITY: [SchemaSection; 4] = [
    SchemaSection::Root,
    SchemaSection::Request,
    SchemaSection::LocalDataset,
    SchemaSection::Presentation,
];

/// Field names that hand ownership of their referenced types to `presentation`.
///
/// The partition is derived structurally (walk from `root_types`, follow the
/// root's `requests` field for `request`, split `*LocalDatasetRequest` members
/// off into `local-dataset`), except for this one classification step. A purely
/// structural split is not possible: the generated TypeScript carries no marker
/// that distinguishes a styling type from a query type, and presentation types
/// hang off request types as often as off the root. Keying on *field* names
/// rather than type names keeps this generic across all widget types and data
/// sources — the field vocabulary is shared by the whole widget schema, while
/// type names are per-widget. Anything unlisted stays in the section that
/// reached it, so a new field defaults to being treated as essential.
const PRESENTATION_FIELDS: &[&str] = &[
    "alignment",
    "autoscale",
    "cell_display_mode",
    "cell_display_mode_options",
    "color_preference",
    "conditional_formats",
    "custom_links",
    "custom_timeframe",
    "custom_unit",
    "display",
    "display_format",
    "display_type",
    "grouped_display",
    "has_background",
    "has_border",
    "horizontal_align",
    "legend",
    "legend_columns",
    "legend_layout",
    "legend_size",
    "line_type",
    "line_width",
    "live_span",
    "margin",
    "markers",
    "message_display",
    "number_format",
    "palette",
    "palette_flip",
    "palette_index",
    "precision",
    "right_yaxis",
    "scaling",
    "show_legend",
    "size_format",
    "sizing",
    "style",
    "text_align",
    "text_formats",
    "time",
    "timeframe",
    "timeseries_background",
    "title_align",
    "unit_name",
    "unit_scale",
    "url_dark_theme",
    "vertical_align",
    "view_mode",
    "viz_type",
    "width",
    "xaxis",
    "yaxis",
];

#[derive(Debug, Deserialize)]
struct WidgetDataSources {
    all_known: Vec<String>,
    per_widget: BTreeMap<String, Vec<String>>,
    group_lookup: BTreeMap<String, Vec<String>>,
}

#[derive(Debug)]
struct WidgetRegistry {
    deps: BTreeMap<String, Vec<String>>,
    data_sources: WidgetDataSources,
    definitions: BTreeMap<String, String>,
}

fn registry() -> &'static WidgetRegistry {
    static REGISTRY: OnceLock<WidgetRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| WidgetRegistry {
        deps: serde_json::from_str(WIDGET_DEPS_JSON)
            .expect("bundled widget dependency registry must be valid JSON"),
        data_sources: serde_json::from_str(WIDGET_DATA_SOURCES_JSON)
            .expect("bundled widget data-source registry must be valid JSON"),
        definitions: parse_typescript_definitions(WIDGET_SCHEMAS_TYPESCRIPT),
    })
}

fn parse_typescript_definitions(input: &str) -> BTreeMap<String, String> {
    split_blocks(input).1.into_iter().collect()
}

/// Split a `// --- TypeName ---` delimited document into the leading header text
/// and the ordered type blocks that follow it.
fn split_blocks(input: &str) -> (String, Vec<(String, String)>) {
    let mut header_lines: Vec<&str> = Vec::new();
    let mut blocks: Vec<(String, String)> = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_lines: Vec<&str> = Vec::new();

    for line in input.lines() {
        let delimiter_name = line
            .strip_prefix("// --- ")
            .and_then(|value| value.strip_suffix(" ---"));
        if let Some(name) = delimiter_name {
            if let Some(previous) = current_name.replace(name.to_string()) {
                blocks.push((previous, current_lines.join("\n").trim().to_string()));
                current_lines.clear();
            }
        } else if current_name.is_some() {
            current_lines.push(line);
        } else {
            header_lines.push(line);
        }
    }

    if let Some(name) = current_name {
        blocks.push((name, current_lines.join("\n").trim().to_string()));
    }
    (header_lines.join("\n").trim().to_string(), blocks)
}

/// Expand the requested sections into the concrete set to emit.
///
/// No flag (or `all`) means every section. `request` implies `root`, because a
/// request type is unusable without the definition that contains it.
fn expand_sections(requested: &[SchemaSection]) -> BTreeSet<SchemaSection> {
    if requested.is_empty() || requested.contains(&SchemaSection::All) {
        return SECTION_ORDER.into_iter().collect();
    }
    let mut selected: BTreeSet<SchemaSection> = requested.iter().copied().collect();
    if selected.contains(&SchemaSection::Request) {
        selected.insert(SchemaSection::Root);
    }
    selected
}

/// Bytes a block contributes to the rendered schema, including its delimiter and
/// the blank line that separates it from the next block.
fn block_bytes(name: &str, definition: &str) -> usize {
    format!("// --- {name} ---\n{definition}").len() + 2
}

/// Field name a definition line declares, if the line declares one.
fn field_name(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.starts_with('/') || trimmed.starts_with('*') {
        return None;
    }
    let name = trimmed[..trimmed.find(':')?].trim_end_matches('?');
    let is_field = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    is_field.then_some(name)
}

/// Assign every block in the payload to exactly one section.
fn partition_blocks(
    blocks: &[(String, String)],
    root_types: &[String],
) -> BTreeMap<String, SchemaSection> {
    let definitions: BTreeMap<&str, &str> = blocks
        .iter()
        .map(|(name, definition)| (name.as_str(), definition.as_str()))
        .collect();
    let mut labels: BTreeMap<String, SchemaSection> = BTreeMap::new();
    let mut pending: BTreeMap<SchemaSection, VecDeque<String>> = BTreeMap::new();
    pending.insert(SchemaSection::Root, root_types.iter().cloned().collect());

    while let Some(section) = SECTION_PRIORITY
        .into_iter()
        .find(|section| pending.get(section).is_some_and(|queue| !queue.is_empty()))
    {
        let name = pending
            .get_mut(&section)
            .and_then(VecDeque::pop_front)
            .expect("section queue was non-empty");
        if labels.contains_key(&name) {
            continue;
        }
        let Some(definition) = definitions.get(name.as_str()) else {
            continue;
        };
        labels.insert(name, section);

        for line in definition.lines() {
            let field = field_name(line);
            // Presentation and local-dataset own their whole subtree; only root
            // and request hand work off to another section.
            let target = match field {
                Some("request" | "requests")
                    if matches!(section, SchemaSection::Root | SchemaSection::Request) =>
                {
                    None
                }
                Some(field)
                    if matches!(section, SchemaSection::Root | SchemaSection::Request)
                        && PRESENTATION_FIELDS.contains(&field) =>
                {
                    Some(SchemaSection::Presentation)
                }
                _ => Some(section),
            };
            for token in line
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .filter(|token| definitions.contains_key(token))
            {
                // A `requests` field mixes the surface request types with the
                // notebook local-dataset variants; split them by name.
                let target = target.unwrap_or(if token.contains("LocalDataset") {
                    SchemaSection::LocalDataset
                } else {
                    SchemaSection::Request
                });
                pending
                    .entry(target)
                    .or_default()
                    .push_back(token.to_string());
            }
        }
    }

    // Anything the walk never reached is kept in `root` so that no type can be
    // dropped from every section.
    for (name, _) in blocks {
        labels.entry(name.clone()).or_insert(SchemaSection::Root);
    }
    labels
}

/// Filter a rendered schema down to the selected sections, appending a notice for
/// every non-empty section left out.
///
/// Returns the schema and the omitted-section summaries for the JSON payload.
fn render_sections(
    schema: &str,
    root_types: &[String],
    selected: &BTreeSet<SchemaSection>,
) -> (String, Vec<serde_json::Value>) {
    let (header, blocks) = split_blocks(schema);
    let labels = partition_blocks(&blocks, root_types);

    let mut omitted = Vec::new();
    for section in SECTION_ORDER {
        if selected.contains(&section) {
            continue;
        }
        let (types, bytes) = blocks
            .iter()
            .filter(|(name, _)| labels.get(name) == Some(&section))
            .fold((0_usize, 0_usize), |(types, bytes), (name, definition)| {
                (types + 1, bytes + block_bytes(name, definition))
            });
        if types > 0 {
            omitted.push(serde_json::json!({
                "section": section.as_str(),
                "types": types,
                "bytes": bytes,
            }));
        }
    }
    if omitted.is_empty() {
        return (schema.to_string(), omitted);
    }

    let mut parts = vec![header];
    parts.extend(
        blocks
            .iter()
            .filter(|(name, _)| labels.get(name).is_some_and(|s| selected.contains(s)))
            .map(|(name, definition)| format!("// --- {name} ---\n{definition}")),
    );
    let notices: Vec<String> = omitted
        .iter()
        .map(|entry| {
            let section = entry["section"].as_str().unwrap_or_default();
            format!(
                "// omitted: {section} ({} types, {} bytes) -> --section {section}",
                entry["types"], entry["bytes"]
            )
        })
        .collect();
    parts.push(notices.join("\n"));
    (parts.join("\n\n"), omitted)
}

pub fn reference_types(cfg: &Config, surface: WidgetSurface) -> Result<()> {
    let rows: Vec<serde_json::Value> = registry()
        .deps
        .keys()
        .filter(|widget_type| surface.supports(widget_type))
        .map(|widget_type| {
            serde_json::json!({
                "type": widget_type,
                "surface": surface.as_str(),
            })
        })
        .collect();
    formatter::output(cfg, &rows)
}

pub fn reference_schema(
    cfg: &Config,
    widget_type: &str,
    surface: WidgetSurface,
    data_source: Option<&str>,
    sections: &[SchemaSection],
) -> Result<()> {
    let reg = registry();
    let deps = reg.deps.get(widget_type).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown widget type {widget_type:?}; run `pup widgets types --surface {}` to see supported types",
            surface.as_str()
        )
    })?;
    if !surface.supports(widget_type) {
        return Err(anyhow::anyhow!(
            "widget type {widget_type:?} is not supported on the {} surface",
            surface.as_str()
        ));
    }

    let available_data_sources = reg
        .data_sources
        .per_widget
        .get(widget_type)
        .cloned()
        .unwrap_or_default();
    let resolved_data_sources = resolve_data_sources(
        data_source,
        &available_data_sources,
        &reg.data_sources,
        widget_type,
    )?;
    let excluded_data_sources: BTreeSet<String> = if data_source.is_some() {
        let resolved: BTreeSet<&str> = resolved_data_sources.iter().map(String::as_str).collect();
        reg.data_sources
            .all_known
            .iter()
            .filter(|source| !resolved.contains(source.as_str()))
            .cloned()
            .collect()
    } else {
        BTreeSet::new()
    };

    let (root_types, schema) =
        assemble_schema(deps, &reg.definitions, &excluded_data_sources, surface);
    let selected = expand_sections(sections);
    let (schema, omitted_sections) = render_sections(&schema, &root_types, &selected);
    let section_names: Vec<&str> = selected
        .iter()
        .copied()
        .map(SchemaSection::as_str)
        .collect();
    formatter::output(
        cfg,
        &serde_json::json!({
            "type": widget_type,
            "surface": surface.as_str(),
            "requested_data_source": data_source,
            "resolved_data_sources": resolved_data_sources,
            "available_data_sources": available_data_sources,
            "root_types": root_types,
            "sections": section_names,
            "omitted_sections": omitted_sections,
            "source": "bundled",
            "schema_format": "typescript",
            "schema_version": WIDGET_SCHEMA_VERSION,
            "schema": schema,
        }),
    )
}

fn resolve_data_sources(
    requested: Option<&str>,
    available: &[String],
    catalog: &WidgetDataSources,
    widget_type: &str,
) -> Result<Vec<String>> {
    let Some(requested) = requested else {
        return Ok(available.to_vec());
    };
    let normalized = requested.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(anyhow::anyhow!("--data-source cannot be empty"));
    }

    let candidates: BTreeSet<&str> = if let Some(group) = catalog.group_lookup.get(&normalized) {
        group.iter().map(String::as_str).collect()
    } else if catalog.all_known.iter().any(|source| source == &normalized) {
        BTreeSet::from([normalized.as_str()])
    } else {
        return Err(anyhow::anyhow!(
            "unknown data source {requested:?} for widget {widget_type:?}; omit --data-source to inspect available_data_sources"
        ));
    };

    let resolved: Vec<String> = available
        .iter()
        .filter(|source| candidates.contains(source.as_str()))
        .cloned()
        .collect();
    if resolved.is_empty() {
        let available_text = if available.is_empty() {
            "none (this widget has no query data source)".to_string()
        } else {
            available.join(", ")
        };
        return Err(anyhow::anyhow!(
            "data source {requested:?} is not supported by widget type {widget_type:?}; available data sources: {available_text}"
        ));
    }
    Ok(resolved)
}

fn assemble_schema(
    deps: &[String],
    all_definitions: &BTreeMap<String, String>,
    excluded_data_sources: &BTreeSet<String>,
    surface: WidgetSurface,
) -> (Vec<String>, String) {
    let mut definitions: BTreeMap<String, String> = deps
        .iter()
        .filter_map(|name| {
            all_definitions
                .get(name)
                .map(|definition| (name.clone(), definition.clone()))
        })
        .collect();
    let mut dead_types = BTreeSet::new();

    if surface == WidgetSurface::Dashboard {
        for name in definitions.keys() {
            if name.contains("LocalDataset") || name.starts_with("StructuredAnalysis") {
                dead_types.insert(name.clone());
            }
        }
    }

    if !excluded_data_sources.is_empty() {
        prune_data_source_aliases(&mut definitions, &mut dead_types, excluded_data_sources);
    }
    remove_dead_union_members(&mut definitions, &dead_types);

    let roots: Vec<String> = deps
        .iter()
        .filter(|name| name.ends_with("WidgetDefinition") && !dead_types.contains(*name))
        .cloned()
        .collect();
    let roots = if roots.is_empty() {
        deps.first().cloned().into_iter().collect()
    } else {
        roots
    };
    let reachable = reachable_types(&definitions, &roots, &dead_types);

    let mut sections = vec![format!("// Root types: {}", roots.join(", "))];
    for name in deps {
        if !reachable.contains(name) {
            continue;
        }
        if let Some(definition) = definitions.get(name) {
            sections.push(format!("// --- {name} ---\n{definition}"));
        }
    }
    (roots, sections.join("\n\n"))
}

fn prune_data_source_aliases(
    definitions: &mut BTreeMap<String, String>,
    dead_types: &mut BTreeSet<String>,
    excluded_data_sources: &BTreeSet<String>,
) {
    let names: Vec<String> = definitions.keys().cloned().collect();
    for name in &names {
        let Some(definition) = definitions.get(name).cloned() else {
            continue;
        };
        let Some((prefix, members)) = type_alias_parts(&definition) else {
            continue;
        };
        if !members.iter().all(|member| is_string_literal(member)) {
            continue;
        }
        let kept: Vec<String> = members
            .into_iter()
            .filter(|member| !excluded_data_sources.contains(member.trim_matches('"')))
            .collect();
        if kept.is_empty() {
            dead_types.insert(name.clone());
        } else {
            definitions.insert(name.clone(), format!("{prefix}{};", kept.join(" | ")));
        }
    }

    loop {
        let mut changed = false;
        for name in &names {
            if dead_types.contains(name) {
                continue;
            }
            let Some(definition) = definitions.get(name).cloned() else {
                continue;
            };
            let Some((prefix, members)) = type_alias_parts(&definition) else {
                continue;
            };
            if members.iter().all(|member| is_string_literal(member)) {
                continue;
            }
            let kept: Vec<String> = members
                .iter()
                .filter(|member| {
                    let member = member.trim();
                    !dead_types.contains(member)
                        && definitions
                            .get(member)
                            .and_then(|value| data_source_type(value))
                            .is_none_or(|source_type| !dead_types.contains(source_type))
                })
                .cloned()
                .collect();
            if kept.is_empty() && !members.is_empty() {
                changed |= dead_types.insert(name.clone());
            } else if kept.len() != members.len() {
                definitions.insert(name.clone(), format!("{prefix}{};", kept.join(" | ")));
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

fn type_alias_parts(definition: &str) -> Option<(String, Vec<String>)> {
    let type_index = definition.find("type ")?;
    let equals_index = definition[type_index..].find(" = ")? + type_index;
    let prefix = definition[..equals_index + 3].to_string();
    let rhs = definition[equals_index + 3..]
        .trim()
        .strip_suffix(';')?
        .trim();
    Some((prefix, split_top_level_union(rhs)))
}

fn split_top_level_union(rhs: &str) -> Vec<String> {
    let mut members = Vec::new();
    let mut start = 0;
    let mut depth = 0_i32;
    let mut quote = None;
    for (index, character) in rhs.char_indices() {
        if let Some(active_quote) = quote {
            if character == active_quote && !rhs[..index].ends_with('\\') {
                quote = None;
            }
            continue;
        }
        match character {
            '"' | '\'' | '`' => quote = Some(character),
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth -= 1,
            '|' if depth == 0 => {
                members.push(rhs[start..index].trim().to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    members.push(rhs[start..].trim().to_string());
    members
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect()
}

fn is_string_literal(value: &str) -> bool {
    let value = value.trim();
    value.len() >= 2 && value.starts_with('"') && value.ends_with('"')
}

fn data_source_type(definition: &str) -> Option<&str> {
    for line in definition.lines() {
        let Some(marker) = line.find("data_source") else {
            continue;
        };
        let after_name = &line[marker + "data_source".len()..];
        let Some(colon) = after_name.find(':') else {
            continue;
        };
        let value = after_name[colon + 1..].trim();
        let end = value
            .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .unwrap_or(value.len());
        if end > 0 {
            return Some(&value[..end]);
        }
    }
    None
}

fn remove_dead_union_members(
    definitions: &mut BTreeMap<String, String>,
    dead_types: &BTreeSet<String>,
) {
    for definition in definitions.values_mut() {
        for dead_type in dead_types {
            *definition = definition
                .replace(&format!(" | {dead_type}"), "")
                .replace(&format!("{dead_type} | "), "");
        }
    }
}

fn reachable_types(
    definitions: &BTreeMap<String, String>,
    roots: &[String],
    dead_types: &BTreeSet<String>,
) -> BTreeSet<String> {
    let known: BTreeSet<&str> = definitions.keys().map(String::as_str).collect();
    let mut reachable = BTreeSet::new();
    let mut queue: VecDeque<String> = roots.iter().cloned().collect();
    while let Some(name) = queue.pop_front() {
        if dead_types.contains(&name) || !reachable.insert(name.clone()) {
            continue;
        }
        let Some(definition) = definitions.get(&name) else {
            continue;
        };
        for token in definition
            .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        {
            if known.contains(token) && !reachable.contains(token) && !dead_types.contains(token) {
                queue.push_back(token.to_string());
            }
        }
    }
    reachable
}

fn parse_experience_type(s: &str) -> Result<WidgetExperienceType> {
    match s {
        "ccm_reports" => Ok(WidgetExperienceType::CCM_REPORTS),
        "logs_reports" => Ok(WidgetExperienceType::LOGS_REPORTS),
        "csv_reports" => Ok(WidgetExperienceType::CSV_REPORTS),
        "product_analytics" => Ok(WidgetExperienceType::PRODUCT_ANALYTICS),
        other => Err(anyhow::anyhow!(
            "unknown experience type: {other:?}; valid values: ccm_reports, logs_reports, csv_reports, product_analytics"
        )),
    }
}

fn make_api(cfg: &Config) -> WidgetsAPI {
    crate::make_api!(WidgetsAPI, cfg)
}

#[allow(clippy::too_many_arguments)]
pub async fn list(
    cfg: &Config,
    experience_type: &str,
    filter_widget_type: Option<String>,
    filter_creator_handle: Option<String>,
    filter_is_favorited: Option<bool>,
    filter_title: Option<String>,
    filter_tags: Option<String>,
    sort: Option<String>,
    page_number: Option<i32>,
    page_size: Option<i32>,
) -> Result<()> {
    let exp_type = parse_experience_type(experience_type)?;
    let api = make_api(cfg);

    let mut params = SearchWidgetsOptionalParams::default();
    if let Some(wt) = filter_widget_type {
        use datadog_api_client::datadog::UnparsedObject;
        use datadog_api_client::datadogV2::model::WidgetType;
        let widget_type = WidgetType::UnparsedObject(UnparsedObject {
            value: serde_json::Value::String(wt),
        });
        params = params.filter_widget_type(widget_type);
    }
    if let Some(handle) = filter_creator_handle {
        params = params.filter_creator_handle(handle);
    }
    if let Some(favorited) = filter_is_favorited {
        params = params.filter_is_favorited(favorited);
    }
    if let Some(title) = filter_title {
        params = params.filter_title(title);
    }
    if let Some(tags) = filter_tags {
        params = params.filter_tags(tags);
    }
    if let Some(s) = sort {
        params = params.sort(s);
    }
    if let Some(n) = page_number {
        params = params.page_number(n as i64);
    }
    if let Some(n) = page_size {
        params = params.page_size(n as i64);
    }

    let resp = api
        .search_widgets(exp_type, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list widgets: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get(cfg: &Config, experience_type: &str, widget_id: &str) -> Result<()> {
    let exp_type = parse_experience_type(experience_type)?;
    let uuid: uuid::Uuid = widget_id
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid widget UUID {widget_id:?}: {e}"))?;
    let api = make_api(cfg);
    let resp = api
        .get_widget(exp_type, uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get widget: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn create(cfg: &Config, experience_type: &str, file: &str) -> Result<()> {
    let exp_type = parse_experience_type(experience_type)?;
    let body: datadog_api_client::datadogV2::model::CreateOrUpdateWidgetRequest =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_widget(exp_type, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create widget: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update(
    cfg: &Config,
    experience_type: &str,
    widget_id: &str,
    file: &str,
) -> Result<()> {
    let exp_type = parse_experience_type(experience_type)?;
    let uuid: uuid::Uuid = widget_id
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid widget UUID {widget_id:?}: {e}"))?;
    let body: datadog_api_client::datadogV2::model::CreateOrUpdateWidgetRequest =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .update_widget(exp_type, uuid, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update widget: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn delete(cfg: &Config, experience_type: &str, widget_id: &str) -> Result<()> {
    let exp_type = parse_experience_type(experience_type)?;
    let uuid: uuid::Uuid = widget_id
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid widget UUID {widget_id:?}: {e}"))?;
    let api = make_api(cfg);
    api.delete_widget(exp_type, uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete widget: {e:?}"))?;
    eprintln!("Widget {widget_id} deleted.");
    Ok(())
}

#[cfg(test)]
mod reference_tests {
    use super::*;

    #[test]
    fn bundled_registry_has_schema_for_every_type() {
        let reg = registry();
        assert_eq!(reg.deps.len(), 41);
        for (widget_type, deps) in &reg.deps {
            assert!(!deps.is_empty(), "{widget_type} has no dependencies");
            assert!(
                deps.iter().any(|name| reg.definitions.contains_key(name)),
                "{widget_type} has no bundled definitions"
            );
        }
    }

    #[test]
    fn surface_filters_notebook_and_dashboard_only_types() {
        assert!(WidgetSurface::Notebook.supports("analysis_sql"));
        assert!(!WidgetSurface::Dashboard.supports("analysis_sql"));
        assert!(WidgetSurface::Dashboard.supports("group"));
        assert!(!WidgetSurface::Notebook.supports("group"));
        assert!(!WidgetSurface::Notebook.supports("event_stream"));
        assert!(WidgetSurface::Notebook.supports("timeseries"));
        assert!(WidgetSurface::Dashboard.supports("timeseries"));
    }

    #[test]
    fn data_source_group_resolves_only_sources_supported_by_widget() {
        let reg = registry();
        let timeseries_sources = &reg.data_sources.per_widget["timeseries"];
        let resolved = resolve_data_sources(
            Some("logs"),
            timeseries_sources,
            &reg.data_sources,
            "timeseries",
        )
        .unwrap();
        assert_eq!(resolved, vec!["logs"]);

        let stream_sources = &reg.data_sources.per_widget["list_stream"];
        let resolved = resolve_data_sources(
            Some("logs"),
            stream_sources,
            &reg.data_sources,
            "list_stream",
        )
        .unwrap();
        assert!(resolved.contains(&"logs_stream".to_string()));
        assert!(!resolved.contains(&"logs".to_string()));
    }

    #[test]
    fn incompatible_data_source_fails_instead_of_widening_schema() {
        let reg = registry();
        let error = resolve_data_sources(
            Some("metrics"),
            &reg.data_sources.per_widget["list_stream"],
            &reg.data_sources,
            "list_stream",
        )
        .unwrap_err();
        assert!(error.to_string().contains("not supported"));
    }

    #[test]
    fn metrics_filter_prunes_other_query_variants() {
        let reg = registry();
        let resolved = BTreeSet::from(["metrics".to_string()]);
        let excluded: BTreeSet<String> = reg
            .data_sources
            .all_known
            .iter()
            .filter(|source| !resolved.contains(*source))
            .cloned()
            .collect();
        let (_, schema) = assemble_schema(
            &reg.deps["timeseries"],
            &reg.definitions,
            &excluded,
            WidgetSurface::Notebook,
        );

        assert!(schema.contains("FormulaAndFunctionMetricQueryDefinition"));
        assert!(schema.contains("data_source: FormulaAndFunctionMetricDataSource"));
        assert!(!schema.contains("FormulaAndFunctionEventQueryDefinition"));
        assert!(!schema.contains("\"logs\""));
    }

    /// Schema for a widget type narrowed to the `metrics` data source, the shape
    /// the section tests are calibrated against.
    fn metrics_schema(widget_type: &str, surface: WidgetSurface) -> (Vec<String>, String) {
        let reg = registry();
        let excluded: BTreeSet<String> = reg
            .data_sources
            .all_known
            .iter()
            .filter(|source| source.as_str() != "metrics")
            .cloned()
            .collect();
        assemble_schema(&reg.deps[widget_type], &reg.definitions, &excluded, surface)
    }

    fn emitted_names(schema: &str) -> Vec<String> {
        split_blocks(schema)
            .1
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    }

    #[test]
    fn default_sections_leave_the_schema_untouched() {
        let (roots, schema) = metrics_schema("toplist", WidgetSurface::Notebook);
        for requested in [vec![], vec![SchemaSection::All]] {
            let selected = expand_sections(&requested);
            assert_eq!(selected.len(), SECTION_ORDER.len());
            let (rendered, omitted) = render_sections(&schema, &roots, &selected);
            assert_eq!(rendered, schema);
            assert!(omitted.is_empty());
            assert!(!rendered.contains("// omitted:"));
        }
    }

    #[test]
    fn root_section_is_a_small_fraction_of_the_full_schema() {
        let (roots, schema) = metrics_schema("toplist", WidgetSurface::Notebook);
        let selected = expand_sections(&[SchemaSection::Root]);
        let (rendered, omitted) = render_sections(&schema, &roots, &selected);

        assert_eq!(
            emitted_names(&rendered),
            vec!["ToplistWidgetDefinition", "ToplistWidgetDefinitionType"]
        );
        assert!(
            rendered.len() * 10 < schema.len(),
            "root section {} is not much smaller than {}",
            rendered.len(),
            schema.len()
        );
        assert_eq!(omitted.len(), 3);
    }

    #[test]
    fn request_section_implies_root_and_omits_presentation() {
        let (roots, schema) = metrics_schema("toplist", WidgetSurface::Notebook);
        let selected = expand_sections(&[SchemaSection::Request]);
        assert!(selected.contains(&SchemaSection::Root));

        let (rendered, omitted) = render_sections(&schema, &roots, &selected);
        let names = emitted_names(&rendered);
        assert!(names.contains(&"ToplistWidgetDefinition".to_string()));
        assert!(names.contains(&"ToplistWidgetRequest".to_string()));
        assert!(names.contains(&"FormulaAndFunctionMetricQueryDefinition".to_string()));
        assert!(!names.contains(&"WidgetConditionalFormat".to_string()));
        assert!(!names.contains(&"TreeLocalDatasetRequest".to_string()));

        let omitted_names: Vec<&str> = omitted
            .iter()
            .map(|entry| entry["section"].as_str().unwrap())
            .collect();
        assert_eq!(omitted_names, vec!["presentation", "local-dataset"]);
    }

    #[test]
    fn omission_notice_reports_size_and_the_flag_that_fetches_it() {
        let (roots, schema) = metrics_schema("toplist", WidgetSurface::Notebook);
        let selected = expand_sections(&[SchemaSection::Root]);
        let (rendered, _) = render_sections(&schema, &roots, &selected);

        assert!(rendered
            .contains("// omitted: presentation (37 types, 6638 bytes) -> --section presentation"));
        assert!(
            rendered.contains("// omitted: request (17 types, 3109 bytes) -> --section request")
        );
        assert!(rendered.contains(
            "// omitted: local-dataset (3 types, 1629 bytes) -> --section local-dataset"
        ));
    }

    #[test]
    fn every_section_selection_only_references_emitted_or_announced_types() {
        let reg = registry();
        for widget_type in reg.deps.keys() {
            if !WidgetSurface::Notebook.supports(widget_type) {
                continue;
            }
            let (roots, schema) = metrics_schema(widget_type, WidgetSurface::Notebook);
            let blocks = split_blocks(&schema).1;
            let in_payload: BTreeSet<&str> = blocks.iter().map(|(name, _)| name.as_str()).collect();
            let labels = partition_blocks(&blocks, &roots);

            for section in SECTION_ORDER {
                let selected = expand_sections(&[section]);
                let (rendered, omitted) = render_sections(&schema, &roots, &selected);
                let announced: BTreeSet<&str> = omitted
                    .iter()
                    .map(|entry| entry["section"].as_str().unwrap())
                    .collect();
                let emitted: BTreeSet<String> = emitted_names(&rendered).into_iter().collect();

                for name in &emitted {
                    let definition = &blocks
                        .iter()
                        .find(|(candidate, _)| candidate == name)
                        .unwrap()
                        .1;
                    for token in definition
                        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                        .filter(|token| in_payload.contains(token))
                    {
                        if emitted.contains(token) {
                            continue;
                        }
                        let owner = labels[token].as_str();
                        assert!(
                            announced.contains(owner),
                            "{widget_type}/{}: {name} references {token} from unannounced section {owner}",
                            section.as_str()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn partition_assigns_every_type_to_exactly_one_section() {
        let reg = registry();
        for widget_type in reg.deps.keys() {
            if !WidgetSurface::Notebook.supports(widget_type) {
                continue;
            }
            let (roots, schema) = metrics_schema(widget_type, WidgetSurface::Notebook);
            let blocks = split_blocks(&schema).1;
            let labels = partition_blocks(&blocks, &roots);
            assert_eq!(
                labels.len(),
                blocks.len(),
                "{widget_type} partition does not cover every type"
            );
            assert!(labels
                .values()
                .all(|section| *section != SchemaSection::All));
        }
    }

    #[test]
    fn dashboard_schema_reports_no_local_dataset_section() {
        let (roots, schema) = metrics_schema("toplist", WidgetSurface::Dashboard);
        let selected = expand_sections(&[SchemaSection::Root]);
        let (rendered, omitted) = render_sections(&schema, &roots, &selected);
        assert!(!rendered.contains("--section local-dataset"));
        assert!(omitted
            .iter()
            .all(|entry| entry["section"] != "local-dataset"));
    }

    #[test]
    fn unknown_section_value_is_rejected() {
        use clap::ValueEnum;
        assert!(SchemaSection::from_str("bogus", true).is_err());
        assert!(SchemaSection::from_str("local_dataset", true).is_err());
        assert_eq!(
            SchemaSection::from_str("local-dataset", true).unwrap(),
            SchemaSection::LocalDataset
        );
    }

    #[test]
    fn dashboard_schema_removes_notebook_local_dataset_variant() {
        let reg = registry();
        let (_, schema) = assemble_schema(
            &reg.deps["timeseries"],
            &reg.definitions,
            &BTreeSet::new(),
            WidgetSurface::Dashboard,
        );
        assert!(schema.contains("TimeseriesWidgetRequest"));
        assert!(!schema.contains("TimeseriesLocalDatasetRequest"));
    }
}

use crate::config::Config;
use anyhow::Result;

// ---------------------------------------------------------------------------
// ANSI colour constants (following status_pages.rs pattern)
// ---------------------------------------------------------------------------

const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_RED: &str = "\x1b[31m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_BOLD: &str = "\x1b[1m";
const ANSI_DIM: &str = "\x1b[2m";
const ANSI_RESET: &str = "\x1b[0m";

// ---------------------------------------------------------------------------
// Migration outcome
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum MigrateStatus {
    Migrated { warnings: Vec<String> },
    Skipped,
    Failed(String),
}

#[derive(Debug)]
struct MigrateOutcome {
    path: std::path::PathBuf,
    status: MigrateStatus,
    services: usize,
    systems: usize,
}

// ---------------------------------------------------------------------------
// Version detection
// ---------------------------------------------------------------------------

const V1_CATALOG_KINDS: &[&str] = &["service", "library", "datastore", "queue", "api"];

/// Returns one of: "v3", "v2.2", "v2.1", "v2", "v1", "v1-noncatalog", "unknown"
fn detect_version(doc: &serde_json::Value) -> &'static str {
    if doc.get("apiVersion").and_then(|v| v.as_str()) == Some("v3") {
        return "v3";
    }
    match doc.get("schema-version").and_then(|v| v.as_str()) {
        Some("v2.2") => return "v2.2",
        Some("v2.1") => return "v2.1",
        Some("v2") => return "v2",
        Some("v1") => {
            let kind = doc.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            if !kind.is_empty() && !V1_CATALOG_KINDS.contains(&kind) {
                return "v1-noncatalog";
            }
            return "v1";
        }
        Some(_) => return "unknown",
        None => {}
    }
    // Implicit v1: info.dd-service present with no schema-version
    if doc.get("info").and_then(|i| i.get("dd-service")).is_some() {
        return "v1";
    }
    "unknown"
}

// ---------------------------------------------------------------------------
// Link type remapping
// ---------------------------------------------------------------------------

fn remap_link_type(t: &str) -> String {
    match t {
        "wiki" => "doc",
        "code" => "repo",
        "url" | "oncall" | "link" => "other",
        other => other,
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// Field accessor helpers
// ---------------------------------------------------------------------------

fn str_field(doc: &serde_json::Value, key: &str) -> Option<String> {
    doc.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn arr_field<'a>(doc: &'a serde_json::Value, key: &str) -> &'a [serde_json::Value] {
    doc.get(key)
        .and_then(|v| v.as_array())
        .map(|v| v.as_slice())
        .unwrap_or(&[])
}

fn nested_str(doc: &serde_json::Value, outer: &str, inner: &str) -> Option<String> {
    doc.get(outer)
        .and_then(|o| o.get(inner))
        .and_then(|v| v.as_str())
        .map(String::from)
}

fn is_empty_value(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => true,
        serde_json::Value::Object(m) => m.is_empty(),
        serde_json::Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Canonical output ordering
// ---------------------------------------------------------------------------

const TOP_FIELD_ORDER: &[&str] = &[
    "apiVersion",
    "kind",
    "metadata",
    "spec",
    "integrations",
    "datadog",
    "extensions",
];
const METADATA_FIELD_ORDER: &[&str] = &[
    "name",
    "displayName",
    "description",
    "owner",
    "additionalOwners",
    "tags",
    "contacts",
    "links",
];
const SPEC_FIELD_ORDER: &[&str] = &[
    "lifecycle",
    "tier",
    "type",
    "languages",
    "dependsOn",
    "componentOf",
];

fn reorder_map(
    obj: &serde_json::Map<String, serde_json::Value>,
    order: &[&str],
) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    for &key in order {
        if let Some(v) = obj.get(key) {
            if !is_empty_value(v) {
                out.insert(key.to_string(), v.clone());
            }
        }
    }
    for (k, v) in obj {
        if !order.contains(&k.as_str()) {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

fn ordered_entity(raw: serde_json::Value) -> serde_json::Value {
    let obj = match raw.as_object() {
        Some(o) => o,
        None => return raw,
    };
    let mut result = reorder_map(obj, TOP_FIELD_ORDER);
    if let Some(serde_json::Value::Object(meta)) = result.get("metadata").cloned() {
        result.insert(
            "metadata".to_string(),
            serde_json::Value::Object(reorder_map(&meta, METADATA_FIELD_ORDER)),
        );
    }
    if let Some(serde_json::Value::Object(spec)) = result.get("spec").cloned() {
        result.insert(
            "spec".to_string(),
            serde_json::Value::Object(reorder_map(&spec, SPEC_FIELD_ORDER)),
        );
    }
    serde_json::Value::Object(result)
}

// ---------------------------------------------------------------------------
// Companion system entity factory
// ---------------------------------------------------------------------------

fn make_system_entity(
    application_name: &str,
    team: Option<&str>,
    service_name: Option<&str>,
) -> serde_json::Value {
    let mut metadata = serde_json::json!({ "name": application_name });
    if let Some(t) = team {
        metadata["owner"] = serde_json::json!(t);
    }
    let mut entity = serde_json::json!({
        "apiVersion": "v3",
        "kind": "system",
        "metadata": metadata,
    });
    if let Some(svc) = service_name {
        entity["spec"] = serde_json::json!({
            "components": [format!("service:{svc}")]
        });
    }
    entity
}

// ---------------------------------------------------------------------------
// Link array helpers (shared by v2/v2.1/v2.2)
// ---------------------------------------------------------------------------

fn migrate_links(links_src: &[serde_json::Value]) -> Vec<serde_json::Value> {
    links_src
        .iter()
        .filter_map(|lnk| {
            let obj = lnk.as_object()?;
            let mut new_link = serde_json::Map::new();
            for (k, v) in obj {
                new_link.insert(k.clone(), v.clone());
            }
            let t = obj.get("type").and_then(|v| v.as_str()).unwrap_or("other");
            new_link.insert("type".to_string(), serde_json::json!(remap_link_type(t)));
            Some(serde_json::Value::Object(new_link))
        })
        .collect()
}

fn repos_to_links(repos: &[serde_json::Value]) -> Vec<serde_json::Value> {
    repos
        .iter()
        .filter_map(|r| {
            let obj = r.as_object()?;
            let mut link = serde_json::json!({ "type": "repo" });
            for key in &["name", "provider", "url"] {
                if let Some(v) = obj.get(*key) {
                    link[key] = v.clone();
                }
            }
            Some(link)
        })
        .collect()
}

fn docs_to_links(docs: &[serde_json::Value]) -> Vec<serde_json::Value> {
    docs.iter()
        .filter_map(|d| {
            let obj = d.as_object()?;
            let mut link = serde_json::json!({ "type": "doc" });
            for key in &["name", "provider", "url"] {
                if let Some(v) = obj.get(*key) {
                    link[key] = v.clone();
                }
            }
            Some(link)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Shared helpers: entity kind detection + integrations builder
// ---------------------------------------------------------------------------

fn detect_entity_kind(doc: &serde_json::Value) -> (String, String) {
    let keys = [
        ("dd-service", "service"),
        ("dd-package", "library"),
        ("dd-datastore", "datastore"),
        ("dd-queue", "queue"),
        ("dd-api", "api"),
    ];
    for (key, kind) in &keys {
        if let Some(name) = str_field(doc, key) {
            return (kind.to_string(), name);
        }
    }
    ("service".to_string(), String::new())
}

fn build_integrations(
    integrations_src: serde_json::Value,
    mut ext_map: serde_json::Map<String, serde_json::Value>,
) -> (
    serde_json::Map<String, serde_json::Value>,
    serde_json::Map<String, serde_json::Value>,
) {
    let mut out = serde_json::Map::new();

    if let Some(pd) = integrations_src.get("pagerduty") {
        if let Some(url) = pd.as_str() {
            out.insert("pagerduty".into(), serde_json::json!({ "serviceURL": url }));
        } else if let Some(obj) = pd.as_object() {
            let service_url = obj
                .get("service-url")
                .or_else(|| obj.get("serviceURL"))
                .and_then(|v| v.as_str());
            if let Some(url) = service_url {
                out.insert("pagerduty".into(), serde_json::json!({ "serviceURL": url }));
            }
        }
    }

    if let Some(og) = integrations_src.get("opsgenie") {
        if let Some(obj) = og.as_object() {
            let service_url = obj
                .get("service-url")
                .or_else(|| obj.get("serviceURL"))
                .and_then(|v| v.as_str());
            if let Some(url) = service_url {
                let mut og_out = serde_json::json!({ "serviceURL": url });
                if let Some(region) = obj.get("region") {
                    og_out["region"] = region.clone();
                }
                out.insert("opsgenie".into(), og_out);
            } else {
                eprintln!(
                    "{ANSI_YELLOW}WARNING: 'integrations.opsgenie' has no 'service-url'. \
                    Preserving in extensions['x-migrated/opsgenie'].{ANSI_RESET}"
                );
                ext_map.insert("x-migrated/opsgenie".into(), og.clone());
            }
        }
    }

    (out, ext_map)
}

// ---------------------------------------------------------------------------
// v1 → v3
// ---------------------------------------------------------------------------

fn migrate_v1(doc: &serde_json::Value) -> (serde_json::Value, Vec<serde_json::Value>) {
    let _info = doc.get("info").cloned().unwrap_or_default();
    let org = doc.get("org").cloned().unwrap_or_default();
    let contact = doc.get("contact").cloned().unwrap_or_default();
    let external_resources = arr_field(doc, "external-resources");
    let tags = arr_field(doc, "tags");
    let integrations_src = doc.get("integrations").cloned().unwrap_or_default();
    let extensions_src = doc.get("extensions").cloned();
    let repos_src = arr_field(doc, "repos");
    let dependencies = arr_field(doc, "dependencies");
    let source_patterns = arr_field(doc, "source_patterns");

    // Entity kind/name from info or root dd-* key
    let entity_keys = [
        ("dd-service", "service"),
        ("dd-package", "library"),
        ("dd-datastore", "datastore"),
        ("dd-queue", "queue"),
        ("dd-api", "api"),
    ];
    let (kind, entity_name) = {
        let mut k = "service".to_string();
        let mut n = String::new();
        for (key, kind_val) in &entity_keys {
            if let Some(name) = nested_str(doc, "info", key).or_else(|| str_field(doc, key)) {
                k = kind_val.to_string();
                n = name;
                break;
            }
        }
        (k, n)
    };

    let team = str_field(&org, "team");
    let application = str_field(&org, "application");

    let mut metadata = serde_json::json!({ "name": entity_name });
    if let Some(dn) = nested_str(doc, "info", "display-name") {
        metadata["displayName"] = serde_json::json!(dn);
    }
    if let Some(desc) = nested_str(doc, "info", "description") {
        metadata["description"] = serde_json::json!(desc);
    }
    if let Some(t) = &team {
        metadata["owner"] = serde_json::json!(t);
    }
    if !tags.is_empty() {
        metadata["tags"] = serde_json::json!(tags);
    }

    // contacts from contact block
    let mut contacts: Vec<serde_json::Value> = Vec::new();
    if let Some(email) = str_field(&contact, "email") {
        contacts.push(serde_json::json!({ "name": "Email", "type": "email", "contact": email }));
    }
    if let Some(slack) = str_field(&contact, "slack") {
        contacts.push(serde_json::json!({ "name": "Slack", "type": "slack", "contact": slack }));
    }
    if !contacts.is_empty() {
        metadata["contacts"] = serde_json::json!(contacts);
    }

    // links from external-resources
    let mut links: Vec<serde_json::Value> = Vec::new();
    for res in external_resources {
        if let Some(obj) = res.as_object() {
            let mut link = serde_json::Map::new();
            if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                link.insert("name".to_string(), serde_json::json!(name));
            }
            let t = obj.get("type").and_then(|v| v.as_str()).unwrap_or("other");
            link.insert("type".to_string(), serde_json::json!(remap_link_type(t)));
            if let Some(url) = obj.get("url").and_then(|v| v.as_str()) {
                link.insert("url".to_string(), serde_json::json!(url));
            }
            links.push(serde_json::Value::Object(link));
        }
    }
    links.extend(repos_to_links(repos_src));
    if let Some(github_url) = str_field(&integrations_src, "github") {
        links.push(serde_json::json!({
            "name": "GitHub", "type": "repo",
            "url": github_url, "provider": "Github"
        }));
    }
    if !links.is_empty() {
        metadata["links"] = serde_json::json!(links);
    }

    // spec
    let mut spec = serde_json::Map::new();
    if let Some(tier) = nested_str(doc, "info", "service-tier") {
        spec.insert("tier".to_string(), serde_json::json!(tier));
    }
    if let Some(app) = &application {
        spec.insert(
            "componentOf".to_string(),
            serde_json::json!([format!("system:{app}")]),
        );
    }
    if !dependencies.is_empty() {
        let depends: Vec<String> = dependencies
            .iter()
            .filter_map(|d| d.as_str())
            .map(|d| {
                if d.contains(':') {
                    d.to_string()
                } else {
                    format!("{kind}:{d}")
                }
            })
            .collect();
        spec.insert("dependsOn".to_string(), serde_json::json!(depends));
    }

    // integrations
    let mut integrations = serde_json::Map::new();
    if let Some(pd) = integrations_src.get("pagerduty") {
        if let Some(url) = pd.as_str() {
            integrations.insert(
                "pagerduty".to_string(),
                serde_json::json!({ "serviceURL": url }),
            );
        } else if pd.is_object() {
            integrations.insert("pagerduty".to_string(), pd.clone());
        }
    }

    // extensions
    let mut ext_map: serde_json::Map<String, serde_json::Value> = extensions_src
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    if !source_patterns.is_empty() {
        ext_map.insert(
            "x-migrated/source_patterns".to_string(),
            serde_json::json!(source_patterns),
        );
    }

    let mut entity = serde_json::json!({
        "apiVersion": "v3",
        "kind": kind,
        "metadata": metadata,
    });
    if !spec.is_empty() {
        entity["spec"] = serde_json::Value::Object(spec);
    }
    if !integrations.is_empty() {
        entity["integrations"] = serde_json::Value::Object(integrations);
    }
    if !ext_map.is_empty() {
        entity["extensions"] = serde_json::Value::Object(ext_map);
    }

    let mut companions = Vec::new();
    if let Some(app) = &application {
        eprintln!(
            "{ANSI_YELLOW}WARNING: 'org.application' field found (value: '{app}'). \
            A companion 'kind: system' entity will be generated.{ANSI_RESET}"
        );
        companions.push(make_system_entity(app, team.as_deref(), Some(&entity_name)));
    }

    (ordered_entity(entity), companions)
}

// ---------------------------------------------------------------------------
// v2 → v3
// ---------------------------------------------------------------------------

const KNOWN_V2_FIELDS: &[&str] = &[
    "schema-version",
    "dd-service",
    "dd-package",
    "dd-datastore",
    "dd-queue",
    "dd-api",
    "team",
    "dd-team",
    "description",
    "display-name",
    "application",
    "tier",
    "lifecycle",
    "type",
    "languages",
    "contacts",
    "links",
    "repos",
    "docs",
    "tags",
    "integrations",
    "extensions",
];

fn migrate_v2(doc: &serde_json::Value) -> (serde_json::Value, Vec<serde_json::Value>) {
    let (kind, service_name) = detect_entity_kind(doc);
    let team = str_field(doc, "team").or_else(|| str_field(doc, "dd-team"));
    let description = str_field(doc, "description");
    let display_name = str_field(doc, "display-name");
    let application = str_field(doc, "application");
    let lifecycle = str_field(doc, "lifecycle");
    let tier = str_field(doc, "tier");
    let service_type = str_field(doc, "type");
    let languages = arr_field(doc, "languages");
    let contacts_src = arr_field(doc, "contacts");
    let links_src = arr_field(doc, "links");
    let repos_src = arr_field(doc, "repos");
    let docs_src = arr_field(doc, "docs");
    let tags = arr_field(doc, "tags");
    let integrations_src = doc.get("integrations").cloned().unwrap_or_default();

    let mut metadata = serde_json::json!({ "name": service_name });
    if let Some(dn) = display_name {
        metadata["displayName"] = serde_json::json!(dn);
    }
    if let Some(d) = description {
        metadata["description"] = serde_json::json!(d);
    }
    if let Some(t) = &team {
        metadata["owner"] = serde_json::json!(t);
    }
    if !tags.is_empty() {
        metadata["tags"] = serde_json::json!(tags);
    }
    if !contacts_src.is_empty() {
        metadata["contacts"] = serde_json::json!(contacts_src);
    }

    let mut links = migrate_links(links_src);
    links.extend(repos_to_links(repos_src));
    links.extend(docs_to_links(docs_src));
    if !links.is_empty() {
        metadata["links"] = serde_json::json!(links);
    }

    let mut spec = serde_json::Map::new();
    if let Some(lc) = lifecycle {
        spec.insert("lifecycle".into(), serde_json::json!(lc));
    }
    if let Some(t) = tier {
        spec.insert("tier".into(), serde_json::json!(t));
    }
    if let Some(st) = service_type {
        spec.insert("type".into(), serde_json::json!(st));
    }
    if !languages.is_empty() {
        spec.insert("languages".into(), serde_json::json!(languages));
    }
    if let Some(app) = &application {
        spec.insert(
            "componentOf".into(),
            serde_json::json!([format!("system:{app}")]),
        );
    }

    let mut ext_map: serde_json::Map<String, serde_json::Value> = doc
        .get("extensions")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    if let Some(obj) = doc.as_object() {
        for (k, v) in obj {
            if !KNOWN_V2_FIELDS.contains(&k.as_str()) {
                ext_map.insert(format!("x-migrated/{k}"), v.clone());
            }
        }
    }

    let (integrations, ext_map) = build_integrations(integrations_src, ext_map);

    let mut entity = serde_json::json!({ "apiVersion": "v3", "kind": kind, "metadata": metadata });
    if !spec.is_empty() {
        entity["spec"] = serde_json::Value::Object(spec);
    }
    if !integrations.is_empty() {
        entity["integrations"] = serde_json::Value::Object(integrations);
    }
    if !ext_map.is_empty() {
        entity["extensions"] = serde_json::Value::Object(ext_map);
    }

    let mut companions = Vec::new();
    if let Some(app) = &application {
        eprintln!(
            "{ANSI_YELLOW}WARNING: 'application' field found (value: '{app}'). \
            A companion 'kind: system' entity will be generated.{ANSI_RESET}"
        );
        companions.push(make_system_entity(
            app,
            team.as_deref(),
            Some(&service_name),
        ));
    }
    (ordered_entity(entity), companions)
}

// ---------------------------------------------------------------------------
// v2.1 → v3
// ---------------------------------------------------------------------------

const KNOWN_V2_1_FIELDS: &[&str] = &[
    "schema-version",
    "dd-service",
    "dd-package",
    "dd-datastore",
    "dd-queue",
    "dd-api",
    "team",
    "dd-team",
    "description",
    "display-name",
    "application",
    "tier",
    "lifecycle",
    "contacts",
    "links",
    "repos",
    "docs",
    "tags",
    "integrations",
    "extensions",
];

fn migrate_v2_1(doc: &serde_json::Value) -> (serde_json::Value, Vec<serde_json::Value>) {
    let (kind, service_name) = detect_entity_kind(doc);
    let team = str_field(doc, "team").or_else(|| str_field(doc, "dd-team"));
    let description = str_field(doc, "description");
    let display_name = str_field(doc, "display-name");
    let application = str_field(doc, "application");
    let lifecycle = str_field(doc, "lifecycle");
    let tier = str_field(doc, "tier");
    let contacts_src = arr_field(doc, "contacts");
    let links_src = arr_field(doc, "links");
    let repos_src = arr_field(doc, "repos");
    let docs_src = arr_field(doc, "docs");
    let tags = arr_field(doc, "tags");
    let integrations_src = doc.get("integrations").cloned().unwrap_or_default();

    let mut metadata = serde_json::json!({ "name": service_name });
    if let Some(dn) = display_name {
        metadata["displayName"] = serde_json::json!(dn);
    }
    if let Some(d) = description {
        metadata["description"] = serde_json::json!(d);
    }
    if let Some(t) = &team {
        metadata["owner"] = serde_json::json!(t);
    }
    if !tags.is_empty() {
        metadata["tags"] = serde_json::json!(tags);
    }
    if !contacts_src.is_empty() {
        metadata["contacts"] = serde_json::json!(contacts_src);
    }

    let mut links = migrate_links(links_src);
    links.extend(repos_to_links(repos_src));
    links.extend(docs_to_links(docs_src));
    if !links.is_empty() {
        metadata["links"] = serde_json::json!(links);
    }

    let mut spec = serde_json::Map::new();
    if let Some(lc) = lifecycle {
        spec.insert("lifecycle".into(), serde_json::json!(lc));
    }
    if let Some(t) = tier {
        spec.insert("tier".into(), serde_json::json!(t));
    }
    if let Some(app) = &application {
        spec.insert(
            "componentOf".into(),
            serde_json::json!([format!("system:{app}")]),
        );
    }

    let mut ext_map: serde_json::Map<String, serde_json::Value> = doc
        .get("extensions")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    if let Some(obj) = doc.as_object() {
        for (k, v) in obj {
            if !KNOWN_V2_1_FIELDS.contains(&k.as_str()) {
                ext_map.insert(format!("x-migrated/{k}"), v.clone());
            }
        }
    }

    let (integrations, ext_map) = build_integrations(integrations_src, ext_map);

    let mut entity = serde_json::json!({ "apiVersion": "v3", "kind": kind, "metadata": metadata });
    if !spec.is_empty() {
        entity["spec"] = serde_json::Value::Object(spec);
    }
    if !integrations.is_empty() {
        entity["integrations"] = serde_json::Value::Object(integrations);
    }
    if !ext_map.is_empty() {
        entity["extensions"] = serde_json::Value::Object(ext_map);
    }

    let mut companions = Vec::new();
    if let Some(app) = &application {
        eprintln!(
            "{ANSI_YELLOW}WARNING: 'application' field found (value: '{app}'). \
            A companion 'kind: system' entity will be generated.{ANSI_RESET}"
        );
        companions.push(make_system_entity(
            app,
            team.as_deref(),
            Some(&service_name),
        ));
    }
    (ordered_entity(entity), companions)
}

// ---------------------------------------------------------------------------
// v2.2 → v3
// ---------------------------------------------------------------------------

const KNOWN_V2_2_FIELDS: &[&str] = &[
    "schema-version",
    "dd-service",
    "dd-package",
    "dd-datastore",
    "dd-queue",
    "dd-api",
    "team",
    "dd-team",
    "description",
    "display-name",
    "application",
    "tier",
    "lifecycle",
    "type",
    "languages",
    "ci-pipeline-fingerprints",
    "contacts",
    "links",
    "repos",
    "docs",
    "tags",
    "integrations",
    "extensions",
];

fn migrate_v2_2(doc: &serde_json::Value) -> (serde_json::Value, Vec<serde_json::Value>) {
    let (kind, service_name) = detect_entity_kind(doc);
    let team = str_field(doc, "team").or_else(|| str_field(doc, "dd-team"));
    let description = str_field(doc, "description");
    let display_name = str_field(doc, "display-name");
    let application = str_field(doc, "application");
    let lifecycle = str_field(doc, "lifecycle");
    let tier = str_field(doc, "tier");
    let service_type = str_field(doc, "type");
    let languages = arr_field(doc, "languages");
    let ci_fps = arr_field(doc, "ci-pipeline-fingerprints");
    let contacts_src = arr_field(doc, "contacts");
    let links_src = arr_field(doc, "links");
    let repos_src = arr_field(doc, "repos");
    let docs_src = arr_field(doc, "docs");
    let tags = arr_field(doc, "tags");
    let integrations_src = doc.get("integrations").cloned().unwrap_or_default();

    let mut metadata = serde_json::json!({ "name": service_name });
    if let Some(dn) = display_name {
        metadata["displayName"] = serde_json::json!(dn);
    }
    if let Some(d) = description {
        metadata["description"] = serde_json::json!(d);
    }
    if let Some(t) = &team {
        metadata["owner"] = serde_json::json!(t);
    }
    if !tags.is_empty() {
        metadata["tags"] = serde_json::json!(tags);
    }
    if !contacts_src.is_empty() {
        metadata["contacts"] = serde_json::json!(contacts_src);
    }

    let mut links = migrate_links(links_src);
    links.extend(repos_to_links(repos_src));
    links.extend(docs_to_links(docs_src));
    if !links.is_empty() {
        metadata["links"] = serde_json::json!(links);
    }

    let mut spec = serde_json::Map::new();
    if let Some(lc) = lifecycle {
        spec.insert("lifecycle".into(), serde_json::json!(lc));
    }
    if let Some(t) = tier {
        spec.insert("tier".into(), serde_json::json!(t));
    }
    if let Some(st) = service_type {
        spec.insert("type".into(), serde_json::json!(st));
    }
    if !languages.is_empty() {
        spec.insert("languages".into(), serde_json::json!(languages));
    }
    if let Some(app) = &application {
        spec.insert(
            "componentOf".into(),
            serde_json::json!([format!("system:{app}")]),
        );
    }

    // ci-pipeline-fingerprints → datadog.pipelines.fingerprints
    let mut datadog_out = serde_json::Map::new();
    if !ci_fps.is_empty() {
        datadog_out.insert(
            "pipelines".into(),
            serde_json::json!({ "fingerprints": ci_fps }),
        );
    }

    let mut ext_map: serde_json::Map<String, serde_json::Value> = doc
        .get("extensions")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    if let Some(obj) = doc.as_object() {
        for (k, v) in obj {
            if !KNOWN_V2_2_FIELDS.contains(&k.as_str()) {
                ext_map.insert(format!("x-migrated/{k}"), v.clone());
            }
        }
    }

    let (integrations, ext_map) = build_integrations(integrations_src, ext_map);

    let mut entity = serde_json::json!({ "apiVersion": "v3", "kind": kind, "metadata": metadata });
    if !spec.is_empty() {
        entity["spec"] = serde_json::Value::Object(spec);
    }
    if !integrations.is_empty() {
        entity["integrations"] = serde_json::Value::Object(integrations);
    }
    if !datadog_out.is_empty() {
        entity["datadog"] = serde_json::Value::Object(datadog_out);
    }
    if !ext_map.is_empty() {
        entity["extensions"] = serde_json::Value::Object(ext_map);
    }

    let mut companions = Vec::new();
    if let Some(app) = &application {
        eprintln!(
            "{ANSI_YELLOW}WARNING: 'application' field found (value: '{app}'). \
            A companion 'kind: system' entity will be generated.{ANSI_RESET}"
        );
        companions.push(make_system_entity(
            app,
            team.as_deref(),
            Some(&service_name),
        ));
    }
    (ordered_entity(entity), companions)
}

// ---------------------------------------------------------------------------
// Multi-document handling
// ---------------------------------------------------------------------------

fn migrate_document(
    doc: &serde_json::Value,
) -> (Option<serde_json::Value>, Vec<serde_json::Value>) {
    match detect_version(doc) {
        "v3" => {
            let name = doc
                .get("metadata")
                .and_then(|m| m.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("(unknown)");
            eprintln!(
                "{ANSI_DIM}INFO: '{name}' is already v3, passing through unchanged.{ANSI_RESET}"
            );
            (Some(doc.clone()), vec![])
        }
        "v1-noncatalog" => {
            let kind = doc
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            eprintln!("{ANSI_YELLOW}WARNING: schema-version=v1, kind='{kind}' is not a catalog entity. Passing through.{ANSI_RESET}");
            (Some(doc.clone()), vec![])
        }
        "unknown" => {
            eprintln!("{ANSI_YELLOW}WARNING: Could not detect schema version. Passing document through unchanged.{ANSI_RESET}");
            (Some(doc.clone()), vec![])
        }
        "v1" => {
            let (e, c) = migrate_v1(doc);
            (Some(e), c)
        }
        "v2" => {
            let (e, c) = migrate_v2(doc);
            (Some(e), c)
        }
        "v2.1" => {
            let (e, c) = migrate_v2_1(doc);
            (Some(e), c)
        }
        "v2.2" => {
            let (e, c) = migrate_v2_2(doc);
            (Some(e), c)
        }
        _ => (Some(doc.clone()), vec![]),
    }
}

fn merge_companion_systems(companions: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut order: Vec<String> = Vec::new();
    let mut merged: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();

    for companion in companions {
        if companion.get("kind").and_then(|v| v.as_str()) != Some("system") {
            continue;
        }
        let name = match companion
            .get("metadata")
            .and_then(|m| m.get("name"))
            .and_then(|n| n.as_str())
        {
            Some(n) => n.to_string(),
            None => continue,
        };
        let new_components: Vec<serde_json::Value> = companion
            .get("spec")
            .and_then(|s| s.get("components"))
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();

        if let Some(existing) = merged.get_mut(&name) {
            let existing_comps = existing["spec"]["components"].as_array_mut().unwrap();
            for c in new_components {
                if !existing_comps.contains(&c) {
                    existing_comps.push(c);
                }
            }
        } else {
            order.push(name.clone());
            merged.insert(name, companion);
        }
    }
    order
        .into_iter()
        .filter_map(|name| merged.remove(&name))
        .collect()
}

/// Parse all YAML documents in `text`, migrate each, and return a flat list:
/// primary entities first, then deduplicated companion system entities.
pub fn migrate_all(text: &str) -> anyhow::Result<Vec<serde_json::Value>> {
    // Split on document separators manually; serde_norway has no multi-doc API
    let segments: Vec<&str> = text.split("\n---").collect();
    let mut primary_docs: Vec<serde_json::Value> = Vec::new();
    let mut all_companions: Vec<serde_json::Value> = Vec::new();

    for segment in &segments {
        // Strip the leading `---` marker, then only the first newline —
        // do NOT trim() all whitespace or we destroy consistent block indentation
        // in files like `---\n    schema-version: v2\n    dd-service: …`.
        let stripped = segment.trim_start_matches('-');
        let trimmed = stripped.strip_prefix('\n').unwrap_or(stripped).trim_end();
        if trimmed.trim().is_empty() {
            continue;
        }
        let doc: serde_json::Value = serde_norway::from_str(trimmed)
            .map_err(|e| anyhow::anyhow!("failed to parse YAML document: {e}"))?;
        if !doc.is_object() {
            eprintln!("{ANSI_YELLOW}WARNING: Skipping non-mapping YAML document.{ANSI_RESET}");
            continue;
        }
        let (migrated, companions) = migrate_document(&doc);
        if let Some(m) = migrated {
            primary_docs.push(m);
        }
        all_companions.extend(companions);
    }

    if primary_docs.is_empty() && all_companions.is_empty() {
        return Ok(vec![]);
    }

    let mut result = primary_docs;
    result.extend(merge_companion_systems(all_companions));
    Ok(result)
}

// ---------------------------------------------------------------------------
// Schema validation
// ---------------------------------------------------------------------------

const SCHEMA_BASE_URL: &str =
    "https://raw.githubusercontent.com/DataDog/schema/main/service-catalog/v3/";

/// All v3 schema filenames to pre-fetch (used for $ref resolution)
const V3_SCHEMA_FILES: &[&str] = &[
    "api",
    "application",
    "custom",
    "datadog_code_locations",
    "datadog_events",
    "datadog_logs",
    "datadog_pipelines",
    "datastore",
    "entity",
    "integration_opsgenie",
    "integration_pagerduty",
    "integrations",
    "metadata",
    "queue",
    "repository",
    "service",
    "system",
];

async fn fetch_schema_by_url(
    client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<serde_json::Value> {
    let resp = client
        .get(url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("failed to fetch schema {url}: {e}"))?;
    if !resp.status().is_success() {
        anyhow::bail!("schema fetch {url} returned HTTP {}", resp.status());
    }
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| anyhow::anyhow!("failed to parse schema {url}: {e}"))
}

/// In-memory retriever for $ref resolution — satisfies jsonschema::Retrieve
struct SchemaRegistry {
    schemas: std::collections::HashMap<String, serde_json::Value>,
}

impl jsonschema::Retrieve for SchemaRegistry {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> std::result::Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        self.schemas
            .get(uri.as_str())
            .cloned()
            .ok_or_else(|| format!("schema not in registry: {uri}").into())
    }
}

/// Pre-fetch all v3 schemas from GitHub, returning a URL → schema map.
async fn fetch_all_schemas() -> anyhow::Result<std::collections::HashMap<String, serde_json::Value>>
{
    let client = reqwest::Client::new();
    let mut map = std::collections::HashMap::new();
    for name in V3_SCHEMA_FILES {
        let url = format!("{SCHEMA_BASE_URL}{name}.schema.json");
        match fetch_schema_by_url(&client, &url).await {
            Ok(schema) => {
                map.insert(url, schema);
            }
            Err(e) => {
                eprintln!(
                    "{ANSI_YELLOW}WARNING: Could not fetch schema '{name}': {e}.{ANSI_RESET}"
                );
            }
        }
    }
    Ok(map)
}

/// Validate all v3 documents. Prints coloured per-entity results.
/// Returns all validation error strings (empty = all valid).
fn validate_docs(
    docs: &[serde_json::Value],
    all_schemas: &std::collections::HashMap<String, serde_json::Value>,
) -> anyhow::Result<Vec<String>> {
    let registry = SchemaRegistry {
        schemas: all_schemas.clone(),
    };

    let mut all_warnings: Vec<String> = Vec::new();
    for doc in docs {
        let name = doc
            .get("metadata")
            .and_then(|m| m.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("(unknown)");
        let kind = doc.get("kind").and_then(|v| v.as_str()).unwrap_or("?");

        if doc.get("apiVersion").and_then(|v| v.as_str()) != Some("v3") {
            println!("{ANSI_DIM}SKIPPED  [{kind}] {name} (not v3){ANSI_RESET}");
            continue;
        }

        let kind_lower = kind.to_lowercase();
        let schema_url = format!("{SCHEMA_BASE_URL}{kind_lower}.schema.json");
        let schema = match registry.schemas.get(&schema_url) {
            Some(s) => s,
            None => {
                println!("{ANSI_DIM}SKIPPED  [{kind}] {name} (no schema){ANSI_RESET}");
                continue;
            }
        };

        let validator = jsonschema::options()
            .with_retriever(SchemaRegistry {
                schemas: registry.schemas.clone(),
            })
            .build(schema)
            .map_err(|e| anyhow::anyhow!("failed to compile schema for '{kind}': {e}"))?;

        let errors: Vec<String> = validator
            .iter_errors(doc)
            .map(|e| format!("[{}] {}", e.instance_path(), e))
            .collect();

        if errors.is_empty() {
            println!("{ANSI_GREEN}\u{2713} VALID    [{kind}] {name}{ANSI_RESET}");
        } else {
            println!("{ANSI_RED}\u{2717} INVALID  [{kind}] {name}{ANSI_RESET}");
            for err in &errors {
                println!("{ANSI_RED}    {err}{ANSI_RESET}");
            }
            for err in errors {
                all_warnings.push(format!("[{kind}] {name}: {err}"));
            }
        }
    }
    Ok(all_warnings)
}

// ---------------------------------------------------------------------------
// File discovery
// ---------------------------------------------------------------------------

pub fn discover_catalog_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                results.extend(discover_catalog_files(&path));
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with(".datadog.yaml"))
                .unwrap_or(false)
            {
                results.push(path);
            }
        }
    }
    results
}

// ---------------------------------------------------------------------------
// Output helpers
// ---------------------------------------------------------------------------

fn prompt_line(msg: &str) -> anyhow::Result<String> {
    use std::io::Write;
    print!("{msg}");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn docs_to_yaml(docs: &[serde_json::Value]) -> anyhow::Result<String> {
    let mut out = String::new();
    for doc in docs {
        let yaml = serde_norway::to_string(doc)
            .map_err(|e| anyhow::anyhow!("failed to serialise to YAML: {e}"))?;
        out.push_str("---\n");
        out.push_str(&yaml);
    }
    Ok(out.trim_end().to_string() + "\n")
}

fn write_output(yaml: &str, source: &std::path::Path) -> anyhow::Result<()> {
    let parent = source.parent().unwrap_or(std::path::Path::new("."));
    let entity_path = parent.join("entity.datadog.yaml");

    println!();
    println!("{ANSI_BOLD}Where would you like to write the output?{ANSI_RESET}");
    println!(
        "  {ANSI_BOLD}[1]{ANSI_RESET} Write to {ANSI_GREEN}{}{ANSI_RESET} (same directory, delete original)",
        entity_path.display()
    );
    println!("  {ANSI_BOLD}[2]{ANSI_RESET} Specify a custom path");
    println!("  {ANSI_BOLD}[3]{ANSI_RESET} Print to stdout only");

    let choice = prompt_line("> ")?;

    match choice.as_str() {
        "1" => {
            std::fs::write(&entity_path, yaml)
                .map_err(|e| anyhow::anyhow!("failed to write '{}': {e}", entity_path.display()))?;
            if source != entity_path {
                std::fs::remove_file(source).map_err(|e| {
                    anyhow::anyhow!("failed to delete original '{}': {e}", source.display())
                })?;
            }
            println!(
                "{ANSI_GREEN}{ANSI_BOLD}\u{2714} Written to {}{ANSI_RESET}",
                entity_path.display()
            );
        }
        "2" => {
            let custom = prompt_line("Path: ")?.trim().to_string();
            if custom.is_empty() {
                anyhow::bail!("no path provided");
            }
            let custom_path = std::path::PathBuf::from(&custom);
            if let Some(p) = custom_path.parent() {
                std::fs::create_dir_all(p).ok();
            }
            std::fs::write(&custom_path, yaml)
                .map_err(|e| anyhow::anyhow!("failed to write '{custom}': {e}"))?;
            println!("{ANSI_GREEN}{ANSI_BOLD}\u{2714} Written to {custom}{ANSI_RESET}");
        }
        _ => {
            print!("{yaml}");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub async fn migrate_schema(_cfg: &Config, file: Option<String>) -> Result<()> {
    let paths: Vec<std::path::PathBuf> = match file {
        Some(f) => {
            let p = std::path::PathBuf::from(&f);
            if p.is_dir() {
                eprintln!(
                    "{ANSI_DIM}Searching for *.datadog.yaml files in '{}'{ANSI_RESET}",
                    p.display()
                );
                let found = discover_catalog_files(&p);
                match found.len() {
                    0 => anyhow::bail!("No *.datadog.yaml files found in '{}'", p.display()),
                    1 => {
                        println!("{ANSI_DIM}Using {}{ANSI_RESET}", found[0].display());
                        found
                    }
                    _ => {
                        println!(
                            "{ANSI_BOLD}Found {} catalog files:{ANSI_RESET}",
                            found.len()
                        );
                        for (i, path) in found.iter().enumerate() {
                            println!("  [{ANSI_BOLD}{}{ANSI_RESET}] {}", i + 1, path.display());
                        }
                        let choice = prompt_line(
                            "Enter number to migrate one, or \"all\" to migrate all: ",
                        )?;
                        if choice.eq_ignore_ascii_case("all") {
                            found
                        } else {
                            let idx: usize = choice
                                .parse::<usize>()
                                .map_err(|_| anyhow::anyhow!("invalid choice '{choice}'"))?;
                            if idx == 0 || idx > found.len() {
                                anyhow::bail!("choice {idx} out of range (1–{})", found.len());
                            }
                            vec![found[idx - 1].clone()]
                        }
                    }
                }
            } else {
                if !p.exists() {
                    anyhow::bail!("path not found: '{}'", p.display());
                }
                vec![p]
            }
        }
        None => {
            let cwd = std::env::current_dir()?;
            eprintln!(
                "{ANSI_DIM}Searching for *.datadog.yaml files in '{}'{ANSI_RESET}",
                cwd.display()
            );
            let found = discover_catalog_files(&cwd);
            match found.len() {
                0 => anyhow::bail!(
                    "No *.datadog.yaml files found. Specify a path: pup idp migrate-schema <file>"
                ),
                1 => {
                    println!("{ANSI_DIM}Using {}{ANSI_RESET}", found[0].display());
                    found
                }
                _ => {
                    println!(
                        "{ANSI_BOLD}Found {} catalog files:{ANSI_RESET}",
                        found.len()
                    );
                    for (i, p) in found.iter().enumerate() {
                        println!("  [{ANSI_BOLD}{}{ANSI_RESET}] {}", i + 1, p.display());
                    }
                    let choice =
                        prompt_line("Enter number to migrate one, or \"all\" to migrate all: ")?;
                    if choice.eq_ignore_ascii_case("all") {
                        found
                    } else {
                        let idx: usize = choice
                            .parse::<usize>()
                            .map_err(|_| anyhow::anyhow!("invalid choice '{choice}'"))?;
                        if idx == 0 || idx > found.len() {
                            anyhow::bail!("choice {idx} out of range (1–{})", found.len());
                        }
                        vec![found[idx - 1].clone()]
                    }
                }
            }
        }
    };

    // For multiple files: confirm once before touching anything.
    if paths.len() > 1 {
        let n = paths.len();
        let answer = prompt_line(&format!(
            "{ANSI_BOLD}{n} files will be migrated and overwritten in place. Continue? [y/N]{ANSI_RESET} "
        ))?.to_lowercase();
        if answer != "y" && answer != "yes" {
            println!("{ANSI_DIM}Aborted.{ANSI_RESET}");
            return Ok(());
        }
    }

    eprintln!("{ANSI_DIM}Fetching v3 schemas for validation...{ANSI_RESET}");
    let schemas = std::sync::Arc::new(fetch_all_schemas().await?);
    let start = std::time::Instant::now();

    if paths.len() > 1 {
        // Process in bounded batches to avoid exhausting OS file descriptors.
        const BATCH: usize = 16;
        let mut outcomes: Vec<MigrateOutcome> = Vec::with_capacity(paths.len());

        for chunk in paths.chunks(BATCH) {
            let handles: Vec<_> = chunk
                .iter()
                .map(|path| {
                    let schemas = std::sync::Arc::clone(&schemas);
                    let path = path.clone();
                    std::thread::spawn(move || migrate_one(&path, true, &schemas))
                })
                .collect();

            for h in handles {
                outcomes.push(h.join().unwrap_or_else(|_| MigrateOutcome {
                    path: std::path::PathBuf::new(),
                    status: MigrateStatus::Failed("thread panicked".into()),
                    services: 0,
                    systems: 0,
                }));
            }
        }

        print_summary(&outcomes, start.elapsed());
    } else {
        let outcome = migrate_one(&paths[0], false, &schemas);
        if let MigrateStatus::Failed(msg) = outcome.status {
            anyhow::bail!("{msg}");
        }
    }

    Ok(())
}

fn print_summary(outcomes: &[MigrateOutcome], elapsed: std::time::Duration) {
    let n_migrated = outcomes
        .iter()
        .filter(|o| matches!(o.status, MigrateStatus::Migrated { .. }))
        .count();
    let invalid: Vec<_> = outcomes
        .iter()
        .filter(
            |o| matches!(&o.status, MigrateStatus::Migrated { warnings } if !warnings.is_empty()),
        )
        .collect();
    let failed: Vec<_> = outcomes
        .iter()
        .filter(|o| matches!(o.status, MigrateStatus::Failed(_)))
        .collect();
    let skipped: Vec<_> = outcomes
        .iter()
        .filter(|o| matches!(o.status, MigrateStatus::Skipped))
        .collect();
    let total_services: usize = outcomes.iter().map(|o| o.services).sum();
    let total_systems: usize = outcomes.iter().map(|o| o.systems).sum();

    println!();
    println!(
        "{ANSI_BOLD}Migration complete{ANSI_RESET} in {:.2}s",
        elapsed.as_secs_f64()
    );
    println!("  {ANSI_GREEN}{ANSI_BOLD}\u{2714}{ANSI_RESET}  {n_migrated} migrated");
    if !invalid.is_empty() {
        println!(
            "  {ANSI_YELLOW}\u{26a0}{ANSI_RESET}  {} written with validation warnings",
            invalid.len()
        );
        for o in &invalid {
            println!("     {ANSI_YELLOW}{}{ANSI_RESET}", o.path.display());
            if let MigrateStatus::Migrated { warnings } = &o.status {
                for w in warnings {
                    println!("       {ANSI_DIM}{w}{ANSI_RESET}");
                }
            }
        }
    }
    if !failed.is_empty() {
        println!("  {ANSI_RED}\u{2717}{ANSI_RESET}  {} failed", failed.len());
        for o in &failed {
            if let MigrateStatus::Failed(msg) = &o.status {
                println!("     {ANSI_RED}{}{ANSI_RESET}: {msg}", o.path.display());
            }
        }
    }
    if !skipped.is_empty() {
        println!(
            "  {ANSI_DIM}\u{2500}  {} skipped{ANSI_RESET}",
            skipped.len()
        );
    }
    println!(
        "  {ANSI_DIM}\u{21b3}  {total_services} service{}, {total_systems} system{} total{ANSI_RESET}",
        if total_services == 1 { "" } else { "s" },
        if total_systems == 1 { "" } else { "s" },
    );
}

fn migrate_one(
    path: &std::path::Path,
    write_in_place: bool,
    schemas: &std::collections::HashMap<String, serde_json::Value>,
) -> MigrateOutcome {
    let failed = |msg: String| MigrateOutcome {
        path: path.to_path_buf(),
        status: MigrateStatus::Failed(msg),
        services: 0,
        systems: 0,
    };

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Broken symlink or deleted file — skip silently
            return MigrateOutcome {
                path: path.to_path_buf(),
                status: MigrateStatus::Skipped,
                services: 0,
                systems: 0,
            };
        }
        Err(e) => return failed(format!("cannot read file: {e}")),
    };

    let source_version = {
        let first: serde_json::Value = serde_norway::from_str(&content).unwrap_or_default();
        detect_version(&first).to_string()
    };

    if !write_in_place {
        println!(
            "{ANSI_BOLD}Migrating{ANSI_RESET} {} {ANSI_DIM}(detected: {source_version}){ANSI_RESET}",
            path.display()
        );
    }

    let docs = match migrate_all(&content) {
        Err(e) => return failed(e.to_string()),
        Ok(d) if d.is_empty() => {
            // Empty or comment-only file — nothing to migrate
            return MigrateOutcome {
                path: path.to_path_buf(),
                status: MigrateStatus::Skipped,
                services: 0,
                systems: 0,
            };
        }
        Ok(d) => d,
    };

    let warnings = validate_docs(&docs, schemas).unwrap_or_else(|e| {
        if !write_in_place {
            eprintln!("{ANSI_YELLOW}WARNING: validation error: {e}{ANSI_RESET}");
        }
        vec![format!("validation error: {e}")]
    });

    if !warnings.is_empty() && !write_in_place {
        let choice = prompt_line("\n[1] Abort  [2] Write anyway > ").unwrap_or_default();
        if choice.trim() != "2" {
            println!("{ANSI_DIM}Aborted. Fix the source file and re-run.{ANSI_RESET}");
            return MigrateOutcome {
                path: path.to_path_buf(),
                status: MigrateStatus::Skipped,
                services: 0,
                systems: 0,
            };
        }
    }

    let yaml_out = match docs_to_yaml(&docs) {
        Ok(y) => y,
        Err(e) => return failed(e.to_string()),
    };

    let services = docs
        .iter()
        .filter(|d| d.get("kind").and_then(|v| v.as_str()) == Some("service"))
        .count();
    let systems = docs
        .iter()
        .filter(|d| d.get("kind").and_then(|v| v.as_str()) == Some("system"))
        .count();

    if write_in_place {
        let dest = path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("entity.datadog.yaml");
        if let Err(e) = std::fs::write(&dest, &yaml_out) {
            return failed(format!("failed to write '{}': {e}", dest.display()));
        }
        if path != dest {
            let _ = std::fs::remove_file(path);
        }
    } else {
        if let Err(e) = write_output(&yaml_out, path) {
            return failed(e.to_string());
        }
        println!();
        println!(
            "  {services} service entit{}, {systems} companion system entit{}",
            if services == 1 { "y" } else { "ies" },
            if systems == 1 { "y" } else { "ies" },
        );
    }

    MigrateOutcome {
        path: path.to_path_buf(),
        status: MigrateStatus::Migrated { warnings },
        services,
        systems,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> serde_json::Value {
        serde_norway::from_str(yaml).expect("invalid yaml in test")
    }

    // --- link type remapping ---

    #[test]
    fn test_link_type_remap() {
        assert_eq!(remap_link_type("wiki"), "doc");
        assert_eq!(remap_link_type("code"), "repo");
        assert_eq!(remap_link_type("url"), "other");
        assert_eq!(remap_link_type("oncall"), "other");
        assert_eq!(remap_link_type("link"), "other");
        assert_eq!(remap_link_type("doc"), "doc");
        assert_eq!(remap_link_type("repo"), "repo");
        assert_eq!(remap_link_type("dashboard"), "dashboard");
        assert_eq!(remap_link_type("runbook"), "runbook");
        assert_eq!(remap_link_type("other"), "other");
        assert_eq!(remap_link_type("custom-type"), "custom-type");
    }

    // --- file discovery ---

    #[test]
    fn test_discover_single_file() {
        let dir = std::env::temp_dir().join(format!("pup-test-discover-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("service.datadog.yaml");
        std::fs::write(&file, "dd-service: foo\n").unwrap();
        let found = discover_catalog_files(&dir);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], file);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_discover_no_files() {
        let dir =
            std::env::temp_dir().join(format!("pup-test-discover-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let found = discover_catalog_files(&dir);
        assert!(found.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_discover_recursive() {
        let dir =
            std::env::temp_dir().join(format!("pup-test-discover-rec-{}", std::process::id()));
        let sub = dir.join("subdir");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.join("a.datadog.yaml"), "x: 1").unwrap();
        std::fs::write(sub.join("b.datadog.yaml"), "x: 2").unwrap();
        std::fs::write(sub.join("not-a-match.yaml"), "x: 3").unwrap();
        let found = discover_catalog_files(&dir);
        assert_eq!(found.len(), 2);
        assert!(found
            .iter()
            .any(|f| f.file_name().unwrap() == "a.datadog.yaml"));
        assert!(found
            .iter()
            .any(|f| f.file_name().unwrap() == "b.datadog.yaml"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // --- multi-document handling ---

    #[test]
    fn test_v3_passthrough() {
        let input = "apiVersion: v3\nkind: service\nmetadata:\n  name: already-v3-service\n";
        let docs = migrate_all(input).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0]["metadata"]["name"], "already-v3-service");
        assert_eq!(docs[0]["apiVersion"], "v3");
    }

    #[test]
    fn test_migrate_all_single_doc() {
        let input = "schema-version: v2\ndd-service: my-svc\nteam: my-team\n";
        let docs = migrate_all(input).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0]["metadata"]["name"], "my-svc");
    }

    #[test]
    fn test_companion_deduplication() {
        let input = concat!(
            "---\nschema-version: v2.1\ndd-service: buildbarn-frontend\nteam: ci-team\napplication: Buildbarn\n",
            "---\nschema-version: v2.1\ndd-service: buildbarn-storage\nteam: ci-team\napplication: Buildbarn\n",
        );
        let docs = migrate_all(input).unwrap();
        // 2 service entities + 1 merged system entity
        assert_eq!(docs.len(), 3);
        let system = docs.iter().find(|d| d["kind"] == "system").unwrap();
        assert_eq!(system["metadata"]["name"], "Buildbarn");
        let components = system["spec"]["components"].as_array().unwrap();
        assert_eq!(components.len(), 2);
        assert!(components
            .iter()
            .any(|c| c.as_str().unwrap_or("").contains("buildbarn-frontend")));
        assert!(components
            .iter()
            .any(|c| c.as_str().unwrap_or("").contains("buildbarn-storage")));
    }

    #[test]
    fn test_unknown_fields_in_extensions() {
        let input = "schema-version: v2\ndd-service: svc\nteam: t\ncustom-field: hello\n";
        let docs = migrate_all(input).unwrap();
        assert_eq!(docs[0]["extensions"]["x-migrated/custom-field"], "hello");
    }

    // --- v1 migration ---

    #[test]
    fn test_migrate_v1_full() {
        let input = r##"
schema-version: v1
info:
  dd-service: payment-service
  display-name: Payment Service
  description: Handles payment processing
  service-tier: Tier 1
org:
  team: payments-team
  application: checkout-platform
contact:
  email: payments@example.com
  slack: "#payments-oncall"
tags:
  - "env:production"
  - "team:payments"
external-resources:
  - name: Runbook
    type: wiki
    url: https://wiki.example.com/payments/runbook
  - name: Source Code
    type: code
    url: https://github.com/example/payment-service
integrations:
  pagerduty: https://events.pagerduty.com/integration/abc123/enqueue
  github: https://github.com/example/payment-service
extensions:
  datadoghq.com/slo:
    - id: slo-payment-availability
"##;
        let doc = parse(input);
        let (entity, companions) = migrate_v1(&doc);
        assert_eq!(entity["apiVersion"], "v3");
        assert_eq!(entity["kind"], "service");
        assert_eq!(entity["metadata"]["name"], "payment-service");
        assert_eq!(entity["metadata"]["displayName"], "Payment Service");
        assert_eq!(
            entity["metadata"]["description"],
            "Handles payment processing"
        );
        assert_eq!(entity["metadata"]["owner"], "payments-team");
        let tags = entity["metadata"]["tags"].as_array().unwrap();
        assert!(tags.contains(&serde_json::json!("env:production")));
        let contacts = entity["metadata"]["contacts"].as_array().unwrap();
        assert!(contacts
            .iter()
            .any(|c| c["type"] == "email" && c["contact"] == "payments@example.com"));
        assert!(contacts.iter().any(|c| c["type"] == "slack"));
        let links = entity["metadata"]["links"].as_array().unwrap();
        let runbook = links.iter().find(|l| l["name"] == "Runbook").unwrap();
        assert_eq!(runbook["type"], "doc"); // wiki → doc
        let code = links.iter().find(|l| l["name"] == "Source Code").unwrap();
        assert_eq!(code["type"], "repo"); // code → repo
        assert!(links
            .iter()
            .any(|l| l["name"] == "GitHub" && l["type"] == "repo"));
        assert_eq!(entity["spec"]["tier"], "Tier 1");
        assert_eq!(entity["spec"]["componentOf"][0], "system:checkout-platform");
        assert_eq!(
            entity["integrations"]["pagerduty"]["serviceURL"],
            "https://events.pagerduty.com/integration/abc123/enqueue"
        );
        assert!(entity["extensions"]["datadoghq.com/slo"].is_array());
        assert_eq!(companions.len(), 1);
        assert_eq!(companions[0]["kind"], "system");
        assert_eq!(companions[0]["metadata"]["name"], "checkout-platform");
        assert_eq!(
            companions[0]["spec"]["components"][0],
            "service:payment-service"
        );
    }

    #[test]
    fn test_migrate_v1_minimal() {
        let doc = parse("schema-version: v1\ninfo:\n  dd-service: my-minimal-service\n");
        let (entity, companions) = migrate_v1(&doc);
        assert_eq!(entity["apiVersion"], "v3");
        assert_eq!(entity["kind"], "service");
        assert_eq!(entity["metadata"]["name"], "my-minimal-service");
        assert!(
            entity.get("spec").is_none()
                || entity["spec"]
                    .as_object()
                    .map(|m| m.is_empty())
                    .unwrap_or(true)
        );
        assert!(companions.is_empty());
    }

    // --- v2 migration ---

    #[test]
    fn test_migrate_v2_minimal() {
        let doc = parse("schema-version: v2\ndd-service: my-v2-service\nteam: my-team\n");
        let (entity, companions) = migrate_v2(&doc);
        assert_eq!(entity["apiVersion"], "v3");
        assert_eq!(entity["metadata"]["name"], "my-v2-service");
        assert_eq!(entity["metadata"]["owner"], "my-team");
        assert!(companions.is_empty());
    }

    #[test]
    fn test_migrate_v2_full() {
        let input = r##"
schema-version: v2
dd-service: inventory-service
team: inventory-team
contacts:
  - name: On-Call
    type: slack
    contact: "#inventory-oncall"
links:
  - name: Dashboard
    type: dashboard
    url: https://app.datadoghq.com/dashboard/abc-123
  - name: Old Wiki
    type: wiki
    url: https://wiki.example.com/inventory
repos:
  - name: Main Repo
    provider: Github
    url: https://github.com/example/inventory-service
docs:
  - name: API Docs
    provider: Confluence
    url: https://confluence.example.com/inventory/api
tags:
  - "env:production"
integrations:
  pagerduty: https://events.pagerduty.com/integration/def456/enqueue
  opsgenie:
    service-url: https://api.opsgenie.com/v2/alerts/inventory
    region: US
extensions:
  datadoghq.com/feature-flags:
    enabled: true
"##;
        let doc = parse(input);
        let (entity, companions) = migrate_v2(&doc);
        assert_eq!(entity["metadata"]["name"], "inventory-service");
        let links = entity["metadata"]["links"].as_array().unwrap();
        assert!(links
            .iter()
            .any(|l| l["name"] == "Old Wiki" && l["type"] == "doc"));
        assert!(links
            .iter()
            .any(|l| l["type"] == "repo"
                && l["url"] == "https://github.com/example/inventory-service"));
        assert!(links
            .iter()
            .any(|l| l["type"] == "doc"
                && l["url"] == "https://confluence.example.com/inventory/api"));
        assert_eq!(
            entity["integrations"]["pagerduty"]["serviceURL"],
            "https://events.pagerduty.com/integration/def456/enqueue"
        );
        assert_eq!(
            entity["integrations"]["opsgenie"]["serviceURL"],
            "https://api.opsgenie.com/v2/alerts/inventory"
        );
        assert_eq!(entity["integrations"]["opsgenie"]["region"], "US");
        assert_eq!(
            entity["extensions"]["datadoghq.com/feature-flags"]["enabled"],
            true
        );
        assert!(companions.is_empty());
    }

    #[test]
    fn test_migrate_v2_link_types() {
        let input = "schema-version: v2\ndd-service: svc\nlinks:\n  - { name: A, type: wiki,    url: http://a }\n  - { name: B, type: code,    url: http://b }\n  - { name: C, type: url,     url: http://c }\n  - { name: D, type: oncall,  url: http://d }\n  - { name: E, type: runbook, url: http://e }\n";
        let doc = parse(input);
        let (entity, _) = migrate_v2(&doc);
        let links = entity["metadata"]["links"].as_array().unwrap();
        let t = |name: &str| {
            links.iter().find(|l| l["name"] == name).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_string()
        };
        assert_eq!(t("A"), "doc");
        assert_eq!(t("B"), "repo");
        assert_eq!(t("C"), "other");
        assert_eq!(t("D"), "other");
        assert_eq!(t("E"), "runbook");
    }

    // --- v2.1 migration ---

    #[test]
    fn test_migrate_v2_1_full() {
        let input = r##"
schema-version: v2.1
dd-service: order-service
team: orders-team
description: Manages order lifecycle
application: commerce-platform
tier: High
lifecycle: production
contacts:
  - name: Orders On-Call
    type: slack
    contact: "#orders-oncall"
repos:
  - name: Orders Service
    provider: Github
    url: https://github.com/example/order-service
integrations:
  pagerduty:
    service-url: https://events.pagerduty.com/integration/ghi789/enqueue
  opsgenie:
    service-url: https://api.opsgenie.com/v2/alerts/orders
    region: EU
"##;
        let doc = parse(input);
        let (entity, companions) = migrate_v2_1(&doc);
        assert_eq!(entity["metadata"]["name"], "order-service");
        assert_eq!(entity["metadata"]["owner"], "orders-team");
        assert_eq!(entity["metadata"]["description"], "Manages order lifecycle");
        assert_eq!(entity["spec"]["lifecycle"], "production");
        assert_eq!(entity["spec"]["tier"], "High");
        assert_eq!(entity["spec"]["componentOf"][0], "system:commerce-platform");
        assert_eq!(
            entity["integrations"]["pagerduty"]["serviceURL"],
            "https://events.pagerduty.com/integration/ghi789/enqueue"
        );
        assert_eq!(entity["integrations"]["opsgenie"]["region"], "EU");
        let links = entity["metadata"]["links"].as_array().unwrap();
        assert!(
            links
                .iter()
                .any(|l| l["type"] == "repo"
                    && l["url"] == "https://github.com/example/order-service")
        );
        assert_eq!(companions.len(), 1);
        assert_eq!(companions[0]["metadata"]["name"], "commerce-platform");
    }

    #[test]
    fn test_migrate_v2_1_companion() {
        let doc = parse("schema-version: v2.1\ndd-service: buildbarn-frontend\nteam: ci-team\napplication: Buildbarn\n");
        let (entity, companions) = migrate_v2_1(&doc);
        assert_eq!(entity["spec"]["componentOf"][0], "system:Buildbarn");
        assert_eq!(companions[0]["metadata"]["name"], "Buildbarn");
    }

    // --- v2.2 migration ---

    #[test]
    fn test_migrate_v2_2_full() {
        let input = r#"
schema-version: v2.2
dd-service: shipping-service
team: logistics-team
description: Manages shipping operations
application: fulfillment-platform
tier: High
lifecycle: production
type: web
languages:
  - go
  - python
ci-pipeline-fingerprints:
  - abc123def456
links:
  - name: Internal Wiki
    type: wiki
    url: https://wiki.example.com/shipping
  - name: Oncall Docs
    type: oncall
    url: https://oncall.example.com/shipping
integrations:
  pagerduty:
    service-url: https://events.pagerduty.com/integration/jkl012/enqueue
"#;
        let doc = parse(input);
        let (entity, companions) = migrate_v2_2(&doc);
        assert_eq!(entity["metadata"]["name"], "shipping-service");
        assert_eq!(entity["spec"]["type"], "web");
        assert_eq!(entity["spec"]["lifecycle"], "production");
        let langs = entity["spec"]["languages"].as_array().unwrap();
        assert!(langs.contains(&serde_json::json!("go")));
        assert_eq!(
            entity["datadog"]["pipelines"]["fingerprints"][0],
            "abc123def456"
        );
        let links = entity["metadata"]["links"].as_array().unwrap();
        assert!(links
            .iter()
            .any(|l| l["name"] == "Internal Wiki" && l["type"] == "doc"));
        assert!(links
            .iter()
            .any(|l| l["name"] == "Oncall Docs" && l["type"] == "other"));
        assert_eq!(
            entity["integrations"]["pagerduty"]["serviceURL"],
            "https://events.pagerduty.com/integration/jkl012/enqueue"
        );
        assert_eq!(companions.len(), 1);
        assert_eq!(companions[0]["metadata"]["name"], "fulfillment-platform");
    }

    #[test]
    fn test_migrate_v2_2_unknown_fields_in_extensions() {
        let doc = parse("schema-version: v2.2\ndd-service: svc\nteam: t\ncustom-field: hello\n");
        let (entity, _) = migrate_v2_2(&doc);
        assert_eq!(entity["extensions"]["x-migrated/custom-field"], "hello");
    }

    // --- version detection ---

    #[test]
    fn test_detect_v3() {
        let doc = parse("apiVersion: v3\nkind: service\nmetadata:\n  name: foo\n");
        assert_eq!(detect_version(&doc), "v3");
    }

    #[test]
    fn test_detect_v2_2() {
        let doc = parse("schema-version: v2.2\ndd-service: foo\n");
        assert_eq!(detect_version(&doc), "v2.2");
    }

    #[test]
    fn test_detect_v2_1() {
        let doc = parse("schema-version: v2.1\ndd-service: foo\n");
        assert_eq!(detect_version(&doc), "v2.1");
    }

    #[test]
    fn test_detect_v2() {
        let doc = parse("schema-version: v2\ndd-service: foo\n");
        assert_eq!(detect_version(&doc), "v2");
    }

    #[test]
    fn test_detect_v1_explicit() {
        let doc = parse("schema-version: v1\nkind: service\ninfo:\n  dd-service: foo\n");
        assert_eq!(detect_version(&doc), "v1");
    }

    #[test]
    fn test_detect_v1_implicit() {
        let doc = parse("info:\n  dd-service: my-service\n");
        assert_eq!(detect_version(&doc), "v1");
    }

    #[test]
    fn test_detect_v1_noncatalog() {
        let doc = parse("schema-version: v1\nkind: mergequeue\nname: foo\n");
        assert_eq!(detect_version(&doc), "v1-noncatalog");
    }

    #[test]
    fn test_detect_unknown() {
        let doc = parse("schema-version: v99\ndd-service: foo\n");
        assert_eq!(detect_version(&doc), "unknown");
    }

    #[test]
    fn test_detect_no_version_no_info() {
        let doc = parse("some-key: some-value\n");
        assert_eq!(detect_version(&doc), "unknown");
    }
}

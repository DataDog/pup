use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct KindListResponse {
    #[serde(default)]
    pub data: Vec<KindResource>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct KindResponse {
    pub data: KindResource,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct KindResource {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub attributes: KindAttributes,
}

impl KindResource {
    pub(super) fn kind(&self) -> &str {
        if self.id.is_empty() {
            &self.attributes.name
        } else {
            &self.id
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct KindAttributes {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub display: KindDisplay,
    #[serde(default)]
    pub attribute_types: BTreeMap<String, KindAttribute>,
    #[serde(default)]
    pub relations: BTreeMap<String, KindRelation>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct KindAttribute {
    #[serde(default, rename = "dataType")]
    pub data_type: String,
    pub calculation: Option<KindCalculation>,
    #[serde(default)]
    pub scopes: Vec<KindScope>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct KindDisplay {
    #[serde(default)]
    pub display_name_property: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct KindScope {
    pub name: String,
    #[serde(default)]
    pub is_primary_tag: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct KindCalculation {
    #[serde(default, rename = "type")]
    pub calculation_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct KindRelation {
    #[serde(default)]
    pub target_kind: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct EntitiesResponse {
    pub data: Vec<EntityResource>,
    #[serde(default)]
    pub included: Vec<EntityResource>,
    #[serde(default)]
    pub meta: EntityMeta,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct EntityResource {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub attributes: BTreeMap<String, Value>,
    #[serde(default)]
    pub relationships: BTreeMap<String, EntityRelationship>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct EntityRelationship {
    pub data: Value,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct EntityMeta {
    pub total_count: Option<usize>,
    pub warnings: Option<Value>,
    #[serde(default)]
    pub page: PageMeta,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct PageMeta {
    #[serde(default)]
    pub next_cursor: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ResourceIdentifier {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub meta: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize)]
pub(super) struct NormalizedEntitiesResponse {
    pub query: QueryEcho,
    pub results: Vec<NormalizedEntity>,
    pub page: NormalizedPage,
    pub count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_count: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_warnings: Option<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub additional_page_warnings: Vec<ServerPageWarnings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_request: Option<NextRequest>,
}

#[derive(Debug, Serialize)]
pub(super) struct NextRequest {
    /// Arguments after the `pup` executable, ready for an argv-based subprocess.
    pub args: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct ServerPageWarnings {
    pub page: usize,
    pub warnings: Value,
}

#[derive(Debug, Serialize)]
pub(super) struct QueryEcho {
    pub query: String,
    pub inferred_kind: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub fields_by_kind: BTreeMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub edge_fields: BTreeMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeseries_interval: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<i64>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub scopes: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub order_by: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_text_match: Option<String>,
    pub include_total_count: bool,
    pub relation_limit: usize,
}

#[derive(Debug, Serialize)]
pub(super) struct NormalizedPage {
    pub limit: usize,
    pub pages_fetched: usize,
    pub stop_reason: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_results: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct NormalizedEntity {
    pub entity: EntityIdentity,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, Value>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub relationships: BTreeMap<String, RelationshipSummary>,
}

#[derive(Debug, Serialize)]
pub(super) struct RelationshipSummary {
    pub count: usize,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sample: Vec<RelatedEntity>,
}

#[derive(Debug, Serialize)]
pub(super) struct RelatedEntity {
    #[serde(flatten)]
    pub entity: EntityIdentity,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub edge_fields: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize)]
pub(super) struct EntityIdentity {
    #[serde(rename = "ref")]
    pub entity_ref: String,
    pub kind: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, Value>,
}

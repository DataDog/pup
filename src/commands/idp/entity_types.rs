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
    pub attribute_types: BTreeMap<String, KindAttribute>,
    #[serde(default)]
    pub relations: BTreeMap<String, KindRelation>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct KindAttribute {
    #[serde(default, rename = "dataType")]
    pub data_type: String,
    pub calculation: Option<KindCalculation>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct KindCalculation {
    #[serde(default, rename = "type")]
    pub calculation_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct KindRelation {
    #[serde(default)]
    pub target_kind: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct EntitiesResponse {
    #[serde(default)]
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
    #[serde(default)]
    pub data: Value,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct EntityMeta {
    pub total_count: Option<usize>,
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
}

#[derive(Debug, Serialize)]
pub(super) struct QueryEcho {
    pub query: String,
    pub inferred_kind: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    pub timeseries_interval: String,
    pub relation_limit: usize,
}

#[derive(Debug, Serialize)]
pub(super) struct NormalizedPage {
    pub limit: usize,
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
    pub sample: Vec<EntityIdentity>,
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

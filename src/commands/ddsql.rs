use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Read};
use std::time::Duration;

use crate::config::{Config, OutputFormat};
use crate::formatter;
use crate::raw_client;
use crate::useragent;
use crate::util_ext;
use crate::version;

fn client_id() -> String {
    let agent = useragent::detect_agent_info();
    if agent.detected {
        format!("pup/{}/{}", version::VERSION, agent.name)
    } else {
        format!("pup/{}", version::VERSION)
    }
}

fn read_query_from_reader(reader: &mut impl Read) -> Result<String> {
    let mut query = String::new();
    reader.read_to_string(&mut query)?;

    if query.trim().is_empty() {
        return Err(anyhow!("DDSQL query is empty"));
    }

    Ok(query)
}

fn resolve_query_from_reader(query: &str, reader: &mut impl Read) -> Result<String> {
    match query {
        "-" => read_query_from_reader(reader),
        query => Ok(query.to_string()),
    }
}

fn resolve_query(query: &str) -> Result<String> {
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    resolve_query_from_reader(query, &mut handle)
}

const DDSQL_DOCS_PATH: &str = "/api/unstable/ddsql-editor/tools/ddsql-docs";
const DDSQL_TABLE_NAMES_PATH: &str = "/api/unstable/ddsql-editor/tools/table-names";
const DDSQL_TABLE_DATA_PATH: &str = "/api/unstable/ddsql-editor/tools/table-data";
const REFERENCE_TABLES_PATH: &str = "/api/v2/reference-tables/tables";

#[derive(Debug, Deserialize, Serialize)]
struct DdsqlDocsResponse {
    docs: String,
}

#[derive(Debug, Deserialize)]
struct DdsqlTableNamesResponse {
    tables: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DdsqlTableDataResponse {
    tables: Vec<DdsqlTableDataEntry>,
}

#[derive(Debug, Deserialize)]
struct DdsqlTableDataEntry {
    #[serde(rename = "Table")]
    table: DdsqlToolTable,
}

#[derive(Debug, Deserialize)]
struct DdsqlToolTable {
    #[serde(rename = "TableName")]
    table_name: String,
    #[serde(rename = "Columns")]
    columns: Vec<DdsqlToolColumn>,
}

#[derive(Debug, Deserialize)]
struct DdsqlToolColumn {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Type")]
    ty: String,
}

#[derive(Debug, Deserialize)]
struct ReferenceTablesResponse {
    data: Vec<ReferenceTableRecord>,
    meta: Option<ReferenceTablesMeta>,
}

#[derive(Debug, Deserialize)]
struct ReferenceTablesMeta {
    page: Option<ReferenceTablesPage>,
}

#[derive(Debug, Deserialize)]
struct ReferenceTablesPage {
    next_offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ReferenceTableRecord {
    attributes: ReferenceTableAttributes,
}

#[derive(Debug, Deserialize)]
struct ReferenceTableAttributes {
    description: String,
    row_count: i64,
    schema: ReferenceTableSchema,
    source: String,
    status: String,
    table_name: String,
}

#[derive(Debug, Deserialize)]
struct ReferenceTableSchema {
    primary_keys: Vec<String>,
    fields: Vec<ReferenceTableField>,
}

#[derive(Debug, Deserialize)]
struct ReferenceTableField {
    name: String,
    #[serde(rename = "type")]
    ty: String,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
struct DdsqlSchemaTable {
    name: String,
    id: String,
    kind: String,
    provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    row_count: Option<i64>,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
struct DdsqlSchemaColumn {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    raw_type: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    primary_key: bool,
}

fn print_plain_text(text: &str) {
    println!("{text}");
}

fn filter_tables<'a>(
    tables: impl IntoIterator<Item = &'a str>,
    query: Option<&str>,
) -> Vec<String> {
    let needle = query.map(str::to_lowercase);
    let mut items: Vec<String> = tables
        .into_iter()
        .filter(|table| {
            needle
                .as_ref()
                .map(|needle| table.to_lowercase().contains(needle))
                .unwrap_or(true)
        })
        .map(str::to_string)
        .collect();
    items.sort();
    items
}

fn normalize_column_name(name: &str) -> String {
    name.trim_matches('"').to_string()
}

fn table_matches_query(name: &str, query: Option<&str>) -> bool {
    query
        .map(|query| name.to_lowercase().contains(&query.to_lowercase()))
        .unwrap_or(true)
}

fn normalize_ddsql_type(raw_type: &str) -> String {
    match raw_type.to_ascii_lowercase().as_str() {
        "string" => "VARCHAR".to_string(),
        "int32" | "int64" => "BIGINT".to_string(),
        "bool" | "boolean" => "BOOLEAN".to_string(),
        "float32" | "float64" | "double" | "decimal" => "DECIMAL".to_string(),
        "timestamp" => "TIMESTAMP".to_string(),
        "json" => "JSON".to_string(),
        "hstore_csv" | "hstore" => "HSTORE".to_string(),
        other => other.to_ascii_uppercase(),
    }
}

fn is_rate_limited(err: &anyhow::Error) -> bool {
    err.to_string().contains("HTTP 429 Too Many Requests")
}

fn output_items<T: Serialize>(
    cfg: &Config,
    items: &T,
    count: usize,
    truncated: bool,
    next_action: Option<String>,
) -> Result<()> {
    let meta = formatter::Metadata {
        count: Some(count),
        truncated,
        command: None,
        next_action,
    };
    formatter::format_and_print(
        items,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&meta),
        cfg.jq.as_deref(),
    )
}

fn parse_ddsql_docs(resp: Value) -> Result<DdsqlDocsResponse> {
    serde_json::from_value(resp).map_err(|e| anyhow!("failed to parse DDSQL docs response: {e}"))
}

fn parse_table_names(resp: Value) -> Result<Vec<String>> {
    let parsed: DdsqlTableNamesResponse = serde_json::from_value(resp)
        .map_err(|e| anyhow!("failed to parse DDSQL table list: {e}"))?;
    Ok(parsed.tables)
}

fn parse_public_columns(resp: Value) -> Result<Vec<DdsqlSchemaColumn>> {
    let parsed: DdsqlTableDataResponse = serde_json::from_value(resp)
        .map_err(|e| anyhow!("failed to parse DDSQL table data: {e}"))?;

    let table = parsed
        .tables
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("no DDSQL table metadata returned"))?
        .table;

    let _ = table.table_name;
    Ok(table
        .columns
        .into_iter()
        .map(|column| DdsqlSchemaColumn {
            name: normalize_column_name(&column.name),
            ty: normalize_ddsql_type(&column.ty),
            raw_type: column.ty,
            primary_key: false,
        })
        .collect())
}

fn parse_reference_tables(resp: Value) -> Result<(Vec<ReferenceTableRecord>, Option<i64>)> {
    let parsed: ReferenceTablesResponse = serde_json::from_value(resp)
        .map_err(|e| anyhow!("failed to parse reference table response: {e}"))?;
    let next_offset = parsed
        .meta
        .and_then(|meta| meta.page)
        .and_then(|page| page.next_offset);

    Ok((parsed.data, next_offset))
}

fn build_reference_table_columns(table: ReferenceTableRecord) -> Vec<DdsqlSchemaColumn> {
    let primary_keys = table.attributes.schema.primary_keys;

    table
        .attributes
        .schema
        .fields
        .into_iter()
        .map(|field| {
            let is_primary = primary_keys.iter().any(|primary| primary == &field.name);
            DdsqlSchemaColumn {
                name: field.name,
                ty: normalize_ddsql_type(&field.ty),
                raw_type: field.ty,
                primary_key: is_primary,
            }
        })
        .collect()
}

#[cfg(test)]
fn parse_reference_table_columns(resp: Value, table_name: &str) -> Result<Vec<DdsqlSchemaColumn>> {
    let (tables, _) = parse_reference_tables(resp)?;

    let table = tables
        .into_iter()
        .find(|table| table.attributes.table_name == table_name)
        .ok_or_else(|| anyhow!("reference table `{table_name}` not found"))?;

    Ok(build_reference_table_columns(table))
}

fn build_reference_table_item(table: ReferenceTableRecord) -> DdsqlSchemaTable {
    DdsqlSchemaTable {
        id: format!("reference_tables.{}", table.attributes.table_name),
        kind: "reference_table".to_string(),
        provider: "reference_tables".to_string(),
        name: table.attributes.table_name,
        description: if table.attributes.description.is_empty() {
            None
        } else {
            Some(table.attributes.description)
        },
        source: Some(table.attributes.source),
        status: Some(table.attributes.status),
        row_count: Some(table.attributes.row_count),
    }
}

fn parse_reference_table_items(resp: Value) -> Result<(Vec<DdsqlSchemaTable>, Option<i64>)> {
    let (tables, next_offset) = parse_reference_tables(resp)?;

    let mut items: Vec<DdsqlSchemaTable> =
        tables.into_iter().map(build_reference_table_item).collect();
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((items, next_offset))
}

fn build_public_table_items(table_names: &[String], query: Option<&str>) -> Vec<DdsqlSchemaTable> {
    filter_tables(table_names.iter().map(|name| name.as_str()), query)
        .into_iter()
        .map(|name| DdsqlSchemaTable {
            provider: name.split('.').next().unwrap_or("unknown").to_string(),
            id: format!("public.{name}"),
            kind: "public".to_string(),
            name,
            description: None,
            source: None,
            status: None,
            row_count: None,
        })
        .collect()
}

async fn get_ddsql_docs(cfg: &Config) -> Result<DdsqlDocsResponse> {
    let resp = raw_client::raw_get(cfg, DDSQL_DOCS_PATH, &[]).await?;
    parse_ddsql_docs(resp)
}

async fn get_public_table_names(cfg: &Config) -> Result<Vec<String>> {
    let resp = raw_client::raw_get(cfg, DDSQL_TABLE_NAMES_PATH, &[]).await?;
    parse_table_names(resp)
}

async fn search_reference_tables(
    cfg: &Config,
    query: Option<&str>,
    max_results: usize,
) -> Result<(Vec<DdsqlSchemaTable>, bool)> {
    let mut results = Vec::new();
    let mut page_offset = 0_i64;
    let target = max_results.max(1);
    let page_limit = target.clamp(25, 100);

    loop {
        let mut params: Vec<(String, String)> = vec![
            ("page[limit]".to_string(), page_limit.to_string()),
            ("page[offset]".to_string(), page_offset.to_string()),
        ];
        if let Some(query) = query {
            params.push(("filter[table_name_contains]".to_string(), query.to_string()));
        }

        let refs: Vec<(&str, &str)> = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let resp = raw_client::raw_get(cfg, REFERENCE_TABLES_PATH, &refs).await?;
        let (mut items, next_offset) = parse_reference_table_items(resp)?;
        items.retain(|item| table_matches_query(&item.name, query));
        results.append(&mut items);
        if results.len() > target {
            results.sort_by(|a, b| a.name.cmp(&b.name));
            return Ok((results, true));
        }

        match next_offset {
            Some(next_offset) => page_offset = next_offset,
            None => break,
        }
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((results, false))
}

async fn get_reference_table_columns(
    cfg: &Config,
    table_name: &str,
) -> Result<Vec<DdsqlSchemaColumn>> {
    let mut page_offset = 0_i64;

    loop {
        let offset_param = page_offset.to_string();
        let params = [
            ("page[limit]", "100"),
            ("page[offset]", offset_param.as_str()),
            ("filter[table_name_contains]", table_name),
        ];
        let resp = raw_client::raw_get(cfg, REFERENCE_TABLES_PATH, &params).await?;
        let (tables, next_offset) = parse_reference_tables(resp)?;

        if let Some(table) = tables
            .into_iter()
            .find(|table| table.attributes.table_name == table_name)
        {
            return Ok(build_reference_table_columns(table));
        }

        match next_offset {
            Some(next_offset) => page_offset = next_offset,
            None => break,
        }
    }

    Err(anyhow!("reference table `{table_name}` not found"))
}

fn normalize_table_lookup(table_id: &str) -> (&str, String) {
    if let Some(name) = table_id.strip_prefix("public.") {
        ("public", name.to_string())
    } else if let Some(name) = table_id.strip_prefix("reference_tables.") {
        ("reference_table", name.to_string())
    } else {
        ("public", table_id.to_string())
    }
}

pub async fn spec(cfg: &Config) -> Result<()> {
    let resp = get_ddsql_docs(cfg).await?;
    match cfg.output_format {
        OutputFormat::Json | OutputFormat::Yaml => formatter::output(cfg, &resp),
        _ => {
            print_plain_text(&resp.docs);
            Ok(())
        }
    }
}

pub async fn schema_tables(
    cfg: &Config,
    query: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<()> {
    let public_table_names = get_public_table_names(cfg).await?;
    let mut items = build_public_table_items(&public_table_names, query);
    let reference_target = offset.saturating_add(limit).saturating_add(1);
    let (reference_items, references_truncated, rate_limit_note) =
        match search_reference_tables(cfg, query, reference_target).await {
            Ok((items, truncated)) => (items, truncated, None),
            Err(err) if is_rate_limited(&err) => (
                Vec::new(),
                true,
                Some(
                    "reference table search hit rate limit; rerun later or use `pup reference-tables list`"
                        .to_string(),
                ),
            ),
            Err(err) => return Err(err),
        };
    items.extend(reference_items);
    items.sort_by(|a, b| a.name.cmp(&b.name));

    let total = items.len();
    let paged: Vec<DdsqlSchemaTable> = items.into_iter().skip(offset).take(limit).collect();
    let truncated = references_truncated || offset.saturating_add(paged.len()) < total;
    let next_action = rate_limit_note.unwrap_or_else(|| {
        "use `pup ddsql schema columns --table-id <id>` for column details".to_string()
    });
    output_items(cfg, &paged, paged.len(), truncated, Some(next_action))
}

pub async fn schema_columns(
    cfg: &Config,
    table_id: &str,
    limit: usize,
    offset: usize,
) -> Result<()> {
    let (kind, table_name) = normalize_table_lookup(table_id);
    let columns = if kind == "reference_table" {
        get_reference_table_columns(cfg, &table_name).await?
    } else {
        let resp = raw_client::raw_post(
            cfg,
            DDSQL_TABLE_DATA_PATH,
            json!({ "tables": [table_name] }),
        )
        .await?;
        parse_public_columns(resp)?
    };

    let total = columns.len();
    let paged: Vec<DdsqlSchemaColumn> = columns.into_iter().skip(offset).take(limit).collect();
    let truncated = offset.saturating_add(paged.len()) < total;
    output_items(
        cfg,
        &paged,
        paged.len(),
        truncated,
        Some("rerun with `--offset <n>` to inspect additional columns".to_string()),
    )
}

/// Build a request for the Advanced Query API (tabular/scalar endpoint).
///
/// Endpoint: POST /api/unstable/advanced/query/tabular
/// Supports OAuth tokens and API keys (unlike the UI-only analysis-workspace endpoint).
fn build_advanced_table_request(
    query: &str,
    from: &str,
    to: &str,
    limit: Option<i32>,
) -> Result<Value> {
    let from_ms =
        util_ext::parse_time_to_unix_millis(from).map_err(|e| anyhow!("invalid --from: {e}"))?;
    let to_ms =
        util_ext::parse_time_to_unix_millis(to).map_err(|e| anyhow!("invalid --to: {e}"))?;

    let mut query_body = json!({
        "dataset": "user_query",
        "time_window": { "from": from_ms, "to": to_ms },
    });
    if let Some(l) = limit {
        query_body["limit"] = json!(l);
    }

    Ok(json!({
        "data": {
            "type": "analysis_workspace_query_request",
            "attributes": {
                "datasets": [{
                    "data_source": "analysis_dataset",
                    "name": "user_query",
                    "query": {
                        "type": "sql_analysis",
                        "sql_query": query,
                    }
                }],
                "query": query_body,
            }
        },
        "meta": {
            "client_id": client_id(),
            "user_query_id": uuid::Uuid::new_v4().to_string(),
            "use_async_querying": true,
        }
    }))
}

/// Extract the query status from an async query response.
///
/// Returns `Ok(Some(query_id))` if the query is still running,
/// `Ok(None)` if the query is done, or an error if the status is unexpected.
fn extract_query_status(resp: &Value) -> Result<Option<String>> {
    // API returns meta.responses[0].queries[0] (new shape) or meta.queries[0] (old shape).
    let query_meta = resp
        .pointer("/meta/responses/0/queries/0")
        .or_else(|| resp.pointer("/meta/queries/0"))
        .ok_or_else(|| anyhow!("unexpected response: missing query status in meta"))?;

    let status = query_meta
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("unexpected response: missing query status"))?;

    match status {
        "done" => Ok(None),
        "running" => {
            let query_id = query_meta
                .get("query_id")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("unexpected response: running query missing query_id"))?
                .to_string();
            Ok(Some(query_id))
        }
        other => Err(anyhow!("unexpected query status: {other}")),
    }
}

/// Build a polling request for an in-progress async query.
///
/// Endpoint: POST /api/unstable/advanced/query/tabular/fetch
/// Same shape as the originating request, but with `query_id` added and the type
/// changed to `advanced_query_fetch_request` (the fetch endpoint rejects the
/// original `analysis_workspace_query_request` type).
fn build_fetch_request(base_body: &Value, query_id: &str) -> Value {
    let mut fetch_body = base_body.clone();
    fetch_body["data"]["attributes"]["query_id"] = json!(query_id);
    fetch_body["data"]["type"] = json!("advanced_query_fetch_request");
    fetch_body
}

/// Submit an async query and poll until completion.
///
/// Sends the initial request and, if the query is still running, polls the fetch
/// endpoint until the query completes. When `command` is provided, it is appended
/// to the User-Agent header for audit log differentiation.
async fn execute_async_query(cfg: &Config, body: Value, command: Option<&str>) -> Result<Value> {
    let ua = useragent::get_with_command(command);
    let resp = raw_client::raw_post_with_ua(
        cfg,
        "/api/unstable/advanced/query/tabular",
        body.clone(),
        ua.clone(),
    )
    .await?;

    let mut query_id = match extract_query_status(&resp)? {
        None => return Ok(resp),
        Some(id) => id,
    };

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        let fetch_body = build_fetch_request(&body, &query_id);
        let poll_resp = raw_client::raw_post_with_ua(
            cfg,
            "/api/unstable/advanced/query/tabular/fetch",
            fetch_body,
            ua.clone(),
        )
        .await?;

        match extract_query_status(&poll_resp)? {
            None => return Ok(poll_resp),
            Some(id) => query_id = id,
        }
    }
}

/// Execute a DDSQL query and return the result as a row-based JSON array.
///
/// Shared function used by both `ddsql table` and `security findings-analyze`.
/// Pass `command` to tag the User-Agent for audit log differentiation.
pub async fn execute_ddsql_query(
    cfg: &Config,
    query: &str,
    from: &str,
    to: &str,
    limit: Option<i32>,
) -> Result<Value> {
    execute_ddsql_query_with_command(cfg, query, from, to, limit, None).await
}

/// Like `execute_ddsql_query`, but with a command identifier appended to the User-Agent.
pub async fn execute_ddsql_query_with_command(
    cfg: &Config,
    query: &str,
    from: &str,
    to: &str,
    limit: Option<i32>,
    command: Option<&str>,
) -> Result<Value> {
    let body = build_advanced_table_request(query, from, to, limit)?;
    let data = execute_async_query(cfg, body, command).await?;
    columnar_to_rows(&data)
}

pub async fn table(
    cfg: &Config,
    query: &str,
    from: &str,
    to: &str,
    _interval: Option<i64>,
    limit: Option<i32>,
    _offset: Option<i32>,
) -> Result<()> {
    let query = resolve_query(query)?;
    let rows = execute_ddsql_query(cfg, &query, from, to, limit).await?;
    formatter::output(cfg, &rows)
}

pub async fn time_series(
    cfg: &Config,
    query: &str,
    from: &str,
    to: &str,
    _interval: Option<i64>,
    limit: i32,
) -> Result<()> {
    let query = resolve_query(query)?;
    let body = build_advanced_table_request(&query, from, to, Some(limit))?;
    let data = execute_async_query(cfg, body, None).await?;
    let rows = columnar_to_rows(&data)?;
    formatter::output(cfg, &rows)
}

/// Transform a DDSQL columnar response into a row-based JSON array.
///
/// The table endpoint returns columns in one of two shapes:
///   Array: {"data": [{"attributes": {"columns": [...]}}]}
///   Object: {"data": {"attributes": {"columns": [...]}}}
///
/// Each column is: {"name": "col1", "values": ["a", "b"]}
///
/// This transforms it to: [{"col1": "a", "col2": 1}, {"col1": "b", "col2": 2}]
fn columnar_to_rows(resp: &Value) -> Result<Value> {
    // Try array shape first (observed in production), then object shape.
    let columns = resp
        .pointer("/data/0/attributes/columns")
        .or_else(|| resp.pointer("/data/attributes/columns"))
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("unexpected response: missing columns in response"))?;

    if columns.is_empty() {
        return Ok(json!([]));
    }

    let num_rows = columns[0]
        .get("values")
        .and_then(Value::as_array)
        .map(|v| v.len())
        .unwrap_or(0);

    let mut rows = Vec::with_capacity(num_rows);
    for i in 0..num_rows {
        let mut row = serde_json::Map::new();
        for col in columns {
            let name = col.get("name").and_then(Value::as_str).unwrap_or("unknown");
            let value = col
                .get("values")
                .and_then(Value::as_array)
                .and_then(|vals| vals.get(i))
                .cloned()
                .unwrap_or(Value::Null);
            row.insert(name.to_string(), value);
        }
        rows.push(Value::Object(row));
    }

    Ok(Value::Array(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_advanced_table_with_limit() {
        let req = build_advanced_table_request("SELECT 1", "1h", "now", Some(10)).unwrap();

        assert_eq!(req["data"]["type"], "analysis_workspace_query_request");
        let attrs = &req["data"]["attributes"];
        assert_eq!(attrs["datasets"][0]["data_source"], "analysis_dataset");
        assert_eq!(attrs["datasets"][0]["name"], "user_query");
        assert_eq!(attrs["datasets"][0]["query"]["type"], "sql_analysis");
        assert_eq!(attrs["datasets"][0]["query"]["sql_query"], "SELECT 1");
        assert_eq!(attrs["query"]["dataset"], "user_query");
        assert_eq!(attrs["query"]["limit"], 10);
        assert!(attrs["query"]["time_window"]["from"].is_i64());
        assert!(attrs["query"]["time_window"]["to"].is_i64());

        let now_ms = chrono::Utc::now().timestamp() * 1000;
        let from = attrs["query"]["time_window"]["from"].as_i64().unwrap();
        let to = attrs["query"]["time_window"]["to"].as_i64().unwrap();
        assert!((to - now_ms).abs() < 2000);
        assert!((from - (now_ms - 3600000)).abs() < 2000);

        assert_eq!(req["meta"]["use_async_querying"], true);
        assert!(req["meta"]["client_id"]
            .as_str()
            .unwrap_or("")
            .starts_with("pup/"));
        assert!(!req["meta"]["user_query_id"]
            .as_str()
            .unwrap_or("")
            .is_empty());
    }

    #[test]
    fn test_build_advanced_table_no_limit() {
        let req = build_advanced_table_request("SELECT 1", "1h", "now", None).unwrap();

        assert_eq!(req["data"]["type"], "analysis_workspace_query_request");
        assert!(
            req["data"]["attributes"]["query"].get("limit").is_none()
                || req["data"]["attributes"]["query"]["limit"].is_null()
        );
    }

    #[test]
    fn test_build_advanced_table_invalid_from() {
        let err = build_advanced_table_request("SELECT 1", "garbage", "now", None).unwrap_err();
        assert!(err.to_string().contains("invalid --from"));
    }

    #[test]
    fn test_resolve_query_accepts_comment_prefix() {
        let query = "-- comment\nSELECT 1";
        assert_eq!(
            resolve_query_from_reader(query, &mut io::empty()).unwrap(),
            query
        );
    }

    #[test]
    fn test_resolve_query_reads_explicit_stdin_marker() {
        let mut stdin = io::Cursor::new("-- comment\nSELECT 1");
        let query = resolve_query_from_reader("-", &mut stdin).unwrap();
        assert_eq!(query, "-- comment\nSELECT 1");
    }

    #[test]
    fn test_columnar_to_rows_array_shape() {
        // Actual production shape: {"data": [{"attributes": {"columns": [...]}}]}
        let resp: Value = serde_json::from_str(
            r#"{"data":[{"attributes":{"columns":[
                {"name":"host","type":"string","values":["h1","h2"]},
                {"name":"cpu","type":"float64","values":[10,20]}
            ]},"id":"ddsql_response","type":"scalar_response"}]}"#,
        )
        .unwrap();
        let rows = columnar_to_rows(&resp).unwrap();
        let arr = rows.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["host"], "h1");
        assert_eq!(arr[0]["cpu"], 10);
        assert_eq!(arr[1]["host"], "h2");
        assert_eq!(arr[1]["cpu"], 20);
    }

    #[test]
    fn test_columnar_to_rows_object_shape() {
        // Fallback shape: {"data": {"attributes": {"columns": [...]}}}
        let resp: Value = serde_json::from_str(
            r#"{"data":{"attributes":{"columns":[
                {"name":"id","values":[42]}
            ]}}}"#,
        )
        .unwrap();
        let rows = columnar_to_rows(&resp).unwrap();
        let arr = rows.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], 42);
    }

    #[test]
    fn test_columnar_to_rows_empty_columns() {
        let resp: Value =
            serde_json::from_str(r#"{"data":[{"attributes":{"columns":[]}}]}"#).unwrap();
        let rows = columnar_to_rows(&resp).unwrap();
        assert_eq!(rows, json!([]));
    }

    #[test]
    fn test_columnar_to_rows_missing_columns() {
        let resp: Value = serde_json::from_str(r#"{"data":[{"attributes":{}}]}"#).unwrap();
        assert!(columnar_to_rows(&resp).is_err());
    }

    #[test]
    fn test_extract_query_status_done() {
        // Old shape (fallback).
        let resp: Value =
            serde_json::from_str(r#"{"meta":{"queries":[{"status":"done","name":"user_query"}]}}"#)
                .unwrap();
        assert!(extract_query_status(&resp).unwrap().is_none());
    }

    #[test]
    fn test_extract_query_status_done_new_shape() {
        // New shape: meta.responses[0].queries[0].
        let resp: Value = serde_json::from_str(
            r#"{"meta":{"responses":[{"queries":[{"status":"done","name":"user_query"}]}]}}"#,
        )
        .unwrap();
        assert!(extract_query_status(&resp).unwrap().is_none());
    }

    #[test]
    fn test_extract_query_status_running() {
        // Old shape (fallback).
        let resp: Value = serde_json::from_str(
            r#"{"meta":{"queries":[{"status":"running","name":"user_query","query_id":"abc-123"}]}}"#,
        )
        .unwrap();
        assert_eq!(
            extract_query_status(&resp).unwrap(),
            Some("abc-123".to_string())
        );
    }

    #[test]
    fn test_extract_query_status_running_new_shape() {
        // New shape: meta.responses[0].queries[0].
        let resp: Value = serde_json::from_str(
            r#"{"meta":{"responses":[{"queries":[{"status":"running","name":"user_query","query_id":"xyz-789"}]}]}}"#,
        )
        .unwrap();
        assert_eq!(
            extract_query_status(&resp).unwrap(),
            Some("xyz-789".to_string())
        );
    }

    #[test]
    fn test_extract_query_status_missing_meta() {
        let resp: Value = serde_json::from_str(r#"{"data":{}}"#).unwrap();
        assert!(extract_query_status(&resp).is_err());
    }

    #[test]
    fn test_extract_query_status_unexpected_status() {
        let resp: Value = serde_json::from_str(
            r#"{"meta":{"queries":[{"status":"failed","name":"user_query"}]}}"#,
        )
        .unwrap();
        let err = extract_query_status(&resp).unwrap_err();
        assert!(err.to_string().contains("unexpected query status"));
    }

    #[test]
    fn test_build_fetch_request_adds_query_id() {
        let base = build_advanced_table_request("SELECT 1", "1h", "now", None).unwrap();
        let fetch = build_fetch_request(&base, "qid-456");
        assert_eq!(fetch["data"]["attributes"]["query_id"], "qid-456");
        // Type must change to advanced_query_fetch_request for the fetch endpoint.
        assert_eq!(fetch["data"]["type"], "advanced_query_fetch_request");
        // Original fields are preserved.
        assert_eq!(
            fetch["data"]["attributes"]["datasets"][0]["query"]["sql_query"],
            "SELECT 1"
        );
    }

    #[test]
    fn test_columnar_to_rows_null_values() {
        let resp: Value = serde_json::from_str(
            r#"{"data":[{"attributes":{"columns":[
                {"name":"a","values":[1,null]},
                {"name":"b","values":[null,"x"]}
            ]}}]}"#,
        )
        .unwrap();
        let rows = columnar_to_rows(&resp).unwrap();
        let arr = rows.as_array().unwrap();
        assert_eq!(arr[0]["a"], 1);
        assert!(arr[0]["b"].is_null());
        assert!(arr[1]["a"].is_null());
        assert_eq!(arr[1]["b"], "x");
    }

    #[test]
    fn test_filter_tables_case_insensitive() {
        let filtered = filter_tables(
            ["aws.ec2_instance", "dd.hosts", "reference_tables.ec2info"],
            Some("EC2"),
        );
        assert_eq!(
            filtered,
            vec![
                "aws.ec2_instance".to_string(),
                "reference_tables.ec2info".to_string()
            ]
        );
    }

    #[test]
    fn test_normalize_ddsql_type() {
        assert_eq!(normalize_ddsql_type("string"), "VARCHAR");
        assert_eq!(normalize_ddsql_type("int64"), "BIGINT");
        assert_eq!(normalize_ddsql_type("bool"), "BOOLEAN");
        assert_eq!(normalize_ddsql_type("json"), "JSON");
        assert_eq!(normalize_ddsql_type("hstore_csv"), "HSTORE");
    }

    #[test]
    fn test_parse_public_columns() {
        let resp: Value = serde_json::from_str(
            r#"{
                "tables": [{
                    "Table": {
                        "TableName": "aws.ec2_instance",
                        "Columns": [
                            {"Name":"\"instance_id\"","Type":"string"},
                            {"Name":"\"launch_time\"","Type":"timestamp"},
                            {"Name":"\"tags\"","Type":"hstore_csv"}
                        ]
                    },
                    "SampleData": ""
                }]
            }"#,
        )
        .unwrap();

        let columns = parse_public_columns(resp).unwrap();
        assert_eq!(
            columns,
            vec![
                DdsqlSchemaColumn {
                    name: "instance_id".to_string(),
                    ty: "VARCHAR".to_string(),
                    raw_type: "string".to_string(),
                    primary_key: false,
                },
                DdsqlSchemaColumn {
                    name: "launch_time".to_string(),
                    ty: "TIMESTAMP".to_string(),
                    raw_type: "timestamp".to_string(),
                    primary_key: false,
                },
                DdsqlSchemaColumn {
                    name: "tags".to_string(),
                    ty: "HSTORE".to_string(),
                    raw_type: "hstore_csv".to_string(),
                    primary_key: false,
                },
            ]
        );
    }

    #[test]
    fn test_parse_reference_table_columns() {
        let resp: Value = serde_json::from_str(
            r#"{
                "data": [
                    {
                        "attributes": {
                            "description": "",
                            "row_count": 1,
                            "source": "LOCAL_FILE",
                            "status": "DONE",
                            "table_name": "other_table",
                            "schema": {
                                "primary_keys": ["ignored"],
                                "fields": [
                                    {"name":"ignored","type":"STRING"}
                                ]
                            }
                        }
                    },
                    {
                        "attributes": {
                            "description": "",
                            "row_count": 2,
                            "source": "LOCAL_FILE",
                            "status": "DONE",
                            "table_name": "ec2info",
                            "schema": {
                                "primary_keys": ["id"],
                                "fields": [
                                    {"name":"id","type":"STRING"},
                                    {"name":"score","type":"INT32"}
                                ]
                            }
                        }
                    }
                ]
            }"#,
        )
        .unwrap();

        let columns = parse_reference_table_columns(resp, "ec2info").unwrap();
        assert_eq!(
            columns,
            vec![
                DdsqlSchemaColumn {
                    name: "id".to_string(),
                    ty: "VARCHAR".to_string(),
                    raw_type: "STRING".to_string(),
                    primary_key: true,
                },
                DdsqlSchemaColumn {
                    name: "score".to_string(),
                    ty: "BIGINT".to_string(),
                    raw_type: "INT32".to_string(),
                    primary_key: false,
                },
            ]
        );
    }

    #[test]
    fn test_parse_reference_table_columns_missing_table() {
        let resp: Value = serde_json::from_str(
            r#"{
                "data": [{
                    "attributes": {
                        "description": "",
                        "row_count": 1,
                        "source": "LOCAL_FILE",
                        "status": "DONE",
                        "table_name": "other_table",
                        "schema": {
                            "primary_keys": [],
                            "fields": []
                        }
                    }
                }]
            }"#,
        )
        .unwrap();

        let err = parse_reference_table_columns(resp, "ec2info").unwrap_err();
        assert!(err
            .to_string()
            .contains("reference table `ec2info` not found"));
    }

    #[test]
    fn test_build_public_table_items() {
        let items = build_public_table_items(
            &["dd.hosts".to_string(), "aws.ec2_instance".to_string()],
            Some("ec2"),
        );
        assert_eq!(
            items,
            vec![DdsqlSchemaTable {
                name: "aws.ec2_instance".to_string(),
                id: "public.aws.ec2_instance".to_string(),
                kind: "public".to_string(),
                provider: "aws".to_string(),
                description: None,
                source: None,
                status: None,
                row_count: None,
            }]
        );
    }

    #[test]
    fn test_normalize_table_lookup() {
        assert_eq!(
            normalize_table_lookup("public.aws.ec2_instance"),
            ("public", "aws.ec2_instance".to_string())
        );
        assert_eq!(
            normalize_table_lookup("reference_tables.ec2info"),
            ("reference_table", "ec2info".to_string())
        );
        assert_eq!(
            normalize_table_lookup("dd.hosts"),
            ("public", "dd.hosts".to_string())
        );
    }
}

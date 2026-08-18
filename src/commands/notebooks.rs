use anyhow::Result;
use datadog_api_client::datadog;
use datadog_api_client::datadogV1::api_notebooks::NotebooksAPI;
use datadog_api_client::datadogV1::model::{NotebookCreateRequest, NotebookUpdateRequest};

use crate::config::Config;
use crate::formatter::{self, Metadata};
use crate::raw_client;
use crate::util;
use crate::util_ext;

const SEARCH_PATH: &str = "/api/v2/notebooks/search";
const MAX_RESULTS: usize = 1000;
const RATE_LIMIT_RETRIES: u32 = 3;

fn compact_validation_details(content: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(content).ok()?;
    let errors = parsed.get("errors")?.as_array()?;
    let mut messages = Vec::new();
    for error in errors {
        let message = error.as_str()?;
        let details = regex::Regex::new(r#"'detail': '((?:\\.|[^'])*)'"#)
            .expect("static detail regex")
            .captures_iter(message)
            .filter_map(|capture| capture.get(1).map(|value| value.as_str()))
            .collect::<Vec<_>>();
        if details.len() >= 4 && details.iter().all(|detail| detail.chars().count() == 1) {
            messages.push(details.concat());
        } else {
            messages.push(message.to_string());
        }
    }
    (!messages.is_empty()).then(|| messages.join("; "))
}

fn notebook_api_error<T: std::fmt::Debug>(
    operation: &str,
    error: datadog::Error<T>,
) -> anyhow::Error {
    match error {
        datadog::Error::ResponseError(response) => {
            let detail = compact_validation_details(&response.content)
                .unwrap_or_else(|| response.content.trim().to_string());
            anyhow::anyhow!(
                "failed to {operation} notebook (HTTP {}): {detail}",
                response.status
            )
        }
        other => anyhow::anyhow!("failed to {operation} notebook: {other}"),
    }
}

fn is_rate_limited(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<raw_client::HttpError>()
        .is_some_and(|error| error.status == 429)
}

async fn get_search_page(cfg: &Config, params: &[(&str, &str)]) -> Result<serde_json::Value> {
    let mut retry = 0;
    loop {
        match raw_client::raw_get(cfg, SEARCH_PATH, params).await {
            Ok(response) => return Ok(response),
            Err(error) if is_rate_limited(&error) && retry < RATE_LIMIT_RETRIES => {
                let delay = 10 * 2_u64.pow(retry);
                retry += 1;
                eprintln!(
                    "Notebook search was rate limited; retrying in {delay}s ({retry}/{RATE_LIMIT_RETRIES})"
                );
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
            }
            Err(error) => return Err(anyhow::anyhow!("failed to search notebooks: {error}")),
        }
    }
}

fn parse_filters(filters: &[String]) -> Result<Vec<(String, String)>> {
    filters
        .iter()
        .flat_map(|filter| filter.split_whitespace())
        .map(|filter| {
            let (field, value) = filter.split_once(':').ok_or_else(|| {
                anyhow::anyhow!(
                    "invalid filter '{filter}': expected FIELD:VALUE (for example, tags:production)"
                )
            })?;
            if field.is_empty() || value.is_empty() {
                anyhow::bail!("invalid filter '{filter}': both FIELD and VALUE must be non-empty");
            }
            Ok((format!("filter[{field}]"), value.to_string()))
        })
        .collect()
}

async fn discover(
    cfg: &Config,
    query: &str,
    filters: &[String],
    sort: &str,
    limit: usize,
    command: &str,
) -> Result<()> {
    if !(1..=MAX_RESULTS).contains(&limit) {
        anyhow::bail!("--limit must be between 1 and {MAX_RESULTS}, got {limit}");
    }
    let filters = parse_filters(filters)?;
    let mut notebooks = Vec::new();

    let page_size = limit.to_string();
    let mut params = vec![
        ("query", query),
        ("sort", sort),
        ("page[size]", page_size.as_str()),
        ("page[number]", "0"),
    ];
    params.extend(
        filters
            .iter()
            .map(|(field, value)| (field.as_str(), value.as_str())),
    );

    let response = get_search_page(cfg, &params).await?;
    let page_data = response
        .get("data")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("notebook search response did not contain data[]"))?;
    let response_meta = response.get("meta").cloned();
    notebooks.extend(page_data.iter().take(limit).cloned());

    let count = notebooks.len();
    let total = response_meta
        .as_ref()
        .and_then(|meta| meta.get("total"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|total| usize::try_from(total).ok());
    let truncated = total.is_some_and(|total| count < total);
    let mut meta = response_meta.unwrap_or_else(|| serde_json::json!({ "total": count }));
    if let Some(meta) = meta.as_object_mut() {
        meta.insert("returned".into(), count.into());
        meta.insert("truncated".into(), truncated.into());
    }
    let payload = serde_json::json!({
        "data": notebooks,
        "meta": meta,
    });
    let metadata = Metadata {
        count: Some(count),
        truncated,
        command: Some(command.to_string()),
        next_action: None,
    };
    formatter::format_and_print(
        &payload,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&metadata),
        cfg.jq.as_deref(),
    )
}

pub async fn search(
    cfg: &Config,
    query: Option<&str>,
    filters: &[String],
    sort: &str,
    limit: usize,
) -> Result<()> {
    discover(
        cfg,
        query.unwrap_or_default(),
        filters,
        sort,
        limit,
        "notebooks search",
    )
    .await
}

pub async fn get(cfg: &Config, notebook_id: i64) -> Result<()> {
    let api = crate::make_api!(NotebooksAPI, cfg);
    let resp = api
        .get_notebook(notebook_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get notebook: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn delete(cfg: &Config, notebook_id: i64) -> Result<()> {
    let api = crate::make_api!(NotebooksAPI, cfg);
    api.delete_notebook(notebook_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete notebook: {e:?}"))?;
    println!("Successfully deleted notebook {notebook_id}");
    Ok(())
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(NotebooksAPI, cfg);
    let body: NotebookCreateRequest = util::read_json_file(file)?;
    let resp = api
        .create_notebook(body)
        .await
        .map_err(|error| notebook_api_error("create", error))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, notebook_id: i64, file: &str) -> Result<()> {
    let api = crate::make_api!(NotebooksAPI, cfg);
    let body: NotebookUpdateRequest = util::read_json_file(file)?;
    let resp = api
        .update_notebook(notebook_id, body)
        .await
        .map_err(|error| notebook_api_error("update", error))?;
    formatter::output(cfg, &resp)
}

pub async fn diff(
    cfg: &Config,
    notebook_id: i64,
    file: &str,
    only: &[String],
    ignore: &[String],
) -> Result<()> {
    let candidate: serde_json::Value = util::read_json_file(file)?;
    let live = raw_client::raw_get(cfg, &format!("/api/v1/notebooks/{notebook_id}"), &[])
        .await
        .map_err(|e| anyhow::anyhow!("failed to get notebook: {e:?}"))?;

    let resource_id = notebook_id.to_string();
    let mut options = util_ext::ResourceDiffOptions::new(
        "notebooks diff",
        "pup notebooks update",
        "notebook",
        &resource_id,
    );
    options.readonly_paths = util_ext::READONLY_NOTEBOOK_FIELDS;
    options.only = only;
    options.ignore = ignore;
    options.no_changes_message = Some(format!("No changes - notebook {notebook_id} is in sync."));
    util_ext::format_resource_diff(cfg, &live, &candidate, &options)
}

/// Append-only update: fetches the current notebook, appends cells from
/// `file` (an array of cell objects), then writes the full modified notebook back.
pub async fn edit(cfg: &Config, notebook_id: i64, file: &str) -> Result<()> {
    let api = crate::make_api!(NotebooksAPI, cfg);

    // Fetch current notebook so we can append without clobbering existing cells.
    let current = api
        .get_notebook(notebook_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to fetch notebook {notebook_id}: {e:?}"))?;

    // Serialize current notebook to Value so we can manipulate cells generically.
    let mut nb: serde_json::Value = serde_json::to_value(&current)
        .map_err(|e| anyhow::anyhow!("failed to serialize notebook: {e:?}"))?;

    // Read the new cells to append from the file (expected: array of cell objects).
    let new_cells: serde_json::Value = util::read_json_file(file)?;
    let new_cells_arr = new_cells
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("--file must contain a JSON array of cell objects"))?;

    // Append new cells to the existing cells array.
    let cells = nb
        .pointer_mut("/data/attributes/cells")
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| anyhow::anyhow!("could not locate cells array in notebook response"))?;
    cells.extend(new_cells_arr.iter().cloned());

    // Write back via the typed update endpoint.
    let update_body: NotebookUpdateRequest = serde_json::from_value(
        nb.get("data")
            .cloned()
            .map(|data| serde_json::json!({ "data": data }))
            .unwrap_or(nb.clone()),
    )
    .map_err(|e| anyhow::anyhow!("failed to build update request: {e:?}"))?;

    let resp = api
        .update_notebook(notebook_id, update_body)
        .await
        .map_err(|error| notebook_api_error("edit", error))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;
    use mockito::Matcher;

    #[test]
    fn test_compact_validation_details_reassembles_character_errors() {
        let content = serde_json::json!({
            "errors": [
                "API input validation failed: {'cells': {3: {'errors': [\
                 {'detail': 'b', 'source': {'pointer': '/data/attributes/definition/requests'}}, \
                 {'detail': 'a', 'source': {'pointer': '/data/attributes/definition/requests'}}, \
                 {'detail': 'd', 'source': {'pointer': '/data/attributes/definition/requests'}}, \
                 {'detail': '!', 'source': {'pointer': '/data/attributes/definition/requests'}}]}}}"
            ]
        })
        .to_string();

        assert_eq!(
            super::compact_validation_details(&content).as_deref(),
            Some("bad!")
        );
    }

    #[test]
    fn test_compact_validation_details_preserves_normal_error() {
        let content = serde_json::json!({"errors": ["Invalid notebook type"]}).to_string();

        assert_eq!(
            super::compact_validation_details(&content).as_deref(),
            Some("Invalid notebook type")
        );
    }

    #[tokio::test]
    async fn test_notebooks_search_without_query_sends_filters_sort_and_default_page_size() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let mock = s
            .mock("GET", super::SEARCH_PATH)
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("query".into(), "".into()),
                Matcher::UrlEncoded("sort".into(), "-modified_at".into()),
                Matcher::UrlEncoded("page[size]".into(), "20".into()),
                Matcher::UrlEncoded("page[number]".into(), "0".into()),
                Matcher::UrlEncoded("filter[tags]".into(), "production".into()),
                Matcher::UrlEncoded("filter[deleted]".into(), "false".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[{"id":"123"}],"meta":{"total":1}}"#)
            .create_async()
            .await;

        super::search(
            &cfg,
            None,
            &["tags:production deleted:false".into()],
            "-modified_at",
            20,
        )
        .await
        .unwrap();
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_notebooks_diff_detects_changes() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/v1/notebooks/12345")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "data": {
                        "id": "12345",
                        "type": "notebooks",
                        "attributes": {
                            "name": "Old Notebook",
                            "cells": [],
                            "modified": "2024-01-01T00:00:00Z"
                        }
                    }
                }"#,
            )
            .create_async()
            .await;
        let path = write_temp_json(
            "pup_notebook_diff_detects_changes.json",
            r#"{
                "data": {
                    "attributes": {
                        "name": "New Notebook",
                        "cells": []
                    }
                }
            }"#,
        );

        let result = super::diff(&cfg, 12345, path.to_str().unwrap(), &[], &[]).await;
        let _ = std::fs::remove_file(path);
        assert!(result.is_ok(), "notebooks diff failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_notebooks_search_without_query_uses_requested_limit_as_page_size() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let response = serde_json::json!({
            "data": (0..21)
                .map(|id| serde_json::json!({ "id": id.to_string() }))
                .collect::<Vec<_>>(),
            "meta": { "total": 21 },
        });
        let mock = s
            .mock("GET", super::SEARCH_PATH)
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("page[size]".into(), "21".into()),
                Matcher::UrlEncoded("page[number]".into(), "0".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(response.to_string())
            .create_async()
            .await;

        super::search(&cfg, None, &[], "name", 21).await.unwrap();
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_notebooks_diff_invalid_json() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused.local");
        let path = write_temp_json("pup_notebook_diff_invalid_json.json", "not valid json {{{");

        let result = super::diff(&cfg, 12345, path.to_str().unwrap(), &[], &[]).await;
        let _ = std::fs::remove_file(path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to parse JSON"));
        cleanup_env();
    }

    #[tokio::test]
    async fn test_notebooks_search_sends_content_query() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let mock = s
            .mock("GET", super::SEARCH_PATH)
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("query".into(), "unique cell text".into()),
                Matcher::UrlEncoded(
                    "filter[metadata.has_computational_cells]".into(),
                    "false".into(),
                ),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[{"id":"456"}],"meta":{"total":1}}"#)
            .create_async()
            .await;

        super::search(
            &cfg,
            Some("unique cell text"),
            &["metadata.has_computational_cells:false".into()],
            "name",
            20,
        )
        .await
        .unwrap();
        mock.assert_async().await;
        cleanup_env();
    }

    #[test]
    fn test_parse_filters_rejects_missing_separator() {
        let err = super::parse_filters(&["tags".into()]).unwrap_err();
        assert!(err.to_string().contains("expected FIELD:VALUE"));
    }

    #[test]
    fn test_parse_filters_preserves_colons_in_values() {
        let filters = super::parse_filters(&["tags:pup-eval:abc".into()]).unwrap();
        assert_eq!(
            filters,
            vec![("filter[tags]".into(), "pup-eval:abc".into())]
        );
    }

    #[test]
    fn test_is_rate_limited_only_matches_http_429() {
        let error = anyhow::Error::new(crate::raw_client::HttpError {
            status: 429,
            method: "GET".into(),
            url: "https://example.test".into(),
            body: "rate limited".into(),
        });
        assert!(super::is_rate_limited(&error));
        assert!(!super::is_rate_limited(&anyhow::anyhow!("other error")));
    }
}

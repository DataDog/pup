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

// Not yet promoted to /api/v2, so the path and response contract may change.
const MARKDOWN_BASE: &str = "/api/unstable/notebooks";
const MARKDOWN_MEDIA_TYPE: &str = "text/markdown";
const JSONAPI_MEDIA_TYPE: &str = "application/vnd.api+json";

async fn markdown_request(
    cfg: &Config,
    method: &str,
    path: &str,
    body: Option<String>,
    accept: &str,
) -> Result<raw_client::HttpResponse> {
    let content_type = body.is_some().then_some(MARKDOWN_MEDIA_TYPE);
    let body_bytes = body.map(String::into_bytes);
    raw_client::raw_request(
        cfg,
        method,
        path,
        &[],
        body_bytes,
        content_type,
        accept,
        &[],
    )
    .await
    .map_err(|error| translate_markdown_error(error, method))
}

fn translate_markdown_error(error: anyhow::Error, method: &str) -> anyhow::Error {
    // Keyed on the detail string, not the status: seen as both 500 and 502.
    if !error
        .to_string()
        .contains("Unable to render the notebook as Markdown")
    {
        return error;
    }
    if method.eq_ignore_ascii_case("GET") {
        return error.context(
            "this notebook has no Markdown representation \
             (notebooks created through the older cells API cannot be projected); \
             use the JSON form of this command instead",
        );
    }
    // The render runs after the mutation, so this does not prove the write failed.
    error.context(
        "the notebook could not be rendered as Markdown after the write, \
         so it is unknown whether the change was applied; \
         check the notebook before retrying, and use the JSON form of this command for it",
    )
}

fn media_type_of(content_type: &str) -> String {
    content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn applied_write_context(error: anyhow::Error, operation: &str) -> anyhow::Error {
    error.context(format!(
        "the {operation} was accepted but its response could not be read, \
         so the notebook has probably changed; check it before retrying"
    ))
}

fn created_notebook_id(resp: raw_client::HttpResponse) -> Result<String> {
    // JSON:API is negotiated because the Markdown projection carries no id.
    let media_type = media_type_of(&resp.content_type);
    if media_type != JSONAPI_MEDIA_TYPE && media_type != "application/json" {
        anyhow::bail!(
            "expected a JSON:API response but the server returned {:?}",
            resp.content_type
        );
    }
    let body: serde_json::Value = serde_json::from_slice(&resp.bytes)
        .map_err(|e| anyhow::anyhow!("failed to parse the notebook create response: {e}"))?;
    body.pointer("/data/id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("notebook create response did not contain data.id"))
}

fn decode_markdown_response(resp: raw_client::HttpResponse) -> Result<String> {
    if media_type_of(&resp.content_type) != MARKDOWN_MEDIA_TYPE {
        anyhow::bail!(
            "expected a Markdown response but the server returned {:?}",
            resp.content_type
        );
    }
    if resp.bytes.iter().all(u8::is_ascii_whitespace) {
        anyhow::bail!("the server returned an empty Markdown document");
    }
    String::from_utf8(resp.bytes)
        .map_err(|e| anyhow::anyhow!("notebook Markdown response was not valid UTF-8: {e}"))
}

fn read_markdown_file(file: &str) -> Result<String> {
    // The endpoints take a JSON file verbatim as prose, so catch it here.
    let content =
        std::fs::read_to_string(file).map_err(|e| anyhow::anyhow!("failed to read {file}: {e}"))?;
    // A byte-order mark would otherwise survive `trim` and hide the leading
    // brace from the JSON check below.
    let trimmed = content.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        anyhow::bail!("{file} is empty");
    }
    // Bare scalars parse as JSON too, so require a leading brace or bracket
    // before treating the file as a misrouted JSON document.
    let looks_like_json = trimmed.starts_with('{') || trimmed.starts_with('[');
    if looks_like_json && serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
        anyhow::bail!("{file} contains JSON, not Markdown; drop --markdown to use the JSON API");
    }
    Ok(content)
}

fn compact_validation_details(content: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(content).ok()?;
    let errors = parsed.get("errors")?.as_array()?;
    let detail_pattern =
        regex::Regex::new(r#"'detail': '((?:\\.|[^'])*)'"#).expect("static detail regex");
    let mut messages = Vec::new();
    for error in errors {
        let message = error.as_str()?;
        let details = detail_pattern
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

    let response = raw_client::raw_get(cfg, SEARCH_PATH, &params)
        .await
        .map_err(|error| anyhow::anyhow!("failed to search notebooks: {error}"))?;
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

pub async fn get(cfg: &Config, notebook_id: i64, markdown: bool) -> Result<()> {
    if markdown {
        let path = format!("{MARKDOWN_BASE}/{notebook_id}");
        let resp = markdown_request(cfg, "GET", &path, None, MARKDOWN_MEDIA_TYPE).await?;
        util_ext::print_text_document(&decode_markdown_response(resp)?);
        return Ok(());
    }
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

pub async fn create(cfg: &Config, file: &str, markdown: bool) -> Result<()> {
    if markdown {
        let content = read_markdown_file(file)?;
        let resp = markdown_request(
            cfg,
            "POST",
            MARKDOWN_BASE,
            Some(content),
            JSONAPI_MEDIA_TYPE,
        )
        .await?;
        println!("{}", created_notebook_id(resp)?);
        return Ok(());
    }
    let api = crate::make_api!(NotebooksAPI, cfg);
    let body: NotebookCreateRequest = util::read_json_file(file)?;
    let resp = api
        .create_notebook(body)
        .await
        .map_err(|error| notebook_api_error("create", error))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, notebook_id: i64, file: &str, markdown: bool) -> Result<()> {
    if markdown {
        let content = read_markdown_file(file)?;
        let path = format!("{MARKDOWN_BASE}/{notebook_id}");
        let resp =
            markdown_request(cfg, "PATCH", &path, Some(content), MARKDOWN_MEDIA_TYPE).await?;
        let document =
            decode_markdown_response(resp).map_err(|e| applied_write_context(e, "update"))?;
        util_ext::print_text_document(&document);
        return Ok(());
    }
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
pub async fn edit(cfg: &Config, notebook_id: i64, file: &str, markdown: bool) -> Result<()> {
    if markdown {
        let content = read_markdown_file(file)?;
        let path = format!("{MARKDOWN_BASE}/{notebook_id}/content");
        let resp = markdown_request(cfg, "POST", &path, Some(content), MARKDOWN_MEDIA_TYPE).await?;
        let document =
            decode_markdown_response(resp).map_err(|e| applied_write_context(e, "append"))?;
        util_ext::print_text_document(&document);
        return Ok(());
    }
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

    fn markdown_response(content_type: &str, body: &str) -> super::raw_client::HttpResponse {
        super::raw_client::HttpResponse {
            content_type: content_type.to_string(),
            bytes: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn test_decode_markdown_response_returns_body() {
        let resp = markdown_response("text/markdown; charset=utf-8", "## hi\n");
        assert_eq!(super::decode_markdown_response(resp).unwrap(), "## hi\n");
    }

    #[test]
    fn test_decode_markdown_response_rejects_json() {
        let resp = markdown_response("application/vnd.api+json", r#"{"data":{}}"#);
        let err = super::decode_markdown_response(resp)
            .unwrap_err()
            .to_string();
        assert!(err.contains("expected a Markdown response"), "got: {err}");
    }

    #[test]
    fn test_decode_markdown_response_rejects_empty_body() {
        let resp = markdown_response("text/markdown", "  \n ");
        let err = super::decode_markdown_response(resp)
            .unwrap_err()
            .to_string();
        assert!(err.contains("empty Markdown document"), "got: {err}");
    }

    #[test]
    fn test_decode_markdown_response_rejects_absent_content_type() {
        let resp = markdown_response("", "## hi");
        let err = super::decode_markdown_response(resp)
            .unwrap_err()
            .to_string();
        assert!(err.contains("expected a Markdown response"), "got: {err}");
    }

    #[test]
    fn test_translate_markdown_error_warns_write_outcome_is_unknown() {
        for method in ["POST", "PATCH"] {
            let raw = anyhow::anyhow!(
                "{method} /api/unstable/notebooks/1 failed (HTTP 500): \
                 Unable to render the notebook as Markdown."
            );
            let translated = super::translate_markdown_error(raw, method).to_string();
            assert!(
                translated.contains("unknown whether the change was applied"),
                "{method} not warned: {translated}"
            );
        }
    }

    #[test]
    fn test_created_notebook_id_extracts_id() {
        let resp = super::raw_client::HttpResponse {
            content_type: "application/vnd.api+json".to_string(),
            bytes: br###"{"data":{"id":"987654","attributes":{"markdown":"## created\n"}}}"###
                .to_vec(),
        };
        assert_eq!(super::created_notebook_id(resp).unwrap(), "987654");
    }

    #[test]
    fn test_applied_write_context_warns_the_change_may_have_landed() {
        let wrapped = super::applied_write_context(anyhow::anyhow!("bad body"), "create");
        assert!(
            wrapped.to_string().contains("check it before retrying"),
            "got: {wrapped}"
        );
        assert!(format!("{wrapped:#}").contains("bad body"), "source lost");
    }

    #[test]
    fn test_created_notebook_id_rejects_neighbouring_json_media_type() {
        let resp = markdown_response("text/json-garbage", r#"{"data":{"id":"1"}}"#);
        let err = super::created_notebook_id(resp).unwrap_err().to_string();
        assert!(err.contains("expected a JSON:API response"), "got: {err}");
    }

    #[test]
    fn test_created_notebook_id_rejects_markdown_response() {
        let resp = markdown_response("text/markdown", "## created\n");
        let err = super::created_notebook_id(resp).unwrap_err().to_string();
        assert!(err.contains("expected a JSON:API response"), "got: {err}");
    }

    #[test]
    fn test_created_notebook_id_reports_missing_id() {
        for (body, want) in [
            (br#"{}"#.to_vec(), "did not contain data"),
            (
                br#"{"data":{"attributes":{"markdown":"x"}}}"#.to_vec(),
                "data.id",
            ),
        ] {
            let resp = super::raw_client::HttpResponse {
                content_type: "application/vnd.api+json".to_string(),
                bytes: body,
            };
            let err = super::created_notebook_id(resp).unwrap_err().to_string();
            assert!(err.contains(want), "expected {want:?}, got: {err}");
        }
    }

    #[tokio::test]
    async fn test_notebooks_create_markdown_negotiates_jsonapi_for_the_id() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("POST", "/api/unstable/notebooks")
            .match_header("content-type", "text/markdown")
            .match_header("accept", "application/vnd.api+json")
            .match_body("## created\n")
            .with_status(201)
            .with_header("content-type", "application/vnd.api+json")
            .with_body(r###"{"data":{"id":"987654","attributes":{"markdown":"## created\n"}}}"###)
            .create_async()
            .await;

        let path = write_temp_json("pup_nb_create_id.md", "## created\n");
        let result = super::create(&cfg, path.to_str().unwrap(), true).await;
        let _ = std::fs::remove_file(path);

        assert!(result.is_ok(), "create failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[test]
    fn test_decode_markdown_response_rejects_neighbouring_media_type() {
        let resp = markdown_response("text/markdown-json", "## hi");
        let err = super::decode_markdown_response(resp)
            .unwrap_err()
            .to_string();
        assert!(err.contains("expected a Markdown response"), "got: {err}");
    }

    #[test]
    fn test_read_markdown_file_rejects_bom_prefixed_json() {
        let path = write_temp_json("pup_nb_bom_json.json", "\u{feff}{\"data\":{\"id\":\"1\"}}");
        let err = super::read_markdown_file(path.to_str().unwrap())
            .unwrap_err()
            .to_string();
        let _ = std::fs::remove_file(path);
        assert!(err.contains("contains JSON, not Markdown"), "got: {err}");
    }

    #[test]
    fn test_read_markdown_file_reports_missing_file() {
        let err = super::read_markdown_file("/nonexistent/pup-nb-missing.md")
            .unwrap_err()
            .to_string();
        assert!(err.contains("failed to read"), "got: {err}");
    }

    #[test]
    fn test_decode_markdown_response_accepts_uppercase_content_type() {
        let resp = markdown_response("Text/Markdown; charset=utf-8", "## hi\n");
        assert_eq!(super::decode_markdown_response(resp).unwrap(), "## hi\n");
    }

    #[test]
    fn test_translate_markdown_error_preserves_original_on_chain() {
        let raw = anyhow::anyhow!(
            "GET /api/unstable/notebooks/1 failed (HTTP 500): \
             Unable to render the notebook as Markdown."
        );
        let translated = super::translate_markdown_error(raw, "GET");
        assert!(translated
            .to_string()
            .contains("no Markdown representation"));
        let chain = format!("{translated:#}");
        assert!(chain.contains("HTTP 500"), "source lost: {chain}");
    }

    #[test]
    fn test_decode_markdown_response_rejects_invalid_utf8() {
        let resp = super::raw_client::HttpResponse {
            content_type: "text/markdown".to_string(),
            bytes: vec![0xff, 0xfe],
        };
        let err = super::decode_markdown_response(resp)
            .unwrap_err()
            .to_string();
        assert!(err.contains("not valid UTF-8"), "got: {err}");
    }

    #[test]
    fn test_translate_markdown_error_explains_unprojectable_notebook() {
        // The backend has returned this failure as both 500 and 502, so the
        // translation must key on the detail string, not the status code.
        for status in ["500", "502"] {
            let raw = anyhow::anyhow!(
                "GET https://api.datadoghq.com/api/unstable/notebooks/1 failed (HTTP {status}): \
                 {{\"errors\":[{{\"detail\":\"Unable to render the notebook as Markdown.\"}}]}}"
            );
            let translated = super::translate_markdown_error(raw, "GET").to_string();
            assert!(
                translated.contains("no Markdown representation"),
                "status {status} not translated: {translated}"
            );
        }
    }

    #[test]
    fn test_translate_markdown_error_passes_other_errors_through() {
        let raw = anyhow::anyhow!("HTTP 404 not found");
        assert_eq!(
            super::translate_markdown_error(raw, "GET").to_string(),
            "HTTP 404 not found"
        );
    }

    #[test]
    fn test_read_markdown_file_rejects_json_document() {
        let path = write_temp_json("pup_nb_md_rejects_json.json", r#"{"data":{"id":"1"}}"#);
        let err = super::read_markdown_file(path.to_str().unwrap())
            .unwrap_err()
            .to_string();
        let _ = std::fs::remove_file(path);
        assert!(err.contains("contains JSON, not Markdown"), "got: {err}");
    }

    #[test]
    fn test_read_markdown_file_allows_bare_scalar_markdown() {
        // `42` parses as valid JSON but is legitimate Markdown prose.
        let path = write_temp_json("pup_nb_md_bare_scalar.md", "42");
        let content = super::read_markdown_file(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(path);
        assert_eq!(content, "42");
    }

    #[test]
    fn test_read_markdown_file_rejects_empty() {
        let path = write_temp_json("pup_nb_md_empty.md", "   \n");
        let err = super::read_markdown_file(path.to_str().unwrap())
            .unwrap_err()
            .to_string();
        let _ = std::fs::remove_file(path);
        assert!(err.contains("is empty"), "got: {err}");
    }

    #[tokio::test]
    async fn test_notebooks_get_markdown_requests_markdown_media_type() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/unstable/notebooks/123")
            .match_header("accept", "text/markdown")
            .with_status(200)
            .with_header("content-type", "text/markdown; charset=utf-8")
            .with_body("---\ntitle: test\n---\n\n## hi\n")
            .create_async()
            .await;

        super::get(&cfg, 123, true).await.unwrap();
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_notebooks_get_markdown_translates_unprojectable_notebook() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/unstable/notebooks/123")
            .with_status(500)
            .with_header("content-type", "application/vnd.api+json")
            .with_body(r#"{"errors":[{"detail":"Unable to render the notebook as Markdown."}]}"#)
            .create_async()
            .await;

        let err = super::get(&cfg, 123, true).await.unwrap_err().to_string();
        mock.assert_async().await;
        assert!(err.contains("no Markdown representation"), "got: {err}");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_notebooks_create_markdown_posts_document() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("POST", "/api/unstable/notebooks")
            .match_header("content-type", "text/markdown")
            .match_header("accept", "application/vnd.api+json")
            .match_body("## new notebook\n")
            .with_status(201)
            .with_header("content-type", "application/vnd.api+json")
            .with_body(r###"{"data":{"id":"1","attributes":{"markdown":"## new notebook\n"}}}"###)
            .create_async()
            .await;

        let path = write_temp_json("pup_nb_create_md.md", "## new notebook\n");
        let result = super::create(&cfg, path.to_str().unwrap(), true).await;
        let _ = std::fs::remove_file(path);

        assert!(result.is_ok(), "create failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_notebooks_update_markdown_patches_document() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("PATCH", "/api/unstable/notebooks/123")
            .match_header("content-type", "text/markdown")
            .match_header("accept", "text/markdown")
            .match_body("## replaced\n")
            .with_status(200)
            .with_header("content-type", "text/markdown; charset=utf-8")
            .with_body("## replaced\n")
            .create_async()
            .await;

        let path = write_temp_json("pup_nb_update_md.md", "## replaced\n");
        let result = super::update(&cfg, 123, path.to_str().unwrap(), true).await;
        let _ = std::fs::remove_file(path);

        assert!(result.is_ok(), "update failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_notebooks_edit_markdown_appends_to_content_endpoint() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("POST", "/api/unstable/notebooks/123/content")
            .match_header("content-type", "text/markdown")
            .match_header("accept", "text/markdown")
            .match_body("## appended\n")
            .with_status(200)
            .with_header("content-type", "text/markdown; charset=utf-8")
            .with_body("## existing\n\n## appended\n")
            .create_async()
            .await;

        let path = write_temp_json("pup_nb_edit_md.md", "## appended\n");
        let result = super::edit(&cfg, 123, path.to_str().unwrap(), true).await;
        let _ = std::fs::remove_file(path);

        assert!(result.is_ok(), "edit failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_notebooks_markdown_rejects_json_file_before_any_request() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("POST", "/api/unstable/notebooks")
            .expect(0)
            .create_async()
            .await;

        let path = write_temp_json("pup_nb_wrong_flag.json", r#"{"data":{"id":"1"}}"#);
        let result = super::create(&cfg, path.to_str().unwrap(), true).await;
        let _ = std::fs::remove_file(path);

        assert!(result.is_err());
        mock.assert_async().await;
        cleanup_env();
    }

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
}

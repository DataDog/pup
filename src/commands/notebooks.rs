use anyhow::Result;
use datadog_api_client::datadogV1::api_notebooks::{ListNotebooksOptionalParams, NotebooksAPI};
use datadog_api_client::datadogV1::model::{NotebookCreateRequest, NotebookUpdateRequest};

use crate::config::Config;
use crate::formatter;
use crate::util;

pub async fn list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(NotebooksAPI, cfg);
    let resp = api
        .list_notebooks(ListNotebooksOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list notebooks: {e:?}"))?;
    formatter::output(cfg, &resp)
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
        .map_err(|e| anyhow::anyhow!("failed to create notebook: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, notebook_id: i64, file: &str) -> Result<()> {
    let api = crate::make_api!(NotebooksAPI, cfg);
    let body: NotebookUpdateRequest = util::read_json_file(file)?;
    let resp = api
        .update_notebook(notebook_id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update notebook: {e:?}"))?;
    formatter::output(cfg, &resp)
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
        .map_err(|e| anyhow::anyhow!("failed to edit notebook: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_notebooks_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::list(&cfg).await;
        cleanup_env();
    }
}

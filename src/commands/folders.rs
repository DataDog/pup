//! Notebook folders use the preview collections API. The UI calls them folders,
//! while the API continues to use collection IDs and paths.

use anyhow::Result;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::config::Config;
use crate::{formatter, raw_client};

const BASE_PATH: &str = "/api/unstable/collections";

fn validate_text(value: &str, field: &str, max_length: usize) -> Result<()> {
    let length = value.chars().count();
    if length > max_length {
        anyhow::bail!("{field} must contain at most {max_length} characters");
    }
    if field == "name" && value.trim().is_empty() {
        anyhow::bail!("name cannot be blank");
    }
    Ok(())
}

async fn write(cfg: &Config, method: &str, path: &str, body: Value) -> Result<Value> {
    let response = raw_client::raw_request(
        cfg,
        method,
        path,
        &[],
        Some(serde_json::to_vec(&body)?),
        Some("application/json"),
        "application/json",
        &[],
    )
    .await?;
    raw_client::parse_response_bytes(&response.bytes)
}

pub async fn list(
    cfg: &Config,
    page: u32,
    page_size: u32,
    query: Option<&str>,
    root_only: bool,
) -> Result<()> {
    if let Some(query) = query {
        if query.chars().count() > 255 {
            anyhow::bail!("query must be at most 255 characters");
        }
    }
    let page = page.to_string();
    let page_size = page_size.to_string();
    let mut params = vec![
        ("page[number]", page.as_str()),
        ("page[size]", page_size.as_str()),
    ];
    if let Some(query) = query {
        params.push(("filter[q]", query));
    }
    if root_only {
        params.push(("filter[root_only]", "true"));
    }
    let response = raw_client::raw_get(cfg, BASE_PATH, &params).await?;
    formatter::output(cfg, &response)
}

pub async fn get(cfg: &Config, folder_id: Uuid, depth: u32) -> Result<()> {
    let path = format!("{BASE_PATH}/{folder_id}");
    let depth = depth.to_string();
    let response = raw_client::raw_get(cfg, &path, &[("depth", depth.as_str())]).await?;
    formatter::output(cfg, &response)
}

pub async fn create(cfg: &Config, name: &str, description: Option<&str>) -> Result<()> {
    validate_text(name, "name", 255)?;
    if let Some(description) = description {
        validate_text(description, "description", 1000)?;
    }
    let mut body = Map::new();
    body.insert("name".into(), Value::String(name.into()));
    if let Some(description) = description {
        body.insert("description".into(), Value::String(description.into()));
    }
    let response = write(cfg, "POST", BASE_PATH, Value::Object(body)).await?;
    formatter::output(cfg, &response)
}

pub async fn update(
    cfg: &Config,
    folder_id: Uuid,
    name: Option<&str>,
    description: Option<&str>,
    clear_description: bool,
) -> Result<()> {
    if name.is_none() && description.is_none() && !clear_description {
        anyhow::bail!("provide --name, --description, or --clear-description");
    }
    let mut body = Map::new();
    if let Some(name) = name {
        validate_text(name, "name", 255)?;
        body.insert("name".into(), Value::String(name.into()));
    }
    if let Some(description) = description {
        validate_text(description, "description", 1000)?;
        body.insert("description".into(), Value::String(description.into()));
    } else if clear_description {
        body.insert("description".into(), Value::Null);
    }
    let path = format!("{BASE_PATH}/{folder_id}");
    let response = write(cfg, "PATCH", &path, Value::Object(body)).await?;
    formatter::output(cfg, &response)
}

pub async fn delete(cfg: &Config, folder_id: Uuid) -> Result<()> {
    let path = format!("{BASE_PATH}/{folder_id}");
    raw_client::raw_request(
        cfg,
        "DELETE",
        &path,
        &[],
        None,
        None,
        "application/json",
        &[],
    )
    .await?;
    formatter::output_no_content(
        cfg,
        "notebooks folders delete",
        Some(&folder_id.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{cleanup_env, lock_env, test_config};

    const ID: &str = "123e4567-e89b-12d3-a456-426614174000";

    #[tokio::test]
    async fn list_sends_filters_and_page() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", BASE_PATH)
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("page[number]".into(), "2".into()),
                mockito::Matcher::UrlEncoded("page[size]".into(), "50".into()),
                mockito::Matcher::UrlEncoded("filter[q]".into(), "team".into()),
                mockito::Matcher::UrlEncoded("filter[root_only]".into(), "true".into()),
            ]))
            .with_status(200)
            .with_body(r#"{"data":[],"meta":{"page":{"total_count":0}}}"#)
            .create_async()
            .await;
        assert!(list(&cfg, 2, 50, Some("team"), true).await.is_ok());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn get_requests_nested_depth() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", format!("{BASE_PATH}/{ID}").as_str())
            .match_query(mockito::Matcher::UrlEncoded("depth".into(), "3".into()))
            .with_status(200)
            .with_body(r#"{"data":{"id":"123e4567-e89b-12d3-a456-426614174000"}}"#)
            .create_async()
            .await;
        assert!(get(&cfg, Uuid::parse_str(ID).unwrap(), 3).await.is_ok());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn create_posts_plain_collection_body() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("POST", BASE_PATH)
            .match_body(mockito::Matcher::Json(
                serde_json::json!({"name":"Team", "description":"Reports"}),
            ))
            .with_status(200)
            .with_body(r#"{"data":{"id":"123e4567-e89b-12d3-a456-426614174000"}}"#)
            .create_async()
            .await;
        assert!(create(&cfg, "Team", Some("Reports")).await.is_ok());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn update_can_clear_description() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("PATCH", format!("{BASE_PATH}/{ID}").as_str())
            .match_body(mockito::Matcher::Json(
                serde_json::json!({"description":null}),
            ))
            .with_status(200)
            .with_body(r#"{"data":{"id":"123e4567-e89b-12d3-a456-426614174000"}}"#)
            .create_async()
            .await;
        assert!(update(&cfg, Uuid::parse_str(ID).unwrap(), None, None, true)
            .await
            .is_ok());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn delete_accepts_empty_success_response() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("DELETE", format!("{BASE_PATH}/{ID}").as_str())
            .with_status(204)
            .create_async()
            .await;
        assert!(delete(&cfg, Uuid::parse_str(ID).unwrap()).await.is_ok());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn invalid_input_and_server_errors_are_reported() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        assert!(create(&cfg, "  ", None).await.is_err());
        assert!(
            update(&cfg, Uuid::parse_str(ID).unwrap(), None, None, false)
                .await
                .is_err()
        );
        let mock = server
            .mock("GET", format!("{BASE_PATH}/{ID}").as_str())
            .match_query(mockito::Matcher::Any)
            .with_status(404)
            .with_body("folder unavailable")
            .create_async()
            .await;
        let error = get(&cfg, Uuid::parse_str(ID).unwrap(), 1)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("HTTP 404"));
        mock.assert_async().await;
        cleanup_env();
    }
}

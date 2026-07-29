use anyhow::Result;
use datadog_api_client::datadogV2::api_datasets::DatasetsAPI;

use crate::config::Config;
use crate::formatter;
use crate::util;

fn make_api(cfg: &Config) -> DatasetsAPI {
    crate::make_api!(DatasetsAPI, cfg)
}

pub async fn list(cfg: &Config) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_all_datasets()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list datasets: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get(cfg: &Config, dataset_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_dataset(dataset_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get dataset: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::DatasetCreateRequest =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_dataset(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create dataset: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, dataset_id: &str, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::DatasetUpdateRequest =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .update_dataset(dataset_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update dataset: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn delete(cfg: &Config, dataset_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.delete_dataset(dataset_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete dataset: {e:?}"))?;
    eprintln!("Dataset {dataset_id} deleted.");
    Ok(())
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_datasets_list() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
        let result = super::list(&cfg).await;
        assert!(result.is_ok(), "datasets list failed: {:?}", result.err());
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_datasets_list_accepts_oauth_bearer_token() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        // Simulate OAuth-only auth: bearer token configured, no API/APP keys.
        cfg.api_key = None;
        cfg.app_key = None;
        cfg.access_token = Some("oauth-bearer-token".into());
        std::env::remove_var("DD_API_KEY");
        std::env::remove_var("DD_APP_KEY");

        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_header("Authorization", "Bearer oauth-bearer-token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        let result = super::list(&cfg).await;
        assert!(
            result.is_ok(),
            "datasets list with OAuth bearer failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_datasets_get() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{}"#).await;
        let result = super::get(&cfg, "test-id").await;
        assert!(result.is_ok(), "datasets get failed: {:?}", result.err());
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_datasets_delete() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "DELETE", "").await;
        let result = super::delete(&cfg, "test-id").await;
        assert!(result.is_ok(), "datasets delete failed: {:?}", result.err());
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }
}

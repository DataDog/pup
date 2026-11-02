use crate::config::Config;
use crate::formatter;
use anyhow::Result;
use datadog_api_client::datadogV2::api_elastic_cloud_integration::ElasticCloudIntegrationAPI;

use datadog_api_client::datadogV2::model::{
    ElasticCloudIntegrationAccountCreateRequest, ElasticCloudIntegrationAccountUpdateRequest,
};

const ACCOUNTS_COLUMNS: &[&str] = &["id", "attributes.name"];

pub async fn accounts_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(ElasticCloudIntegrationAPI, cfg);
    let resp = api
        .list_elastic_cloud_integration_accounts()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Elastic Cloud accounts: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).rows_at("/data"),
    )
}

pub async fn accounts_get(cfg: &Config, account_id: &str) -> Result<()> {
    let api = crate::make_api!(ElasticCloudIntegrationAPI, cfg);
    let resp = api
        .get_elastic_cloud_integration_account(account_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get Elastic Cloud account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).row_at("/data"),
    )
}

pub async fn accounts_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(ElasticCloudIntegrationAPI, cfg);
    let body: ElasticCloudIntegrationAccountCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_elastic_cloud_integration_account(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create Elastic Cloud account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).row_at("/data"),
    )
}

pub async fn accounts_update(cfg: &Config, account_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(ElasticCloudIntegrationAPI, cfg);
    let body: ElasticCloudIntegrationAccountUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_elastic_cloud_integration_account(account_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update Elastic Cloud account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).row_at("/data"),
    )
}

pub async fn accounts_delete(cfg: &Config, account_id: &str) -> Result<()> {
    let api = crate::make_api!(ElasticCloudIntegrationAPI, cfg);
    api.delete_elastic_cloud_integration_account(account_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete Elastic Cloud account: {e:?}"))?;
    println!("Elastic Cloud account {account_id} deleted.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    #[tokio::test]
    async fn test_elastic_cloud_accounts_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_elastic_cloud_accounts_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_get(&cfg, "id").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_elastic_cloud_accounts_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_create(&cfg, "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_elastic_cloud_accounts_update() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_update(&cfg, "id", "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_elastic_cloud_accounts_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_delete(&cfg, "id").await;
        cleanup_env();
    }
}

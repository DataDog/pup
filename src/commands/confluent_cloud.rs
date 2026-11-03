use crate::config::Config;
use crate::formatter;
use anyhow::Result;
use datadog_api_client::datadogV2::api_confluent_cloud::ConfluentCloudAPI;

use datadog_api_client::datadogV2::model::{
    ConfluentAccountCreateRequest, ConfluentAccountUpdateRequest, ConfluentResourceRequest,
};

const ACCOUNTS_COLUMNS: &[&str] = &["id", "attributes.api_key", "attributes.tags"];

const RESOURCES_COLUMNS: &[&str] = &[
    "id",
    "attributes.resource_type",
    "attributes.enable_custom_metrics",
    "attributes.tags",
];

pub async fn accounts_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(ConfluentCloudAPI, cfg);
    let resp = api
        .list_confluent_account()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Confluent Cloud accounts: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).rows_at("/data"),
    )
}

pub async fn accounts_get(cfg: &Config, account_id: &str) -> Result<()> {
    let api = crate::make_api!(ConfluentCloudAPI, cfg);
    let resp = api
        .get_confluent_account(account_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get Confluent Cloud account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).row_at("/data"),
    )
}

pub async fn accounts_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(ConfluentCloudAPI, cfg);
    let body: ConfluentAccountCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_confluent_account(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create Confluent Cloud account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).row_at("/data"),
    )
}

pub async fn accounts_update(cfg: &Config, account_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(ConfluentCloudAPI, cfg);
    let body: ConfluentAccountUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_confluent_account(account_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update Confluent Cloud account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).row_at("/data"),
    )
}

pub async fn accounts_delete(cfg: &Config, account_id: &str) -> Result<()> {
    let api = crate::make_api!(ConfluentCloudAPI, cfg);
    api.delete_confluent_account(account_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete Confluent Cloud account: {e:?}"))?;
    println!("Confluent Cloud account {account_id} deleted.");
    Ok(())
}

pub async fn resources_list(cfg: &Config, account_id: &str) -> Result<()> {
    let api = crate::make_api!(ConfluentCloudAPI, cfg);
    let resp = api
        .list_confluent_resource(account_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Confluent Cloud resources: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(RESOURCES_COLUMNS).rows_at("/data"),
    )
}

pub async fn resources_get(cfg: &Config, account_id: &str, resource_id: &str) -> Result<()> {
    let api = crate::make_api!(ConfluentCloudAPI, cfg);
    let resp = api
        .get_confluent_resource(account_id.to_string(), resource_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get Confluent Cloud resource: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(RESOURCES_COLUMNS).row_at("/data"),
    )
}

pub async fn resources_create(cfg: &Config, account_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(ConfluentCloudAPI, cfg);
    let body: ConfluentResourceRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_confluent_resource(account_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create Confluent Cloud resource: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(RESOURCES_COLUMNS).row_at("/data"),
    )
}

pub async fn resources_update(
    cfg: &Config,
    account_id: &str,
    resource_id: &str,
    file: &str,
) -> Result<()> {
    let api = crate::make_api!(ConfluentCloudAPI, cfg);
    let body: ConfluentResourceRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_confluent_resource(account_id.to_string(), resource_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update Confluent Cloud resource: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(RESOURCES_COLUMNS).row_at("/data"),
    )
}

pub async fn resources_delete(cfg: &Config, account_id: &str, resource_id: &str) -> Result<()> {
    let api = crate::make_api!(ConfluentCloudAPI, cfg);
    api.delete_confluent_resource(account_id.to_string(), resource_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete Confluent Cloud resource: {e:?}"))?;
    println!("Confluent Cloud resource {resource_id} deleted.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    #[tokio::test]
    async fn test_confluent_cloud_accounts_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_confluent_cloud_accounts_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_get(&cfg, "id").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_confluent_cloud_accounts_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_create(&cfg, "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_confluent_cloud_accounts_update() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_update(&cfg, "id", "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_confluent_cloud_accounts_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_delete(&cfg, "id").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_confluent_cloud_resources_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::resources_list(&cfg, "acct").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_confluent_cloud_resources_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::resources_get(&cfg, "acct", "id").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_confluent_cloud_resources_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::resources_create(&cfg, "acct", "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_confluent_cloud_resources_update() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::resources_update(&cfg, "acct", "id", "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_confluent_cloud_resources_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::resources_delete(&cfg, "acct", "id").await;
        cleanup_env();
    }
}

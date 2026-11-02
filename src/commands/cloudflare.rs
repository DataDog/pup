use crate::config::Config;
use crate::formatter;
use anyhow::Result;
use datadog_api_client::datadogV2::api_cloudflare_integration::CloudflareIntegrationAPI;

use datadog_api_client::datadogV2::model::{
    CloudflareAccountCreateRequest, CloudflareAccountUpdateRequest,
};

const ACCOUNTS_COLUMNS: &[&str] = &["id", "attributes.name", "attributes.email"];

pub async fn accounts_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(CloudflareIntegrationAPI, cfg);
    let resp = api
        .list_cloudflare_accounts()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Cloudflare accounts: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).rows_at("/data"),
    )
}

pub async fn accounts_get(cfg: &Config, account_id: &str) -> Result<()> {
    let api = crate::make_api!(CloudflareIntegrationAPI, cfg);
    let resp = api
        .get_cloudflare_account(account_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get Cloudflare account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).row_at("/data"),
    )
}

pub async fn accounts_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(CloudflareIntegrationAPI, cfg);
    let body: CloudflareAccountCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_cloudflare_account(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create Cloudflare account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).row_at("/data"),
    )
}

pub async fn accounts_update(cfg: &Config, account_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(CloudflareIntegrationAPI, cfg);
    let body: CloudflareAccountUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_cloudflare_account(account_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update Cloudflare account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).row_at("/data"),
    )
}

pub async fn accounts_delete(cfg: &Config, account_id: &str) -> Result<()> {
    let api = crate::make_api!(CloudflareIntegrationAPI, cfg);
    api.delete_cloudflare_account(account_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete Cloudflare account: {e:?}"))?;
    println!("Cloudflare account {account_id} deleted.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    #[tokio::test]
    async fn test_cloudflare_accounts_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cloudflare_accounts_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_get(&cfg, "id").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cloudflare_accounts_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_create(&cfg, "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cloudflare_accounts_update() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_update(&cfg, "id", "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cloudflare_accounts_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_delete(&cfg, "id").await;
        cleanup_env();
    }
}

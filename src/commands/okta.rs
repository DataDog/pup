use crate::config::Config;
use crate::formatter;
use anyhow::Result;
use datadog_api_client::datadogV2::api_okta_integration::OktaIntegrationAPI;

use datadog_api_client::datadogV2::model::{OktaAccountRequest, OktaAccountUpdateRequest};

const ACCOUNTS_COLUMNS: &[&str] = &[
    "id",
    "attributes.name",
    "attributes.domain",
    "attributes.auth_method",
];

pub async fn accounts_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(OktaIntegrationAPI, cfg);
    let resp = api
        .list_okta_accounts()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Okta accounts: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).rows_at("/data"),
    )
}

pub async fn accounts_get(cfg: &Config, account_id: &str) -> Result<()> {
    let api = crate::make_api!(OktaIntegrationAPI, cfg);
    let resp = api
        .get_okta_account(account_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get Okta account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).row_at("/data"),
    )
}

pub async fn accounts_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(OktaIntegrationAPI, cfg);
    let body: OktaAccountRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_okta_account(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create Okta account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).row_at("/data"),
    )
}

pub async fn accounts_update(cfg: &Config, account_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(OktaIntegrationAPI, cfg);
    let body: OktaAccountUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_okta_account(account_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update Okta account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).row_at("/data"),
    )
}

pub async fn accounts_delete(cfg: &Config, account_id: &str) -> Result<()> {
    let api = crate::make_api!(OktaIntegrationAPI, cfg);
    api.delete_okta_account(account_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete Okta account: {e:?}"))?;
    println!("Okta account {account_id} deleted.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    #[tokio::test]
    async fn test_okta_accounts_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_okta_accounts_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_get(&cfg, "id").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_okta_accounts_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_create(&cfg, "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_okta_accounts_update() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_update(&cfg, "id", "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_okta_accounts_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_delete(&cfg, "id").await;
        cleanup_env();
    }
}

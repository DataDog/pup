use crate::config::Config;
use crate::formatter;
use anyhow::Result;
use datadog_api_client::datadogV2::api_fastly_integration::FastlyIntegrationAPI;

use datadog_api_client::datadogV2::model::{
    FastlyAccountCreateRequest, FastlyAccountUpdateRequest, FastlyServiceRequest,
};

const ACCOUNTS_COLUMNS: &[&str] = &["id", "attributes.name"];

const SERVICES_COLUMNS: &[&str] = &["id", "attributes.tags"];

pub async fn accounts_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(FastlyIntegrationAPI, cfg);
    let resp = api
        .list_fastly_accounts()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Fastly accounts: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).rows_at("/data"),
    )
}

pub async fn accounts_get(cfg: &Config, account_id: &str) -> Result<()> {
    let api = crate::make_api!(FastlyIntegrationAPI, cfg);
    let resp = api
        .get_fastly_account(account_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get Fastly account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).row_at("/data"),
    )
}

pub async fn accounts_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(FastlyIntegrationAPI, cfg);
    let body: FastlyAccountCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_fastly_account(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create Fastly account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).row_at("/data"),
    )
}

pub async fn accounts_update(cfg: &Config, account_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(FastlyIntegrationAPI, cfg);
    let body: FastlyAccountUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_fastly_account(account_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update Fastly account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNTS_COLUMNS).row_at("/data"),
    )
}

pub async fn accounts_delete(cfg: &Config, account_id: &str) -> Result<()> {
    let api = crate::make_api!(FastlyIntegrationAPI, cfg);
    api.delete_fastly_account(account_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete Fastly account: {e:?}"))?;
    println!("Fastly account {account_id} deleted.");
    Ok(())
}

pub async fn services_list(cfg: &Config, account_id: &str) -> Result<()> {
    let api = crate::make_api!(FastlyIntegrationAPI, cfg);
    let resp = api
        .list_fastly_services(account_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Fastly services: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(SERVICES_COLUMNS).rows_at("/data"),
    )
}

pub async fn services_get(cfg: &Config, account_id: &str, service_id: &str) -> Result<()> {
    let api = crate::make_api!(FastlyIntegrationAPI, cfg);
    let resp = api
        .get_fastly_service(account_id.to_string(), service_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get Fastly service: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(SERVICES_COLUMNS).row_at("/data"),
    )
}

pub async fn services_create(cfg: &Config, account_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(FastlyIntegrationAPI, cfg);
    let body: FastlyServiceRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_fastly_service(account_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create Fastly service: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(SERVICES_COLUMNS).row_at("/data"),
    )
}

pub async fn services_update(
    cfg: &Config,
    account_id: &str,
    service_id: &str,
    file: &str,
) -> Result<()> {
    let api = crate::make_api!(FastlyIntegrationAPI, cfg);
    let body: FastlyServiceRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_fastly_service(account_id.to_string(), service_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update Fastly service: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(SERVICES_COLUMNS).row_at("/data"),
    )
}

pub async fn services_delete(cfg: &Config, account_id: &str, service_id: &str) -> Result<()> {
    let api = crate::make_api!(FastlyIntegrationAPI, cfg);
    api.delete_fastly_service(account_id.to_string(), service_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete Fastly service: {e:?}"))?;
    println!("Fastly service {service_id} deleted.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    #[tokio::test]
    async fn test_fastly_accounts_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_fastly_accounts_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_get(&cfg, "id").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_fastly_accounts_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_create(&cfg, "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_fastly_accounts_update() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_update(&cfg, "id", "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_fastly_accounts_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::accounts_delete(&cfg, "id").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_fastly_services_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::services_list(&cfg, "acct").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_fastly_services_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::services_get(&cfg, "acct", "id").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_fastly_services_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::services_create(&cfg, "acct", "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_fastly_services_update() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::services_update(&cfg, "acct", "id", "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_fastly_services_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::services_delete(&cfg, "acct", "id").await;
        cleanup_env();
    }
}

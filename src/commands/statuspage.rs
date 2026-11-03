//! Atlassian Statuspage integration. Distinct from `pup status-pages`, which
//! manages Datadog's own Status Pages product.

use crate::config::Config;
use crate::formatter;
use anyhow::Result;
use datadog_api_client::datadogV2::api_statuspage_integration::StatuspageIntegrationAPI;
use datadog_api_client::datadogV2::model::{
    StatuspageAccountCreateRequest, StatuspageAccountUpdateRequest,
    StatuspageUrlSettingCreateRequest, StatuspageUrlSettingUpdateRequest,
};

const ACCOUNT_COLUMNS: &[&str] = &["id", "attributes.api_key"];
const URL_SETTINGS_COLUMNS: &[&str] = &["id", "attributes.url", "attributes.custom_tags"];

/// The Statuspage account is a singleton per organization, so there is no
/// `list` and no account id to pass.
pub async fn account_get(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(StatuspageIntegrationAPI, cfg);
    let resp = api
        .get_statuspage_account()
        .await
        .map_err(|e| anyhow::anyhow!("failed to get Statuspage account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNT_COLUMNS).row_at("/data"),
    )
}

pub async fn account_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(StatuspageIntegrationAPI, cfg);
    let body: StatuspageAccountCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_statuspage_account(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create Statuspage account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNT_COLUMNS).row_at("/data"),
    )
}

pub async fn account_update(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(StatuspageIntegrationAPI, cfg);
    let body: StatuspageAccountUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_statuspage_account(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update Statuspage account: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ACCOUNT_COLUMNS).row_at("/data"),
    )
}

pub async fn account_delete(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(StatuspageIntegrationAPI, cfg);
    api.delete_statuspage_account()
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete Statuspage account: {e:?}"))?;
    println!("Statuspage account deleted.");
    Ok(())
}

pub async fn url_settings_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(StatuspageIntegrationAPI, cfg);
    let resp = api
        .list_statuspage_url_settings()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Statuspage URL settings: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(URL_SETTINGS_COLUMNS).rows_at("/data"),
    )
}

pub async fn url_settings_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(StatuspageIntegrationAPI, cfg);
    let body: StatuspageUrlSettingCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_statuspage_url_setting(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create Statuspage URL setting: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(URL_SETTINGS_COLUMNS).row_at("/data"),
    )
}

pub async fn url_settings_update(cfg: &Config, setting_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(StatuspageIntegrationAPI, cfg);
    let body: StatuspageUrlSettingUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_statuspage_url_setting(setting_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update Statuspage URL setting: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(URL_SETTINGS_COLUMNS).row_at("/data"),
    )
}

pub async fn url_settings_delete(cfg: &Config, setting_id: &str) -> Result<()> {
    let api = crate::make_api!(StatuspageIntegrationAPI, cfg);
    api.delete_statuspage_url_setting(setting_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete Statuspage URL setting: {e:?}"))?;
    println!("Statuspage URL setting {setting_id} deleted.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    #[tokio::test]
    async fn test_statuspage_account_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::account_get(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_statuspage_account_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::account_create(&cfg, "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_statuspage_account_update() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::account_update(&cfg, "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_statuspage_account_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let _ = super::account_delete(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_statuspage_url_settings_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::url_settings_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_statuspage_url_settings_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::url_settings_create(&cfg, "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_statuspage_url_settings_update() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::url_settings_update(&cfg, "id", "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_statuspage_url_settings_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let _ = super::url_settings_delete(&cfg, "id").await;
        cleanup_env();
    }
}

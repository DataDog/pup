use anyhow::Result;
use datadog_api_client::datadogV2::api_service_definition::{
    GetServiceDefinitionOptionalParams, ListServiceDefinitionsOptionalParams, ServiceDefinitionAPI,
};

use crate::config::Config;
use crate::formatter;

pub async fn list(cfg: &Config, page_size: i64, page_number: i64) -> Result<()> {
    let api = crate::make_api!(ServiceDefinitionAPI, cfg);
    let resp = api
        .list_service_definitions(
            ListServiceDefinitionsOptionalParams::default()
                .page_size(page_size)
                .page_number(page_number),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to list services: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get(cfg: &Config, service_name: &str) -> Result<()> {
    let api = crate::make_api!(ServiceDefinitionAPI, cfg);
    let resp = api
        .get_service_definition(
            service_name.to_string(),
            GetServiceDefinitionOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to get service: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_service_catalog_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::list(&cfg, 10, 0).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_service_catalog_list_with_pagination() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::list(&cfg, 50, 2).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_service_catalog_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::get(&cfg, "svc1").await;
        cleanup_env();
    }
}

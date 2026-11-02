use crate::config::Config;
use crate::formatter;
use anyhow::Result;
use datadog_api_client::datadogV2::api_salesforce_integration::SalesforceIntegrationAPI;
use datadog_api_client::datadogV2::model::{
    SalesforceIncidentsTemplateCreateRequest, SalesforceIncidentsTemplateUpdateRequest,
};

const ORGANIZATIONS_COLUMNS: &[&str] = &[
    "id",
    "attributes.name",
    "attributes.instance_url",
    "attributes.sfdc_org_id",
];
const TEMPLATES_COLUMNS: &[&str] = &[
    "id",
    "attributes.name",
    "attributes.subject",
    "attributes.priority",
];

/// Salesforce organizations are connected through the Datadog UI's OAuth flow,
/// so the API exposes only list and delete.
pub async fn organizations_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(SalesforceIntegrationAPI, cfg);
    let resp = api
        .get_salesforce_organizations()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Salesforce organizations: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(ORGANIZATIONS_COLUMNS).rows_at("/data"),
    )
}

pub async fn organizations_delete(cfg: &Config, org_id: &str) -> Result<()> {
    let api = crate::make_api!(SalesforceIntegrationAPI, cfg);
    api.delete_salesforce_organization(org_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete Salesforce organization: {e:?}"))?;
    println!("Salesforce organization {org_id} deleted.");
    Ok(())
}

pub async fn templates_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(SalesforceIntegrationAPI, cfg);
    let resp = api
        .get_incident_templates()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Salesforce incident templates: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(TEMPLATES_COLUMNS).rows_at("/data"),
    )
}

pub async fn templates_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(SalesforceIntegrationAPI, cfg);
    let body: SalesforceIncidentsTemplateCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_incident_template(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create Salesforce incident template: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(TEMPLATES_COLUMNS).row_at("/data"),
    )
}

pub async fn templates_update(cfg: &Config, template_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(SalesforceIntegrationAPI, cfg);
    let body: SalesforceIncidentsTemplateUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_incident_template(template_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update Salesforce incident template: {e:?}"))?;
    formatter::output_with_table(
        cfg,
        &resp,
        formatter::TableOptions::new(TEMPLATES_COLUMNS).row_at("/data"),
    )
}

pub async fn templates_delete(cfg: &Config, template_id: &str) -> Result<()> {
    let api = crate::make_api!(SalesforceIntegrationAPI, cfg);
    api.delete_incident_template(template_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete Salesforce incident template: {e:?}"))?;
    println!("Salesforce incident template {template_id} deleted.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    #[tokio::test]
    async fn test_salesforce_organizations_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::organizations_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_salesforce_organizations_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let _ = super::organizations_delete(&cfg, "org").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_salesforce_templates_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::templates_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_salesforce_templates_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::templates_create(&cfg, "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_salesforce_templates_update() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::templates_update(&cfg, "id", "/tmp/pup-missing.json").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_salesforce_templates_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let _ = super::templates_delete(&cfg, "id").await;
        cleanup_env();
    }
}
